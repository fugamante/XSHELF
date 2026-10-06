#![cfg(unix)]
mod common;
use common::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::{Command, Output};

const LEGACY: &[u8] = b"\n{\"ts\":\"2026-01-01\",\"tool\":\"test\"}\ninvalid\n";

fn migrate(repo: &TempRepo, args: &[&str], source: Option<&std::path::Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cxrs"));
    command.current_dir(&repo.root).env("HOME", &repo.home);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("CX_") {
            command.env_remove(name);
        }
    }
    for name in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"] {
        command.env_remove(name);
    }
    if let Some(path) = source {
        command.env("CX_LOG_FILE", path);
    }
    command
        .args(["logs", "migrate"])
        .args(args)
        .output()
        .unwrap()
}

fn seed(repo: &TempRepo) {
    let log = repo.runs_log();
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    fs::write(log, LEGACY).unwrap();
}

#[test]
fn migrate_output_contract() {
    let repo = TempRepo::new("migrate");
    seed(&repo);
    let outside = tempfile::tempdir().unwrap();
    let output = outside.path().join("nested/output.jsonl");
    let result = migrate(&repo, &["--out", output.to_str().unwrap()], None);
    assert!(result.status.success(), "{}", stderr_str(&result));
    let text = stdout_str(&result);
    for label in [
        "in:",
        "out:",
        "entries_in: 2",
        "entries_out: 1",
        "invalid_json_skipped: 1",
        "legacy_normalized: 1",
        "modern_normalized: 0",
        "status: wrote",
    ] {
        assert!(text.contains(label), "{text}");
    }
    assert_eq!(fs::read(repo.runs_log()).unwrap(), LEGACY);
    assert_eq!(
        fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::write(&output, b"prior output").unwrap();
    let result = migrate(&repo, &["--out", output.to_str().unwrap()], None);
    assert!(result.status.success(), "{}", stderr_str(&result));
    let row: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(row["tool"], "test");
    assert_eq!(row["execution_mode"], "legacy");
}

#[test]
fn migrate_unique_backups() {
    let repo = TempRepo::new("migrate");
    seed(&repo);
    let source = repo.runs_log();
    let relative = ".cx/cxlogs/../cxlogs/runs.jsonl";
    let denied = migrate(&repo, &["--out", relative], None);
    assert!(!denied.status.success());
    assert_eq!(fs::read(&source).unwrap(), LEGACY);
    let first = migrate(&repo, &["--out", relative, "--in-place"], None);
    assert!(first.status.success(), "{}", stderr_str(&first));
    assert!(stdout_str(&first).contains("status: replaced"));
    let first_backup = stdout_str(&first)
        .lines()
        .find_map(|line| line.strip_prefix("backup: "))
        .unwrap()
        .to_owned();
    assert_eq!(fs::read(&first_backup).unwrap(), LEGACY);
    let normalized = fs::read(&source).unwrap();
    let second = migrate(&repo, &["--in-place"], None);
    assert!(second.status.success(), "{}", stderr_str(&second));
    let second_backup = stdout_str(&second)
        .lines()
        .find_map(|line| line.strip_prefix("backup: "))
        .unwrap()
        .to_owned();
    assert_ne!(first_backup, second_backup);
    assert_eq!(fs::read(second_backup).unwrap(), normalized);
}

#[test]
fn migrate_repo_symlinks() {
    let repo = TempRepo::new("migrate");
    seed(&repo);
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(outside.path().join("cxlogs")).unwrap();
    let victim = outside.path().join("cxlogs/runs.jsonl");
    fs::write(&victim, LEGACY).unwrap();
    fs::rename(repo.root.join(".cx"), repo.root.join("saved")).unwrap();
    symlink(outside.path(), repo.root.join(".cx")).unwrap();
    let result = migrate(&repo, &["--in-place"], None);
    assert!(!result.status.success());
    assert_eq!(fs::read(&victim).unwrap(), LEGACY);
    assert_eq!(
        fs::read_dir(outside.path().join("cxlogs")).unwrap().count(),
        1
    );
}

#[test]
fn migrate_temp_symlink() {
    let repo = TempRepo::new("migrate");
    seed(&repo);
    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("victim");
    fs::write(&victim, b"sentinel").unwrap();
    let log_parent = repo.runs_log().parent().unwrap().to_owned();
    symlink(&victim, log_parent.join("runs.migrated.jsonl.tmp")).unwrap();
    let result = migrate(&repo, &[], None);
    assert!(result.status.success(), "{}", stderr_str(&result));
    assert_eq!(fs::read(&victim).unwrap(), b"sentinel");
    assert!(log_parent.join("runs.migrated.jsonl").is_file());
    let result = migrate(&repo, &["--out", "other.jsonl"], Some(&repo.runs_log()));
    assert!(result.status.success(), "{}", stderr_str(&result));
    assert!(repo.root.join("other.jsonl").is_file());
}

#[test]
fn migrate_home_symlinks() {
    let repo = TempRepo::new("migrate");
    seed(&repo);
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir(outside.path().join("cxlogs")).unwrap();
    let victim = outside.path().join("cxlogs/runs.jsonl");
    fs::write(&victim, LEGACY).unwrap();
    symlink(outside.path(), repo.home.join(".cx")).unwrap();
    let selected = repo.home.join(".cx/cxlogs/runs.jsonl");
    let result = migrate(&repo, &["--in-place"], Some(&selected));
    assert!(!result.status.success());
    symlink(outside.path(), repo.root.join("redirect")).unwrap();
    let result = migrate(&repo, &["--out", "redirect/../out.jsonl"], None);
    assert!(!result.status.success());
    assert_eq!(fs::read(&victim).unwrap(), LEGACY);
    assert_eq!(fs::read(repo.runs_log()).unwrap(), LEGACY);
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
}

#[test]
fn migrate_fifo_rejected() {
    use std::time::Duration;
    use wait_timeout::ChildExt;
    let repo = TempRepo::new("migrate");
    let fifo = repo.root.join("fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_cxrs"));
    command.current_dir(&repo.root).env("HOME", &repo.home);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("CX_") {
            command.env_remove(name);
        }
    }
    for name in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"] {
        command.env_remove(name);
    }
    let mut child = command
        .env("CX_LOG_FILE", &fifo)
        .args(["logs", "migrate"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    match child.wait_timeout(Duration::from_secs(2)).unwrap() {
        Some(status) => assert!(!status.success()),
        None => {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("migration blocked on nonregular source");
        }
    }
    assert!(fs::symlink_metadata(fifo).is_ok());
}
