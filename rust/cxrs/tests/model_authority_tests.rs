#![cfg(unix)]

mod common;

use common::{TempRepo, stderr_str, stdout_str};
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn mock_ollama(repo: &TempRepo) {
    repo.write_mock(
        "ollama",
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$MOCK_TRACE\"\ncat >/dev/null\nprintf '%s\\n' synthetic-response\n",
    );
}

#[test]
fn repo_state_rejected() {
    let repo = TempRepo::new("cxrs-model-authority");
    mock_ollama(&repo);
    fs::create_dir_all(repo.state_file().parent().unwrap()).unwrap();
    fs::write(
        repo.state_file(),
        json!({"preferences": {"llm_backend": "ollama", "ollama_model": "repo-model"}}).to_string(),
    )
    .unwrap();
    let trace = repo.root.join("ollama-trace");
    let trace_str = trace.to_str().unwrap();
    let blocked = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[("MOCK_TRACE", trace_str)],
    );
    assert!(!blocked.status.success(), "{}", stdout_str(&blocked));
    assert!(stderr_str(&blocked).contains("not approved"));
    assert!(!trace.exists(), "unapproved repo preference reached Ollama");

    let explicit = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[
            ("MOCK_TRACE", trace_str),
            ("CX_LLM_BACKEND", "ollama"),
            ("CX_OLLAMA_MODEL", "explicit-model"),
        ],
    );
    assert!(
        explicit.status.success(),
        "stdout={} stderr={}",
        stdout_str(&explicit),
        stderr_str(&explicit)
    );
    assert_eq!(fs::read_to_string(&trace).unwrap(), "run explicit-model\n");

    fs::remove_file(&trace).unwrap();
    let selected = repo.run(&["llm", "use", "ollama", "approved-model"]);
    assert!(selected.status.success(), "{}", stderr_str(&selected));
    let approved = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[("MOCK_TRACE", trace_str)],
    );
    assert!(approved.status.success(), "{}", stderr_str(&approved));
    assert_eq!(fs::read_to_string(&trace).unwrap(), "run approved-model\n");

    let broker = repo.run(&["broker", "set", "--policy", "quality"]);
    assert!(broker.status.success(), "{}", stderr_str(&broker));
    let show = repo.run(&["broker", "show", "--json"]);
    assert!(show.status.success(), "{}", stderr_str(&show));
    let payload: serde_json::Value = serde_json::from_str(&stdout_str(&show)).unwrap();
    assert_eq!(payload["broker_policy"], "quality");

    fs::write(
        repo.state_file(),
        json!({"preferences": {"llm_backend": "ollama", "ollama_model": "swapped-model"}})
            .to_string(),
    )
    .unwrap();
    fs::remove_file(&trace).unwrap();
    let changed = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[("MOCK_TRACE", trace_str)],
    );
    assert!(!changed.status.success());
    assert!(stderr_str(&changed).contains("not approved"));
    assert!(!trace.exists(), "changed repo preference reached Ollama");

    let outside = repo.home.join("outside-state.json");
    fs::write(&outside, "synthetic-private-state").unwrap();
    fs::remove_file(repo.state_file()).unwrap();
    std::os::unix::fs::symlink(&outside, repo.state_file()).unwrap();
    let explicit = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[
            ("MOCK_TRACE", trace_str),
            ("CX_LLM_BACKEND", "ollama"),
            ("CX_OLLAMA_MODEL", "explicit-model"),
        ],
    );
    assert!(explicit.status.success(), "{}", stderr_str(&explicit));
    assert_eq!(fs::read_to_string(&trace).unwrap(), "run explicit-model\n");
    assert_eq!(
        fs::read_to_string(outside).unwrap(),
        "synthetic-private-state"
    );
}

#[test]
fn repo_alias_guarded() {
    let repo = TempRepo::new("cxrs-model-alias");
    mock_ollama(&repo);
    let registry = repo.local_models_file();
    fs::create_dir_all(registry.parent().unwrap()).unwrap();
    fs::write(
        &registry,
        json!({"contract_version":"local_models.v1","models":[{
            "id":"ollama:chosen", "alias":"chosen", "backend":"ollama",
            "resolved_model":"repo-selected"
        }]})
        .to_string(),
    )
    .unwrap();
    let trace = repo.root.join("ollama-trace");
    let trace_str = trace.to_str().unwrap();
    let selected = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[
            ("MOCK_TRACE", trace_str),
            ("CX_LLM_BACKEND", "ollama"),
            ("CX_OLLAMA_MODEL", "chosen"),
        ],
    );
    assert!(selected.status.success(), "{}", stderr_str(&selected));
    assert_eq!(fs::read_to_string(&trace).unwrap(), "run chosen\n");

    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "chosen",
        "--backend",
        "ollama",
        "--model",
        "approved-target",
        "--replace",
    ]);
    assert!(add.status.success(), "{}", stderr_str(&add));
    let approved = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[
            ("MOCK_TRACE", trace_str),
            ("CX_LLM_BACKEND", "ollama"),
            ("CX_OLLAMA_MODEL", "chosen"),
        ],
    );
    assert!(approved.status.success(), "{}", stderr_str(&approved));
    assert_eq!(
        fs::read_to_string(&trace).unwrap(),
        "run chosen\nrun approved-target\n"
    );

    let use_alias = repo.run(&["llm", "use", "ollama", "chosen"]);
    assert!(use_alias.status.success(), "{}", stderr_str(&use_alias));
    fs::remove_file(&trace).unwrap();
    let persisted = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[("MOCK_TRACE", trace_str)],
    );
    assert!(persisted.status.success(), "{}", stderr_str(&persisted));
    assert_eq!(fs::read_to_string(&trace).unwrap(), "run approved-target\n");

    fs::write(
        &registry,
        json!({"contract_version":"local_models.v1","models":[{
            "id":"ollama:chosen", "alias":"chosen", "backend":"ollama",
            "resolved_model":"swapped-target"
        }]})
        .to_string(),
    )
    .unwrap();
    fs::remove_file(&trace).unwrap();
    let changed = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[
            ("MOCK_TRACE", trace_str),
            ("CX_LLM_BACKEND", "ollama"),
            ("CX_OLLAMA_MODEL", "chosen"),
        ],
    );
    assert!(changed.status.success(), "{}", stderr_str(&changed));
    assert_eq!(fs::read_to_string(&trace).unwrap(), "run chosen\n");

    fs::write(&registry, "{malformed").unwrap();
    fs::remove_file(&trace).unwrap();
    let malformed = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[
            ("MOCK_TRACE", trace_str),
            ("CX_LLM_BACKEND", "ollama"),
            ("CX_OLLAMA_MODEL", "chosen"),
        ],
    );
    assert!(malformed.status.success(), "{}", stderr_str(&malformed));
    assert_eq!(fs::read_to_string(&trace).unwrap(), "run chosen\n");
}

#[test]
fn receipt_symlink_rejected() {
    let repo = TempRepo::new("cxrs-model-receipt");
    let registry = repo.local_models_file();
    fs::create_dir_all(registry.parent().unwrap()).unwrap();
    fs::write(
        &registry,
        json!({"contract_version":"local_models.v1","models":[{
            "id":"ollama:bad", "alias":"bad", "backend":"ollama",
            "resolved_model":"repo-selected"
        }]})
        .to_string(),
    )
    .unwrap();
    let rejected = repo.run(&["llm", "use", "ollama", "bad"]);
    assert!(!rejected.status.success());
    assert!(stderr_str(&rejected).contains("not approved"));
    assert!(
        !repo.state_file().exists(),
        "failed alias changed persisted backend"
    );

    let authority = repo.home.join(".cx/model_authority");
    fs::create_dir_all(authority.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&repo.root, &authority).unwrap();
    let blocked = repo.run(&["llm", "use", "ollama", "literal-model"]);
    assert!(!blocked.status.success());
    assert!(stderr_str(&blocked).contains("model authority"));
    assert_eq!(
        fs::read_dir(&repo.root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| { e.file_name().to_string_lossy().ends_with(".json") })
            .count(),
        0,
        "symlinked receipt wrote into repository"
    );
    fs::remove_file(authority).unwrap();
    mock_ollama(&repo);
    let trace = repo.root.join("ollama-trace");
    let blocked = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[("MOCK_TRACE", trace.to_str().unwrap())],
    );
    assert!(!blocked.status.success());
    assert!(stderr_str(&blocked).contains("not approved"));
    assert!(!trace.exists(), "failed receipt write reached provider");
}

#[test]
fn home_receipt_guarded() {
    let repo = TempRepo::new("cxrs-home-receipt");
    mock_ollama(&repo);
    let selected = repo.run(&["llm", "use", "ollama", "approved-model"]);
    assert!(selected.status.success(), "{}", stderr_str(&selected));
    let authority = repo.home.join(".cx/model_authority");
    let receipt = fs::read_dir(&authority)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "json"))
        .unwrap();
    let trace = repo.root.join("ollama-trace");
    let env = [("MOCK_TRACE", trace.to_str().unwrap())];

    fs::write(&receipt, vec![b'a'; 1024 * 1024 + 1]).unwrap();
    let oversized = repo.run_with_env(&["llm", "smoke", "synthetic prompt"], &env);
    assert!(!oversized.status.success());
    assert!(stderr_str(&oversized).contains("size limit"));
    assert!(!trace.exists());

    fs::remove_file(&receipt).unwrap();
    let outside = repo.home.join("outside-receipt");
    fs::write(&outside, "synthetic-private-receipt").unwrap();
    std::os::unix::fs::symlink(&outside, &receipt).unwrap();
    let linked = repo.run_with_env(&["llm", "smoke", "synthetic prompt"], &env);
    assert!(!linked.status.success());
    assert!(stderr_str(&linked).contains("open receipt"));
    assert!(!trace.exists());
    assert_eq!(
        fs::read_to_string(outside).unwrap(),
        "synthetic-private-receipt"
    );

    let mut perms = fs::metadata(&repo.home).unwrap().permissions();
    perms.set_mode(0o777);
    fs::set_permissions(&repo.home, perms).unwrap();
    let writable = repo.run_with_env(&["llm", "smoke", "synthetic prompt"], &env);
    assert!(!writable.status.success());
    assert!(stderr_str(&writable).contains("HOME has unsafe"));
    assert!(!trace.exists());
}

#[test]
fn primary_home_optional() {
    let repo = TempRepo::new("cxrs-primary-home");
    repo.write_mock(
        "codex",
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$MOCK_TRACE\"\ncat >/dev/null\nprintf 'synthetic-primary-response'\n",
    );
    let mut perms = fs::metadata(&repo.home).unwrap().permissions();
    perms.set_mode(0o777);
    fs::set_permissions(&repo.home, perms).unwrap();
    let trace = repo.root.join("primary-trace");
    let env = [("MOCK_TRACE", trace.to_str().unwrap())];
    let allowed = repo.run_with_env(&["llm", "smoke", "synthetic prompt"], &env);
    assert!(allowed.status.success(), "{}", stderr_str(&allowed));
    assert!(fs::read_to_string(&trace).unwrap().contains("exec"));

    fs::write(
        repo.state_file(),
        json!({"preferences":{"ollama_model":"unused-local-model"}}).to_string(),
    )
    .unwrap();
    fs::remove_file(&trace).unwrap();
    let unused = repo.run_with_env(&["llm", "smoke", "synthetic prompt"], &env);
    assert!(unused.status.success(), "{}", stderr_str(&unused));
    assert!(fs::read_to_string(&trace).unwrap().contains("exec"));

    let mut perms = fs::metadata(&repo.home).unwrap().permissions();
    perms.set_mode(0o700);
    fs::set_permissions(&repo.home, perms).unwrap();
    let selected = repo.run(&["llm", "use", "primary"]);
    assert!(selected.status.success(), "{}", stderr_str(&selected));
    let mut perms = fs::metadata(&repo.home).unwrap().permissions();
    perms.set_mode(0o777);
    fs::set_permissions(&repo.home, perms).unwrap();
    fs::remove_file(&trace).unwrap();
    let persisted = repo.run_with_env(&["llm", "smoke", "synthetic prompt"], &env);
    assert!(persisted.status.success(), "{}", stderr_str(&persisted));
    assert!(fs::read_to_string(&trace).unwrap().contains("exec"));

    fs::write(
        repo.state_file(),
        json!({"preferences":{"llm_backend":"ollama","ollama_model":"repo-model"}}).to_string(),
    )
    .unwrap();
    fs::remove_file(&trace).unwrap();
    let blocked = repo.run_with_env(&["llm", "smoke", "synthetic prompt"], &env);
    assert!(!blocked.status.success());
    assert!(stderr_str(&blocked).contains("HOME has unsafe"));
    assert!(!trace.exists());
}

#[test]
fn verify_state_guarded() {
    let repo = TempRepo::new("cxrs-model-verify");
    let python = repo.mock_bin.join("mlx-python");
    repo.write_mock(
        "mlx-python",
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$MOCK_TRACE\"\nprintf 'OK'\n",
    );
    let selected = repo.run(&["llm", "use", "mlx", "approved-model"]);
    assert!(selected.status.success(), "{}", stderr_str(&selected));
    fs::write(
        repo.state_file(),
        json!({"preferences":{"llm_backend":"mlx","mlx_model":"swapped-model"}}).to_string(),
    )
    .unwrap();
    let trace = repo.root.join("mlx-trace");
    let env = [
        ("CX_MLX_PYTHON", python.to_str().unwrap()),
        ("MOCK_TRACE", trace.to_str().unwrap()),
    ];
    let blocked = repo.run_with_env(&["llm", "verify", "mlx", "--json"], &env);
    assert!(!blocked.status.success());
    assert!(stderr_str(&blocked).contains("not approved"));
    assert!(!trace.exists(), "changed repo model reached MLX");

    let explicit = repo.run_with_env(
        &["llm", "verify", "mlx", "--json"],
        &[env[0], env[1], ("CX_MLX_MODEL", "explicit-model")],
    );
    assert!(explicit.status.success(), "{}", stderr_str(&explicit));
    assert!(
        fs::read_to_string(trace)
            .unwrap()
            .contains("explicit-model")
    );
}

#[test]
fn parent_symlink_rejected() {
    let repo = TempRepo::new("cxrs-model-state-link");
    let outside = repo.root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::remove_dir_all(repo.root.join(".cx")).unwrap();
    std::os::unix::fs::symlink(&outside, repo.root.join(".cx")).unwrap();
    let out = repo.run(&["llm", "use", "ollama", "literal-model"]);
    assert!(!out.status.success());
    assert!(stderr_str(&out).contains("JSON parent"));
    assert!(!outside.join("state.json").exists());
}

#[test]
fn state_leaf_rejected() {
    let repo = TempRepo::new("cxrs-state-leaf");
    let private = repo.home.join("private-state.json");
    let contents =
        json!({"preferences":{"llm_backend":"ollama"},"private_marker":"synthetic-secret"})
            .to_string();
    fs::write(&private, &contents).unwrap();
    std::os::unix::fs::symlink(&private, repo.state_file()).unwrap();
    let out = repo.run(&["llm", "use", "ollama", "literal-model"]);
    assert!(!out.status.success());
    assert!(stderr_str(&out).contains("cannot open JSON file"));
    assert!(
        fs::symlink_metadata(repo.state_file())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(private).unwrap(), contents);
}

#[test]
fn registry_leaf_rejected() {
    let repo = TempRepo::new("cxrs-registry-leaf");
    let private = repo.home.join("private-models.json");
    let contents = json!({"contract_version":"local_models.v1","models":[],"private_marker":"synthetic-secret"}).to_string();
    fs::write(&private, &contents).unwrap();
    std::os::unix::fs::symlink(&private, repo.local_models_file()).unwrap();
    let out = repo.run(&[
        "llm",
        "models",
        "add",
        "chosen",
        "--backend",
        "ollama",
        "--model",
        "literal-model",
    ]);
    assert!(!out.status.success());
    assert!(stderr_str(&out).contains("cannot open JSON file"));
    assert!(
        fs::symlink_metadata(repo.local_models_file())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(private).unwrap(), contents);
}

#[test]
fn failed_revoke_safe() {
    let repo = TempRepo::new("cxrs-model-revoke");
    mock_ollama(&repo);
    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "chosen",
        "--backend",
        "ollama",
        "--model",
        "approved-target",
    ]);
    assert!(add.status.success(), "{}", stderr_str(&add));
    let registry = repo.local_models_file();
    let original = fs::read_to_string(&registry).unwrap();

    let mut perms = fs::metadata(&repo.home).unwrap().permissions();
    perms.set_mode(0o777);
    fs::set_permissions(&repo.home, perms).unwrap();
    let failed = repo.run(&["llm", "models", "remove", "chosen"]);
    assert!(!failed.status.success());
    assert!(stderr_str(&failed).contains("HOME has unsafe"));
    assert_eq!(fs::read_to_string(&registry).unwrap(), original);

    let mut perms = fs::metadata(&repo.home).unwrap().permissions();
    perms.set_mode(0o700);
    fs::set_permissions(&repo.home, perms).unwrap();
    let removed = repo.run(&["llm", "models", "remove", "chosen"]);
    assert!(removed.status.success(), "{}", stderr_str(&removed));
    fs::write(&registry, original).unwrap();
    let trace = repo.root.join("ollama-trace");
    let run = repo.run_with_env(
        &["llm", "smoke", "synthetic prompt"],
        &[
            ("MOCK_TRACE", trace.to_str().unwrap()),
            ("CX_LLM_BACKEND", "ollama"),
            ("CX_OLLAMA_MODEL", "chosen"),
        ],
    );
    assert!(run.status.success(), "{}", stderr_str(&run));
    assert_eq!(fs::read_to_string(trace).unwrap(), "run chosen\n");
}
