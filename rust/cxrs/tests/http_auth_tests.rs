mod common;

use common::*;
use serde_json::Value;
use std::sync::{Mutex, MutexGuard, OnceLock};

fn suite_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("http auth test lock")
}

#[test]
fn basic_auth_cov() {
    let _guard = suite_lock();
    if std::process::Command::new("curl")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let repo = TempRepo::new("cxrs-it");
    let (url, captured, handle) = run_fixture_http_server_once(r#"{"text":"basic-auth-ok"}"#);
    let out = repo.run_with_env(
        &["cxo", "echo", "http-basic"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", &url),
            ("CX_HTTP_AUTH_PROFILE", "basic"),
            ("CX_HTTP_AUTH_USERNAME", "alice"),
            ("CX_HTTP_AUTH_PASSWORD", "secret"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), "basic-auth-ok");
    handle.join().expect("fixture join");

    let req = captured
        .lock()
        .expect("fixture lock")
        .clone()
        .expect("captured request");
    assert_eq!(req.authorization.as_deref(), Some("Basic YWxpY2U6c2VjcmV0"));
}

#[test]
fn header_auth_cov() {
    let _guard = suite_lock();
    if std::process::Command::new("curl")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let repo = TempRepo::new("cxrs-it");
    let (url, captured, handle) = run_fixture_http_server_once(r#"{"text":"header-auth-ok"}"#);
    let out = repo.run_with_env(
        &["cxo", "echo", "http-header"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", &url),
            ("CX_HTTP_AUTH_PROFILE", "header"),
            ("CX_HTTP_AUTH_HEADER", "X-API-Key"),
            ("CX_HTTP_AUTH_VALUE", "key-123"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), "header-auth-ok");
    handle.join().expect("fixture join");

    let req = captured
        .lock()
        .expect("fixture lock")
        .clone()
        .expect("captured request");
    assert_eq!(
        req.headers.get("x-api-key").map(String::as_str),
        Some("key-123")
    );
}

#[test]
fn core_auth_mode() {
    let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|path| path.join(".codex").join("schemas").is_dir())
        .expect("repo root")
        .to_path_buf();
    let out = std::process::Command::new(repo.join("bin").join("cx"))
        .args(["core", "--json"])
        .env("CX_PROVIDER_ADAPTER", "http-curl")
        .env("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer")
        .env("CX_HTTP_AUTH_PROFILE", "header")
        .env("CX_HTTP_AUTH_HEADER", "X-API-Key")
        .current_dir(&repo)
        .output()
        .expect("run core --json");

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("parse core json");
    let provider = payload.get("provider").expect("provider object");
    assert_eq!(
        provider.get("http_auth_mode").and_then(Value::as_str),
        Some("header")
    );
    assert_eq!(
        provider.get("http_auth_header").and_then(Value::as_str),
        Some("X-API-Key")
    );
}
