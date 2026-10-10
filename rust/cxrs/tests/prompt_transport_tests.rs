#![cfg(unix)]

mod common;

use common::{TempRepo, stderr_str, stdout_str};
use std::fs;
use std::process::Output;

const MOCK: &str = r#"#!/usr/bin/env python3
import json, os, pathlib, stat, sys
assert stat.S_ISREG(os.fstat(0).st_mode)
assert stat.S_IMODE(os.fstat(0).st_mode) == 0o600
assert os.fstat(0).st_nlink == 0
pathlib.Path(os.environ['PROMPT_ARGV_RECORD']).write_text(json.dumps(sys.argv[1:]))
pathlib.Path(os.environ['PROMPT_STDIN_RECORD']).write_bytes(sys.stdin.buffer.read())
print('MOCK_OK')
"#;

fn setup() -> TempRepo {
    let repo = TempRepo::new("local-prompt");
    repo.write_mock("llama-cli", MOCK);
    repo.write_mock("mlx-python", MOCK);
    repo
}

fn invoke(repo: &TempRepo, backend: &str, args: &[&str], extra: &[(&str, &str)]) -> Output {
    let argv_path = repo.root.join("provider-argv.json");
    let stdin_path = repo.root.join("provider-stdin.txt");
    let mlx_python = repo.mock_bin.join("mlx-python");
    let mlx_python = mlx_python.to_str().expect("mock path is UTF-8");
    let argv_path = argv_path.to_str().expect("record path is UTF-8");
    let stdin_path = stdin_path.to_str().expect("record path is UTF-8");
    let mut envs = vec![
        ("CX_LLM_BACKEND", backend),
        ("CX_LLAMA_CPP_MODEL", "/models/synthetic.gguf"),
        ("CX_MLX_MODEL", "synthetic/model"),
        ("CX_MLX_PYTHON", mlx_python),
        ("CXLOG_ENABLED", "0"),
        ("PROMPT_ARGV_RECORD", argv_path),
        ("PROMPT_STDIN_RECORD", stdin_path),
    ];
    envs.extend_from_slice(extra);
    repo.run_with_env(args, &envs)
}

fn recorded(repo: &TempRepo) -> (Vec<String>, String) {
    let argv: Vec<String> = serde_json::from_slice(
        &fs::read(repo.root.join("provider-argv.json")).expect("provider argv record"),
    )
    .expect("provider argv JSON");
    let stdin =
        fs::read_to_string(repo.root.join("provider-stdin.txt")).expect("provider stdin record");
    (argv, stdin)
}

#[test]
fn repo_prompt_stdin() {
    for backend in ["llamacpp", "mlx"] {
        let repo = setup();
        let marker = "SYNTHETIC_REPO_PROMPT_MARKER_89f27a";
        fs::write(
            repo.root.join("repository_prompt.txt"),
            format!("Owned repository content: {marker}\n"),
        )
        .unwrap();
        let out = invoke(
            &repo,
            backend,
            &["cxol", "/bin/cat", "repository_prompt.txt"],
            &[
                ("CX_LLAMA_CPP_ARGS", "-n 8 --temp 0"),
                ("CX_MLX_ARGS", "--temp 0.1 --seed 9"),
            ],
        );
        assert!(
            out.status.success(),
            "{backend}: stdout={} stderr={}",
            stdout_str(&out),
            stderr_str(&out)
        );
        assert!(stdout_str(&out).contains("MOCK_OK"));
        let (argv, stdin) = recorded(&repo);
        assert!(stdin.contains(marker), "{backend}: prompt not delivered");
        assert!(argv.iter().all(|arg| !arg.contains(marker)), "{argv:?}");
        if backend == "llamacpp" {
            assert!(argv.windows(2).any(|a| a == ["-f", "/dev/stdin"]));
            assert!(!argv.iter().any(|arg| arg == "-p"));
            assert!(argv.windows(2).any(|a| a == ["-n", "8"]));
            assert!(stdin.ends_with('\n'));
        } else {
            assert!(argv.windows(2).any(|a| a == ["--prompt", "-"]));
            assert!(argv.windows(2).any(|a| a == ["--seed", "9"]));
        }
    }
}

#[test]
fn prompt_byte_parity() {
    for backend in ["llamacpp", "mlx"] {
        let repo = setup();
        for prompt in ["", "línea\n", "two lines\n\n", "literal\\ntext\\tend"] {
            let out = invoke(&repo, backend, &["llm", "smoke", prompt], &[]);
            assert!(out.status.success(), "{backend}: {}", stderr_str(&out));
            assert!(stdout_str(&out).contains("MOCK_OK"));
            let (argv, stdin) = recorded(&repo);
            assert!(
                argv.iter()
                    .all(|arg| !arg.contains(prompt) || prompt.is_empty())
            );
            let delivered = if backend == "llamacpp" {
                // llama-cli -f removes exactly one trailing newline.
                stdin.strip_suffix('\n').expect("llama input terminator")
            } else {
                &stdin
            };
            let expected = if backend == "mlx" {
                prompt.replace("\\n", "\n").replace("\\t", "\t")
            } else {
                prompt.to_string()
            };
            assert_eq!(delivered, expected, "{backend} prompt parity");
        }
    }
}

#[test]
fn large_prompt_timeout() {
    for backend in ["llamacpp", "mlx"] {
        let repo = setup();
        let mock = if backend == "llamacpp" {
            "llama-cli"
        } else {
            "mlx-python"
        };
        repo.write_mock(mock, "#!/usr/bin/env bash\nsleep 10\n");
        fs::write(repo.root.join("large.txt"), "SYNTHETIC ".repeat(30_000)).unwrap();
        let out = invoke(
            &repo,
            backend,
            &["cxol", "/bin/cat", "large.txt"],
            &[("CX_CMD_TIMEOUT_SECS", "1")],
        );
        assert!(!out.status.success(), "{backend} unexpectedly succeeded");
        assert!(
            stderr_str(&out).contains("timed out"),
            "{}",
            stderr_str(&out)
        );
    }
}

#[test]
fn failure_prompt_secrecy() {
    for backend in ["llamacpp", "mlx"] {
        let repo = setup();
        let mock = if backend == "llamacpp" {
            "llama-cli"
        } else {
            "mlx-python"
        };
        repo.write_mock(
            mock,
            &MOCK.replace(
                "print('MOCK_OK')",
                "print('fixture failure', file=sys.stderr)\nsys.exit(7)",
            ),
        );
        let marker = "SYNTHETIC_FAILURE_PROMPT_MARKER_f4e921";
        fs::write(repo.root.join("prompt.txt"), marker).unwrap();
        let out = invoke(&repo, backend, &["cxol", "/bin/cat", "prompt.txt"], &[]);
        assert!(!out.status.success(), "{backend} unexpectedly succeeded");
        let error = stderr_str(&out);
        assert!(error.contains("fixture failure"), "{error}");
        assert!(!error.contains(marker), "prompt leaked in error: {error}");
        let (argv, stdin) = recorded(&repo);
        assert!(stdin.contains(marker));
        assert!(argv.iter().all(|arg| !arg.contains(marker)));
    }
}

#[test]
fn spool_failure_stops() {
    for backend in ["llamacpp", "mlx"] {
        let repo = setup();
        let missing = repo.root.join("missing-temp-dir");
        let missing = missing.to_str().expect("temp path is UTF-8");
        let marker = "SYNTHETIC_SPOOL_FAILURE_MARKER_746e12";
        let out = invoke(
            &repo,
            backend,
            &["llm", "smoke", marker],
            &[("TMPDIR", missing)],
        );
        assert!(!out.status.success(), "{backend} unexpectedly succeeded");
        let error = stderr_str(&out);
        assert!(error.contains("could not create private prompt"), "{error}");
        assert!(!error.contains(marker), "prompt leaked in error: {error}");
        assert!(!repo.root.join("provider-argv.json").exists());
    }
}
