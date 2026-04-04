use serde_json::Value;
use std::env;
use std::path::Path;
use std::process::Command;

use crate::llm::extract_agent_text;
use crate::logs::load_values;
use crate::paths::resolve_log_file;
use crate::process::run_command_output_with_timeout;
use crate::runtime::{llm_backend, llm_bin_name};
use crate::task_cmds::task_readiness_value;
use crate::tasks::read_tasks;

type JsonlRunner = fn(&str) -> Result<String, String>;
type CxoRunner = fn(&[String]) -> i32;

fn bin_in_path(bin: &str) -> bool {
    let path = match env::var_os("PATH") {
        Some(v) => v,
        None => return false,
    };
    env::split_paths(&path).any(|dir| {
        let candidate = dir.join(bin);
        Path::new(&candidate).is_file()
    })
}

fn check_required_bins(backend: &str, llm_bin: &str) -> usize {
    let required = ["git", "jq"];
    let mut missing_required = 0usize;
    for bin in required {
        if bin_in_path(bin) {
            println!("OK: {bin}");
        } else {
            println!("MISSING: {bin}");
            missing_required += 1;
        }
    }
    if bin_in_path(llm_bin) {
        println!("OK: {llm_bin} (selected backend: {backend})");
    } else {
        println!("MISSING: {llm_bin} (selected backend: {backend})");
        missing_required += 1;
    }
    if backend != "codex" {
        if bin_in_path("codex") {
            println!("OK: codex (recommended primary backend)");
        } else {
            println!("WARN: codex not found (recommended primary backend)");
        }
    }
    missing_required
}

fn probe_json_pipeline(backend: &str, run_llm_jsonl: JsonlRunner) -> Result<(), i32> {
    println!();
    println!("== llm json pipeline ({backend}) ==");
    let probe = run_llm_jsonl("ping").map_err(|e| {
        crate::cx_eprintln!("FAIL: {backend} json pipeline failed: {e}");
        1
    })?;
    let mut agent_count = 0u64;
    let mut reasoning_count = 0u64;
    for line in probe.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("type").and_then(Value::as_str) != Some("item.completed") {
            continue;
        }
        let t = v
            .get("item")
            .and_then(|i| i.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if t == "agent_message" {
            agent_count += 1;
        } else if t == "reasoning" {
            reasoning_count += 1;
        }
    }
    println!("agent_message events: {agent_count}");
    println!("reasoning events:     {reasoning_count}");
    if agent_count < 1 {
        crate::cx_eprintln!("FAIL: expected >=1 agent_message event");
        return Err(1);
    }
    Ok(())
}

fn probe_text_pipeline(backend: &str, run_llm_jsonl: JsonlRunner) -> Result<(), i32> {
    println!();
    println!("== _codex_text equivalent ==");
    let probe2 = run_llm_jsonl("2+2? (just the number)").map_err(|e| {
        crate::cx_eprintln!("FAIL: {backend} text probe failed: {e}");
        1
    })?;
    let txt = extract_agent_text(&probe2).unwrap_or_default();
    println!("output: {txt}");
    if txt.trim() != "4" {
        println!("WARN: expected '4', got '{}'", txt.trim());
    }
    Ok(())
}

fn print_git_context() {
    println!();
    println!("== git context (optional) ==");
    let mut repo_cmd = Command::new("git");
    repo_cmd.args(["rev-parse", "--is-inside-work-tree"]);
    match run_command_output_with_timeout(repo_cmd, "git rev-parse --is-inside-work-tree") {
        Ok(out) if out.status.success() => {
            println!("in git repo: yes");
            let mut branch_cmd = Command::new("git");
            branch_cmd.args(["rev-parse", "--abbrev-ref", "HEAD"]);
            if let Ok(branch_out) =
                run_command_output_with_timeout(branch_cmd, "git rev-parse --abbrev-ref HEAD")
            {
                let branch = String::from_utf8_lossy(&branch_out.stdout)
                    .trim()
                    .to_string();
                if !branch.is_empty() {
                    println!("branch: {branch}");
                }
            }
        }
        _ => println!("in git repo: no (skip git-based checks)"),
    }
}

fn readiness_summary_lines(task_readiness: &Value) -> Vec<String> {
    vec![
        format!(
            "task_readiness_mode: {}",
            task_readiness
                .get("recommended_mode")
                .and_then(Value::as_str)
                .unwrap_or("sequential")
        ),
        format!(
            "task_readiness_mixed: {}",
            task_readiness
                .get("can_run_mixed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ),
        format!(
            "task_readiness_parallel: {}",
            task_readiness
                .get("can_run_parallel")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ),
        format!(
            "task_readiness_waves: {}",
            task_readiness
                .get("waves")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
        format!(
            "task_readiness_parallel_waves: {}",
            task_readiness
                .get("parallel_waves")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
        format!(
            "task_readiness_largest_parallel_wave: {}",
            task_readiness
                .get("largest_parallel_wave")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
    ]
}

fn print_task_readiness() {
    println!();
    println!("== task readiness ==");
    let task_readiness = match read_tasks() {
        Ok(tasks) => task_readiness_value(&tasks, "pending"),
        Err(e) => serde_json::json!({
            "recommended_mode": "sequential",
            "can_run_mixed": false,
            "can_run_parallel": false,
            "waves": 0,
            "parallel_waves": 0,
            "largest_parallel_wave": 0,
            "strict_plan_reason": format!("task_read_failed: {e}")
        }),
    };
    for line in readiness_summary_lines(&task_readiness) {
        println!("{line}");
    }
    if let Some(reason) = task_readiness
        .get("strict_plan_reason")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
    {
        println!("task_readiness_reason: {reason}");
    }
}

fn task_execution_advice_lines(latest_summary: Option<&Value>) -> Vec<String> {
    let Some(summary) = latest_summary else {
        return vec!["task_execution_advice: no recent task run-all summary".to_string()];
    };
    let halted_remaining = summary
        .get("run_all_halted_remaining")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fallback_rows = summary
        .get("run_all_backend_fallback_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fallback_map = summary
        .get("run_all_backend_fallbacks")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .unwrap_or("none");
    let failed = summary
        .get("run_all_failed")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let critical = summary
        .get("run_all_critical_errors")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mode = summary
        .get("run_all_mode")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    let mut lines = vec![
        format!("task_execution_last_mode: {mode}"),
        format!("task_execution_halted_remaining: {halted_remaining}"),
        format!("task_execution_backend_fallback_rows: {fallback_rows}"),
    ];

    if halted_remaining > 0 {
        lines.push(format!(
            "task_execution_advice: rerun pending work after resolving the critical stop; {halted_remaining} task(s) were left unscheduled"
        ));
    } else if critical > 0 {
        lines.push(
            "task_execution_advice: inspect the latest critical failure before widening concurrency"
                .to_string(),
        );
    } else if fallback_rows > 0 {
        lines.push(format!(
            "task_execution_advice: review backend availability or pool policy; fallback observed: {fallback_map}"
        ));
    } else if failed > 0 {
        lines.push(
            "task_execution_advice: inspect failed task ids from the latest run-all summary before retrying"
                .to_string(),
        );
    } else {
        lines.push(
            "task_execution_advice: latest run-all summary is operationally clean".to_string(),
        );
    }
    lines
}

fn print_task_execution_advice() {
    println!();
    println!("== task execution advice ==");
    let latest_summary = resolve_log_file()
        .and_then(|p| load_values(&p, 200).ok())
        .and_then(|rows| {
            rows.into_iter()
                .rev()
                .find(|v| v.get("tool").and_then(Value::as_str) == Some("cxtask_runall"))
        });
    for line in task_execution_advice_lines(latest_summary.as_ref()) {
        println!("{line}");
    }
}

pub fn print_doctor(run_llm_jsonl: JsonlRunner) -> i32 {
    let backend = llm_backend();
    let llm_bin = llm_bin_name();
    println!("== cxrs doctor ==");
    let missing_required = check_required_bins(&backend, llm_bin);
    if missing_required > 0 {
        println!("FAIL: install required binaries before using cxrs.");
        return 1;
    }
    if let Err(code) = probe_json_pipeline(&backend, run_llm_jsonl) {
        return code;
    }
    if let Err(code) = probe_text_pipeline(&backend, run_llm_jsonl) {
        return code;
    }
    print_task_readiness();
    print_task_execution_advice();
    print_git_context();

    println!();
    println!("PASS: core pipeline looks healthy.");
    0
}

pub fn cmd_health(run_llm_jsonl: JsonlRunner, run_cxo: CxoRunner) -> i32 {
    let backend = llm_backend();
    let llm_bin = llm_bin_name();
    println!("== {backend} version ==");
    let mut version_cmd = Command::new(llm_bin);
    version_cmd.arg("--version");
    match run_command_output_with_timeout(version_cmd, &format!("{llm_bin} --version")) {
        Ok(out) => print!("{}", String::from_utf8_lossy(&out.stdout)),
        Err(e) => {
            crate::cx_eprintln!("cxrs health: {backend} --version failed: {e}");
            return 1;
        }
    }
    println!();
    println!("== {backend} json ==");
    let jsonl = match run_llm_jsonl("ping") {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("cxrs health: {backend} json failed: {e}");
            return 1;
        }
    };
    let lines: Vec<&str> = jsonl.lines().collect();
    let keep = lines.len().saturating_sub(4);
    for line in &lines[keep..] {
        println!("{line}");
    }
    println!();
    println!("== _codex_text ==");
    let txt = extract_agent_text(&jsonl).unwrap_or_default();
    println!("{txt}");
    println!();
    println!("== cxo test ==");
    let code = run_cxo(&["git".to_string(), "status".to_string()]);
    if code != 0 {
        return code;
    }
    println!();
    println!("All systems operational.");
    0
}

#[cfg(test)]
mod tests {
    use super::{readiness_summary_lines, task_execution_advice_lines};

    #[test]
    fn readiness_summary_cov() {
        let lines = readiness_summary_lines(&serde_json::json!({
            "recommended_mode": "mixed",
            "can_run_mixed": true,
            "can_run_parallel": false,
            "waves": 3,
            "parallel_waves": 2,
            "largest_parallel_wave": 4
        }));
        let joined = lines.join("\n");
        assert!(joined.contains("task_readiness_mode: mixed"), "{joined}");
        assert!(joined.contains("task_readiness_mixed: true"), "{joined}");
        assert!(
            joined.contains("task_readiness_parallel: false"),
            "{joined}"
        );
        assert!(joined.contains("task_readiness_waves: 3"), "{joined}");
        assert!(
            joined.contains("task_readiness_parallel_waves: 2"),
            "{joined}"
        );
        assert!(
            joined.contains("task_readiness_largest_parallel_wave: 4"),
            "{joined}"
        );
    }

    #[test]
    fn task_execution_advice_cov() {
        let lines = task_execution_advice_lines(Some(&serde_json::json!({
            "run_all_mode": "mixed",
            "run_all_halted_remaining": 2,
            "run_all_backend_fallback_rows": 1,
            "run_all_backend_fallbacks": "codex->ollama=1",
            "run_all_failed": 1,
            "run_all_critical_errors": 1
        })));
        let joined = lines.join("\n");
        assert!(
            joined.contains("task_execution_last_mode: mixed"),
            "{joined}"
        );
        assert!(
            joined.contains("task_execution_halted_remaining: 2"),
            "{joined}"
        );
        assert!(
            joined.contains("task_execution_backend_fallback_rows: 1"),
            "{joined}"
        );
        assert!(
            joined.contains("rerun pending work after resolving the critical stop"),
            "{joined}"
        );
    }
}
