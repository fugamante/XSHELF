use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn repo_root() -> PathBuf {
    let mut cur = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..6 {
        if cur.join(".cx").join("schemas").is_dir() && cur.join("bin").join("cx").is_file() {
            return cur;
        }
        if !cur.pop() {
            break;
        }
    }
    panic!(
        "unable to resolve repo root from {}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).display()
    );
}

#[test]
fn bin_cx_version_reports_runtime() {
    let repo = repo_root();
    let out = Command::new(repo.join("bin").join("cx"))
        .arg("version")
        .current_dir(&repo)
        .output()
        .expect("run bin/cx version");

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("execution_path:"), "{stdout}");
}

#[test]
fn bin_xshelf_version_reports_runtime() {
    let repo = repo_root();
    let out = Command::new(repo.join("bin").join("xshelf"))
        .arg("version")
        .current_dir(&repo)
        .output()
        .expect("run bin/xshelf version");

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("execution_path:"), "{stdout}");
}

#[test]
fn lib_cx_sh_exports_functions() {
    let repo = repo_root();
    let script = format!(
        "source '{}' >/dev/null 2>&1; declare -F cx >/dev/null && declare -F cxversion >/dev/null",
        repo.join("lib").join("cx.sh").display()
    );
    let out = Command::new("bash")
        .arg("-lc")
        .arg(script)
        .current_dir(&repo)
        .output()
        .expect("source lib/cx.sh");

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn core_tq_json() {
    let repo = repo_root();
    let out = Command::new(repo.join("bin").join("cx"))
        .args(["core", "--json"])
        .current_dir(&repo)
        .output()
        .expect("run bin/cx core --json");

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("parse core json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("core.v1")
    );
    assert_eq!(
        payload
            .get("backend_capabilities")
            .and_then(|v| v.get("turboquant"))
            .and_then(|v| v.get("cx_runtime_support"))
            .and_then(Value::as_str),
        Some("none")
    );
    assert_eq!(
        payload
            .get("backend_capabilities")
            .and_then(|v| v.get("turboquant"))
            .and_then(|v| v.get("selected_backend_role"))
            .and_then(Value::as_str),
        Some("standard_provider")
    );
}

#[test]
fn core_tls_posture() {
    let repo = repo_root();
    let out = Command::new(repo.join("bin").join("cx"))
        .args(["core", "--json"])
        .env("CX_PROVIDER_ADAPTER", "http-curl")
        .env("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer")
        .env("CX_HTTP_ALLOWED_HOSTS", "api.example.test")
        .env("CX_HTTP_CA_BUNDLE", "/tmp/test-ca.pem")
        .env("CX_HTTP_CLIENT_CERT", "/tmp/test-client.pem")
        .env("CX_HTTP_CLIENT_KEY", "/tmp/test-client.key")
        .env("CX_HTTP_TLS_MIN_VERSION", "1.3")
        .env("CX_HTTP_FOLLOW_REDIRECTS", "1")
        .env("CX_HTTP_MAX_REDIRECTS", "5")
        .current_dir(&repo)
        .output()
        .expect("run bin/cx core --json with http tls");

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("parse core json");
    let posture = payload
        .get("provider")
        .and_then(|v| v.get("http_tls_posture"))
        .expect("http tls posture");
    assert_eq!(
        posture.get("https_required").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        posture.get("allowlist_active").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        posture.get("min_tls_version").and_then(Value::as_str),
        Some("1.3")
    );
    assert_eq!(
        posture.get("follow_redirects").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        posture.get("max_redirects").and_then(Value::as_u64),
        Some(5)
    );
    assert_eq!(
        posture.get("ca_bundle").and_then(Value::as_str),
        Some("set")
    );
    assert_eq!(
        posture.get("client_cert").and_then(Value::as_str),
        Some("set")
    );
    assert_eq!(
        posture.get("client_key").and_then(Value::as_str),
        Some("set")
    );
}

#[test]
fn version_tq_json() {
    let repo = repo_root();
    let out = Command::new(repo.join("bin").join("cx"))
        .args(["version", "--json"])
        .current_dir(&repo)
        .output()
        .expect("run bin/cx version --json");

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("parse version json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("version.v1")
    );
    assert_eq!(
        payload
            .get("backend_capabilities")
            .and_then(|v| v.get("turboquant"))
            .and_then(|v| v.get("cx_runtime_support"))
            .and_then(Value::as_str),
        Some("none")
    );
    assert_eq!(payload.get("name").and_then(Value::as_str), Some("cxrs"));
}

#[test]
fn version_tls_posture() {
    let repo = repo_root();
    let out = Command::new(repo.join("bin").join("cx"))
        .args(["version", "--json"])
        .env("CX_PROVIDER_ADAPTER", "http-curl")
        .env("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer")
        .env("CX_HTTP_TLS_MIN_VERSION", "1.2")
        .env("CX_HTTP_FOLLOW_REDIRECTS", "0")
        .current_dir(&repo)
        .output()
        .expect("run bin/cx version --json with http tls");

    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("parse version json");
    let posture = payload
        .get("provider")
        .and_then(|v| v.get("http_tls_posture"))
        .expect("http tls posture");
    assert_eq!(
        posture.get("min_tls_version").and_then(Value::as_str),
        Some("1.2")
    );
    assert_eq!(
        posture.get("follow_redirects").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        posture.get("max_redirects").and_then(Value::as_u64),
        Some(0)
    );
}
