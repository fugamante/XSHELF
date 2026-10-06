mod common;

use common::*;
use serde_json::Value;

const IMAGE: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn fixture() -> TempRepo {
    let repo = TempRepo::new("cxrs-sandbox-authority");
    repo.write_mock(
        "docker",
        r#"#!/usr/bin/env python3
import json,os,sys
from pathlib import Path
a=sys.argv[1:]
with open('docker-calls','a') as f: f.write(json.dumps(a)+'\n')
if a[0]=='--version': print('Docker mock'); sys.exit(0)
if a[0]=='image':
 print('sha256:'+'a'*64); sys.exit(0)
if a[0]=='run':
 e={}
 for i,v in enumerate(a[:-1]):
  if v=='-e':
   k,sep,value=a[i+1].partition('='); e[k]=value if sep else os.environ.get(k)
 Path('container-env').write_text(json.dumps(e))
 sys.exit(0)
sys.exit(1)
"#,
    );
    repo.write_mock_primary("#!/bin/sh\nprintf invoked > provider-marker\nexit 0\n");
    repo
}

fn configure(repo: &TempRepo, enabled: bool) {
    assert!(
        repo.run(&["task", "sandbox", "set-image", "attacker/image:latest"])
            .status
            .success()
    );
    let command = if enabled { "enable" } else { "disable" };
    assert!(repo.run(&["task", "sandbox", command]).status.success());
}

fn add(repo: &TempRepo, extra: &[&str]) -> String {
    let mut args = vec!["task", "add", "Describe progress"];
    args.extend_from_slice(extra);
    let out = repo.run(&args);
    assert!(out.status.success());
    stdout_str(&out).trim().to_string()
}

fn no_sinks(repo: &TempRepo) {
    for path in [
        "docker-calls",
        "container-env",
        "provider-marker",
        "wrapper-marker",
    ] {
        assert!(!repo.root.join(path).exists(), "unexpected sink {path}");
    }
}

fn authorized<'a>(extra: &'a [(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut env = vec![
        ("CX_TASK_TRUST_SANDBOX", "1"),
        ("CX_TASK_SANDBOX_IMAGE", IMAGE),
    ];
    env.extend_from_slice(extra);
    env
}

#[test]
fn stored_settings_deny() {
    for mode in ["--text", "--json"] {
        for grant in ["", "0", "true", "yes", "01"] {
            let repo = fixture();
            configure(&repo, true);
            let id = add(&repo, &["--converge", "judge", "--replicas", "2"]);
            let out = repo.run_with_env(
                &["task", "run", &id, mode],
                &[
                    ("CX_TASK_TRUST_SANDBOX", grant),
                    ("CX_TASK_SANDBOX_IMAGE", ""),
                    ("CX_TASK_TRUST_PROVIDER", "1"),
                    ("CX_TASK_SANDBOX_ACTIVE", "1"),
                    ("CX_UNSAFE", "1"),
                    ("CXFIX_FORCE", "1"),
                    ("OPENAI_API_KEY", "SYNTHETIC"),
                ],
            );
            assert!(!out.status.success());
            assert!(stderr_str(&out).contains("CX_TASK_TRUST_SANDBOX"));
            if mode == "--json" {
                let v: Value = serde_json::from_str(&stdout_str(&out)).unwrap();
                assert_eq!(v["contract_version"], "task-run.v1");
                assert_eq!(v["status"], "failed");
                assert!(v["execution_id"].is_null());
            }
            no_sinks(&repo);
        }
    }
}

#[test]
fn readiness_admission_denies() {
    for enabled in [false, true] {
        for mode in ["--text", "--json"] {
            let repo = fixture();
            configure(&repo, enabled);
            let out = repo.run_with_env(
                &["task", "sandbox", "check", mode],
                &[
                    ("CX_TASK_TRUST_SANDBOX", "0"),
                    ("CX_TASK_SANDBOX_IMAGE", ""),
                ],
            );
            assert!(!out.status.success());
            no_sinks(&repo);
            if mode == "--json" {
                let v: Value = serde_json::from_str(&stdout_str(&out)).unwrap();
                assert_eq!(v["contract_version"], "task-sandbox-readiness.v1");
                assert_eq!(v["ready"], false);
                assert_eq!(v["entrypoint_available"], false);
            }
        }
    }
    let repo = fixture();
    configure(&repo, false);
    assert!(
        !repo
            .run_with_env(&["task", "sandbox", "check", "--json"], &authorized(&[]))
            .status
            .success()
    );
    no_sinks(&repo);
}

#[test]
fn malformed_images_deny() {
    for image in [
        "",
        "attacker/image:latest",
        "--privileged",
        "--entrypoint=/bin/sh",
        "sha256:abc",
        " sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        for probe in [false, true] {
            let repo = fixture();
            configure(&repo, true);
            let id = add(&repo, &[]);
            let args = if probe {
                vec!["task", "sandbox", "check", "--json"]
            } else {
                vec!["task", "run", &id, "--json"]
            };
            let out = repo.run_with_env(
                &args,
                &[
                    ("CX_TASK_TRUST_SANDBOX", "1"),
                    ("CX_TASK_SANDBOX_IMAGE", image),
                ],
            );
            assert!(!out.status.success(), "{image}");
            no_sinks(&repo);
        }
    }
}

#[test]
fn shared_names_deny() {
    for selection in [
        "OPENAI_API_KEY,",
        "*",
        "CX_TASK_TRUST_COMMANDS",
        "CX_TASK_TRUST_PROVIDER",
        "CX_TASK_SANDBOX_ACTIVE",
        "CX_TASK_SANDBOX_IMAGE",
        "CX_EXECUTION_LANE",
        "CX_REPO_ROOT",
        "PATH",
        "HOME",
        "BASH_ENV",
        "ENV",
        "LD_PRELOAD",
        "DYLD_INSERT_LIBRARIES",
        "CXFIX_FORCE",
    ] {
        for probe in [false, true] {
            let repo = fixture();
            configure(&repo, true);
            let id = add(&repo, &[]);
            let args = if probe {
                vec!["task", "sandbox", "check", "--json"]
            } else {
                vec!["task", "run", &id, "--json"]
            };
            let out = repo.run_with_env(
                &args,
                &authorized(&[("CX_TASK_SANDBOX_SHARE_ENV", selection)]),
            );
            assert!(!out.status.success(), "{selection}");
            no_sinks(&repo);
        }
    }
}

#[test]
fn missing_image_denies() {
    for output in [
        "",
        "sha256:bad",
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    ] {
        let repo = fixture();
        configure(&repo, true);
        repo.write_mock(
            "docker",
            &format!(
                "#!/bin/sh\nprintf '%s' '{}' > inspect-marker\nprintf '%s' '{}'\n",
                "called", output
            ),
        );
        let id = add(&repo, &["--converge", "judge", "--replicas", "2"]);
        let out = repo.run_with_env(&["task", "run", &id, "--json"], &authorized(&[]));
        assert!(!out.status.success());
        assert!(stderr_str(&out).contains("no image will be pulled"));
        assert!(repo.root.join("inspect-marker").exists());
        assert!(!repo.root.join("provider-marker").exists());
    }
}

#[test]
fn authorized_environment_runs() {
    for sharing in ["", "OPENAI_API_KEY,HTTP_PROXY"] {
        let repo = fixture();
        configure(&repo, true);
        let id = add(&repo, &[]);
        let out = repo.run_with_env(
            &["task", "run", &id, "--json"],
            &authorized(&[
                ("CX_TASK_SANDBOX_SHARE_ENV", sharing),
                ("OPENAI_API_KEY", "SYNTHETIC_API"),
                ("HTTP_PROXY", "SYNTHETIC_PROXY"),
                ("CX_REPO_TOKEN", "SYNTHETIC_REPO"),
                ("OLLAMA_TOKEN", "SYNTHETIC_OLLAMA"),
                ("CX_UNSAFE", "01"),
                ("CXFIX_FORCE", "+1"),
                ("CXFIX_RUN", "1"),
                ("CX_TASK_TRUST_PROVIDER", "1"),
            ]),
        );
        assert!(out.status.success(), "{}", stderr_str(&out));
        let e = read_json(&repo.root.join("container-env"));
        assert_eq!(e["CX_TASK_TRUST_COMMANDS"], "1");
        assert_eq!(e["CX_TASK_TRUST_PROVIDER"], "1");
        assert_eq!(e["CX_UNSAFE"], "1");
        assert_eq!(e["CXFIX_FORCE"], "1");
        assert_eq!(e["CXFIX_RUN"], "1");
        assert_eq!(e["CX_TASK_SANDBOX_ACTIVE"], "1");
        assert_eq!(e["CX_TASK_SANDBOX_IMAGE"], IMAGE);
        assert_eq!(e["BASH_ENV"], "/dev/null");
        assert_eq!(e["ENV"], "/dev/null");
        assert!(e.get("CX_REPO_TOKEN").is_none());
        assert!(e.get("OLLAMA_TOKEN").is_none());
        if sharing.is_empty() {
            assert!(e.get("OPENAI_API_KEY").is_none());
            assert!(e.get("HTTP_PROXY").is_none());
        } else {
            assert_eq!(e["OPENAI_API_KEY"], "SYNTHETIC_API");
            assert_eq!(e["HTTP_PROXY"], "SYNTHETIC_PROXY");
        }
        let calls = parse_jsonl(&repo.root.join("docker-calls"));
        let args = calls.last().unwrap().as_array().unwrap();
        for option in [
            "--pull=never",
            "--entrypoint=/bin/bash",
            "--no-healthcheck",
            "--noprofile",
            "--norc",
        ] {
            assert!(args.iter().any(|v| v == option));
        }
        let serialized = serde_json::to_string(&calls).unwrap();
        assert!(!serialized.contains("SYNTHETIC_"));
        assert!(!serialized.contains("./bin/xshelf"));
        assert!(!serialized.contains("./bin/cx"));
    }
}

#[test]
fn readiness_is_restricted() {
    let repo = fixture();
    configure(&repo, true);
    let out = repo.run_with_env(
        &["task", "sandbox", "check", "--json"],
        &authorized(&[
            ("CX_TASK_TRUST_REPO_EXEC", "1"),
            ("CX_TASK_SANDBOX_SHARE_ENV", "OPENAI_API_KEY"),
            ("OPENAI_API_KEY", "SYNTHETIC_API"),
        ]),
    );
    assert!(out.status.success(), "{}", stdout_str(&out));
    let e = read_json(&repo.root.join("container-env"));
    assert!(e.get("OPENAI_API_KEY").is_none());
    let calls = parse_jsonl(&repo.root.join("docker-calls"));
    let args = calls.last().unwrap().as_array().unwrap();
    for option in [
        "--pull=never",
        "--entrypoint=/bin/bash",
        "--network=none",
        "--read-only",
    ] {
        assert!(args.iter().any(|v| v == option));
    }
    assert!(args.iter().any(|v| {
        v.as_str()
            .is_some_and(|s| s.contains("target=/work,readonly"))
    }));
    assert!(args.last().unwrap().as_str().unwrap().contains("./bin/cx"));
}

#[test]
fn invalid_executable_denies() {
    for path in [
        "",
        "relative/app",
        "//work/bin/xshelf",
        "/work/bin/xshelf",
        "/work/../bin/xshelf",
        "/usr/../work/bin/cx",
        "/bin/app\n",
    ] {
        let repo = fixture();
        configure(&repo, true);
        let id = add(&repo, &[]);
        let out = repo.run_with_env(
            &["task", "run", &id],
            &authorized(&[("CX_TASK_SANDBOX_EXECUTABLE", path)]),
        );
        assert!(!out.status.success());
        no_sinks(&repo);
    }
}

#[test]
fn parallel_denial_accounts() {
    let repo = fixture();
    configure(&repo, true);
    for _ in 0..2 {
        add(&repo, &["--mode", "parallel"]);
    }
    let out = repo.run_with_env(
        &[
            "task",
            "run-all",
            "--mode",
            "parallel",
            "--max-workers",
            "2",
            "--events-jsonl",
            "--json",
        ],
        &[
            ("CX_TASK_TRUST_SANDBOX", "0"),
            ("CX_TASK_SANDBOX_IMAGE", ""),
        ],
    );
    assert!(!out.status.success());
    let v: Value = serde_json::from_str(&stdout_str(&out)).unwrap();
    assert_eq!(v["failed"], 2);
    assert_eq!(v["contract_version"], "task-run-all.v1");
    let events = parse_jsonl(&repo.task_events_log());
    assert_eq!(events.iter().filter(|v| v["event"] == "failed").count(), 2);
    no_sinks(&repo);
}
