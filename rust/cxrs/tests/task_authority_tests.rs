mod common;

use common::*;
use serde_json::Value;
use std::process::{Command, Output};

fn fixture() -> TempRepo {
    let repo = TempRepo::new("cxrs-task-authority");
    repo.write_mock(
        "task-probe",
        "#!/bin/sh\nprintf invoked > command-marker\nprintf synthetic-output\n",
    );
    repo.write_mock(
        "docker",
        "#!/bin/sh\nprintf invoked > docker-marker\nexit 1\n",
    );
    repo.write_mock_primary(
        r#"#!/bin/sh
cat > provider-marker
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"task ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":2,"output_tokens":1}}'
"#,
    );
    repo
}

fn add(repo: &TempRepo, objective: &str, extra: &[&str]) -> String {
    let mut args = vec!["task", "add", objective];
    args.extend_from_slice(extra);
    let out = repo.run(&args);
    assert!(out.status.success(), "{}", stderr_str(&out));
    stdout_str(&out).trim().to_string()
}

fn run_absent(repo: &TempRepo, args: &[&str]) -> Output {
    // Exercise the missing process grant independently of trusted fixture setup.
    Command::new(env!("CARGO_BIN_EXE_cxrs"))
        .args(args)
        .current_dir(&repo.root)
        .env_clear()
        .env("HOME", &repo.home)
        .env("PATH", format!("{}:/usr/bin:/bin", repo.mock_bin.display()))
        .env("CX_LLM_BACKEND", "primary")
        .env("CX_TASK_SANDBOX_ENABLED", "0")
        .output()
        .expect("run without command authority")
}

fn no_sinks(repo: &TempRepo) {
    for marker in ["command-marker", "provider-marker", "docker-marker"] {
        assert!(
            !repo.root.join(marker).exists(),
            "unexpected sink: {marker}"
        );
    }
}

#[test]
fn blocked_formats() {
    for format in ["text", "json"] {
        let repo = fixture();
        let blocked = add(
            &repo,
            "cxo echo blocked",
            &[
                "--depends-on",
                "task_999",
                "--mode",
                "parallel",
                "--backend",
                "primary",
            ],
        );
        let out = repo.run(&[
            "task",
            "run-all",
            "--mode",
            "parallel",
            "--backend-pool",
            "primary",
            "--summary",
            format,
            "--text",
        ]);
        let stdout = stdout_str(&out);
        assert_eq!(
            out.status.code(),
            Some(1),
            "format={format} stdout={stdout}"
        );
        if format == "json" {
            // Text mode keeps the existing preflight lines ahead of --summary json.
            let json = stdout.find('{').expect("summary JSON envelope");
            let summary: Value = serde_json::from_str(&stdout[json..]).expect("summary JSON");
            assert_eq!(summary["contract_version"], "task-run-all-summary.v1");
            assert_eq!(summary["scheduled"], 1);
            assert_eq!(summary["failed"], 1);
            assert_eq!(summary["blocked"], 1);
            assert_eq!(summary["failed_task_ids"][0], blocked);
        } else {
            assert!(stdout.contains("run-all summary:"), "{stdout}");
            assert!(stdout.contains("failed=1, blocked=1"), "{stdout}");
            assert!(
                stdout.contains(&format!("run-all failed_task_ids: {blocked}")),
                "{stdout}"
            );
        }
        no_sinks(&repo);
    }
}

#[test]
fn absent_authority_denies() {
    for mode in ["--text", "--json"] {
        let repo = fixture();
        let id = add(&repo, "cx task-probe synthetic", &[]);
        let out = run_absent(&repo, &["task", "run", &id, mode]);
        assert!(!out.status.success(), "{}", stdout_str(&out));
        assert!(stderr_str(&out).contains("CX_TASK_TRUST_COMMANDS=1"));
        no_sinks(&repo);
        if mode == "--json" {
            let value: Value = serde_json::from_str(&stdout_str(&out)).unwrap();
            assert_eq!(value["contract_version"], "task-run.v1");
            assert_eq!(value["status"], "failed");
            assert!(value["execution_id"].is_null());
        }
        let show = repo.run(&["task", "show", &id]);
        let value: Value = serde_json::from_str(&stdout_str(&show)).unwrap();
        assert_eq!(value["status"], "failed");
    }
}

#[test]
fn alternate_paths_deny() {
    for objective in [
        "cx task-probe",
        "cxj task-probe",
        "cxo task-probe",
        "next task-probe",
        "fix task-probe",
        "fix-run task-probe",
        "commitjson",
        "diffsum",
        "\"cx\" task-probe",
        "cx task-probe '",
    ] {
        for mode in ["--text", "--json"] {
            let repo = fixture();
            let id = add(&repo, objective, &[]);
            for grant in ["", "0", "true", "yes", "01"] {
                let out = repo.run_with_env(
                    &["task", "run", &id, mode],
                    &[
                        ("CX_TASK_TRUST_COMMANDS", grant),
                        ("CX_TASK_TRUST_PROVIDER", "1"),
                        ("CX_TASK_SANDBOX_ACTIVE", "1"),
                    ],
                );
                assert!(!out.status.success(), "{objective}: {}", stdout_str(&out));
                assert!(stderr_str(&out).contains("CX_TASK_TRUST_COMMANDS=1"));
                no_sinks(&repo);
            }
        }
    }
}

#[test]
fn overrides_replicas_deny() {
    for mode in ["--text", "--json"] {
        let repo = fixture();
        let id = add(
            &repo,
            "cxo task-probe",
            &["--converge", "judge", "--replicas", "2"],
        );
        // Repository preference copies and a container handoff cannot grant authority.
        let out = repo.run(&["task", "sandbox", "set-image", "synthetic:local"]);
        assert!(out.status.success());
        assert!(repo.run(&["task", "sandbox", "enable"]).status.success());
        let mut state: Value =
            serde_json::from_slice(&std::fs::read(repo.state_file()).unwrap()).unwrap();
        state["preferences"]["task_trust_commands"] = Value::Bool(true);
        std::fs::write(repo.state_file(), serde_json::to_vec(&state).unwrap()).unwrap();
        let out = repo.run_with_env(
            &[
                "task",
                "run",
                &id,
                mode,
                "--mode",
                "lean",
                "--backend",
                "primary",
            ],
            &[
                ("CX_TASK_TRUST_COMMANDS", "0"),
                ("CX_TASK_TRUST_PROVIDER", "1"),
            ],
        );
        assert!(!out.status.success());
        assert!(stderr_str(&out).contains("CX_TASK_TRUST_COMMANDS=1"));
        no_sinks(&repo);
    }
}

#[test]
fn managed_worker_denies() {
    let repo = fixture();
    let id = add(&repo, "cx task-probe", &[]);
    let out = repo.run_with_env(
        &["task", "run", &id, "--managed-by-parent", "--json"],
        &[("CX_TASK_TRUST_COMMANDS", "0")],
    );
    assert!(!out.status.success());
    let value: Value = serde_json::from_str(&stdout_str(&out)).unwrap();
    assert_eq!(value["status"], "failed");
    assert_eq!(value["managed_by_parent"], true);
    no_sinks(&repo);
}

#[test]
fn run_all_denies() {
    for plan in ["sequential", "parallel"] {
        let repo = fixture();
        for _ in 0..2 {
            add(&repo, "cxo task-probe", &["--mode", plan]);
        }
        let out = repo.run_with_env(
            &[
                "task",
                "run-all",
                "--mode",
                plan,
                "--max-workers",
                "2",
                "--events-jsonl",
                "--json",
            ],
            &[("CX_TASK_TRUST_COMMANDS", "0")],
        );
        assert!(!out.status.success(), "{}", stdout_str(&out));
        let value: Value = serde_json::from_str(&stdout_str(&out)).unwrap();
        assert_eq!(value["contract_version"], "task-run-all.v1");
        assert_eq!(value["failed"], 2);
        let events = parse_jsonl(&repo.task_events_log());
        assert!(
            events
                .iter()
                .all(|row| row["contract_version"] == "task-events.v1")
        );
        assert_eq!(
            events.iter().filter(|row| row["event"] == "failed").count(),
            2
        );
        let summary = events.last().expect("persisted summary event");
        assert_eq!(summary["event"], "summary");
        assert_eq!(summary["failed"], 2);
        assert_eq!(summary["complete"], 0);
        no_sinks(&repo);
    }
}

#[test]
fn reviewed_commands_run() {
    for mode in ["--text", "--json"] {
        let repo = fixture();
        let id = add(&repo, "cxo task-probe synthetic", &[]);
        let out = repo.run_with_env(
            &["task", "run", &id, mode],
            &[
                ("CX_TASK_TRUST_COMMANDS", "1"),
                ("CX_TASK_SANDBOX_ENABLED", "0"),
            ],
        );
        assert!(out.status.success(), "{}", stderr_str(&out));
        assert!(repo.root.join("command-marker").exists());
        assert!(repo.root.join("provider-marker").exists());
        assert!(!repo.root.join("docker-marker").exists());
    }
}

#[test]
fn ordinary_prompts_run() {
    for objective in [
        "Describe task progress",
        "capture task-probe",
        "llm use ollama model",
    ] {
        for mode in ["--text", "--json"] {
            let repo = fixture();
            let id = add(&repo, objective, &[]);
            let out = run_absent(&repo, &["task", "run", &id, mode]);
            assert!(out.status.success(), "{}", stderr_str(&out));
            assert!(!repo.root.join("command-marker").exists());
            let prompt = std::fs::read_to_string(repo.root.join("provider-marker")).unwrap();
            assert!(prompt.contains(objective));
        }
    }
}

#[test]
fn direct_capture_runs() {
    let repo = fixture();
    let out = repo.run_with_env(
        &["capture", "task-probe", "synthetic"],
        &[("CX_TASK_TRUST_COMMANDS", "0")],
    );
    assert!(out.status.success(), "{}", stderr_str(&out));
    assert!(repo.root.join("command-marker").exists());
    assert!(!repo.root.join("provider-marker").exists());
}
