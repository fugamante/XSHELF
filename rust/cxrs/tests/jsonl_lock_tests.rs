mod common;

use common::{TempRepo, stderr_str, stdout_str};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn schema_failure(repo: &TempRepo) -> std::process::Output {
    repo.run_with_env(
        &["next", "echo", "synthetic"],
        &[
            ("CX_PROVIDER_ADAPTER", "mock"),
            ("CX_MOCK_PLAIN_RESPONSE", "not-json"),
        ],
    )
}

fn jsonl_rows(path: &Path) -> Vec<Value> {
    let data = fs::read_to_string(path).unwrap();
    assert!(data.ends_with('\n'), "unterminated row: {}", path.display());
    assert!(data.lines().all(|line| !line.is_empty()));
    data.lines()
        .map(|line| serde_json::from_str(line).expect("complete JSONL row"))
        .collect()
}

#[test]
fn concurrent_cli_records() {
    let repo = TempRepo::new("cxrs-log-concurrent");
    let seed = schema_failure(&repo);
    assert!(!seed.status.success(), "{}", stdout_str(&seed));

    let alias = repo.home.join("run-log-alias.jsonl");
    symlink(repo.runs_log(), &alias).unwrap();
    let mut children = Vec::new();
    let markers: Vec<String> = (0..8)
        .map(|id| format!("writer-{id}-{}", "x".repeat(16 * 1024)))
        .collect();
    for (id, marker) in markers.iter().enumerate() {
        let capture = id % 2 == 0;
        let args = if capture {
            &["capture", "/bin/echo", "synthetic"][..]
        } else {
            &["next", "echo", "synthetic"][..]
        };
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cxrs"));
        cmd.args(args)
            .current_dir(&repo.root)
            .env("HOME", &repo.home)
            .env("CX_EXECUTION_LANE_DETAIL", marker)
            .env_remove("CX_LOG_FILE")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if capture && id % 4 == 0 {
            cmd.env("CX_LOG_FILE", &alias);
        }
        if !capture {
            cmd.env("CX_PROVIDER_ADAPTER", "mock")
                .env("CX_MOCK_PLAIN_RESPONSE", "not-json");
        }
        children.push((cmd.spawn().unwrap(), capture));
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    while children
        .iter_mut()
        .any(|(child, _)| child.try_wait().unwrap().is_none())
    {
        if Instant::now() >= deadline {
            for (child, _) in &mut children {
                let _ = child.kill();
                let _ = child.wait();
            }
            panic!("concurrent CLI writers did not exit");
        }
        thread::sleep(Duration::from_millis(20));
    }
    for (child, expected_success) in children {
        let out = child.wait_with_output().unwrap();
        assert_eq!(
            out.status.success(),
            expected_success,
            "stdout={} stderr={}",
            stdout_str(&out),
            stderr_str(&out)
        );
    }

    let runs = jsonl_rows(&repo.runs_log());
    assert_eq!(runs.len(), 14, "run rows");
    for (id, marker) in markers.iter().enumerate() {
        let count = runs
            .iter()
            .filter(|row| row["execution_lane_detail"].as_str() == Some(marker))
            .count();
        assert_eq!(count, if id % 2 == 0 { 1 } else { 2 }, "writer {id}");
    }
    let failures = jsonl_rows(&repo.schema_fail_log());
    assert_eq!(failures.len(), 5, "schema failure rows");
    let ids: HashSet<&str> = failures
        .iter()
        .map(|row| row["quarantine_id"].as_str().expect("quarantine id"))
        .collect();
    assert_eq!(ids.len(), 5, "distinct quarantine records");
    for id in ids {
        assert!(repo.quarantine_file(id).is_file());
        assert_eq!(
            runs.iter()
                .filter(|row| row["quarantine_id"].as_str() == Some(id))
                .count(),
            2,
            "run rows for quarantine {id}"
        );
    }
}
