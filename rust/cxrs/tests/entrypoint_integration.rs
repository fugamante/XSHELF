use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn repo_root() -> PathBuf {
    let mut cur = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..6 {
        if cur.join(".codex").join("schemas").is_dir() && cur.join("bin").join("cx").is_file() {
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
