mod common;

use common::*;
use serde_json::Value;
use std::fs;

const MOCK: &str = r#"#!/usr/bin/env bash
cat >/dev/null
printf 'start %s\n' "$CX_TASK_ID" >> .cx/wave-order
if [[ -f .cx/check-overlap ]]; then
  : > ".cx/started-$CX_TASK_ID"
  if [[ "$CX_TASK_ID" == task_001 ]]; then peer=task_002; else peer=task_001; fi
  for ((i=0; i<500; i++)); do
    [[ -f ".cx/started-$peer" ]] && break
    sleep 0.01
  done
  [[ -f ".cx/started-$peer" ]] || exit 1
fi
sleep 0.25
printf 'end %s\n' "$CX_TASK_ID" >> .cx/wave-order
if [[ "$CX_TASK_ID" == task_001 && -f .cx/fail-root ]]; then exit 1; fi
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":5}}'
"#;

fn add(repo: &TempRepo, label: &str, mode: &str, extra: &[&str]) -> String {
    let objective = format!("cxo echo {label}");
    let mut args = vec![
        "task",
        "add",
        objective.as_str(),
        "--role",
        "implementer",
        "--backend",
        "primary",
        "--mode",
        mode,
    ];
    args.extend_from_slice(extra);
    let out = repo.run(&args);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    stdout_str(&out).trim().to_string()
}

fn run_workers(repo: &TempRepo, mode: &str, workers: &str) -> std::process::Output {
    repo.run(&[
        "task",
        "run-all",
        "--status",
        "pending",
        "--mode",
        mode,
        "--backend-pool",
        "primary",
        "--backend-cap",
        "primary=2",
        "--max-workers",
        workers,
        "--events-jsonl",
        "--json",
    ])
}

fn run(repo: &TempRepo, mode: &str) -> std::process::Output {
    run_workers(repo, mode, "2")
}

fn order(repo: &TempRepo) -> Vec<String> {
    fs::read_to_string(repo.root.join(".cx/wave-order"))
        .expect("wave log")
        .lines()
        .map(str::to_string)
        .collect()
}

fn pos(rows: &[String], line: &str) -> usize {
    rows.iter()
        .position(|row| row == line)
        .unwrap_or_else(|| panic!("missing {line}: {rows:?}"))
}

fn events(out: &std::process::Output) -> Vec<Value> {
    stderr_str(out)
        .lines()
        .filter(|line| line.trim_start().starts_with('{'))
        .map(|line| serde_json::from_str::<Value>(line).expect("event JSON"))
        .collect()
}

fn event_pos(rows: &[Value], task: &str, kind: &str) -> usize {
    rows.iter()
        .position(|row| {
            row.get("task_id").and_then(Value::as_str) == Some(task)
                && row.get("event").and_then(Value::as_str) == Some(kind)
        })
        .unwrap_or_else(|| panic!("missing {kind} for {task}: {rows:?}"))
}

#[test]
fn mixed_dependency_order() {
    let repo = TempRepo::new("wave-auth");
    repo.write_mock_primary(MOCK);
    let root = add(&repo, "root", "sequential", &[]);
    let child = add(&repo, "child", "parallel", &["--depends-on", &root]);

    let out = run(&repo, "mixed");
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let lines = order(&repo);
    assert!(pos(&lines, &format!("end {root}")) < pos(&lines, &format!("start {child}")));
    let rows = events(&out);
    assert!(event_pos(&rows, &root, "completed") < event_pos(&rows, &child, "started"));
    let summary: Value = serde_json::from_str(&stdout_str(&out)).expect("summary JSON");
    assert_eq!(summary.get("complete").and_then(Value::as_u64), Some(2));
}

#[test]
fn parallel_resource_order() {
    let repo = TempRepo::new("wave-auth");
    repo.write_mock_primary(MOCK);
    let first = add(
        &repo,
        "first",
        "parallel",
        &["--resource-keys", "repo:write"],
    );
    let second = add(
        &repo,
        "second",
        "parallel",
        &["--resource-keys", "repo:write"],
    );

    let out = run(&repo, "parallel");
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let lines = order(&repo);
    assert!(pos(&lines, &format!("end {first}")) < pos(&lines, &format!("start {second}")));
    let rows = events(&out);
    assert!(event_pos(&rows, &first, "completed") < event_pos(&rows, &second, "started"));
}

#[test]
fn parallel_read_overlap() {
    let repo = TempRepo::new("wave-auth");
    repo.write_mock_primary(MOCK);
    let first = add(
        &repo,
        "first",
        "parallel",
        &["--resource-keys", "repo:read"],
    );
    let second = add(
        &repo,
        "second",
        "parallel",
        &["--resource-keys", "repo:read"],
    );
    fs::write(repo.root.join(".cx/check-overlap"), b"overlap required").expect("overlap marker");

    let out = run(&repo, "parallel");
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let lines = order(&repo);
    let last_start =
        pos(&lines, &format!("start {first}")).max(pos(&lines, &format!("start {second}")));
    let first_end = pos(&lines, &format!("end {first}")).min(pos(&lines, &format!("end {second}")));
    assert!(
        last_start < first_end,
        "same-wave tasks were serialized: {lines:?}"
    );
}

#[test]
fn sequential_parent_block() {
    let repo = TempRepo::new("wave-auth");
    repo.write_mock_primary(MOCK);
    let root = add(&repo, "root", "sequential", &[]);
    let child = add(&repo, "child", "parallel", &["--depends-on", &root]);
    fs::write(repo.root.join(".cx/fail-root"), b"synthetic failure").expect("fail marker");

    let out = run_workers(&repo, "mixed", "1");
    assert_eq!(out.status.code(), Some(1), "stderr={}", stderr_str(&out));
    let lines = order(&repo);
    assert!(
        !lines.contains(&format!("start {child}")),
        "child executed: {lines:?}"
    );
    let rows = events(&out);
    assert!(event_pos(&rows, &root, "failed") < event_pos(&rows, &child, "blocked"));
    let summary: Value = serde_json::from_str(&stdout_str(&out)).expect("summary JSON");
    assert_eq!(summary.get("blocked").and_then(Value::as_u64), Some(1));
}

#[test]
fn sequential_parent_success() {
    let repo = TempRepo::new("wave-auth");
    repo.write_mock_primary(MOCK);
    let root = add(&repo, "root", "sequential", &[]);
    let child = add(&repo, "child", "parallel", &["--depends-on", &root]);

    let out = run_workers(&repo, "mixed", "1");
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    let lines = order(&repo);
    assert!(pos(&lines, &format!("end {root}")) < pos(&lines, &format!("start {child}")));
    let summary: Value = serde_json::from_str(&stdout_str(&out)).expect("summary JSON");
    assert_eq!(summary.get("complete").and_then(Value::as_u64), Some(2));
    assert_eq!(summary.get("blocked").and_then(Value::as_u64), Some(0));
}

#[test]
fn failed_parent_block() {
    let repo = TempRepo::new("wave-auth");
    repo.write_mock_primary(MOCK);
    let root = add(&repo, "root", "sequential", &[]);
    let child = add(&repo, "child", "parallel", &["--depends-on", &root]);
    fs::write(repo.root.join(".cx/fail-root"), b"synthetic failure").expect("fail marker");

    let out = run(&repo, "mixed");
    assert_eq!(
        out.status.code(),
        Some(1),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let lines = order(&repo);
    assert!(
        !lines.contains(&format!("start {child}")),
        "child executed: {lines:?}"
    );
    let rows = events(&out);
    assert!(event_pos(&rows, &root, "failed") < event_pos(&rows, &child, "blocked"));
    let summary: Value = serde_json::from_str(&stdout_str(&out)).expect("summary JSON");
    assert_eq!(summary.get("blocked").and_then(Value::as_u64), Some(1));
    let tasks: Value = serde_json::from_str(&fs::read_to_string(repo.tasks_file()).expect("tasks"))
        .expect("task projection");
    assert!(
        tasks
            .as_array()
            .expect("task list")
            .iter()
            .all(|task| task["status"] == "failed")
    );
}
