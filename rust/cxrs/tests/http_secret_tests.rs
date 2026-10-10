mod common;

use common::*;
use serde_json::{Value, json};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(not(unix))]
use std::path::Path;

const MOCK: &str = r#"#!/usr/bin/env python3
import json
import os
import pathlib
import stat
import sys
import time

args = sys.argv[1:]
config_path = pathlib.Path(args[args.index('--config') + 1])
record = {
    'args': args,
    'config': config_path.read_text(),
    'config_path': str(config_path),
    'config_mode': stat.S_IMODE(config_path.stat().st_mode),
    'config_links': config_path.stat().st_nlink,
    'pid': os.getpid(),
    'dir_mode': stat.S_IMODE(config_path.parent.stat().st_mode),
    'http_env': sorted(k for k in os.environ if k.startswith('CX_HTTP_')),
    'body': sys.stdin.read() if 'POST' in args else '',
}
pathlib.Path(os.environ['HTTP_CAPTURE']).write_text(json.dumps(record))
if os.environ.get('HTTP_MOCK_EXIT') == 'sleep':
    time.sleep(3)
if os.environ.get('HTTP_MOCK_EXIT') == 'nonzero':
    sys.exit(23)
if 'GET' in args:
    print('{"data":[{"id":"mock-model"}]}')
else:
    print('{"text":"mock-ok"}')
"#;

fn capture(repo: &TempRepo) -> Value {
    let raw = fs::read(repo.root.join("curl-capture.json")).expect("capture exists");
    serde_json::from_slice(&raw).expect("capture json")
}

fn assert_private(record: &Value, secrets: &[&str]) {
    let args = record["args"].as_array().expect("argv");
    assert_eq!(args.first().and_then(Value::as_str), Some("-q"));
    let args_text = serde_json::to_string(args).expect("argv json");
    for secret in secrets {
        assert!(!args_text.contains(secret), "secret in curl argv");
    }
    assert_eq!(record["http_env"], json!([]));
    let path = record["config_path"].as_str().expect("config path");
    #[cfg(unix)]
    {
        assert!(path.starts_with("/dev/fd/"), "named curl config: {path}");
        assert_eq!(record["config_mode"].as_u64(), Some(0o600));
        assert_eq!(record["config_links"].as_u64(), Some(0));
    }
    #[cfg(not(unix))]
    {
        assert!(!Path::new(path).exists(), "config survived curl completion");
        assert!(!Path::new(path).parent().expect("config parent").exists());
    }
}

#[test]
fn post_auth_privacy() {
    let repo = TempRepo::new("cxrs-http-secret");
    repo.write_mock("curl", MOCK);
    let capture_path = repo.root.join("curl-capture.json");
    let capture_path = capture_path.to_str().expect("capture path");
    let url = "https://api.example.test/infer?key=SYNTH_URL_91";
    let base = [
        ("CX_PROVIDER_ADAPTER", "http-curl"),
        ("CX_HTTP_PROVIDER_URL", url),
        ("HTTP_CAPTURE", capture_path),
        ("CX_HTTP_PROVIDER_TOKEN", ""),
        ("CX_HTTP_PROVIDER_TOKEN_FILE", ""),
        ("CX_HTTP_AUTH_PASSWORD", ""),
        ("CX_HTTP_AUTH_PASSWORD_FILE", ""),
        ("CX_HTTP_AUTH_VALUE", ""),
        ("CX_HTTP_AUTH_VALUE_FILE", ""),
    ];

    let mut envs = base.to_vec();
    envs.push(("CX_HTTP_PROVIDER_TOKEN", "SYNTH_BEARER_71"));
    let out = repo.run_with_env(&["cxo", "echo", "body-one"], &envs);
    assert!(out.status.success(), "{}", stderr_str(&out));
    let row = capture(&repo);
    assert!(
        row["config"]
            .as_str()
            .unwrap()
            .contains("Bearer SYNTH_BEARER_71")
    );
    assert!(row["config"].as_str().unwrap().contains("SYNTH_URL_91"));
    assert!(row["body"].as_str().unwrap().contains("body-one"));
    assert_private(&row, &["SYNTH_BEARER_71", "SYNTH_URL_91"]);

    let secret = repo.root.join("token.secret");
    fs::write(&secret, "SYNTH_FILE_72\n").expect("write token");
    #[cfg(unix)]
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o600)).expect("protect token");
    let mut envs = base.to_vec();
    envs.push(("CX_HTTP_PROVIDER_TOKEN_FILE", secret.to_str().unwrap()));
    let out = repo.run_with_env(&["cxo", "echo", "body-two"], &envs);
    assert!(out.status.success(), "{}", stderr_str(&out));
    let row = capture(&repo);
    assert!(
        row["config"]
            .as_str()
            .unwrap()
            .contains("Bearer SYNTH_FILE_72")
    );
    assert_private(&row, &["SYNTH_FILE_72", "SYNTH_URL_91"]);

    let mut envs = base.to_vec();
    envs.extend([
        ("CX_HTTP_AUTH_PROFILE", "basic"),
        ("CX_HTTP_AUTH_USERNAME", "synthetic-user"),
        ("CX_HTTP_AUTH_PASSWORD", "SYNTH_BASIC_73"),
    ]);
    let out = repo.run_with_env(&["cxo", "echo", "body-three"], &envs);
    assert!(out.status.success(), "{}", stderr_str(&out));
    let row = capture(&repo);
    let config = row["config"].as_str().unwrap();
    let encoded = config
        .split("Authorization: Basic ")
        .nth(1)
        .and_then(|tail| tail.split('"').next())
        .expect("encoded Basic credentials");
    assert_private(&row, &["SYNTH_BASIC_73", encoded, "SYNTH_URL_91"]);

    let mut envs = base.to_vec();
    envs.extend([
        ("CX_HTTP_AUTH_PROFILE", "header"),
        ("CX_HTTP_AUTH_HEADER", "X-API-Key"),
        ("CX_HTTP_AUTH_VALUE", "SYNTH_HEADER_74"),
    ]);
    let out = repo.run_with_env(&["cxo", "echo", "body-four"], &envs);
    assert!(out.status.success(), "{}", stderr_str(&out));
    let row = capture(&repo);
    assert!(
        row["config"]
            .as_str()
            .unwrap()
            .contains("X-API-Key: SYNTH_HEADER_74")
    );
    assert_private(&row, &["SYNTH_HEADER_74", "SYNTH_URL_91"]);
}

#[test]
fn tls_probe_privacy() {
    let repo = TempRepo::new("cxrs-http-secret");
    repo.write_mock("curl", MOCK);
    let capture_path = repo.root.join("curl-capture.json");
    let capture_path = capture_path.to_str().expect("capture path");
    let out = repo.run_with_env(
        &["cxo", "echo", "tls-body"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer"),
            ("CX_HTTP_CLIENT_CERT", "/tmp/client.pem:SYNTH_CERT_PASS_81"),
            ("CX_HTTP_CLIENT_KEY", "/tmp/client.key"),
            ("HTTP_CAPTURE", capture_path),
        ],
    );
    assert!(out.status.success(), "{}", stderr_str(&out));
    let row = capture(&repo);
    assert!(
        row["config"]
            .as_str()
            .unwrap()
            .contains("SYNTH_CERT_PASS_81")
    );
    assert_private(&row, &["SYNTH_CERT_PASS_81", "/tmp/client.key"]);

    let out = repo.run_with_env(
        &["llm", "resident", "probe-models", "--json"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_REQUEST_PROFILE", "openai_json"),
            ("CX_HTTP_PROVIDER_URL", "http://127.0.0.1:11434/infer"),
            ("CX_HTTP_PROVIDER_TOKEN", "SYNTH_PROBE_82"),
            ("HTTP_CAPTURE", capture_path),
        ],
    );
    assert!(out.status.success(), "{}", stderr_str(&out));
    let row = capture(&repo);
    assert!(
        row["config"]
            .as_str()
            .unwrap()
            .contains("Bearer SYNTH_PROBE_82")
    );
    assert!(row["config"].as_str().unwrap().contains("/v1/models"));
    assert_private(&row, &["SYNTH_PROBE_82"]);
}

#[test]
fn probe_auth_runtime() {
    let repo = TempRepo::new("cxrs-http-secret");
    let (url, captured, handle) =
        run_fixture_http_server_once(r#"{"data":[{"id":"synthetic-model"}]}"#);
    let out = repo.run_with_env(
        &["llm", "resident", "probe-models", "--json"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_REQUEST_PROFILE", "openai_json"),
            ("CX_HTTP_PROVIDER_URL", url.as_str()),
            ("CX_HTTP_PROVIDER_TOKEN", "SYNTH_PROBE_REAL_85"),
        ],
    );
    handle.join().expect("fixture join");
    assert!(out.status.success(), "{}", stderr_str(&out));
    let req = captured.lock().unwrap().clone().expect("captured request");
    assert_eq!(req.method, "GET");
    assert_eq!(req.path, "/v1/models");
    assert_eq!(
        req.authorization.as_deref(),
        Some("Bearer SYNTH_PROBE_REAL_85")
    );
}

#[test]
fn failure_cleanup() {
    let repo = TempRepo::new("cxrs-http-secret");
    repo.write_mock("curl", MOCK);
    let capture_path = repo.root.join("curl-capture.json");
    let capture_path = capture_path.to_str().expect("capture path");
    for mode in ["nonzero", "sleep"] {
        let out = repo.run_with_env(
            &["cxo", "echo", "failed-body"],
            &[
                ("CX_PROVIDER_ADAPTER", "http-curl"),
                ("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer"),
                ("CX_HTTP_PROVIDER_TOKEN", "SYNTH_FAIL_83"),
                ("HTTP_CAPTURE", capture_path),
                ("HTTP_MOCK_EXIT", mode),
                ("CX_CMD_TIMEOUT_SECS", "1"),
            ],
        );
        assert!(!out.status.success(), "mode={mode}");
        assert_private(&capture(&repo), &["SYNTH_FAIL_83"]);
    }
}

#[cfg(unix)]
#[test]
fn parent_abort_cleanup() {
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Duration;

    let repo = TempRepo::new("cxrs-http-secret");
    repo.write_mock("curl", MOCK);
    let capture_path = repo.root.join("curl-capture.json");
    let temp_root = repo.root.join("config-temp");
    fs::create_dir(&temp_root).expect("create temp root");
    let path = format!(
        "{}:{}",
        repo.mock_bin.display(),
        std::env::var("PATH").unwrap()
    );
    let mut parent = Command::new(env!("CARGO_BIN_EXE_cxrs"))
        .args(["cxo", "echo", "abrupt-parent"])
        .current_dir(&repo.root)
        .env("HOME", &repo.home)
        .env("PATH", path)
        .env("TMPDIR", &temp_root)
        .env("CX_PROVIDER_ADAPTER", "http-curl")
        .env("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer")
        .env("CX_HTTP_PROVIDER_TOKEN", "SYNTH_ABORT_86")
        .env("CX_CMD_TIMEOUT_SECS", "10")
        .env("HTTP_CAPTURE", &capture_path)
        .env("HTTP_MOCK_EXIT", "sleep")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn parent");
    for _ in 0..250 {
        if capture_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    if !capture_path.exists() {
        let _ = parent.kill();
        let _ = parent.wait();
        panic!("curl mock was not reached");
    }
    let row = capture(&repo);
    parent.kill().expect("kill parent");
    parent.wait().expect("reap parent");
    let curl_pid = row["pid"].as_i64().expect("mock pid");
    unsafe {
        libc::kill(curl_pid as libc::pid_t, libc::SIGTERM);
    }
    assert_private(&row, &["SYNTH_ABORT_86"]);
    assert_eq!(
        fs::read_dir(&temp_root).expect("read temp root").count(),
        0,
        "abrupt parent exit left a named config"
    );
}

#[cfg(unix)]
#[test]
fn closed_stdio_safe() {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    let repo = TempRepo::new("cxrs-http-secret");
    repo.write_mock("curl", MOCK);
    let capture_path = repo.root.join("curl-capture.json");
    let path = format!(
        "{}:{}",
        repo.mock_bin.display(),
        std::env::var("PATH").unwrap()
    );
    let mut parent = Command::new(env!("CARGO_BIN_EXE_cxrs"));
    parent
        .args(["cxo", "echo", "closed-stdio"])
        .current_dir(&repo.root)
        .env("HOME", &repo.home)
        .env("PATH", path)
        .env("CX_PROVIDER_ADAPTER", "http-curl")
        .env("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer")
        .env("CX_HTTP_PROVIDER_TOKEN", "SYNTH_STDIO_87")
        .env("HTTP_CAPTURE", &capture_path);
    unsafe {
        parent.pre_exec(|| {
            for fd in 0..=2 {
                libc::close(fd);
            }
            Ok(())
        });
    }
    let _ = parent.status().expect("run with closed stdio");
    let row = capture(&repo);
    let fd = row["config_path"]
        .as_str()
        .unwrap()
        .strip_prefix("/dev/fd/")
        .unwrap()
        .parse::<i32>()
        .unwrap();
    assert!(fd >= 3, "config descriptor reused stdio");
    assert_private(&row, &["SYNTH_STDIO_87"]);
}

#[test]
fn redirect_auth_kept() {
    let repo = TempRepo::new("cxrs-http-secret");
    repo.write_mock("curl", MOCK);
    let capture_path = repo.root.join("curl-capture.json");
    let out = repo.run_with_env(
        &["cxo", "echo", "redirect-body"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer"),
            ("CX_HTTP_PROVIDER_TOKEN", "SYNTH_REDIRECT_84"),
            ("CX_HTTP_FOLLOW_REDIRECTS", "1"),
            ("HTTP_CAPTURE", capture_path.to_str().unwrap()),
        ],
    );
    assert!(out.status.success(), "{}", stderr_str(&out));
    let row = capture(&repo);
    let args = row["args"].as_array().expect("curl args");
    assert!(args.iter().any(|arg| arg == "-L"));
    assert!(
        row["config"]
            .as_str()
            .unwrap()
            .contains("Bearer SYNTH_REDIRECT_84")
    );
    assert_private(&row, &["SYNTH_REDIRECT_84"]);
}

#[test]
fn control_rejects() {
    let repo = TempRepo::new("cxrs-http-secret");
    repo.write_mock("curl", MOCK);
    let capture_path = repo.root.join("curl-capture.json");
    let out = repo.run_with_env(
        &["cxo", "echo", "bad-header"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer"),
            ("CX_HTTP_AUTH_PROFILE", "header"),
            ("CX_HTTP_AUTH_HEADER", "X-API-Key"),
            ("CX_HTTP_AUTH_VALUE", "ok\nurl = malicious"),
            ("HTTP_CAPTURE", capture_path.to_str().unwrap()),
        ],
    );
    assert!(!out.status.success());
    assert!(stderr_str(&out).contains("control character"));
    assert!(!stderr_str(&out).contains("malicious"));
    assert!(!capture_path.exists());
}
