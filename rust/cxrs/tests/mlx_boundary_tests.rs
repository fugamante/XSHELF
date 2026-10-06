#![cfg(unix)]
mod common;
use common::{TempRepo, stderr_str, stdout_str};
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};

fn fixture(preferred: &str) -> TempRepo {
    let repo = TempRepo::new("mlx-boundary");
    let added = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Fixture",
        "--preferred-args",
        preferred,
    ]);
    assert!(added.status.success(), "{}", stderr_str(&added));
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env python3
import json, os, pathlib, stat, sys
args = sys.argv[1:]
pathlib.Path(os.environ['MLX_RECORD']).write_text(json.dumps(args))
if '--out' not in args:
    print('OK', end='')
    sys.exit(0)
out = pathlib.Path(args[args.index('--out') + 1])
assert out.is_file() and not out.is_symlink()
assert stat.S_IMODE(out.stat().st_mode) == 0o600
assert stat.S_IMODE(out.parent.stat().st_mode) == 0o700
assert out.stat().st_size == 0
pathlib.Path(os.environ['MLX_PATH']).write_text(str(out))
mode = os.environ.get('MLX_MODE', 'success')
if mode == 'failure':
    print('fixture runner failure', file=sys.stderr)
    sys.exit(3)
if mode == 'invalid':
    out.write_text('not json')
else:
    out.write_text(json.dumps({'passes': 1, 'total': 1, 'runs': [
        {'prompt_tokens_per_sec': 100.0, 'decode_tokens_per_sec': 50.0,
         'wall_ms': 20, 'cache_nbytes': 1024, 'peak_memory_gb': 0.5}]}))
"#,
    );
    repo
}

fn invoke(repo: &TempRepo, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cxrs"));
    command
        .current_dir(&repo.root)
        .args(args)
        .env("HOME", &repo.home);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("CX_") {
            command.env_remove(name);
        }
    }
    for name in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"] {
        command.env_remove(name);
    }
    command.env("CX_MLX_PYTHON", repo.mock_bin.join("mlx-python"));
    command.env("MLX_RECORD", repo.root.join("arguments.json"));
    command.env("MLX_PATH", repo.root.join("output-path"));
    for (name, value) in envs {
        command.env(name, value);
    }
    command.output().expect("fixture command")
}

fn recorded(repo: &TempRepo) -> Vec<String> {
    serde_json::from_slice(&fs::read(repo.root.join("arguments.json")).unwrap()).unwrap()
}

#[test]
fn registry_defaults_excluded() {
    for trust in [None, Some("false"), Some(""), Some("invalid")] {
        let repo = fixture("--temp 0.7 --seed 11");
        let inspect = repo.run(&["llm", "models", "inspect", "tiny", "--json"]);
        let metadata: Value = serde_json::from_str(&stdout_str(&inspect)).unwrap();
        let id = metadata["id"].as_str().unwrap();
        assert_eq!(metadata["preferred_args"], "--temp 0.7 --seed 11");
        assert!(repo.run(&["llm", "use", "mlx", "tiny"]).status.success());
        for selector in [Some("tiny"), Some(id), None] {
            let mut envs = vec![("CX_LLM_BACKEND", "mlx"), ("CX_MLX_ARGS", "--temp 0.1")];
            if let Some(value) = trust {
                envs.push(("CX_MLX_TRUST_REGISTRY_ARGS", value));
            }
            if let Some(value) = selector {
                envs.push(("CX_MLX_MODEL", value));
            }
            let result = invoke(&repo, &["cxo", "echo", "fixture"], &envs);
            assert!(result.status.success(), "{}", stderr_str(&result));
            let args = recorded(&repo);
            assert!(
                !args.iter().any(|arg| arg == "0.7" || arg == "11"),
                "{args:?}"
            );
            assert_eq!(args[args.len() - 2..], ["--temp", "0.1"]);
        }
        let mut envs = vec![("CX_MLX_MODEL", "tiny")];
        if let Some(value) = trust {
            envs.push(("CX_MLX_TRUST_REGISTRY_ARGS", value));
        }
        let result = invoke(&repo, &["llm", "verify", "mlx", "--json"], &envs);
        assert!(result.status.success(), "{}", stderr_str(&result));
        assert!(!recorded(&repo).contains(&"0.7".to_string()));
        let value: Value = serde_json::from_str(&stdout_str(&result)).unwrap();
        assert_eq!(value["contract_version"], "llm-verify.v1");
        assert_eq!(
            value["result"]["model"]["preferred_args"],
            "--temp 0.7 --seed 11"
        );
    }
}

#[test]
fn registry_trust_precedence() {
    let repo = fixture("--temp 0.7 --seed 11");
    let result = invoke(
        &repo,
        &["llm", "verify", "mlx", "--json"],
        &[
            ("CX_MLX_MODEL", "tiny"),
            ("CX_MLX_TRUST_REGISTRY_ARGS", "true"),
            ("CX_MLX_ARGS", "--temp 0.1 --seed 9"),
        ],
    );
    assert!(result.status.success(), "{}", stderr_str(&result));
    let args = recorded(&repo);
    let registry = args.iter().position(|arg| arg == "0.7").unwrap();
    let explicit = args.iter().position(|arg| arg == "0.1").unwrap();
    assert!(registry < explicit);
    assert_eq!(args.last().unwrap(), "9");
    let malformed = fixture("'");
    let excluded = invoke(
        &malformed,
        &["llm", "verify", "mlx", "--json"],
        &[("CX_MLX_MODEL", "tiny")],
    );
    assert!(excluded.status.success(), "{}", stderr_str(&excluded));
    fs::remove_file(malformed.root.join("arguments.json")).unwrap();
    let included = invoke(
        &malformed,
        &["llm", "verify", "mlx", "--json"],
        &[
            ("CX_MLX_MODEL", "tiny"),
            ("CX_MLX_TRUST_REGISTRY_ARGS", "true"),
        ],
    );
    assert!(!included.status.success());
    assert!(stderr_str(&included).contains("parse registry preferred_args"));
    assert!(!malformed.root.join("arguments.json").exists());
}

#[test]
fn benchmark_script_required() {
    let repo = fixture("--temp 0.7");
    let args = ["llm", "verify", "mlx", "--profile", "benchmark", "--json"];
    for script in [None, Some(""), Some("   ")] {
        let mut envs = vec![("CX_MLX_MODEL", "tiny")];
        if let Some(value) = script {
            envs.push(("CX_MLX_VERIFY_SCRIPT", value));
        }
        let result = invoke(&repo, &args, &envs);
        assert!(!result.status.success());
        assert!(stderr_str(&result).contains("requires explicit CX_MLX_VERIFY_SCRIPT"));
        assert!(!repo.root.join("arguments.json").exists());
    }
}

#[test]
fn benchmark_output_private() {
    let repo = fixture("--temp 0.7");
    let temporary = tempfile::tempdir().unwrap();
    fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let temp_path = temporary.path().to_str().unwrap();
    let decoy = temporary.path().join("cxrs-mlx-verify-decoy-0.json");
    fs::write(&decoy, b"sentinel").unwrap();
    let script = repo.root.join("trusted-probe.py");
    fs::write(
        &script,
        b"# inert fixture; selected interpreter is a mock\n",
    )
    .unwrap();
    for mode in ["success", "failure", "invalid"] {
        for trust in [None, Some("false"), Some(""), Some("invalid"), Some("true")] {
            let mut envs = vec![
                ("CX_MLX_MODEL", "tiny"),
                ("CX_MLX_VERIFY_SCRIPT", script.to_str().unwrap()),
                ("TMPDIR", temp_path),
                ("TMP", temp_path),
                ("TEMP", temp_path),
                ("CX_MLX_ARGS", "--temp 0.1"),
                ("MLX_MODE", mode),
            ];
            if let Some(value) = trust {
                envs.push(("CX_MLX_TRUST_REGISTRY_ARGS", value));
            }
            let result = invoke(
                &repo,
                &["llm", "verify", "mlx", "--profile", "benchmark", "--json"],
                &envs,
            );
            assert_eq!(
                result.status.success(),
                mode == "success",
                "{}",
                stderr_str(&result)
            );
            let out_path = std::path::PathBuf::from(
                fs::read_to_string(repo.root.join("output-path")).unwrap(),
            );
            assert!(out_path.starts_with(temporary.path()));
            assert!(!out_path.exists());
            assert!(!out_path.parent().unwrap().exists());
            assert_eq!(fs::read(&decoy).unwrap(), b"sentinel");
            assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 1);
            let args = recorded(&repo);
            assert_eq!(
                args.contains(&"--preferred-args".to_string()),
                trust == Some("true")
            );
            assert_eq!(args[args.len() - 2..], ["--mlx-args", "--temp 0.1"]);
            if mode == "success" {
                let value: Value = serde_json::from_str(&stdout_str(&result)).unwrap();
                assert_eq!(value["contract_version"], "llm-verify.v1");
                assert_eq!(value["result"]["runtime"]["prompt_tps_mean"], 100.0);
                assert_eq!(value["result"]["memory"]["cache_metric_value"], 1024.0);
            }
        }
    }
    let launch = invoke(
        &repo,
        &["llm", "verify", "mlx", "--profile", "benchmark", "--json"],
        &[
            ("CX_MLX_MODEL", "tiny"),
            ("CX_MLX_VERIFY_SCRIPT", script.to_str().unwrap()),
            ("CX_MLX_PYTHON", "/fixture/missing-interpreter"),
            ("TMPDIR", temp_path),
            ("TMP", temp_path),
            ("TEMP", temp_path),
        ],
    );
    assert!(!launch.status.success());
    assert!(stderr_str(&launch).contains("benchmark runner failed"));
    assert_eq!(fs::read_dir(temporary.path()).unwrap().count(), 1);
    assert_eq!(fs::read(&decoy).unwrap(), b"sentinel");
}
