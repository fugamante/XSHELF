#![cfg(unix)]

mod common;

use common::{
    TempRepo, expect_schema_fail, parse_jsonl, stderr_str, stdout_str, write_quarantine_fixture,
    write_runs_log_rows,
};
use serde_json::Value;
use sha2::Digest;
use std::fs;
use std::os::unix::fs::symlink;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

const BAD_RESPONSE: &str = r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"not-json"}}'
"#;

const GOOD_RESPONSE: &str = r#"#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"{\"commands\":[\"echo ok\"]}"}}'
"#;

#[test]
fn quarantine_traversal_guard() {
    let repo = TempRepo::new("cxrs-qauth");
    repo.write_mock_primary(BAD_RESPONSE);
    let failed = repo.run(&["next", "echo", "synthetic"]);
    assert!(!failed.status.success());
    let _ = expect_schema_fail(&repo);

    // This file lies outside .cx/quarantine and would be read by the old join.
    fs::write(repo.root.join("probe.json"), "{synthetic-invalid-json")
        .expect("write outside probe");
    for id in ["../../probe", "../probe", "a/b", r"a\b", ".", "", "%2e%2e"] {
        let show = repo.run(&["quarantine", "show", id]);
        assert!(!show.status.success(), "show accepted {id:?}");
        assert!(
            stderr_str(&show).contains("invalid quarantine id"),
            "show {id:?}: {}",
            stderr_str(&show)
        );
        assert!(!stderr_str(&show).contains("invalid quarantine JSON"));
    }
    let oversized_id = "a".repeat(251);
    let show = repo.run(&["quarantine", "show", &oversized_id]);
    assert!(!show.status.success());
    assert!(stderr_str(&show).contains("invalid quarantine id"));

    let mut rows = parse_jsonl(&repo.runs_log());
    rows.last_mut()
        .and_then(Value::as_object_mut)
        .expect("run row")
        .insert("quarantine_id".to_string(), Value::from("../../probe"));
    write_runs_log_rows(&repo, &rows);
    let strict = repo.run(&["logs", "validate", "--strict"]);
    assert!(!strict.status.success());
    assert!(stdout_str(&strict).contains("invalid quarantine id"));
    assert!(!stdout_str(&strict).contains("invalid quarantine JSON"));

    let marker = repo.home.join("unexpected-provider-call");
    repo.write_mock_primary(&format!(
        "#!/usr/bin/env bash\ntouch '{}'\nexit 1\n",
        marker.display()
    ));
    let replay = repo.run(&["replay", "../../probe"]);
    assert!(!replay.status.success());
    assert!(stderr_str(&replay).contains("invalid quarantine id"));
    assert!(
        !marker.exists(),
        "replay invoked a provider after invalid ID"
    );
}

#[test]
fn quarantine_symlink_guard() {
    let repo = TempRepo::new("cxrs-qauth");
    write_quarantine_fixture(&repo, "valid_id", "next", "{}", "prompt", "raw");
    let outside = repo.home.join("outside.json");
    fs::copy(repo.quarantine_file("valid_id"), &outside).expect("copy synthetic record");
    fs::remove_file(repo.quarantine_file("valid_id")).expect("remove record");
    symlink(&outside, repo.quarantine_file("valid_id")).expect("link record");

    let show = repo.run(&["quarantine", "show", "valid_id"]);
    assert!(!show.status.success(), "symlinked file was read");
    assert!(stderr_str(&show).contains("cannot open"));
    let list = repo.run(&["quarantine", "list"]);
    assert!(list.status.success());
    assert!(stdout_str(&list).contains("entries: 0"));

    fs::remove_dir_all(repo.quarantine_dir()).expect("remove quarantine directory");
    let external_dir = repo.home.join("external-quarantine");
    fs::create_dir_all(&external_dir).expect("create external directory");
    fs::copy(&outside, external_dir.join("valid_id.json")).expect("copy external record");
    symlink(&external_dir, repo.quarantine_dir()).expect("link quarantine directory");
    let show = repo.run(&["quarantine", "show", "valid_id"]);
    assert!(
        !show.status.success(),
        "symlinked quarantine directory was read"
    );
    assert!(stderr_str(&show).contains("cannot open quarantine directory"));
    let list = repo.run(&["quarantine", "list"]);
    assert!(!list.status.success(), "symlinked directory was listed");

    repo.write_mock_primary(BAD_RESPONSE);
    let failed = repo.run(&["next", "echo", "synthetic"]);
    assert!(
        !failed.status.success(),
        "symlinked directory accepted a write"
    );
    assert!(
        stderr_str(&failed).contains("cannot open quarantine directory"),
        "write path was not reached: {}",
        stderr_str(&failed)
    );
    let names: Vec<_> = fs::read_dir(&external_dir)
        .expect("read external directory")
        .map(|entry| entry.expect("external entry").file_name())
        .collect();
    assert_eq!(
        names.len(),
        1,
        "quarantine write escaped into external directory"
    );
}

#[test]
fn quarantine_parent_guard() {
    let repo = TempRepo::new("cxrs-qauth");
    fs::create_dir_all(repo.quarantine_dir()).expect("create quarantine directory");
    fs::create_dir(repo.quarantine_file("directory_id")).expect("create nonregular leaf");
    let show = repo.run(&["quarantine", "show", "directory_id"]);
    assert!(!show.status.success());
    assert!(stderr_str(&show).contains("not a regular file"));

    let outside = repo.home.join("external-cx");
    fs::rename(repo.root.join(".cx"), &outside).expect("move .cx outside repo");
    symlink(&outside, repo.root.join(".cx")).expect("link .cx parent");
    let show = repo.run(&["quarantine", "show", "directory_id"]);
    assert!(!show.status.success());
    assert!(stderr_str(&show).contains("cannot open quarantine directory"));

    repo.write_mock_primary(BAD_RESPONSE);
    let failed = repo.run(&["next", "echo", "synthetic"]);
    assert!(!failed.status.success());
    assert!(
        stderr_str(&failed).contains("cannot open quarantine directory")
            || stderr_str(&failed).contains("cannot open JSON parent"),
        "symlinked .cx did not fail closed: {}",
        stderr_str(&failed)
    );
    assert_eq!(
        fs::read_dir(outside.join("quarantine"))
            .expect("external quarantine")
            .count(),
        1,
        "write escaped through .cx symlink"
    );
}

#[test]
fn quarantine_list_guard() {
    let repo = TempRepo::new("cxrs-qauth");
    write_quarantine_fixture(&repo, "valid_id", "next", "{}", "prompt", "raw");
    let outside = repo.home.join("external.json");
    let mut record: Value = serde_json::from_str(
        &fs::read_to_string(repo.quarantine_file("valid_id")).expect("read fixture"),
    )
    .expect("parse fixture");
    record["reason"] = Value::from("SYNTHETIC_EXTERNAL_MARKER");
    fs::write(&outside, record.to_string()).expect("write external record");
    symlink(&outside, repo.quarantine_file("alias")).expect("link external record");

    let large = repo.quarantine_file("oversize");
    fs::File::create(&large)
        .expect("create sparse record")
        .set_len(64 * 1024 * 1024 + 1)
        .expect("size sparse record");
    let show = repo.run(&["quarantine", "show", "oversize"]);
    assert!(!show.status.success());
    assert!(stderr_str(&show).contains("exceeds maximum size"));

    let list = repo.run(&["quarantine", "list"]);
    assert!(list.status.success());
    assert!(stdout_str(&list).contains("valid_id"));
    assert!(!stdout_str(&list).contains("SYNTHETIC_EXTERNAL_MARKER"));
    assert!(stdout_str(&list).contains("entries: 1"));
}

#[test]
fn quarantine_race_guard() {
    let repo = TempRepo::new("cxrs-qauth");
    write_quarantine_fixture(&repo, "race_id", "next", "{}", "prompt", "local raw");
    let leaf = repo.quarantine_file("race_id");
    let local = fs::read(&leaf).expect("read local record");
    let external = repo.home.join("external.json");
    let mut record: Value = serde_json::from_slice(&local).expect("parse local record");
    record["raw_response"] = Value::from("SYNTHETIC_EXTERNAL_MARKER");
    record["raw_sha256"] = Value::from(format!(
        "{:x}",
        sha2::Sha256::digest(b"SYNTHETIC_EXTERNAL_MARKER")
    ));
    fs::write(&external, record.to_string()).expect("write external record");

    let running = Arc::new(AtomicBool::new(true));
    let stop = Arc::clone(&running);
    let worker = thread::spawn(move || {
        while stop.load(Ordering::Relaxed) {
            let _ = fs::remove_file(&leaf);
            let _ = symlink(&external, &leaf);
            thread::yield_now();
            let _ = fs::remove_file(&leaf);
            let _ = fs::write(&leaf, &local);
            thread::yield_now();
        }
    });
    for _ in 0..40 {
        let show = repo.run(&["quarantine", "show", "race_id"]);
        assert!(
            !stdout_str(&show).contains("SYNTHETIC_EXTERNAL_MARKER"),
            "external record escaped during swap"
        );
        if show.status.success() {
            assert!(stdout_str(&show).contains("local raw"));
        }
    }
    running.store(false, Ordering::Relaxed);
    worker.join().expect("join swap worker");
}

#[test]
fn quarantine_contracts() {
    let repo = TempRepo::new("cxrs-qauth");
    repo.write_mock_primary(BAD_RESPONSE);
    let failed = repo.run(&["next", "echo", "synthetic"]);
    assert!(!failed.status.success());
    let id = expect_schema_fail(&repo);

    let show = repo.run(&["quarantine", "show", &id]);
    assert!(show.status.success(), "{}", stderr_str(&show));
    assert_eq!(
        serde_json::from_str::<Value>(&stdout_str(&show)).expect("show JSON")["id"],
        id
    );
    let list = repo.run(&["quarantine", "list"]);
    assert!(list.status.success());
    assert!(stdout_str(&list).contains(&id));
    let strict = repo.run(&["logs", "validate", "--strict"]);
    assert!(
        strict.status.success(),
        "stdout={} stderr={}",
        stdout_str(&strict),
        stderr_str(&strict)
    );

    repo.write_mock_primary(GOOD_RESPONSE);
    let replay = repo.run(&["replay", &id]);
    assert!(
        replay.status.success(),
        "stdout={} stderr={}",
        stdout_str(&replay),
        stderr_str(&replay)
    );
    assert!(stdout_str(&replay).contains("echo ok"));
}
