mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cxrs"))
        .current_dir(root.join("project"))
        .args(args)
        .env("CX_LOG_FILE", root.join("should-not-exist.jsonl"))
        .output()
        .unwrap()
}

fn setup() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("project")).unwrap();
    let store = dir.path().join("store");
    fs::create_dir(&store).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();
    }
    (dir, store)
}

fn import(root: &Path, store: &Path, profile: &str, raw: &str) -> Value {
    let input = root.join("fixture.txt");
    fs::write(&input, raw).unwrap();
    let out = run(
        root,
        &[
            "evidence",
            "import",
            "--store",
            store.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--profile",
            profile,
            "--status",
            "101",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn cargo_detail_recovery() {
    let (dir, store) = setup();
    let mut raw = String::new();
    for i in 0..300 {
        raw.push_str(&format!("   Compiling synthetic_{i} v0.1.0\n"));
    }
    raw.push_str("warning: unused synthetic import\nwarning: unused synthetic import\n");
    raw.push_str("test broken ... FAILED\nthread 'broken' panicked at src/lib.rs:9\nassertion left == right failed\n  left: 1\n right: 2\n");
    raw.push_str("test result: FAILED. 9 passed; 1 failed; 0 ignored\n");
    let v = import(dir.path(), &store, "cargo", &raw);
    let text = v["summary"]["text"].as_str().unwrap();
    for span in [
        "broken",
        "left: 1",
        "right: 2",
        "9 passed; 1 failed",
        "unused synthetic import",
    ] {
        assert!(text.contains(span), "missing {span}: {text}");
    }
    assert_eq!(text.matches("warning: unused synthetic import").count(), 1);
    assert!(text.len() < raw.len() / 4);
    assert_eq!(v["exit_status"], 101);
    let contract = common::load_fixture_json("evidence_contract.json");
    assert_eq!(v["schema_version"], contract["schema_version"]);
    common::assert_fixture_contract(
        &v,
        &contract,
        "summary_keys",
        &[("summary", "summary_fields", "evidence.summary")],
    );
    assert_eq!(v["summary"]["profile"], "cargo");
    assert!(!dir.path().join("should-not-exist.jsonl").exists());
    let out = run(
        dir.path(),
        &[
            "evidence",
            "show",
            "--store",
            store.to_str().unwrap(),
            "--id",
            v["id"].as_str().unwrap(),
            "--start",
            "1",
            "--lines",
            "2",
        ],
    );
    let details: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        details["text"],
        "   Compiling synthetic_0 v0.1.0\n   Compiling synthetic_1 v0.1.0"
    );
    common::assert_has_keys(
        &details,
        &common::fixture_keys(&contract, "detail_keys"),
        "evidence.detail",
    );
    assert_eq!(details["schema_version"], contract["schema_version"]);
    assert_eq!(details["next_start"], 3);
    assert_eq!(details["exit_status"], 101);
}

#[test]
fn xcode_and_fallback() {
    let (dir, store) = setup();
    let raw = "CompileSwift synthetic\n".repeat(500)
        + "ld: warning: duplicate libraries: '-lc++'\nTest Suite 'Synthetic' passed\nExecuted 24 tests, with 0 failures in 0.2 seconds\n** TEST SUCCEEDED **\n";
    let v = import(dir.path(), &store, "xcode", &raw);
    for span in [
        "duplicate libraries",
        "Executed 24 tests",
        "** TEST SUCCEEDED **",
    ] {
        assert!(v["summary"]["text"].as_str().unwrap().contains(span));
    }
    assert!(v["summary"]["summary_chars"].as_u64().unwrap() < 1000);
    let v = import(dir.path(), &store, "cargo", "privacy scan failed\n");
    assert_eq!(v["summary"]["profile"], "generic");
    assert_eq!(v["summary"]["text"], "privacy scan failed\n");
    assert_eq!(v["summary"]["truncated"], false);
}

#[test]
fn late_result_priority() {
    let (dir, store) = setup();
    let mut raw = (0..200)
        .map(|i| format!("warning: diagnostic {i}\n"))
        .collect::<String>();
    raw.push_str("test result: FAILED. 99 passed; 1 failed\n");
    let v = import(dir.path(), &store, "cargo", &raw);
    assert!(
        v["summary"]["text"]
            .as_str()
            .unwrap()
            .starts_with("201: test result: FAILED")
    );
    assert_eq!(v["summary"]["truncated"], true);
}

#[test]
fn unicode_detail_paging() {
    let (dir, store) = setup();
    let raw = "界".repeat(13000) + "\nlast";
    let v = import(dir.path(), &store, "generic", &raw);
    let base = [
        "evidence",
        "show",
        "--store",
        store.to_str().unwrap(),
        "--id",
        v["id"].as_str().unwrap(),
        "--lines",
        "1",
    ];
    let first = run(dir.path(), &base);
    let first: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["next_offset"], 12000);
    assert_eq!(first["next_start"], Value::Null);
    let mut args = base.to_vec();
    args.extend(["--offset", "12000"]);
    let second = run(dir.path(), &args);
    let second: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(second["text"].as_str().unwrap().chars().count(), 1000);
    assert_eq!(second["next_start"], 2);
    assert_eq!(second["truncated"], false);
}

#[test]
fn capture_status_args() {
    let (dir, store) = setup();
    let out = run(
        dir.path(),
        &[
            "evidence",
            "capture",
            "--store",
            store.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            "printf '%s\\n' \"$1\"; pwd; exit 7",
            "sh",
            "literal $value with spaces",
        ],
    );
    assert_eq!(out.status.code(), Some(7));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let text = v["summary"]["text"].as_str().unwrap();
    assert!(text.contains("literal $value with spaces"));
    assert!(text.contains(dir.path().canonicalize().unwrap().to_str().unwrap()));
    assert_eq!(v["exit_status"], 7);
}

#[test]
fn invalid_option_preflight() {
    let (dir, store) = setup();
    let out = run(
        dir.path(),
        &[
            "evidence",
            "capture",
            "--store",
            store.to_str().unwrap(),
            "--profile",
            "cargo",
            "--",
            "touch",
            "unexpected",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(!dir.path().join("project/unexpected").exists());
    assert_eq!(fs::read_dir(&store).unwrap().count(), 0);
    let out = run(
        dir.path(),
        &[
            "evidence",
            "show",
            "--store",
            store.to_str().unwrap(),
            "--id",
            "../../other",
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let out = run(
        dir.path(),
        &[
            "evidence",
            "show",
            "--store",
            store.to_str().unwrap(),
            "--id",
            "a",
            "--lines",
            "201",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
}

#[cfg(unix)]
#[test]
fn private_storage_guards() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (dir, store) = setup();
    fs::set_permissions(&store, fs::Permissions::from_mode(0o755)).unwrap();
    let out = run(
        dir.path(),
        &[
            "evidence",
            "capture",
            "--store",
            store.to_str().unwrap(),
            "--",
            "touch",
            "unexpected",
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(!dir.path().join("project/unexpected").exists());
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();
    let v = import(dir.path(), &store, "generic", "private synthetic record");
    let id = v["id"].as_str().unwrap();
    let record = store.join(format!("{id}.json"));
    assert_eq!(
        fs::metadata(&record).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::rename(&record, store.join("original")).unwrap();
    symlink(store.join("original"), &record).unwrap();
    let out = run(
        dir.path(),
        &[
            "evidence",
            "show",
            "--store",
            store.to_str().unwrap(),
            "--id",
            id,
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
}

#[test]
fn invalid_store_preflight() {
    let (dir, _) = setup();
    let project = dir.path().join("project");
    let local = project.join("local-store");
    fs::create_dir(&local).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&local, fs::Permissions::from_mode(0o700)).unwrap();
    }
    for path in [
        "relative",
        local.to_str().unwrap(),
        "/nonexistent-xshelf-evidence-store",
    ] {
        let out = run(
            dir.path(),
            &[
                "evidence",
                "capture",
                "--store",
                path,
                "--",
                "touch",
                "unexpected",
            ],
        );
        assert_eq!(out.status.code(), Some(1));
        assert!(!project.join("unexpected").exists());
    }
    assert_eq!(fs::read_dir(local).unwrap().count(), 0);
}

#[test]
fn corrupt_record_rejected() {
    let (dir, store) = setup();
    let v = import(dir.path(), &store, "generic", "synthetic\n");
    let id = v["id"].as_str().unwrap();
    let path = store.join(format!("{id}.json"));
    let mut record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    record["schema_version"] = 99.into();
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    let out = run(
        dir.path(),
        &[
            "evidence",
            "show",
            "--store",
            store.to_str().unwrap(),
            "--id",
            id,
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
}

#[test]
fn records_are_independent() {
    let (dir, store) = setup();
    let a = import(dir.path(), &store, "generic", "first\n");
    let b = import(dir.path(), &store, "generic", "second\n");
    assert_ne!(a["id"], b["id"]);
    let out = run(
        dir.path(),
        &[
            "evidence",
            "show",
            "--store",
            store.to_str().unwrap(),
            "--id",
            a["id"].as_str().unwrap(),
        ],
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["text"], "first");
    let mut keys: Vec<_> = a.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "exit_status",
            "id",
            "kind",
            "retention",
            "schema_version",
            "summary"
        ]
    );
}

#[test]
fn compiler_flag_noise() {
    let (dir, store) = setup();
    let command =
        "builtin-SwiftDriver -- swiftc -warnings-as-errors -D SWIFT_TREAT_WARNINGS_AS_ERRORS=YES "
            .to_string()
            + &" -I synthetic/include".repeat(20)
            + "\n";
    let raw = (0..100)
        .map(|i| format!("unit_{i} {command}"))
        .collect::<String>()
        + "ld: warning: newer XCTest library version\nwarning: metadata extraction skipped\n** BUILD SUCCEEDED **\n";
    let result = import(dir.path(), &store, "xcode", &raw);
    let summary = result["summary"]["text"].as_str().unwrap();
    assert!(summary.contains("newer XCTest library version"));
    assert!(summary.contains("metadata extraction skipped"));
    assert!(summary.contains("** BUILD SUCCEEDED **"));
}
