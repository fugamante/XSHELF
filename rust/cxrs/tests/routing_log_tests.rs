mod common;

use common::{TempRepo, stderr_str, stdout_str};
use std::fs;

#[test]
fn where_data_safe() {
    let repo = TempRepo::new("cxrs-route");
    let strange = repo.root.join("odd' ; touch route_executed; echo '");
    fs::create_dir_all(strange.join("lib")).expect("create synthetic library");
    fs::write(strange.join("lib/cx.sh"), "safe_probe() { :; }\n")
        .expect("write synthetic function");
    let root = strange.to_str().expect("utf8 synthetic path");

    let normal = repo.run_with_env(&["where", "safe_probe"], &[("CX_REPO_ROOT", root)]);
    assert!(normal.status.success(), "stderr={}", stderr_str(&normal));
    assert!(stdout_str(&normal).contains("safe_probe: route=bash"));
    assert!(!repo.root.join("route_executed").exists());

    let hostile = repo.run_with_env(
        &["where", "safe_probe; touch argument_executed"],
        &[("CX_REPO_ROOT", root)],
    );
    assert!(hostile.status.success(), "stderr={}", stderr_str(&hostile));
    assert!(stdout_str(&hostile).contains("route=unknown"));
    assert!(!repo.root.join("route_executed").exists());
    assert!(!repo.root.join("argument_executed").exists());

    let compat = repo.run_with_env(
        &["cx-compat", "cxwhere", "safe_probe"],
        &[("CX_REPO_ROOT", root)],
    );
    assert!(compat.status.success(), "stderr={}", stderr_str(&compat));
    assert!(stdout_str(&compat).contains("safe_probe: route=bash"));
}

#[test]
fn capture_log_optout() {
    let repo = TempRepo::new("cxrs-log-optout");
    let log = repo.runs_log();
    let log_path = log.to_str().expect("utf8 synthetic log path");
    let disabled = repo.run_with_env(
        &["capture", "true"],
        &[("CXLOG_ENABLED", "0"), ("CX_LOG_FILE", log_path)],
    );
    assert!(
        disabled.status.success(),
        "stderr={}",
        stderr_str(&disabled)
    );
    assert!(!log.exists(), "opt-out created a run log");

    let enabled = repo.run_with_env(
        &["capture", "true"],
        &[("CXLOG_ENABLED", "1"), ("CX_LOG_FILE", log_path)],
    );
    assert!(enabled.status.success(), "stderr={}", stderr_str(&enabled));
    let original = fs::read(&log).expect("normal capture run log");
    assert!(!original.is_empty());

    let disabled_again = repo.run_with_env(
        &["capture", "true"],
        &[("CXLOG_ENABLED", "0"), ("CX_LOG_FILE", log_path)],
    );
    assert!(
        disabled_again.status.success(),
        "stderr={}",
        stderr_str(&disabled_again)
    );
    assert_eq!(fs::read(&log).expect("existing log"), original);
}

#[test]
fn schema_log_optout() {
    let repo = TempRepo::new("cxrs-schema-optout");
    let out = repo.run_with_env(
        &["next", "echo", "mock-fail"],
        &[
            ("CX_PROVIDER_ADAPTER", "mock"),
            ("CX_MOCK_PLAIN_RESPONSE", "not-json"),
            ("CXLOG_ENABLED", "0"),
        ],
    );
    assert!(!out.status.success(), "expected schema failure");
    assert!(!repo.runs_log().exists(), "opt-out created a run log");
    assert!(repo.schema_fail_log().exists());
    assert!(
        fs::read_dir(repo.quarantine_dir())
            .expect("quarantine directory")
            .next()
            .is_some()
    );
}
