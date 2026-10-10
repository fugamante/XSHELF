mod common;

use common::{TempRepo, stderr_str, stdout_str};
use serde_json::Value;
use std::fs::{self, File};
use std::io::{BufWriter, Seek, SeekFrom, Write};

fn write_log(repo: &TempRepo, rows: &[&str]) {
    let path = repo.runs_log();
    fs::create_dir_all(path.parent().expect("log parent")).expect("create log parent");
    fs::write(path, format!("{}\n", rows.join("\n"))).expect("write synthetic log");
}

fn row(id: &str, preview: &str) -> String {
    serde_json::json!({
        "execution_id": id,
        "timestamp": "2026-10-10T12:00:00Z",
        "ts": "2026-10-10T12:00:00Z",
        "command": "cxo",
        "tool": "cxo",
        "backend_used": "primary",
        "prompt_preview": preview,
        "duration_ms": 1,
    })
    .to_string()
}

#[test]
fn recent_order() {
    let repo = TempRepo::new("log-boundary");
    let old = row("old", "OLD");
    let new = row("new", "LATEST");
    write_log(&repo, &[&old, "not json", &new]);

    let stats = repo.run(&["logs", "stats", "2", "--json"]);
    assert!(stats.status.success(), "stderr={}", stderr_str(&stats));
    let payload: Value = serde_json::from_str(&stdout_str(&stats)).expect("stats JSON");
    assert_eq!(payload["window_runs"], 2);

    let trace = repo.run(&["trace", "2"]);
    assert!(trace.status.success(), "stderr={}", stderr_str(&trace));
    assert!(stdout_str(&trace).contains("prompt_preview: OLD"));
    let tail = repo.run(&["log-tail", "2"]);
    assert!(tail.status.success(), "stderr={}", stderr_str(&tail));
    assert!(stdout_str(&tail).contains("not json"));
    assert!(stdout_str(&tail).contains("LATEST"));
    assert!(!stdout_str(&tail).contains("OLD"));
    assert!(
        stdout_str(&tail).find("not json") < stdout_str(&tail).find("LATEST"),
        "tail rows must remain chronological"
    );
}

#[test]
fn task_diag_recent() {
    let repo = TempRepo::new("log-boundary");
    let added = repo.run(&["task", "add", "Synthetic inspection task"]);
    assert!(added.status.success(), "stderr={}", stderr_str(&added));
    let listed = repo.run(&["task", "list", "--json"]);
    assert!(listed.status.success(), "stderr={}", stderr_str(&listed));
    let list: Value = serde_json::from_str(&stdout_str(&listed)).expect("task list JSON");
    let task_id = list["tasks"][0]["id"].as_str().expect("task id");
    let path = repo.runs_log();
    fs::create_dir_all(path.parent().expect("log parent")).expect("create log parent");
    let mut writer = BufWriter::new(File::create(path).expect("create log"));
    for _ in 0..55_000 {
        writeln!(writer, "{}", row("old", "OLD")).expect("write old row");
    }
    let latest = serde_json::json!({
        "execution_id": "task-current",
        "timestamp": "2026-10-10T12:00:00Z",
        "task_id": task_id,
        "tool": "task",
        "backend_used": "primary",
        "duration_ms": 2
    });
    writeln!(writer, "{latest}").expect("write matching row");
    writer.flush().expect("flush log");

    let shown = repo.run(&["task", "show", task_id]);
    assert!(shown.status.success(), "stderr={}", stderr_str(&shown));
    let task: Value = serde_json::from_str(&stdout_str(&shown)).expect("task show JSON");
    assert_eq!(task["latest_run"]["execution_id"], "task-current");
    assert_eq!(task["latest_run_lookup"], "found");
    let diag = repo.run(&["diag", "--json", "--window", "1"]);
    assert!(diag.status.success(), "stderr={}", stderr_str(&diag));
    let payload: Value = serde_json::from_str(&stdout_str(&diag)).expect("diag JSON");
    assert_eq!(payload["last_run_id"], "task-current");
}

#[test]
fn sparse_recent_inspection() {
    let repo = TempRepo::new("log-boundary");
    let added = repo.run(&["task", "add", "Synthetic large-log task"]);
    assert!(added.status.success(), "stderr={}", stderr_str(&added));
    let listed = repo.run(&["task", "list", "--json"]);
    let list: Value = serde_json::from_str(&stdout_str(&listed)).expect("task list JSON");
    let task_id = list["tasks"][0]["id"].as_str().expect("task id");
    let empty = repo.run(&["task", "show", task_id]);
    assert!(empty.status.success());
    let empty_task: Value = serde_json::from_str(&stdout_str(&empty)).expect("empty task JSON");
    assert_eq!(empty_task["latest_run_lookup"], "not_found");
    let path = repo.runs_log();
    fs::create_dir_all(path.parent().expect("log parent")).expect("create log parent");
    let mut file = File::create(&path).expect("create sparse log");
    file.set_len(129 * 1024 * 1024)
        .expect("extend sparse history");
    file.seek(SeekFrom::End(0)).expect("seek to recent row");
    writeln!(file).expect("close historical row");
    writeln!(
        file,
        "{}",
        serde_json::json!({
            "execution_id": "recent-large",
            "task_id": task_id,
            "timestamp": "2026-10-10T12:00:00Z",
            "tool": "task"
        })
    )
    .expect("append recent row");
    drop(file);

    let shown = repo.run(&["task", "show", task_id]);
    assert!(shown.status.success(), "stderr={}", stderr_str(&shown));
    let task: Value = serde_json::from_str(&stdout_str(&shown)).expect("task show JSON");
    assert_eq!(task["latest_run"]["execution_id"], "recent-large");
    let diag = repo.run(&["diag", "--json", "--window", "1"]);
    assert!(diag.status.success(), "stderr={}", stderr_str(&diag));
    let payload: Value = serde_json::from_str(&stdout_str(&diag)).expect("diag JSON");
    assert_eq!(payload["last_run_id"], "recent-large");
}

#[test]
fn recent_skips_history() {
    let repo = TempRepo::new("log-boundary");
    let path = repo.runs_log();
    fs::create_dir_all(path.parent().expect("log parent")).expect("create log parent");
    let mut writer = BufWriter::new(File::create(path).expect("create log"));
    let old = row("old", "OLD");
    for _ in 0..55_000 {
        writeln!(writer, "{old}").expect("write old row");
    }
    writeln!(writer, "{}", row("new", "LATEST")).expect("write newest row");
    writer.flush().expect("flush log");

    let stats = repo.run(&["logs", "stats", "1", "--json"]);
    assert!(stats.status.success(), "stderr={}", stderr_str(&stats));
    let payload: Value = serde_json::from_str(&stdout_str(&stats)).expect("stats JSON");
    assert_eq!(payload["window_runs"], 1);
    let trace = repo.run(&["trace", "1"]);
    assert!(trace.status.success(), "stderr={}", stderr_str(&trace));
    assert!(stdout_str(&trace).contains("prompt_preview: LATEST"));
    let quota = repo.run(&["quota", "probe", "30", "--json"]);
    assert!(quota.status.success(), "stderr={}", stderr_str(&quota));
}

#[test]
fn quota_window_budget() {
    let repo = TempRepo::new("log-boundary");
    let path = repo.runs_log();
    fs::create_dir_all(path.parent().expect("log parent")).expect("create log parent");
    let mut writer = BufWriter::new(File::create(path).expect("create log"));
    let old = serde_json::json!({
        "execution_id": "old",
        "timestamp": "2020-01-01T00:00:00Z",
        "tool": "capture",
        "prompt_preview": "x".repeat(600_000)
    });
    for _ in 0..8 {
        writeln!(writer, "{old}").expect("write old row");
    }
    writeln!(
        writer,
        "{}",
        serde_json::json!({
            "execution_id": "recent",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "tool": "capture"
        })
    )
    .expect("write recent row");
    writer.flush().expect("flush log");

    let narrow = repo.run(&["quota", "probe", "1", "--json"]);
    assert!(narrow.status.success(), "stderr={}", stderr_str(&narrow));
    let broad = repo.run(&["quota", "probe", "9999", "--json"]);
    assert_eq!(broad.status.code(), Some(1));
    assert!(stderr_str(&broad).contains("result exceeds"));
}

#[test]
fn task_lookup_status() {
    let repo = TempRepo::new("log-boundary");
    let added = repo.run(&["task", "add", "Synthetic unrun task"]);
    assert!(added.status.success(), "stderr={}", stderr_str(&added));
    let listed = repo.run(&["task", "list", "--json"]);
    let list: Value = serde_json::from_str(&stdout_str(&listed)).expect("task list JSON");
    let task_id = list["tasks"][0]["id"].as_str().expect("task id");
    let path = repo.runs_log();
    fs::create_dir_all(path.parent().expect("log parent")).expect("create log parent");
    let mut writer = BufWriter::new(File::create(path).expect("create log"));
    let chunk = vec![b'\n'; 1024 * 1024];
    for _ in 0..129 {
        writer.write_all(&chunk).expect("write bounded rows");
    }
    writer.flush().expect("flush log");

    let shown = repo.run(&["task", "show", task_id]);
    assert!(shown.status.success(), "stderr={}", stderr_str(&shown));
    let task: Value = serde_json::from_str(&stdout_str(&shown)).expect("task show JSON");
    assert_eq!(task["id"], task_id);
    assert_eq!(task["latest_run"], Value::Null);
    assert_eq!(task["latest_run_lookup"], "unavailable");
    assert!(stderr_str(&shown).contains("scan exceeds"));
}

#[test]
fn oversized_row_error() {
    let repo = TempRepo::new("log-boundary");
    let huge = format!("{{\"note\":\"{}\"}}", "x".repeat(1024 * 1024));
    write_log(&repo, &[&huge]);
    for args in [
        &["logs", "stats", "1", "--json"][..],
        &["quota", "probe", "30", "--json"][..],
        &["trace", "1"][..],
        &["log-tail", "1"][..],
        &["logs", "validate"][..],
    ] {
        let out = repo.run(args);
        assert_eq!(out.status.code(), Some(1), "args={args:?}");
        assert!(
            stderr_str(&out).contains("exceeds 1048576 bytes"),
            "args={args:?} stderr={}",
            stderr_str(&out)
        );
    }
    let target = repo.home.join("migrated.jsonl");
    let migrated = repo.run(&[
        "logs",
        "migrate",
        "--out",
        target.to_str().expect("synthetic target path"),
    ]);
    assert_eq!(migrated.status.code(), Some(1));
    assert!(!target.exists());
}

#[test]
fn migration_output_cap() {
    let repo = TempRepo::new("log-boundary");
    let legacy = serde_json::json!({
        "ts": "2026-10-10T12:00:00Z",
        "tool": "capture",
        "repo_root": repo.root.to_str().expect("synthetic root"),
        "prompt_preview": "x".repeat(1024 * 1024 - 700)
    })
    .to_string();
    assert!(legacy.len() < 1024 * 1024);
    write_log(&repo, &[&legacy]);
    let target = repo.home.join("migrated.jsonl");
    let out = repo.run(&[
        "logs",
        "migrate",
        "--out",
        target.to_str().expect("synthetic target"),
    ]);
    assert_eq!(out.status.code(), Some(1), "stderr={}", stderr_str(&out));
    assert!(stderr_str(&out).contains("normalized run log row exceeds"));
    assert!(!target.exists());
}

#[cfg(unix)]
#[test]
fn repo_symlink_reads() {
    use std::os::unix::fs::symlink;

    let repo = TempRepo::new("log-boundary");
    let outside = repo.home.join("outside.jsonl");
    fs::write(
        &outside,
        format!("{}\n", row("outside", "SYNTHETIC-PRIVATE-MARKER")),
    )
    .expect("write outside log");
    let path = repo.runs_log();
    fs::create_dir_all(path.parent().expect("log parent")).expect("create log parent");
    symlink(&outside, &path).expect("link log leaf");

    for args in [
        &["trace", "1"][..],
        &["log-tail", "1"][..],
        &["logs", "validate"][..],
    ] {
        let out = repo.run(args);
        assert_eq!(out.status.code(), Some(1), "args={args:?}");
        assert!(!stdout_str(&out).contains("SYNTHETIC-PRIVATE-MARKER"));
    }
    let selected = repo.run(&["llm", "use", "primary"]);
    assert!(
        selected.status.success(),
        "stderr={}",
        stderr_str(&selected)
    );
    assert!(!stdout_str(&selected).contains("SYNTHETIC-PRIVATE-MARKER"));
    assert!(!stderr_str(&selected).contains("SYNTHETIC-PRIVATE-MARKER"));

    let alias = path.to_str().expect("synthetic path");
    let explicit = repo.run_with_env(&["trace", "1"], &[("CX_LOG_FILE", alias)]);
    assert!(
        explicit.status.success(),
        "stderr={}",
        stderr_str(&explicit)
    );
    assert!(stdout_str(&explicit).contains("SYNTHETIC-PRIVATE-MARKER"));

    fs::remove_file(&path).expect("remove leaf link");
    fs::remove_dir(path.parent().expect("log parent")).expect("remove empty parent");
    let outside_dir = repo.home.join("outside-logs");
    fs::create_dir_all(&outside_dir).expect("outside dir");
    fs::copy(&outside, outside_dir.join("runs.jsonl")).expect("outside parent log");
    symlink(&outside_dir, path.parent().expect("log parent")).expect("link log parent");
    let parent = repo.run(&["trace", "1"]);
    assert_eq!(parent.status.code(), Some(1));
    assert!(!stdout_str(&parent).contains("SYNTHETIC-PRIVATE-MARKER"));
}

#[cfg(unix)]
#[test]
fn repo_swap_safe() {
    use std::os::unix::fs::symlink;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let repo = TempRepo::new("log-boundary");
    let path = repo.runs_log();
    fs::create_dir_all(path.parent().expect("log parent")).expect("create log parent");
    let owned = format!("{}\n", row("owned", "OWNED-MARKER"));
    fs::write(&path, &owned).expect("write owned log");
    let outside = repo.home.join("outside.jsonl");
    fs::write(
        &outside,
        format!("{}\n", row("outside", "SYNTHETIC-PRIVATE-MARKER")),
    )
    .expect("write outside log");
    let running = Arc::new(AtomicBool::new(true));
    let race_running = Arc::clone(&running);
    let race_path = path.clone();
    let swapper = std::thread::spawn(move || {
        let mut i = 0;
        while race_running.load(Ordering::Relaxed) {
            let staged = race_path.with_extension("swap");
            if i % 2 == 0 {
                symlink(&outside, &staged).expect("stage symlink");
            } else {
                fs::write(&staged, &owned).expect("stage owned log");
            }
            fs::rename(&staged, &race_path).expect("swap log");
            i += 1;
            std::thread::yield_now();
        }
    });

    for _ in 0..12 {
        for args in [&["trace", "1"][..], &["log-tail", "1"][..]] {
            let out = repo.run(args);
            assert!(!stdout_str(&out).contains("SYNTHETIC-PRIVATE-MARKER"));
        }
    }
    running.store(false, Ordering::Relaxed);
    swapper.join().expect("log swapper");
}
