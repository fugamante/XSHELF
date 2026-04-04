mod common;

use common::*;
use serde_json::Value;

#[test]
fn task_lifecycle_add_claim_complete() {
    let repo = TempRepo::new("cxrs-it");

    let add = repo.run(&[
        "task",
        "add",
        "Implement parser hardening",
        "--role",
        "implementer",
    ]);
    assert!(
        add.status.success(),
        "stdout={} stderr={}",
        stdout_str(&add),
        stderr_str(&add)
    );
    let id = stdout_str(&add).trim().to_string();
    assert!(id.starts_with("task_"), "unexpected task id: {id}");

    let claim = repo.run(&["task", "claim", &id]);
    assert!(claim.status.success(), "stderr={}", stderr_str(&claim));
    assert!(stdout_str(&claim).contains("in_progress"));

    let complete = repo.run(&["task", "complete", &id]);
    assert!(
        complete.status.success(),
        "stderr={}",
        stderr_str(&complete)
    );
    assert!(stdout_str(&complete).contains("complete"));

    let tasks = read_json(&repo.tasks_file());
    let task = tasks
        .as_array()
        .expect("tasks array")
        .iter()
        .find(|t| t.get("id").and_then(Value::as_str) == Some(id.as_str()))
        .expect("task exists");
    assert_eq!(task.get("status").and_then(Value::as_str), Some("complete"));
}

#[test]
fn show_latest_run() {
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "codex",
        r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#,
    );

    let add = repo.run(&[
        "task",
        "add",
        "cxo echo show-summary",
        "--role",
        "implementer",
        "--backend",
        "codex",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let id = stdout_str(&add).trim().to_string();

    let run = repo.run(&["task", "run", &id]);
    assert!(
        run.status.success(),
        "stdout={} stderr={}",
        stdout_str(&run),
        stderr_str(&run)
    );

    let show = repo.run(&["task", "show", &id]);
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    let out: Value = serde_json::from_str(&stdout_str(&show)).expect("valid json");
    let latest = out.get("latest_run").expect("latest_run field");
    assert!(latest.is_object(), "latest_run should be object: {latest}");
    assert!(latest.get("execution_id").is_some(), "missing execution_id");
    assert!(
        latest.get("duration_ms").is_some(),
        "missing duration_ms: {latest}"
    );
    let readiness = out.get("run_readiness").expect("run_readiness field");
    assert_eq!(
        readiness.get("runnable_now").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        readiness.get("recommended_reason").and_then(Value::as_str),
        Some("inspect_non_pending_task")
    );
}

#[test]
fn show_list_alias() {
    let repo = TempRepo::new("cxrs-it");
    let add = repo.run(&["task", "add", "Alias list", "--role", "implementer"]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let id = stdout_str(&add).trim().to_string();

    let show_list = repo.run(&["task", "show", "list"]);
    assert!(
        show_list.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show_list),
        stderr_str(&show_list)
    );
    let text = stdout_str(&show_list);
    assert!(text.contains("id | role | status | parent_id | objective"));
    assert!(text.contains(&id), "{text}");
}

#[test]
fn show_ready_readiness() {
    let repo = TempRepo::new("cxrs-it");
    let add = repo.run(&["task", "add", "Ready task", "--role", "implementer"]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let id = stdout_str(&add).trim().to_string();

    let show = repo.run(&["task", "show", &id]);
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    let out: Value = serde_json::from_str(&stdout_str(&show)).expect("valid json");
    let readiness = out.get("run_readiness").expect("run_readiness field");
    assert_eq!(
        readiness.get("runnable_now").and_then(Value::as_bool),
        Some(true)
    );
    let run_cmd = format!("cx task run {id}");
    assert_eq!(
        readiness.get("recommended_command").and_then(Value::as_str),
        Some(run_cmd.as_str())
    );
}

#[test]
fn show_blocked_readiness() {
    let repo = TempRepo::new("cxrs-it");

    let parent = repo.run(&["task", "add", "Parent task", "--role", "implementer"]);
    assert!(parent.status.success(), "stderr={}", stderr_str(&parent));
    let parent_id = stdout_str(&parent).trim().to_string();

    let child = repo.run(&[
        "task",
        "add",
        "Child task",
        "--role",
        "implementer",
        "--depends-on",
        &parent_id,
    ]);
    assert!(child.status.success(), "stderr={}", stderr_str(&child));
    let child_id = stdout_str(&child).trim().to_string();

    let show = repo.run(&["task", "show", &child_id]);
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    let out: Value = serde_json::from_str(&stdout_str(&show)).expect("valid json");
    let readiness = out.get("run_readiness").expect("run_readiness field");
    assert_eq!(
        readiness.get("runnable_now").and_then(Value::as_bool),
        Some(false)
    );
    let blocked = format!("unresolved dependencies: {parent_id}");
    assert_eq!(
        readiness.get("blocked_reason").and_then(Value::as_str),
        Some(blocked.as_str())
    );
    assert_eq!(
        readiness.get("recommended_command").and_then(Value::as_str),
        Some("cx task check --json")
    );
}
