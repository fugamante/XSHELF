mod common;

use common::*;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Output;

fn record(id: &str, replicas: u32, retries: u32, converge: &str) -> Value {
    json!({
        "id": id,
        "parent_id": null,
        "role": "implementer",
        "objective": "Synthetic task prompt",
        "context_ref": "",
        "backend": "auto",
        "model": null,
        "profile": "balanced",
        "converge": converge,
        "replicas": replicas,
        "max_concurrency": null,
        "run_mode": "sequential",
        "depends_on": [],
        "resource_keys": [],
        "max_retries": retries,
        "timeout_secs": null,
        "status": "pending",
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z"
    })
}

fn write_tasks(repo: &TempRepo, tasks: Vec<Value>) {
    fs::write(repo.tasks_file(), serde_json::to_vec(&tasks).unwrap()).unwrap();
}

fn run_mock(repo: &TempRepo, args: &[&str]) -> Output {
    repo.run_with_env(
        args,
        &[
            ("CX_PROVIDER_ADAPTER", "mock"),
            ("CX_MOCK_PLAIN_RESPONSE", "synthetic-answer"),
            ("CX_TASK_TRUST_COMMANDS", "0"),
            ("CX_TASK_SANDBOX_ENABLED", "0"),
        ],
    )
}

fn task_runs(repo: &TempRepo) -> usize {
    fs::read_to_string(repo.runs_log())
        .ok()
        .into_iter()
        .flat_map(|text| text.lines().map(str::to_owned).collect::<Vec<_>>())
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter(|row| row["tool"] == "cxtask_run")
        .count()
}

fn run_rows(repo: &TempRepo) -> Vec<Value> {
    fs::read_to_string(repo.runs_log())
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn line_count(path: &Path) -> usize {
    fs::read_to_string(path).unwrap_or_default().lines().count()
}

#[test]
fn add_budget() {
    let repo = TempRepo::new("cxrs-task-budget-add");
    for extra in [
        vec!["--converge", "majority", "--replicas", "17"],
        vec![
            "--converge",
            "majority",
            "--replicas",
            "2",
            "--max-retries",
            "16",
        ],
        vec!["--max-retries", "4294967295"],
        vec![
            "--converge",
            "judge",
            "--replicas",
            "16",
            "--max-retries",
            "1",
        ],
    ] {
        let mut args = vec!["task", "add", "Synthetic task prompt"];
        args.extend(extra);
        let out = repo.run(&args);
        assert_eq!(out.status.code(), Some(2), "{}", stderr_str(&out));
        assert!(stderr_str(&out).contains("exceeds maximum"));
    }
    assert!(!repo.tasks_file().exists());
}

#[test]
fn direct_budget() {
    let repo = TempRepo::new("cxrs-task-budget-direct");
    write_tasks(&repo, vec![record("task_001", 17, 0, "majority")]);
    let out = run_mock(&repo, &["task", "run", "task_001", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let value: Value = serde_json::from_str(&stdout_str(&out)).unwrap();
    assert_eq!(value["contract_version"], "task-run.v1");
    assert_eq!(value["status"], "error");
    assert!(value["error"].as_str().unwrap().contains("replicas=17"));
    assert_eq!(task_runs(&repo), 0);
    let tasks: Value =
        serde_json::from_str(&fs::read_to_string(repo.tasks_file()).unwrap()).unwrap();
    assert_eq!(tasks[0]["status"], "pending");
    let show = repo.run(&["task", "show", "task_001"]);
    assert!(show.status.success(), "{}", stderr_str(&show));
}

#[test]
fn runall_budget() {
    for mode in ["sequential", "parallel"] {
        for format in ["--text", "--json"] {
            let repo = TempRepo::new("cxrs-task-budget-all");
            write_tasks(
                &repo,
                vec![
                    record("task_001", 2, 0, "majority"),
                    record("task_002", 2, 16, "majority"),
                ],
            );
            let plan = repo.run(&["task", "run-all", "--mode", mode, "--plan-json"]);
            assert!(plan.status.success(), "{}", stderr_str(&plan));
            let out = run_mock(&repo, &["task", "run-all", "--mode", mode, format]);
            assert_eq!(
                out.status.code(),
                Some(1),
                "mode={mode} {}",
                stderr_str(&out)
            );
            assert!(stderr_str(&out).contains("potential executions=34"));
            // Text planning may print its existing preview before admission.
            // JSON preflight errors use stderr and emit no partial contract.
            if format == "--json" {
                assert!(stdout_str(&out).is_empty());
            } else {
                assert!(!stdout_str(&out).contains("status=complete"));
            }
            assert_eq!(task_runs(&repo), 0);
            let tasks: Value =
                serde_json::from_str(&fs::read_to_string(repo.tasks_file()).unwrap()).unwrap();
            assert!(
                tasks
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|task| task["status"] == "pending")
            );
        }
    }
}

#[test]
fn valid_reuse() {
    let repo = TempRepo::new("cxrs-task-budget-valid");
    let add = repo.run(&[
        "task",
        "add",
        "Synthetic task prompt",
        "--converge",
        "majority",
        "--replicas",
        "2",
        "--max-retries",
        "1",
    ]);
    assert!(add.status.success(), "{}", stderr_str(&add));
    let id = stdout_str(&add).trim().to_string();
    let out = run_mock(&repo, &["task", "run", &id, "--json"]);
    assert!(out.status.success(), "{}", stderr_str(&out));
    let value: Value = serde_json::from_str(&stdout_str(&out)).unwrap();
    assert_eq!(value["status"], "complete");
    assert_eq!(task_runs(&repo), 2);
}

#[test]
fn valid_runall_json() {
    for mode in ["sequential", "parallel"] {
        let repo = TempRepo::new("cxrs-task-budget-valid-all");
        repo.write_mock_primary(
            r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":2,"output_tokens":1}}'
"#,
        );
        write_tasks(&repo, vec![record("task_001", 2, 1, "majority")]);
        let out = run_mock(&repo, &["task", "run-all", "--mode", mode, "--json"]);
        assert!(out.status.success(), "mode={mode} {}", stderr_str(&out));
        let value: Value = serde_json::from_str(&stdout_str(&out)).unwrap();
        assert_eq!(value["contract_version"], "task-run-all.v1");
        assert_eq!(value["complete"], 1);
        assert_eq!(task_runs(&repo), 2);
    }
}

#[test]
fn inherited_retry_budget() {
    let repo = TempRepo::new("cxrs-task-budget-inherited");
    write_tasks(&repo, vec![record("task_001", 2, 0, "majority")]);
    let out = repo.run_with_env(
        &["task", "run", "task_001", "--json"],
        &[("CX_TASK_RETRY_MAX", "16"), ("CX_PROVIDER_ADAPTER", "mock")],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout_str(&out).contains("potential executions=34"));
    assert_eq!(task_runs(&repo), 0);
    let tasks: Value =
        serde_json::from_str(&fs::read_to_string(repo.tasks_file()).unwrap()).unwrap();
    assert_eq!(tasks[0]["status"], "pending");
}

#[test]
fn sandbox_budget_binds_stored_task() {
    for (mode, limit) in [("majority", "1"), ("judge", "2")] {
        let repo = TempRepo::new("cxrs-task-budget-handoff");
        write_tasks(&repo, vec![record("task_001", 2, 0, mode)]);
        let out = repo.run_with_env(
            &[
                "task",
                "run",
                "task_001",
                "--managed-by-parent",
                "--sandbox-budget-v1",
                limit,
                "--json",
            ],
            &[("CX_PROVIDER_ADAPTER", "mock")],
        );
        assert_eq!(out.status.code(), Some(1));
        assert!(stdout_str(&out).contains("exceeds admitted sandbox budget"));
        assert_eq!(task_runs(&repo), 0);
        let tasks: Value =
            serde_json::from_str(&fs::read_to_string(repo.tasks_file()).unwrap()).unwrap();
        assert_eq!(tasks[0]["status"], "pending");
    }
}

#[test]
fn sandbox_single_handoff() {
    const IMAGE: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let image_root = tempfile::tempdir().unwrap();
    let image_bin = image_root.path().join("xshelf-image");
    fs::copy(env!("CARGO_BIN_EXE_cxrs"), &image_bin).unwrap();
    assert!(!image_bin.starts_with("/work"));
    let image_bin = image_bin.to_str().unwrap();
    for converge in ["majority", "first_valid"] {
        for format in ["--json", "--text"] {
            let repo = TempRepo::new("cxrs-task-budget-sandbox");
            repo.write_mock("docker", r#"#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
 image) printf '%s\n' 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'; exit 0 ;;
 --version) printf '%s\n' mock; exit 0 ;;
 run) printf 'x\n' >> docker-calls ;;
 *) exit 1 ;;
esac
# Docker options must precede IMAGE; shell arguments start immediately after it.
task_env=0
while [[ $# -gt 0 ]]; do
  [[ "$1" == sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ]] && break
  if [[ "$1" == -e ]]; then
    shift
    [[ "$1" == CX_TASK_ID ]] && task_env=1
    export "$1"
  fi
  shift
done
[[ "$task_env" == 1 && "${CX_TASK_ID:-}" == "$MOCK_EXPECT_TASK_ID" ]]
[[ "${1:-}" == sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ]]
shift
[[ "${1:-}" == --noprofile && "${2:-}" == --norc && "${3:-}" == -c ]]
shift 3
[[ $# == 1 ]]
script="$1"
PATH="$MOCK_IMAGE_BIN:/usr/bin:/bin" /bin/bash -c "$script"
if [[ "${MOCK_FOREIGN_LOG:-}" == "1" ]]; then
  printf '%s\n' '{"tool":"cxtask_converge","task_id":"task_foreign","execution_id":"foreign"}' >> .cx/cxlogs/runs.jsonl
fi
"#);
            repo.write_mock_primary(
                r#"#!/usr/bin/env bash
printf 'x\n' >> provider-calls
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":2,"output_tokens":1}}'
"#,
            );
            assert!(
                repo.run(&["task", "sandbox", "set-image", "synthetic:local"])
                    .status
                    .success()
            );
            assert!(repo.run(&["task", "sandbox", "enable"]).status.success());
            let add = repo.run(&[
                "task",
                "add",
                "Describe progress",
                "--converge",
                converge,
                "--replicas",
                "2",
            ]);
            assert!(add.status.success(), "{}", stderr_str(&add));
            let id = stdout_str(&add).trim().to_string();
            let mock_bin = repo.mock_bin.to_str().unwrap();
            let inject_foreign = if converge == "majority" && format == "--json" {
                "1"
            } else {
                "0"
            };
            let out = repo.run_with_env(
                &["task", "run", &id, format],
                &[
                    ("CX_LLM_BACKEND", "primary"),
                    ("CX_TASK_TRUST_SANDBOX", "1"),
                    ("CX_TASK_SANDBOX_IMAGE", IMAGE),
                    ("CX_TASK_SANDBOX_EXECUTABLE", image_bin),
                    ("CX_TASK_SANDBOX_SHARE_ENV", "MOCK_IMAGE_BIN"),
                    ("MOCK_IMAGE_BIN", mock_bin),
                    ("MOCK_FOREIGN_LOG", inject_foreign),
                    ("MOCK_EXPECT_TASK_ID", &id),
                ],
            );
            assert!(
                out.status.success(),
                "{converge} {format} stderr={} stdout={}",
                stderr_str(&out),
                stdout_str(&out)
            );
            assert_eq!(line_count(&repo.root.join("docker-calls")), 1);
            let expected = if converge == "first_valid" { 1 } else { 2 };
            assert_eq!(line_count(&repo.root.join("provider-calls")), expected);
            let rows = run_rows(&repo);
            assert_eq!(
                rows.iter()
                    .filter(|row| row["tool"] == "cxtask_run")
                    .count(),
                expected
            );
            let converge_row = rows
                .iter()
                .filter(|row| row["tool"] == "cxtask_converge" && row["task_id"] == id)
                .collect::<Vec<_>>();
            assert_eq!(converge_row.len(), 1);
            let votes = &converge_row[0]["converge_votes"];
            let winner = votes["winner"].as_u64().unwrap();
            let selected = votes["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .find(|candidate| candidate["index"] == winner)
                .unwrap();
            let selected_id = selected["execution_id"].as_str().unwrap();
            if format == "--json" {
                let payload: Value = serde_json::from_str(&stdout_str(&out)).unwrap();
                assert_eq!(payload["contract_version"], "task-run.v1");
                assert_eq!(payload["status"], "complete");
                assert_eq!(payload["execution_id"], selected_id);
            } else {
                assert!(stdout_str(&out).contains(&format!("execution_id: {selected_id}")));
            }
        }
    }
}
