mod common;

use common::*;

#[test]
fn seq_pool_denied() {
    let repo = TempRepo::new("cxrs-it");
    let marker = repo.root.join("primary-invoked");
    repo.write_mock_primary(&format!(
        "#!/bin/sh\nprintf invoked > '{}'\nexit 0\n",
        marker.display()
    ));
    let add = repo.run(&[
        "task",
        "add",
        "cxo echo synthetic",
        "--role",
        "implementer",
        "--backend",
        "auto",
        "--mode",
        "sequential",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));

    for format in ["--text", "--json"] {
        let out = repo.run_with_env(
            &[
                "task",
                "run-all",
                "--mode",
                "sequential",
                "--backend-pool",
                "ollama",
                format,
            ],
            &[("CX_DISABLE_OLLAMA", "1"), ("CX_DISABLE_CODEX", "1")],
        );
        assert_eq!(
            out.status.code(),
            Some(1),
            "stdout={} stderr={}",
            stdout_str(&out),
            stderr_str(&out)
        );
        assert!(stderr_str(&out).contains("no available backend from --backend-pool"));
        assert!(!marker.exists(), "disabled primary provider was invoked");
    }
    let tasks = read_json(&repo.tasks_file());
    assert_eq!(tasks[0]["status"], "pending");
}

#[test]
fn seq_pool_pinned() {
    let repo = TempRepo::new("cxrs-it");
    let marker = repo.root.join("primary-invoked");
    repo.write_mock_primary(&format!(
        "#!/bin/sh\nprintf invoked > '{}'\nexit 0\n",
        marker.display()
    ));
    repo.write_mock(
        "ollama",
        "#!/bin/sh\nif [ \"$1\" = list ]; then echo 'NAME ID SIZE MODIFIED'; exit 0; fi\nrm -- \"$0\"\nprintf 'ok\\n'\n",
    );
    for objective in ["cxo echo first", "cxo echo second"] {
        let add = repo.run(&[
            "task",
            "add",
            objective,
            "--role",
            "implementer",
            "--backend",
            "auto",
            "--mode",
            "sequential",
        ]);
        assert!(add.status.success(), "stderr={}", stderr_str(&add));
    }
    let out = repo.run_with_env(
        &[
            "task",
            "run-all",
            "--mode",
            "sequential",
            "--backend-pool",
            "ollama",
        ],
        &[("CX_DISABLE_CODEX", "1"), ("CX_OLLAMA_MODEL", "synthetic")],
    );
    assert!(
        !marker.exists(),
        "disabled primary provider was invoked; stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        !repo.mock_bin.join("ollama").exists(),
        "mock backend did not disappear"
    );
}
