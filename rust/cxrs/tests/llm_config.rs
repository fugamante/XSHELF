mod common;

use common::{TempRepo, read_json, stderr_str, stdout_str};
use serde_json::Value;
use std::fs;

#[test]
fn llm_use_persists_backend_and_model() {
    let repo = TempRepo::new("cxrs-llm");

    let out = repo.run(&["llm", "use", "ollama", "llama3.1"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("quota_probe: backend=ollama service_kind=local_unmetered"),
        "stderr={}",
        stderr_str(&out)
    );

    let show = repo.run(&["llm", "show"]);
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    let text = stdout_str(&show);
    assert!(text.contains("llm_backend: ollama"), "{text}");
    assert!(text.contains("ollama_model: llama3.1"), "{text}");

    let state = read_json(&repo.state_file());
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("llm_backend"))
            .and_then(Value::as_str),
        Some("ollama")
    );
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("ollama_model"))
            .and_then(Value::as_str),
        Some("llama3.1")
    );
}

#[test]
fn llm_use_codex_triggers_quota_probe_notice() {
    let repo = TempRepo::new("cxrs-llm");
    let out = repo.run(&["llm", "use", "codex"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("quota_probe: backend=codex"),
        "stderr={}",
        stderr_str(&out)
    );
}

#[test]
fn llm_unset_clears_model_backend_all() {
    let repo = TempRepo::new("cxrs-llm");

    let out = repo.run(&["llm", "use", "ollama", "llama3.1"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );

    let unset_model = repo.run(&["llm", "unset", "model"]);
    assert!(
        unset_model.status.success(),
        "stdout={} stderr={}",
        stdout_str(&unset_model),
        stderr_str(&unset_model)
    );

    let show_after_model = repo.run(&["llm", "show"]);
    assert!(show_after_model.status.success());
    let show_text = stdout_str(&show_after_model);
    assert!(show_text.contains("llm_backend: ollama"), "{show_text}");
    assert!(show_text.contains("ollama_model: <unset>"), "{show_text}");

    let unset_backend = repo.run(&["llm", "unset", "backend"]);
    assert!(unset_backend.status.success());
    let show_after_backend = repo.run(&["llm", "show"]);
    let show_backend_text = stdout_str(&show_after_backend);
    assert!(
        show_backend_text.contains("llm_backend: codex"),
        "{show_backend_text}"
    );

    let out2 = repo.run(&["llm", "use", "ollama", "llama3.1"]);
    assert!(out2.status.success());
    let unset_all = repo.run(&["llm", "unset", "all"]);
    assert!(unset_all.status.success());

    let state = read_json(&repo.state_file());
    assert!(
        state
            .get("preferences")
            .and_then(|v| v.get("llm_backend"))
            .is_some_and(Value::is_null)
    );
    assert!(
        state
            .get("preferences")
            .and_then(|v| v.get("ollama_model"))
            .is_some_and(Value::is_null)
    );
}

#[test]
fn ollama_without_model_fails_noninteractive_clear() {
    let repo = TempRepo::new("cxrs-llm");

    assert!(repo.run(&["llm", "unset", "all"]).status.success());
    assert!(repo.run(&["llm", "use", "ollama"]).status.success());

    let out = repo.run(&["cxo", "echo", "hi"]);
    assert!(
        !out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let err = stderr_str(&out);
    assert!(
        err.contains("ollama model is unset"),
        "expected unset-model guidance in stderr; got: {err}"
    );
}

#[test]
fn llm_use_llamacpp_persists_model_path() {
    let repo = TempRepo::new("cxrs-llm");

    let out = repo.run(&["llm", "use", "llama.cpp", "/models/tiny.gguf"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("quota_probe: backend=llamacpp service_kind=local_unmetered"),
        "stderr={}",
        stderr_str(&out)
    );

    let show = repo.run(&["llm", "show"]);
    assert!(show.status.success());
    let text = stdout_str(&show);
    assert!(text.contains("llm_backend: llamacpp"), "{text}");
    assert!(
        text.contains("llama_cpp_model: /models/tiny.gguf"),
        "{text}"
    );

    let state = read_json(&repo.state_file());
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("llm_backend"))
            .and_then(Value::as_str),
        Some("llamacpp")
    );
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("llama_cpp_model"))
            .and_then(Value::as_str),
        Some("/models/tiny.gguf")
    );
}

#[test]
fn llamacpp_backend_runs_llama_cli_mock() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "llama-cli",
        r#"#!/usr/bin/env bash
printf 'llama.cpp ok'
"#,
    );

    let out = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "llamacpp"),
            ("CX_LLAMA_CPP_MODEL", "/models/tiny.gguf"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(stdout_str(&out).contains("llama.cpp ok"));
}

#[test]
fn llamacpp_backend_uses_hf_repo_quant() {
    let repo = TempRepo::new("cxrs-llm");
    let args_file = repo.root.join("llama_args.txt");
    repo.write_mock(
        "llama-cli",
        r#"#!/usr/bin/env bash
printf '%s\n' "$@" > "$LLAMA_ARGS_FILE"
printf 'llama.cpp hf ok'
"#,
    );
    let args_path = args_file.display().to_string();

    let out = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "llamacpp"),
            ("CX_LLAMA_CPP_MODEL", "ggml-org/Qwen3-0.6B-GGUF:Q4_0"),
            ("CX_LLAMA_CPP_ARGS", "-n 8 --temp 0"),
            ("LLAMA_ARGS_FILE", &args_path),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(stdout_str(&out).contains("llama.cpp hf ok"));
    let args = fs::read_to_string(&args_file).expect("read llama-cli args");
    assert!(args.contains("-hf\n"), "{args}");
    assert!(args.contains("ggml-org/Qwen3-0.6B-GGUF:Q4_0\n"), "{args}");
    assert!(!args.contains("-m\n"), "{args}");
    assert!(args.contains("-n\n8\n"), "{args}");
    assert!(args.contains("--temp\n0\n"), "{args}");
}

#[test]
fn llm_use_mlx_persists_model_name() {
    let repo = TempRepo::new("cxrs-llm");

    let out = repo.run(&["llm", "use", "mlx", "mlx-community/Tiny"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("quota_probe: backend=mlx service_kind=local_unmetered"),
        "stderr={}",
        stderr_str(&out)
    );

    let show = repo.run(&["llm", "show"]);
    assert!(show.status.success());
    let text = stdout_str(&show);
    assert!(text.contains("llm_backend: mlx"), "{text}");
    assert!(text.contains("mlx_model: mlx-community/Tiny"), "{text}");

    let state = read_json(&repo.state_file());
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("llm_backend"))
            .and_then(Value::as_str),
        Some("mlx")
    );
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("mlx_model"))
            .and_then(Value::as_str),
        Some("mlx-community/Tiny")
    );
}

#[test]
fn models_crud() {
    let repo = TempRepo::new("cxrs-llm");

    let empty = repo.run(&["llm", "models", "list"]);
    assert!(
        empty.status.success(),
        "stdout={} stderr={}",
        stdout_str(&empty),
        stderr_str(&empty)
    );
    assert!(stdout_str(&empty).contains("local_models: <empty>"));

    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "qwen-coder",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Qwen-Coder-4bit",
        "--provider",
        "huggingface",
        "--repo-id",
        "mlx-community/Qwen-Coder-4bit",
        "--quantization",
        "4bit",
        "--format",
        "mlx",
        "--size-bytes",
        "1234",
        "--trust-remote-code",
        "false",
    ]);
    assert!(
        add.status.success(),
        "stdout={} stderr={}",
        stdout_str(&add),
        stderr_str(&add)
    );
    let add_text = stdout_str(&add);
    assert!(add_text.contains("model_alias: qwen-coder"), "{add_text}");
    assert!(add_text.contains("model_backend: mlx"), "{add_text}");
    assert!(add_text.contains("model_id: mlx:qwen-coder"), "{add_text}");

    let list = repo.run(&["llm", "models", "list"]);
    assert!(list.status.success());
    let list_text = stdout_str(&list);
    assert!(
        list_text.contains(
            "- alias=qwen-coder backend=mlx id=mlx:qwen-coder model=mlx-community/Qwen-Coder-4bit"
        ),
        "{list_text}"
    );

    let inspect = repo.run(&["llm", "models", "inspect", "qwen-coder", "--json"]);
    assert!(
        inspect.status.success(),
        "stdout={} stderr={}",
        stdout_str(&inspect),
        stderr_str(&inspect)
    );
    let inspected = serde_json::from_str::<Value>(&stdout_str(&inspect)).expect("inspect JSON");
    assert_eq!(
        inspected.get("alias").and_then(Value::as_str),
        Some("qwen-coder")
    );
    assert_eq!(
        inspected.get("backend").and_then(Value::as_str),
        Some("mlx")
    );
    assert_eq!(
        inspected.get("resolved_model").and_then(Value::as_str),
        Some("mlx-community/Qwen-Coder-4bit")
    );
    assert_eq!(
        inspected.get("size_bytes").and_then(Value::as_u64),
        Some(1234)
    );
    assert_eq!(
        inspected.get("trust_remote_code").and_then(Value::as_str),
        Some("false")
    );

    let stored = read_json(&repo.local_models_file());
    assert_eq!(
        stored.get("contract_version").and_then(Value::as_str),
        Some("local_models.v1")
    );

    let remove = repo.run(&["llm", "models", "remove", "mlx:qwen-coder"]);
    assert!(
        remove.status.success(),
        "stdout={} stderr={}",
        stdout_str(&remove),
        stderr_str(&remove)
    );
    assert!(stdout_str(&remove).contains("removed_model_alias: qwen-coder"));

    let after = repo.run(&["llm", "models", "list"]);
    assert!(stdout_str(&after).contains("local_models: <empty>"));
}

#[test]
fn models_replace() {
    let repo = TempRepo::new("cxrs-llm");

    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny",
    ]);
    assert!(add.status.success());

    let duplicate = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny-v2",
    ]);
    assert!(!duplicate.status.success());
    assert!(
        stderr_str(&duplicate).contains("already exists"),
        "{}",
        stderr_str(&duplicate)
    );

    let replace = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny-v2",
        "--replace",
    ]);
    assert!(
        replace.status.success(),
        "stdout={} stderr={}",
        stdout_str(&replace),
        stderr_str(&replace)
    );

    let inspect = repo.run(&["llm", "models", "inspect", "tiny", "--json"]);
    let inspected = serde_json::from_str::<Value>(&stdout_str(&inspect)).expect("inspect JSON");
    assert_eq!(
        inspected.get("resolved_model").and_then(Value::as_str),
        Some("mlx-community/Tiny-v2")
    );
}

#[test]
fn mlx_backend_runs_mlx_lm_mock() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
printf 'mlx ok'
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();

    let out = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_MLX_MODEL", "mlx-community/Tiny"),
            ("CX_MLX_PYTHON", &mlx_python),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(stdout_str(&out).contains("mlx ok"));
}

#[test]
fn mlx_backend_normalizes_generate_banner_output() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
cat <<'OUT'
==========
Prompt: say ok
Generation: OK
==========
Prompt tokens: 3
Generation tokens: 1
OUT
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();

    let out = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_MLX_MODEL", "mlx-community/Tiny"),
            ("CX_MLX_PYTHON", &mlx_python),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), "OK");
}

#[test]
fn llm_check_reports_mlx_runtime_status() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
exit 0
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();

    let out = repo.run_with_env(
        &["llm", "check", "mlx"],
        &[
            ("CX_MLX_MODEL", "mlx-community/Tiny"),
            ("CX_MLX_PYTHON", &mlx_python),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let text = stdout_str(&out);
    assert!(text.contains("backend: mlx"), "{text}");
    assert!(text.contains("runtime_ok: yes"), "{text}");
    assert!(text.contains("model_ok: yes"), "{text}");
}

#[test]
fn llm_smoke_runs_current_backend() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "llama-cli",
        r#"#!/usr/bin/env bash
printf 'smoke ok'
"#,
    );

    let out = repo.run_with_env(
        &["llm", "smoke", "say", "ok"],
        &[
            ("CX_LLM_BACKEND", "llamacpp"),
            ("CX_LLAMA_CPP_MODEL", "/models/tiny.gguf"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let text = stdout_str(&out);
    assert!(text.contains("smoke_backend: llamacpp"), "{text}");
    assert!(text.contains("smoke_output:\nsmoke ok"), "{text}");
}
