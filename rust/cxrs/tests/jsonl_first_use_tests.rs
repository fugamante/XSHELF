mod common;

use common::{TempRepo, stderr_str};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn first_use_append() {
    let repo = TempRepo::new("cxrs-log-first-use");
    let log = repo.schema_fail_log();
    assert!(
        !log.parent().unwrap().exists(),
        "start without log directory"
    );

    let mut children = Vec::new();
    for _ in 0..64 {
        let child = Command::new(env!("CARGO_BIN_EXE_cxrs"))
            .args(["next", "echo", "synthetic"])
            .current_dir(&repo.root)
            .env("HOME", &repo.home)
            .env("CX_PROVIDER_ADAPTER", "mock")
            .env("CX_MOCK_PLAIN_RESPONSE", "not-json")
            .env_remove("CX_LOG_FILE")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        children.push(child);
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while children
        .iter_mut()
        .any(|child| child.try_wait().unwrap().is_none())
    {
        if Instant::now() >= deadline {
            for child in &mut children {
                let _ = child.kill();
                let _ = child.wait();
            }
            panic!("first-use writers did not exit");
        }
        thread::sleep(Duration::from_millis(20));
    }
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success(), "{}", stderr_str(&output));
        assert!(
            !stderr_str(&output).contains("failed opening"),
            "lost schema-failure observation: {}",
            stderr_str(&output)
        );
    }

    let reused = repo.run_with_env(
        &["next", "echo", "synthetic"],
        &[
            ("CX_PROVIDER_ADAPTER", "mock"),
            ("CX_MOCK_PLAIN_RESPONSE", "not-json"),
        ],
    );
    assert!(!reused.status.success());

    let rows: Vec<Value> = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 65, "every failure and reuse has one row");
    let ids: HashSet<&str> = rows
        .iter()
        .map(|row| row["quarantine_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 65);
    for id in ids {
        assert!(repo.quarantine_file(id).is_file());
    }
}
