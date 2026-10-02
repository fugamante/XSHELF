mod common;

use common::*;
use serde_json::Value;
use std::sync::{Arc, Barrier};
use std::thread;

fn legacy_task(objective: &str) -> Value {
    serde_json::json!({
        "id": "task_001",
        "parent_id": null,
        "role": "implementer",
        "objective": objective,
        "context_ref": "",
        "backend": "auto",
        "model": null,
        "profile": "balanced",
        "converge": "none",
        "replicas": 1,
        "max_concurrency": null,
        "run_mode": "sequential",
        "depends_on": [],
        "resource_keys": [],
        "max_retries": null,
        "timeout_secs": null,
        "status": "pending",
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z"
    })
}

#[test]
fn concurrent_adds_safe() {
    const WRITERS: usize = 16;
    let repo = Arc::new(TempRepo::new("cxrs-task-ledger-add"));
    let gate = Arc::new(Barrier::new(WRITERS));
    let mut joins = Vec::new();
    for index in 0..WRITERS {
        let repo = Arc::clone(&repo);
        let gate = Arc::clone(&gate);
        joins.push(thread::spawn(move || {
            gate.wait();
            repo.run(&[
                "task",
                "add",
                &format!("concurrent task {index}"),
                "--role",
                "implementer",
            ])
        }));
    }
    let outputs: Vec<_> = joins
        .into_iter()
        .map(|join| join.join().expect("task add writer panicked"))
        .collect();
    for output in &outputs {
        assert!(
            output.status.success(),
            "stdout={} stderr={}",
            stdout_str(output),
            stderr_str(output)
        );
    }

    let tasks = read_json(&repo.tasks_file());
    let rows = tasks.as_array().expect("tasks array");
    assert_eq!(rows.len(), WRITERS);
    let ids: std::collections::HashSet<&str> = rows
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .collect();
    assert_eq!(ids.len(), WRITERS);
    assert!(repo.root.join(".cx").join("task_ledger").is_dir());
}

#[test]
fn concurrent_status_safe() {
    let repo = Arc::new(TempRepo::new("cxrs-task-ledger-status"));
    let left = repo.run(&["task", "add", "left status"]);
    let right = repo.run(&["task", "add", "right status"]);
    assert!(left.status.success(), "stderr={}", stderr_str(&left));
    assert!(right.status.success(), "stderr={}", stderr_str(&right));
    let left_id = stdout_str(&left).trim().to_string();
    let right_id = stdout_str(&right).trim().to_string();
    let gate = Arc::new(Barrier::new(2));

    let left_repo = Arc::clone(&repo);
    let left_gate = Arc::clone(&gate);
    let left_join = thread::spawn(move || {
        left_gate.wait();
        left_repo.run(&["task", "complete", &left_id])
    });
    let right_repo = Arc::clone(&repo);
    let right_gate = Arc::clone(&gate);
    let right_join = thread::spawn(move || {
        right_gate.wait();
        right_repo.run(&["task", "fail", &right_id])
    });
    let left_out = left_join.join().expect("left status writer panicked");
    let right_out = right_join.join().expect("right status writer panicked");
    assert!(
        left_out.status.success(),
        "stderr={}",
        stderr_str(&left_out)
    );
    assert!(
        right_out.status.success(),
        "stderr={}",
        stderr_str(&right_out)
    );

    let tasks = read_json(&repo.tasks_file());
    let statuses: std::collections::HashMap<&str, &str> = tasks
        .as_array()
        .expect("tasks array")
        .iter()
        .filter_map(|row| {
            Some((
                row.get("objective")?.as_str()?,
                row.get("status")?.as_str()?,
            ))
        })
        .collect();
    assert_eq!(statuses.get("left status"), Some(&"complete"));
    assert_eq!(statuses.get("right status"), Some(&"failed"));
}

#[test]
fn empty_read_clean() {
    let repo = TempRepo::new("cxrs-task-ledger-read");
    let list = repo.run(&["task", "list"]);
    assert!(list.status.success(), "stderr={}", stderr_str(&list));
    assert!(!repo.root.join(".cx").join("tasks.lock").exists());
    assert!(!repo.root.join(".cx").join("task_ledger").exists());
}

#[test]
fn lock_crash_safe() {
    let repo = TempRepo::new("cxrs-task-ledger-lock-crash");
    let existing = serde_json::json!([legacy_task("survives lock-only crash")]);
    std::fs::write(
        repo.tasks_file(),
        serde_json::to_vec_pretty(&existing).expect("encode existing tasks"),
    )
    .expect("write existing task array");
    std::fs::write(repo.root.join(".cx").join("tasks.lock"), b"")
        .expect("simulate crash after lock creation");

    let list = repo.run(&["task", "list", "--json"]);
    assert!(list.status.success(), "stderr={}", stderr_str(&list));
    let payload: Value = serde_json::from_str(&stdout_str(&list)).expect("valid list json");
    assert_eq!(payload.get("count").and_then(Value::as_u64), Some(1));
    assert!(!repo.root.join(".cx").join("task_ledger").exists());
}

#[test]
fn legacy_bootstrap_safe() {
    let repo = TempRepo::new("cxrs-task-ledger-migrate");
    let existing = serde_json::json!([legacy_task("existing task")]);
    std::fs::write(
        repo.tasks_file(),
        serde_json::to_vec_pretty(&existing).expect("encode existing tasks"),
    )
    .expect("write existing task array");

    let add = repo.run(&["task", "add", "post-migration task"]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let tasks = read_json(&repo.tasks_file());
    assert_eq!(tasks.as_array().expect("tasks array").len(), 2);
    let ledger = repo.root.join(".cx").join("task_ledger");
    assert_eq!(
        std::fs::read_dir(ledger)
            .expect("read task ledger")
            .filter_map(Result::ok)
            .filter(
                |entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json")
            )
            .count(),
        2
    );
}

#[test]
fn mutation_rejects_drift() {
    let repo = TempRepo::new("cxrs-task-ledger-drift");
    let first = repo.run(&["task", "add", "first task"]);
    assert!(first.status.success(), "stderr={}", stderr_str(&first));
    let ledger = repo.root.join(".cx").join("task_ledger");
    let before = std::fs::read_dir(&ledger).unwrap().count();
    std::fs::write(repo.tasks_file(), b"[]\n").expect("simulate uncoordinated writer");

    let second = repo.run(&["task", "add", "must not overwrite drift"]);
    assert!(!second.status.success());
    assert!(stderr_str(&second).contains("changed outside the authoritative task ledger"));
    assert_eq!(std::fs::read_dir(&ledger).unwrap().count(), before);
    assert_eq!(std::fs::read(repo.tasks_file()).unwrap(), b"[]\n");
}

#[test]
fn drift_read_inspection() {
    let repo = TempRepo::new("cxrs-task-ledger-drift-read");
    let first = repo.run(&["task", "add", "authoritative task"]);
    assert!(first.status.success(), "stderr={}", stderr_str(&first));
    let drift = serde_json::json!([legacy_task("uncoordinated task")]);
    let drift_bytes = serde_json::to_vec_pretty(&drift).expect("encode drift");
    std::fs::write(repo.tasks_file(), &drift_bytes).expect("simulate uncoordinated writer");

    let list = repo.run(&["task", "list", "--json"]);
    assert!(list.status.success(), "stderr={}", stderr_str(&list));
    assert!(stderr_str(&list).contains("derived projection ignored"));
    let payload: Value = serde_json::from_str(&stdout_str(&list)).expect("valid list json");
    assert_eq!(payload.get("count").and_then(Value::as_u64), Some(1));
    let rows = payload
        .get("tasks")
        .and_then(Value::as_array)
        .expect("task rows");
    assert_eq!(
        rows[0].get("objective").and_then(Value::as_str),
        Some("authoritative task")
    );
    assert_eq!(std::fs::read(repo.tasks_file()).unwrap(), drift_bytes);

    let second = repo.run(&["task", "add", "must not overwrite drift"]);
    assert!(!second.status.success());
    assert!(stderr_str(&second).contains("changed outside the authoritative task ledger"));
    assert_eq!(std::fs::read(repo.tasks_file()).unwrap(), drift_bytes);
}

#[test]
fn ledger_readonly() {
    let repo = TempRepo::new("cxrs-task-ledger-readonly");
    let add = repo.run(&["task", "add", "read-only projection"]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let cx = repo.root.join(".cx");
    let lock = cx.join("tasks.lock");
    let snapshot = repo.tasks_file();
    let metadata = cx.join("tasks.snapshot.json");
    let ledger = cx.join("task_ledger");
    let entry = std::fs::read_dir(&ledger)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let paths = [&lock, &snapshot, &metadata, &entry];
    let before: Vec<Vec<u8>> = paths
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect();
    let modified_before: Vec<_> = paths
        .iter()
        .map(|path| std::fs::metadata(path).unwrap().modified().unwrap())
        .collect();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&lock).unwrap().permissions();
        permissions.set_mode(0o444);
        std::fs::set_permissions(&lock, permissions).unwrap();
    }

    let list = repo.run(&["task", "list", "--json"]);
    assert!(list.status.success(), "stderr={}", stderr_str(&list));
    for ((path, expected), modified) in paths.iter().zip(before).zip(modified_before) {
        assert_eq!(std::fs::read(path).unwrap(), expected);
        assert_eq!(
            std::fs::metadata(path).unwrap().modified().unwrap(),
            modified
        );
    }
}

#[test]
fn stale_projection_readonly() {
    let repo = TempRepo::new("cxrs-task-ledger-stale-read");
    let add = repo.run(&["task", "add", "ledger survives projection loss"]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let metadata = repo.root.join(".cx").join("tasks.snapshot.json");
    std::fs::remove_file(repo.tasks_file()).unwrap();
    std::fs::remove_file(&metadata).unwrap();

    let list = repo.run(&["task", "list", "--json"]);
    assert!(list.status.success(), "stderr={}", stderr_str(&list));
    let payload: Value = serde_json::from_str(&stdout_str(&list)).unwrap();
    assert_eq!(payload.get("count").and_then(Value::as_u64), Some(1));
    assert!(!repo.tasks_file().exists());
    assert!(!metadata.exists());
}

#[test]
fn missing_lock_fails() {
    let repo = TempRepo::new("cxrs-task-ledger-missing-lock");
    let add = repo.run(&["task", "add", "lock is required"]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    std::fs::remove_file(repo.root.join(".cx").join("tasks.lock")).unwrap();
    let list = repo.run(&["task", "list"]);
    assert!(!list.status.success());
    assert!(stderr_str(&list).contains("task ledger exists without"));
}

#[test]
fn lost_ledger_rejected() {
    let repo = TempRepo::new("cxrs-task-ledger-lost");
    let add = repo.run(&["task", "add", "retain committed history"]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));
    let ledger = repo.root.join(".cx/task_ledger");
    for entry in std::fs::read_dir(&ledger).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    let snapshot = std::fs::read(repo.tasks_file()).unwrap();
    let metadata = repo.root.join(".cx/tasks.snapshot.json");
    let marker = std::fs::read(&metadata).unwrap();
    for args in [
        vec!["task", "list", "--json"],
        vec!["task", "add", "must not bootstrap"],
    ] {
        let out = repo.run(&args);
        assert_eq!(
            out.status.code(),
            Some(1),
            "stdout={} stderr={}",
            stdout_str(&out),
            stderr_str(&out)
        );
        assert!(stderr_str(&out).contains("without authoritative ledger"));
        assert_eq!(std::fs::read(repo.tasks_file()).unwrap(), snapshot);
        assert_eq!(std::fs::read(&metadata).unwrap(), marker);
        assert_eq!(std::fs::read_dir(&ledger).unwrap().count(), 0);
    }
}
