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
    let out = repo.run(&["llm", "use", "primary"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert!(
        stderr_str(&out).contains("quota_probe: backend=primary"),
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
        show_backend_text.contains("llm_backend: primary"),
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
fn llm_use_mlx_alias_persists_resolved_model() {
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
    assert!(
        add.status.success(),
        "stdout={} stderr={}",
        stdout_str(&add),
        stderr_str(&add)
    );

    let out = repo.run(&["llm", "use", "mlx", "tiny"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let text = stdout_str(&out);
    assert!(text.contains("mlx_model: mlx-community/Tiny"), "{text}");

    let show = repo.run(&["llm", "show"]);
    assert!(show.status.success(), "stderr={}", stderr_str(&show));
    let show_text = stdout_str(&show);
    assert!(
        show_text.contains("mlx_model: mlx-community/Tiny"),
        "{show_text}"
    );
    assert!(show_text.contains("mlx_model_alias: tiny"), "{show_text}");
    assert!(
        show_text.contains("mlx_model_resolved_model: mlx-community/Tiny"),
        "{show_text}"
    );

    let state = read_json(&repo.state_file());
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
fn models_inspect_reports_accounting_paths() {
    let repo = TempRepo::new("cxrs-llm");
    let missing_path = repo.root.join("does-not-exist");
    let model_dir = repo.root.join("model-dir");
    let cache_dir = repo.root.join("cache-dir");
    fs::create_dir_all(&model_dir).expect("create model dir");
    fs::create_dir_all(&cache_dir).expect("create cache dir");
    fs::write(model_dir.join("model.bin"), b"abc").expect("write model file");
    fs::write(cache_dir.join("cache.bin"), b"12345").expect("write cache file");

    let missing_path_s = missing_path.display().to_string();
    let model_dir_s = model_dir.display().to_string();
    let cache_dir_s = cache_dir.display().to_string();

    let add_missing = repo.run(&[
        "llm",
        "models",
        "add",
        "missing-local",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/MissingLocal",
        "--local-path",
        &missing_path_s,
    ]);
    assert!(
        add_missing.status.success(),
        "stderr={}",
        stderr_str(&add_missing)
    );

    let inspect_missing = repo.run(&["llm", "models", "inspect", "missing-local", "--json"]);
    assert!(
        inspect_missing.status.success(),
        "stdout={} stderr={}",
        stdout_str(&inspect_missing),
        stderr_str(&inspect_missing)
    );
    let missing_json =
        serde_json::from_str::<Value>(&stdout_str(&inspect_missing)).expect("inspect missing");
    assert_eq!(
        missing_json
            .get("inspect")
            .and_then(|v| v.get("resolved_model_status"))
            .and_then(Value::as_str),
        Some("missing_local_path")
    );
    assert_eq!(
        missing_json
            .get("inspect")
            .and_then(|v| v.get("local_path"))
            .and_then(|v| v.get("status"))
            .and_then(Value::as_str),
        Some("missing")
    );

    let add_resolved = repo.run(&[
        "llm",
        "models",
        "add",
        "with-local-dir",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/LocalDir",
        "--local-path",
        &model_dir_s,
        "--cache-path",
        &cache_dir_s,
    ]);
    assert!(
        add_resolved.status.success(),
        "stderr={}",
        stderr_str(&add_resolved)
    );

    let inspect_cheap = repo.run(&["llm", "models", "inspect", "with-local-dir", "--json"]);
    assert!(
        inspect_cheap.status.success(),
        "stderr={}",
        stderr_str(&inspect_cheap)
    );
    let cheap_json =
        serde_json::from_str::<Value>(&stdout_str(&inspect_cheap)).expect("inspect cheap");
    assert_eq!(
        cheap_json
            .get("inspect")
            .and_then(|v| v.get("accounting_mode"))
            .and_then(Value::as_str),
        Some("cheap")
    );
    assert_eq!(
        cheap_json
            .get("inspect")
            .and_then(|v| v.get("local_path"))
            .and_then(|v| v.get("path_kind"))
            .and_then(Value::as_str),
        Some("dir")
    );
    assert!(
        cheap_json
            .get("inspect")
            .and_then(|v| v.get("local_path"))
            .and_then(|v| v.get("size_bytes"))
            .is_some_and(Value::is_null)
    );

    let inspect_disk = repo.run(&[
        "llm",
        "models",
        "inspect",
        "with-local-dir",
        "--disk-usage",
        "--json",
    ]);
    assert!(
        inspect_disk.status.success(),
        "stderr={}",
        stderr_str(&inspect_disk)
    );
    let disk_json =
        serde_json::from_str::<Value>(&stdout_str(&inspect_disk)).expect("inspect disk");
    assert_eq!(
        disk_json
            .get("inspect")
            .and_then(|v| v.get("accounting_mode"))
            .and_then(Value::as_str),
        Some("disk_usage")
    );
    assert_eq!(
        disk_json
            .get("inspect")
            .and_then(|v| v.get("local_path"))
            .and_then(|v| v.get("size_bytes"))
            .and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        disk_json
            .get("inspect")
            .and_then(|v| v.get("cache_path"))
            .and_then(|v| v.get("size_bytes"))
            .and_then(Value::as_u64),
        Some(5)
    );
}

#[test]
fn llm_verify_mlx_smoke_resolves_registry_alias() {
    let repo = TempRepo::new("cxrs-llm");
    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny-Resolved",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));

    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
printf 'OK'
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();
    let out = repo.run_with_env(
        &["llm", "verify", "mlx", "--json"],
        &[("CX_MLX_MODEL", "tiny"), ("CX_MLX_PYTHON", &mlx_python)],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v = serde_json::from_str::<Value>(&stdout_str(&out)).expect("verify json");
    assert_eq!(
        v.get("contract_version").and_then(Value::as_str),
        Some("llm-verify.v1")
    );
    assert_eq!(v.get("backend").and_then(Value::as_str), Some("mlx"));
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("model"))
            .and_then(|v| v.get("input"))
            .and_then(Value::as_str),
        Some("tiny")
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("model"))
            .and_then(|v| v.get("resolved"))
            .and_then(Value::as_str),
        Some("mlx-community/Tiny-Resolved")
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("correctness"))
            .and_then(|v| v.get("exact"))
            .and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn llm_verify_mlx_benchmark_emits_typed_metrics() {
    let repo = TempRepo::new("cxrs-llm");
    let add = repo.run(&[
        "llm",
        "models",
        "add",
        "bench",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/BenchResolved",
    ]);
    assert!(add.status.success(), "stderr={}", stderr_str(&add));

    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
set -euo pipefail
OUT=""
PREV=""
for ARG in "$@"; do
  if [ "$PREV" = "--out" ]; then
    OUT="$ARG"
    break
  fi
  PREV="$ARG"
done
cat > "$OUT" <<'JSON'
{
  "contract_version": "turboquant-mlx.v2",
  "backend": "mlx",
  "model": "mlx-community/BenchResolved",
  "context_target": 8192,
  "runs": [
    {
      "prompt_name": "smoke",
      "passed": true,
      "prompt_tokens_per_sec": 100.0,
      "decode_tokens_per_sec": 50.0,
      "peak_memory_gb": 1.2,
      "cache_nbytes": 1000,
      "wall_ms": 200
    },
    {
      "prompt_name": "retrieval",
      "passed": true,
      "prompt_tokens_per_sec": 120.0,
      "decode_tokens_per_sec": 70.0,
      "peak_memory_gb": 1.5,
      "cache_nbytes": 1200,
      "wall_ms": 300
    }
  ],
  "passes": 2,
  "total": 2
}
JSON
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();
    let out = repo.run_with_env(
        &["llm", "verify", "mlx", "--profile", "benchmark", "--json"],
        &[("CX_MLX_MODEL", "bench"), ("CX_MLX_PYTHON", &mlx_python)],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let v = serde_json::from_str::<Value>(&stdout_str(&out)).expect("verify json");
    assert_eq!(v.get("profile").and_then(Value::as_str), Some("benchmark"));
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("memory"))
            .and_then(|v| v.get("cache_metric_kind"))
            .and_then(Value::as_str),
        Some("cache_nbytes")
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("memory"))
            .and_then(|v| v.get("cache_metric_unit"))
            .and_then(Value::as_str),
        Some("bytes")
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("correctness"))
            .and_then(|v| v.get("exact"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("runtime"))
            .and_then(|v| v.get("prompt_tps_mean"))
            .and_then(Value::as_f64),
        Some(110.0)
    );
    assert_eq!(
        v.get("result")
            .and_then(|v| v.get("memory"))
            .and_then(|v| v.get("peak_memory_gb_max"))
            .and_then(Value::as_f64),
        Some(1.5)
    );
}

#[test]
fn llm_use_mlx_alias_shows_resolved_model() {
    let repo = TempRepo::new("cxrs-llm");
    assert!(
        repo.run(&[
            "llm",
            "models",
            "add",
            "tiny",
            "--backend",
            "mlx",
            "--model",
            "mlx-community/Tiny-Resolved",
        ])
        .status
        .success()
    );

    let use_out = repo.run(&["llm", "use", "mlx", "tiny"]);
    assert!(
        use_out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&use_out),
        stderr_str(&use_out)
    );

    let show = repo.run(&["llm", "show"]);
    assert!(
        show.status.success(),
        "stdout={} stderr={}",
        stdout_str(&show),
        stderr_str(&show)
    );
    let text = stdout_str(&show);
    assert!(text.contains("llm_backend: mlx"), "{text}");
    assert!(
        text.contains("mlx_model: mlx-community/Tiny-Resolved"),
        "{text}"
    );
    assert!(text.contains("mlx_model_alias: tiny"), "{text}");
    assert!(
        text.contains("mlx_model_resolved_model: mlx-community/Tiny-Resolved"),
        "{text}"
    );
    assert!(
        text.contains("active_model_resolved_model: mlx-community/Tiny-Resolved"),
        "{text}"
    );

    let state = read_json(&repo.state_file());
    assert_eq!(
        state
            .get("preferences")
            .and_then(|v| v.get("mlx_model"))
            .and_then(Value::as_str),
        Some("mlx-community/Tiny-Resolved")
    );
}

#[test]
fn mlx_alias_exec_resolution_env_override() {
    let repo = TempRepo::new("cxrs-llm");
    assert!(
        repo.run(&[
            "llm",
            "models",
            "add",
            "tiny",
            "--backend",
            "mlx",
            "--model",
            "mlx-community/Tiny-Resolved",
        ])
        .status
        .success()
    );
    assert!(repo.run(&["llm", "use", "mlx", "tiny"]).status.success());

    let alias_args_file = repo.root.join("mlx_alias_args.txt");
    let alias_args_path = alias_args_file.display().to_string();
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
printf '%s\n' "$@" > "$MLX_ARGS_FILE"
printf 'mlx ok'
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();

    let alias_run = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_MLX_MODEL", "tiny"),
            ("CX_MLX_PYTHON", &mlx_python),
            ("MLX_ARGS_FILE", &alias_args_path),
        ],
    );
    assert!(
        alias_run.status.success(),
        "stdout={} stderr={}",
        stdout_str(&alias_run),
        stderr_str(&alias_run)
    );
    let alias_args = fs::read_to_string(&alias_args_file).expect("read mlx alias args");
    assert!(
        alias_args.contains("--model\nmlx-community/Tiny-Resolved\n"),
        "{alias_args}"
    );

    let direct_args_file = repo.root.join("mlx_direct_args.txt");
    let direct_args_path = direct_args_file.display().to_string();
    let direct_run = repo.run_with_env(
        &["cxo", "echo", "hi"],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_MLX_MODEL", "mlx-community/Direct-From-Env"),
            ("CX_MLX_PYTHON", &mlx_python),
            ("MLX_ARGS_FILE", &direct_args_path),
        ],
    );
    assert!(
        direct_run.status.success(),
        "stdout={} stderr={}",
        stdout_str(&direct_run),
        stderr_str(&direct_run)
    );
    let direct_args = fs::read_to_string(&direct_args_file).expect("read mlx direct args");
    assert!(
        direct_args.contains("--model\nmlx-community/Direct-From-Env\n"),
        "{direct_args}"
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

#[test]
fn llm_resident_show_json_contract() {
    let repo = TempRepo::new("cxrs-llm");
    let out = repo.run(&["llm", "resident", "show", "--json"]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload = serde_json::from_str::<Value>(&stdout_str(&out)).expect("resident show json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("llm-resident.v1")
    );
    assert_eq!(
        payload.get("selected_transport").and_then(Value::as_str),
        Some("process")
    );
    assert_eq!(
        payload
            .get("runtime_capability")
            .and_then(|v| v.get("resident_server"))
            .and_then(Value::as_bool),
        Some(false)
    );
}

#[test]
fn llm_resident_probe_uses_openai_profile() {
    let repo = TempRepo::new("cxrs-llm");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
cat <<'JSON'
{"data":[{"id":"mlx-local"},{"id":"qwen-local"}]}
JSON
"#,
    );
    let out = repo.run_with_env(
        &["llm", "resident", "probe-models", "--json"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_REQUEST_PROFILE", "openai_json"),
            (
                "CX_HTTP_PROVIDER_URL",
                "http://127.0.0.1:11434/v1/chat/completions",
            ),
            ("CX_HTTP_REQUIRE_HTTPS", "0"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    let payload = serde_json::from_str::<Value>(&stdout_str(&out)).expect("resident probe json");
    assert_eq!(
        payload.get("contract_version").and_then(Value::as_str),
        Some("llm-resident.v1")
    );
    assert_eq!(
        payload.get("selected_transport").and_then(Value::as_str),
        Some("http")
    );
    assert_eq!(
        payload
            .get("runtime_capability")
            .and_then(|v| v.get("resident_server"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        payload
            .get("probe")
            .and_then(|v| v.get("probe_url"))
            .and_then(Value::as_str),
        Some("http://127.0.0.1:11434/v1/models")
    );
    assert_eq!(
        payload
            .get("probe")
            .and_then(|v| v.get("model_count"))
            .and_then(Value::as_u64),
        Some(2)
    );
}

#[test]
fn task_model_alias_resolves_active_backend() {
    let repo = TempRepo::new("cxrs-llm");
    let args_file = repo.root.join("mlx_task_args.txt");
    repo.write_mock(
        "mlx-python",
        r#"#!/usr/bin/env bash
printf '%s\n' "$@" > "$MLX_TASK_ARGS_FILE"
printf 'mlx task ok'
"#,
    );
    let mlx_python = repo.mock_bin.join("mlx-python").display().to_string();
    let args_path = args_file.display().to_string();

    let add_model = repo.run(&[
        "llm",
        "models",
        "add",
        "tiny-task",
        "--backend",
        "mlx",
        "--model",
        "mlx-community/Tiny-Task",
    ]);
    assert!(
        add_model.status.success(),
        "stdout={} stderr={}",
        stdout_str(&add_model),
        stderr_str(&add_model)
    );

    let add_task = repo.run(&[
        "task",
        "add",
        "cxo echo model-override",
        "--role",
        "implementer",
        "--model",
        "tiny-task",
    ]);
    assert!(
        add_task.status.success(),
        "stdout={} stderr={}",
        stdout_str(&add_task),
        stderr_str(&add_task)
    );
    let task_id = stdout_str(&add_task).trim().to_string();

    let run = repo.run_with_env(
        &["task", "run", &task_id],
        &[
            ("CX_LLM_BACKEND", "mlx"),
            ("CX_MLX_PYTHON", &mlx_python),
            ("MLX_TASK_ARGS_FILE", &args_path),
        ],
    );
    assert!(
        run.status.success(),
        "stdout={} stderr={}",
        stdout_str(&run),
        stderr_str(&run)
    );
    assert!(stdout_str(&run).contains("mlx task ok"));

    let args = fs::read_to_string(&args_file).expect("read mlx task args");
    assert!(
        args.contains("--model\nmlx-community/Tiny-Task\n"),
        "{args}"
    );
}
