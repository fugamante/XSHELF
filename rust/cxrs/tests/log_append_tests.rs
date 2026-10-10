mod common;

use common::{TempRepo, stderr_str, stdout_str};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::{FileTypeExt, symlink};
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

#[test]
fn schema_symlinks() {
    for parent_link in [false, true] {
        let repo = TempRepo::new("cxrs-log-boundary");
        let victim = repo.home.join("victim.jsonl");
        fs::write(&victim, b"sentinel\n").unwrap();
        let path = repo.schema_fail_log();
        if parent_link {
            let target = repo.home.join("outside");
            fs::create_dir_all(&target).unwrap();
            fs::write(target.join("schema_failures.jsonl"), b"sentinel\n").unwrap();
            symlink(&target, path.parent().unwrap()).unwrap();
            let out = schema_failure(&repo);
            assert!(!out.status.success());
            assert_eq!(
                fs::read(target.join("schema_failures.jsonl")).unwrap(),
                b"sentinel\n"
            );
        } else {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            symlink(&victim, &path).unwrap();
            let out = schema_failure(&repo);
            assert!(!out.status.success());
            assert_eq!(fs::read(&victim).unwrap(), b"sentinel\n");
        }
    }
}

#[test]
fn schema_fifo() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let path = repo.schema_fail_log();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let out = File::create(repo.home.join("schema.out")).unwrap();
    let err = File::create(repo.home.join("schema.err")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_cxrs"))
        .args(["next", "echo", "synthetic"])
        .current_dir(&repo.root)
        .env("HOME", &repo.home)
        .env("CX_PROVIDER_ADAPTER", "mock")
        .env("CX_MOCK_PLAIN_RESPONSE", "not-json")
        .env_remove("CX_LOG_FILE")
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .unwrap();
    assert!(
        wait_exit(&mut child).is_some(),
        "schema append blocked on FIFO"
    );
    assert!(
        fs::read_dir(repo.quarantine_dir())
            .unwrap()
            .next()
            .is_some()
    );
    assert!(path.symlink_metadata().unwrap().file_type().is_fifo());
}

#[test]
fn run_symlink() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let victim = repo.home.join("outside.jsonl");
    fs::write(&victim, b"sentinel\n").unwrap();
    let path = repo.runs_log();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    symlink(&victim, &path).unwrap();
    let _ = repo.run_with_env(
        &["next", "echo", "synthetic"],
        &[
            ("CX_PROVIDER_ADAPTER", "mock"),
            (
                "CX_MOCK_PLAIN_RESPONSE",
                "{\"commands\":[\"echo synthetic\"]}",
            ),
        ],
    );
    assert_eq!(fs::read(&victim).unwrap(), b"sentinel\n");
}

#[test]
fn event_symlinks() {
    for parent_link in [false, true] {
        let repo = TempRepo::new("cxrs-log-boundary");
        repo.write_mock_primary("#!/bin/sh\ncat >/dev/null\nprintf 'synthetic\\n'\n");
        let add = repo.run(&["task", "add", "cxo echo synthetic", "--backend", "primary"]);
        assert!(add.status.success(), "{}", stderr_str(&add));
        let victim = repo.home.join("victim.jsonl");
        fs::write(&victim, b"sentinel\n").unwrap();
        let path = repo.task_events_log();
        if parent_link {
            let target = repo.home.join("outside");
            fs::create_dir_all(&target).unwrap();
            fs::write(target.join("task_events.jsonl"), b"sentinel\n").unwrap();
            fs::create_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
            symlink(&target, path.parent().unwrap()).unwrap();
        } else {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            symlink(&victim, &path).unwrap();
        }

        let out = repo.run_with_env(
            &[
                "task",
                "run-all",
                "--events-jsonl",
                "--json",
                "--backend-pool",
                "primary",
            ],
            &[("CX_TASK_TRUST_COMMANDS", "0")],
        );
        assert!(
            stderr_str(&out).contains("failed to append event"),
            "stdout={} stderr={}",
            stdout_str(&out),
            stderr_str(&out)
        );
        assert!(stderr_str(&out).contains("task-events.v1"));
        assert_eq!(fs::read(&victim).unwrap(), b"sentinel\n");
        if parent_link {
            assert_eq!(
                fs::read(repo.home.join("outside/task_events.jsonl")).unwrap(),
                b"sentinel\n"
            );
        }
    }
}

#[test]
fn operator_alias() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let victim = repo.home.join("operator-log.jsonl");
    let alias = repo.home.join("operator-alias.jsonl");
    fs::write(&victim, b"sentinel\n").unwrap();
    symlink(&victim, &alias).unwrap();
    let out = repo.run_with_env(
        &["next", "echo", "synthetic"],
        &[
            ("CX_PROVIDER_ADAPTER", "mock"),
            (
                "CX_MOCK_PLAIN_RESPONSE",
                "{\"commands\":[\"echo synthetic\"]}",
            ),
            ("CX_LOG_FILE", alias.to_str().unwrap()),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let data = fs::read_to_string(&victim).unwrap();
    assert!(data.starts_with("sentinel\n"));
    assert!(data.contains("cxrs_next"));
}

#[test]
fn event_fifo() {
    let repo = TempRepo::new("cxrs-log-boundary");
    repo.write_mock_primary("#!/bin/sh\ncat >/dev/null\nprintf 'synthetic\\n'\n");
    let add = repo.run(&["task", "add", "cxo echo synthetic", "--backend", "primary"]);
    assert!(add.status.success(), "{}", stderr_str(&add));
    let path = repo.task_events_log();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let fifo = Command::new("mkfifo").arg(&path).status().unwrap();
    assert!(fifo.success());
    let out = File::create(repo.home.join("task.out")).unwrap();
    let err = File::create(repo.home.join("task.err")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_cxrs"))
        .args([
            "task",
            "run-all",
            "--events-jsonl",
            "--json",
            "--backend-pool",
            "primary",
        ])
        .current_dir(&repo.root)
        .env("HOME", &repo.home)
        .env(
            "PATH",
            format!(
                "{}:{}",
                repo.mock_bin.display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env("CX_TASK_TRUST_COMMANDS", "0")
        .env_remove("CX_LOG_FILE")
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .unwrap();
    assert!(
        wait_exit(&mut child).is_some(),
        "task append blocked on FIFO"
    );
    assert!(
        fs::read_to_string(repo.home.join("task.err"))
            .unwrap()
            .contains("failed to append event")
    );
}

#[test]
fn event_override() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let victim = repo.home.join("outside.jsonl");
    fs::write(&victim, b"{\"event\":\"outside\"}\n").unwrap();
    let path = repo.task_events_log();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    symlink(&victim, &path).unwrap();
    let resolved = repo
        .root
        .canonicalize()
        .unwrap()
        .join(".codex/cxlogs/task_events.jsonl");
    let out = repo.run_with_env(
        &["task", "events", "--json"],
        &[("CX_LOG_FILE", resolved.to_str().unwrap())],
    );
    assert!(!out.status.success(), "{}", stdout_str(&out));
    assert!(!stdout_str(&out).contains("outside"));
}

fn follow_process(repo: &TempRepo, out: &File, err: &File) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_cxrs"))
        .args(["task", "events", "--jsonl", "--follow", "--limit", "1"])
        .current_dir(&repo.root)
        .env("HOME", &repo.home)
        .env_remove("CX_LOG_FILE")
        .stdout(Stdio::from(out.try_clone().unwrap()))
        .stderr(Stdio::from(err.try_clone().unwrap()))
        .spawn()
        .unwrap()
}

fn wait_exit(child: &mut std::process::Child) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        thread::sleep(Duration::from_millis(50));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    None
}

fn wait_event(repo: &TempRepo, child: &mut std::process::Child, expected: &str) {
    let path = repo.home.join("follow.out");
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let output = fs::read_to_string(&path).unwrap();
        if output.contains(expected) {
            return;
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("follower exited {status} before {expected}: {output}");
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("follower did not emit {expected}: {output}");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn follow_oversized() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let path = repo.task_events_log();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{\"event\":\"seed\"}\n").unwrap();
    let out = File::create(repo.home.join("follow.out")).unwrap();
    let err = File::create(repo.home.join("follow.err")).unwrap();
    let mut child = follow_process(&repo, &out, &err);
    wait_event(&repo, &mut child, "\"event\":\"seed\"");
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(b"{\"event\":\"").unwrap();
    file.write_all(&vec![b'x'; 1024 * 1024 + 1]).unwrap();
    file.write_all(b"\"}\n").unwrap();
    let status = wait_exit(&mut child);
    assert_eq!(status.and_then(|s| s.code()), Some(1));
    assert!(
        fs::read_to_string(repo.home.join("follow.err"))
            .unwrap()
            .contains("exceeds")
    );
}

#[test]
fn follow_append() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let path = repo.task_events_log();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{\"event\":\"seed\"}\n").unwrap();
    let out = File::create(repo.home.join("follow.out")).unwrap();
    let err = File::create(repo.home.join("follow.err")).unwrap();
    let mut child = follow_process(&repo, &out, &err);
    wait_event(&repo, &mut child, "\"event\":\"seed\"");
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(b"{\"event\":\"later\"}\n").unwrap();
    wait_event(&repo, &mut child, "\"event\":\"later\"");
    child.kill().unwrap();
    child.wait().unwrap();
    let output = fs::read_to_string(repo.home.join("follow.out")).unwrap();
    assert!(output.contains("\"event\":\"seed\""), "{output}");
    assert!(output.contains("\"event\":\"later\""), "{output}");
}

#[test]
fn follow_split() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let path = repo.task_events_log();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{\"event\":\"seed\"}\n").unwrap();
    let out = File::create(repo.home.join("follow.out")).unwrap();
    let err = File::create(repo.home.join("follow.err")).unwrap();
    let mut child = follow_process(&repo, &out, &err);
    wait_event(&repo, &mut child, "\"event\":\"seed\"");
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(b"{\"event\":\"split").unwrap();
    thread::sleep(Duration::from_millis(650));
    file.write_all(b"-row\"}\n").unwrap();
    wait_event(&repo, &mut child, "split-row");
    child.kill().unwrap();
    child.wait().unwrap();
    let output = fs::read_to_string(repo.home.join("follow.out")).unwrap();
    assert_eq!(output.matches("split-row").count(), 1, "{output}");
}

#[test]
fn follow_start_partial() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let path = repo.task_events_log();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{\"event\":\"seed\"}\n{\"event\":\"split").unwrap();
    let out = File::create(repo.home.join("follow.out")).unwrap();
    let err = File::create(repo.home.join("follow.err")).unwrap();
    let mut child = follow_process(&repo, &out, &err);
    wait_event(&repo, &mut child, "\"event\":\"seed\"");
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"-row\"}\n")
        .unwrap();
    wait_event(&repo, &mut child, "split-row");
    child.kill().unwrap();
    child.wait().unwrap();
    let output = fs::read_to_string(repo.home.join("follow.out")).unwrap();
    assert_eq!(output.matches("split-row").count(), 1, "{output}");
}

#[test]
fn follow_symlink() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let path = repo.task_events_log();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{\"event\":\"seed\"}\n").unwrap();
    let victim = repo.home.join("outside.jsonl");
    fs::write(&victim, b"{\"event\":\"outside\"}\n").unwrap();
    let out = File::create(repo.home.join("follow.out")).unwrap();
    let err = File::create(repo.home.join("follow.err")).unwrap();
    let mut child = follow_process(&repo, &out, &err);
    wait_event(&repo, &mut child, "\"event\":\"seed\"");
    fs::rename(&path, repo.home.join("old-events.jsonl")).unwrap();
    symlink(&victim, &path).unwrap();
    let status = wait_exit(&mut child);
    assert_eq!(status.and_then(|s| s.code()), Some(1));
    let output = fs::read_to_string(repo.home.join("follow.out")).unwrap();
    assert!(!output.contains("outside"), "{output}");
}

#[test]
fn follow_rotation() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let path = repo.task_events_log();
    let out = File::create(repo.home.join("follow.out")).unwrap();
    let err = File::create(repo.home.join("follow.err")).unwrap();
    let mut child = follow_process(&repo, &out, &err);
    thread::sleep(Duration::from_millis(650));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{\"event\":\"created\"}\n").unwrap();
    wait_event(&repo, &mut child, "\"event\":\"created\"");
    fs::rename(&path, repo.home.join("old-events.jsonl")).unwrap();
    fs::write(&path, b"{\"event\":\"rotated\"}\n").unwrap();
    wait_event(&repo, &mut child, "\"event\":\"rotated\"");
    child.kill().unwrap();
    child.wait().unwrap();
    let output = fs::read_to_string(repo.home.join("follow.out")).unwrap();
    assert_eq!(output.matches("created").count(), 1, "{output}");
    assert_eq!(output.matches("rotated").count(), 1, "{output}");
}

#[test]
fn follow_burst() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let path = repo.task_events_log();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{\"event\":\"seed\"}\n").unwrap();
    let out = File::create(repo.home.join("follow.out")).unwrap();
    let err = File::create(repo.home.join("follow.err")).unwrap();
    let mut child = follow_process(&repo, &out, &err);
    wait_event(&repo, &mut child, "\"event\":\"seed\"");
    let mut writer = BufWriter::new(OpenOptions::new().append(true).open(&path).unwrap());
    for id in 0..1100 {
        writeln!(writer, "{{\"event\":\"burst\",\"id\":{id}}}").unwrap();
    }
    writer.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let output = loop {
        let output = fs::read_to_string(repo.home.join("follow.out")).unwrap();
        if output.matches("\"event\":\"burst\"").count() == 1100 || Instant::now() >= deadline {
            break output;
        }
        thread::sleep(Duration::from_millis(50));
    };
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(
        output.matches("\"event\":\"burst\"").count(),
        1100,
        "{output}"
    );
    assert!(output.contains("\"id\":1099"));
}

#[test]
fn follow_touch() {
    let repo = TempRepo::new("cxrs-log-boundary");
    let path = repo.task_events_log();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"{\"event\":\"seed\"}\n").unwrap();
    let out = File::create(repo.home.join("follow.out")).unwrap();
    let err = File::create(repo.home.join("follow.err")).unwrap();
    let mut child = follow_process(&repo, &out, &err);
    wait_event(&repo, &mut child, "\"event\":\"seed\"");
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + Duration::from_secs(5))
        .unwrap();
    // Leave the length unchanged across two follow polls before the liveness row.
    thread::sleep(Duration::from_millis(1100));
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{\"event\":\"after-touch\"}\n")
        .unwrap();
    wait_event(&repo, &mut child, "after-touch");
    child.kill().unwrap();
    child.wait().unwrap();
    let output = fs::read_to_string(repo.home.join("follow.out")).unwrap();
    assert_eq!(output.matches("\"event\":\"seed\"").count(), 1, "{output}");
    assert_eq!(output.matches("after-touch").count(), 1, "{output}");
}
