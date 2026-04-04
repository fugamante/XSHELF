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
const WAVE_QUEUE_PRESSURE_MS: u64 = 2000;

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

pub(crate) fn latest_run_all_sum() -> Option<Value> {
    resolve_log_file()
        .and_then(|p| load_values(&p, 200).ok())
        .and_then(|rows| {
            rows.into_iter()
                .rev()
                .find(|v| v.get("tool").and_then(Value::as_str) == Some("cxtask_runall"))
        })
}

pub(crate) fn latest_wave_sum() -> Option<Value> {
    let rows = resolve_log_file().and_then(|p| load_values(&p, 200).ok())?;
    let mut latest_wave_index: Option<u64> = None;
    let mut latest_wave_mode: Option<String> = None;
    let mut latest_wave_size = 0u64;
    let mut max_queue_wave_index: Option<u64> = None;
    let mut max_queue_wave_ms = 0u64;

    for row in rows {
        let Some(wave_index) = row.get("wave_index").and_then(Value::as_u64) else {
            continue;
        };
        let has_task = row
            .get("task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|task| !task.is_empty());
        if !has_task {
            continue;
        }
        latest_wave_index = Some(wave_index);
        latest_wave_mode = row
            .get("wave_mode")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        latest_wave_size = row.get("wave_size").and_then(Value::as_u64).unwrap_or(0);
        let queue_ms = row.get("queue_ms").and_then(Value::as_u64).unwrap_or(0);
        if queue_ms >= max_queue_wave_ms {
            max_queue_wave_ms = queue_ms;
            max_queue_wave_index = Some(wave_index);
        }
    }

    latest_wave_index.map(|idx| {
        serde_json::json!({
            "latest_wave_index": idx,
            "latest_wave_mode": latest_wave_mode,
            "latest_wave_size": latest_wave_size,
            "max_queue_wave_index": max_queue_wave_index,
            "max_queue_wave_ms": max_queue_wave_ms
        })
    })
}

fn wave_pressure_advice(mode: &str, wave_summary: Option<&Value>) -> Option<String> {
    let wave = wave_summary?;
    let max_queue_wave_ms = wave
        .get("max_queue_wave_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let max_queue_wave_index = wave
        .get("max_queue_wave_index")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if max_queue_wave_index <= 1 || max_queue_wave_ms < WAVE_QUEUE_PRESSURE_MS {
        return None;
    }
    Some(format!(
        "queue pressure is concentrating in later waves; latest mode={mode}, max queue was {}ms in wave {}",
        max_queue_wave_ms, max_queue_wave_index
    ))
}

pub(crate) fn exec_advice_lines(
    latest_summary: Option<&Value>,
    wave_summary: Option<&Value>,
) -> Vec<String> {
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
    let latest_wave_index = wave_summary
        .and_then(|w| w.get("latest_wave_index"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let max_queue_wave_index = wave_summary
        .and_then(|w| w.get("max_queue_wave_index"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let max_queue_wave_ms = wave_summary
        .and_then(|w| w.get("max_queue_wave_ms"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let mut lines = vec![
        format!("task_execution_last_mode: {mode}"),
        format!("task_execution_halted_remaining: {halted_remaining}"),
        format!("task_execution_backend_fallback_rows: {fallback_rows}"),
        format!("task_execution_latest_wave_index: {latest_wave_index}"),
        format!("task_execution_max_queue_wave_index: {max_queue_wave_index}"),
        format!("task_execution_max_queue_wave_ms: {max_queue_wave_ms}"),
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
    } else if let Some(advice) = wave_pressure_advice(mode, wave_summary) {
        lines.push(format!("task_execution_advice: {advice}"));
    } else {
        lines.push(
            "task_execution_advice: latest run-all summary is operationally clean".to_string(),
        );
    }
    lines
}

pub(crate) fn exec_reco_lines(
    latest_summary: Option<&Value>,
    wave_summary: Option<&Value>,
) -> Vec<String> {
    let Some(summary) = latest_summary else {
        return vec![
            "task_execution_recommendation_1: cx task check --json".to_string(),
            "task_execution_recommendation_2: cx scheduler --json".to_string(),
        ];
    };
    let halted_remaining = summary
        .get("run_all_halted_remaining")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fallback_rows = summary
        .get("run_all_backend_fallback_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);
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

    if halted_remaining > 0 {
        return vec![
            "task_execution_recommendation_1: cx scheduler --json --window 20".to_string(),
            "task_execution_recommendation_2: cx task run-all --status pending".to_string(),
        ];
    }
    if critical > 0 {
        return vec![
            "task_execution_recommendation_1: cx scheduler --json --window 20".to_string(),
            "task_execution_recommendation_2: cx task check --json".to_string(),
        ];
    }
    if fallback_rows > 0 {
        return vec![
            "task_execution_recommendation_1: cx scheduler --json --window 20".to_string(),
            "task_execution_recommendation_2: cx doctor".to_string(),
        ];
    }
    if failed > 0 {
        return vec![
            "task_execution_recommendation_1: cx task check --json".to_string(),
            "task_execution_recommendation_2: cx task run-all --status pending".to_string(),
        ];
    }
    if wave_pressure_advice(mode, wave_summary).is_some() {
        let narrower = match mode {
            "parallel" => "cx task run-all --mode mixed --status pending",
            "mixed" => "cx task run-all --mode sequential --status pending",
            _ => "cx scheduler --json --window 20",
        };
        return vec![
            format!("task_execution_recommendation_1: {narrower}"),
            "task_execution_recommendation_2: cx scheduler --json --window 20".to_string(),
        ];
    }
    vec![
        "task_execution_recommendation_1: cx diag --json".to_string(),
        "task_execution_recommendation_2: cx scheduler --json".to_string(),
    ]
}

pub(crate) fn exec_diag_value(
    latest_summary: Option<&Value>,
    wave_summary: Option<&Value>,
) -> Value {
    let Some(summary) = latest_summary else {
        return serde_json::json!({
            "last_mode": Value::Null,
            "halted_remaining": 0,
            "backend_fallback_rows": 0,
            "latest_wave_index": Value::Null,
            "max_queue_wave_index": Value::Null,
            "max_queue_wave_ms": 0,
            "advice": "no recent task run-all summary",
            "recommendations": [
                "cx task check --json",
                "cx scheduler --json"
            ]
        });
    };
    let mode = summary
        .get("run_all_mode")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let halted_remaining = summary
        .get("run_all_halted_remaining")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fallback_rows = summary
        .get("run_all_backend_fallback_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let latest_wave_index = wave_summary
        .and_then(|w| w.get("latest_wave_index"))
        .cloned()
        .unwrap_or(Value::Null);
    let max_queue_wave_index = wave_summary
        .and_then(|w| w.get("max_queue_wave_index"))
        .cloned()
        .unwrap_or(Value::Null);
    let max_queue_wave_ms = wave_summary
        .and_then(|w| w.get("max_queue_wave_ms"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let advice = exec_advice_lines(Some(summary), wave_summary)
        .into_iter()
        .find_map(|line| {
            line.strip_prefix("task_execution_advice: ")
                .map(str::to_string)
        })
        .unwrap_or_else(|| "no recent task run-all summary".to_string());
    let recommendations = exec_reco_lines(Some(summary), wave_summary)
        .into_iter()
        .filter_map(|line| {
            line.split_once(": ")
                .map(|(_, cmd)| Value::String(cmd.to_string()))
        })
        .collect::<Vec<Value>>();
    serde_json::json!({
        "last_mode": mode,
        "halted_remaining": halted_remaining,
        "backend_fallback_rows": fallback_rows,
        "latest_wave_index": latest_wave_index,
        "max_queue_wave_index": max_queue_wave_index,
        "max_queue_wave_ms": max_queue_wave_ms,
        "advice": advice,
        "recommendations": recommendations
    })
}

fn print_exec_advice() {
    println!();
    println!("== task execution advice ==");
    let latest_summary = latest_run_all_sum();
    let latest_wave = latest_wave_sum();
    for line in exec_advice_lines(latest_summary.as_ref(), latest_wave.as_ref()) {
        println!("{line}");
    }
    for line in exec_reco_lines(latest_summary.as_ref(), latest_wave.as_ref()) {
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
    print_exec_advice();
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
    use super::{exec_advice_lines, exec_diag_value, exec_reco_lines, readiness_summary_lines};
    use serde_json::Value;

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
    fn exec_advice_cov() {
        let lines = exec_advice_lines(
            Some(&serde_json::json!({
                "run_all_mode": "mixed",
                "run_all_halted_remaining": 2,
                "run_all_backend_fallback_rows": 1,
                "run_all_backend_fallbacks": "codex->ollama=1",
                "run_all_failed": 1,
                "run_all_critical_errors": 1
            })),
            None,
        );
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

    #[test]
    fn exec_reco_cov() {
        let lines = exec_reco_lines(
            Some(&serde_json::json!({
                "run_all_mode": "mixed",
                "run_all_halted_remaining": 2,
                "run_all_backend_fallback_rows": 1,
                "run_all_failed": 1,
                "run_all_critical_errors": 1
            })),
            None,
        );
        let joined = lines.join("\n");
        assert!(
            joined.contains("task_execution_recommendation_1: cx scheduler --json --window 20"),
            "{joined}"
        );
        assert!(
            joined.contains("task_execution_recommendation_2: cx task run-all --status pending"),
            "{joined}"
        );
    }

    #[test]
    fn exec_diag_cov() {
        let value = exec_diag_value(
            Some(&serde_json::json!({
                "run_all_mode": "mixed",
                "run_all_halted_remaining": 2,
                "run_all_backend_fallback_rows": 1,
                "run_all_backend_fallbacks": "codex->ollama=1",
                "run_all_failed": 1,
                "run_all_critical_errors": 1
            })),
            None,
        );
        assert_eq!(
            value.get("last_mode").and_then(Value::as_str),
            Some("mixed")
        );
        assert_eq!(
            value.get("halted_remaining").and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            value.get("backend_fallback_rows").and_then(Value::as_u64),
            Some(1)
        );
        let recs = value
            .get("recommendations")
            .and_then(Value::as_array)
            .expect("recommendations");
        assert!(
            recs.iter()
                .any(|v| v.as_str() == Some("cx scheduler --json --window 20")),
            "{value}"
        );
    }

    #[test]
    fn exec_wave_cov() {
        let summary = serde_json::json!({
            "run_all_mode": "mixed",
            "run_all_halted_remaining": 0,
            "run_all_backend_fallback_rows": 0,
            "run_all_failed": 0,
            "run_all_critical_errors": 0
        });
        let wave = serde_json::json!({
            "latest_wave_index": 3,
            "max_queue_wave_index": 3,
            "max_queue_wave_ms": 2400
        });
        let lines = exec_advice_lines(Some(&summary), Some(&wave));
        let joined = lines.join("\n");
        assert!(
            joined.contains("task_execution_latest_wave_index: 3"),
            "{joined}"
        );
        assert!(
            joined.contains("queue pressure is concentrating in later waves"),
            "{joined}"
        );
        let recs = exec_reco_lines(Some(&summary), Some(&wave)).join("\n");
        assert!(
            recs.contains("task_execution_recommendation_1: cx task run-all --mode sequential --status pending"),
            "{recs}"
        );
        let value = exec_diag_value(Some(&summary), Some(&wave));
        assert_eq!(
            value.get("max_queue_wave_ms").and_then(Value::as_u64),
            Some(2400)
        );
    }
}
