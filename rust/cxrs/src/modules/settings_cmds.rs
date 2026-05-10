use serde_json::Value;
use std::process::Command;

use crate::config::cli_app_name;

use crate::analytics::quota_probe_for_backend_days;
use crate::local_models::{
    AddLocalModelInput, add_record, find_record, list_records, record_json, registry_json,
    remove_record,
};
use crate::process::run_command_output_with_timeout;
use crate::provider_adapter::resolve_provider_adapter;
use crate::runtime::{
    llama_cpp_model_preference, llm_backend, llm_model, mlx_model_preference,
    ollama_model_preference,
};
use crate::state::{
    ensure_state_value, parse_cli_value, set_state_path, set_value_at_path, state_cache_clear,
    value_at_path, write_json_atomic,
};

pub fn cmd_state_show() -> i32 {
    let (_state_file, state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} state show: {e}", cli_app_name());
            return 1;
        }
    };
    match serde_json::to_string_pretty(&state) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!("{} state show: failed to render JSON: {e}", cli_app_name());
            1
        }
    }
}

pub fn cmd_state_get(key: &str) -> i32 {
    let (state_file, state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} state get: {e}", cli_app_name());
            return 1;
        }
    };
    let Some(v) = value_at_path(&state, key) else {
        crate::cx_eprintln!("{} state get: key not found: {key}", cli_app_name());
        crate::cx_eprintln!("state_file: {}", state_file.display());
        return 1;
    };
    match v {
        Value::String(s) => println!("{s}"),
        _ => println!("{}", v),
    }
    0
}

pub fn cmd_state_set(key: &str, raw_value: &str) -> i32 {
    let (state_file, mut state) = match ensure_state_value() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} state set: {e}", cli_app_name());
            return 1;
        }
    };
    if let Err(e) = set_value_at_path(&mut state, key, parse_cli_value(raw_value)) {
        crate::cx_eprintln!("{} state set: {e}", cli_app_name());
        return 1;
    }
    if let Err(e) = write_json_atomic(&state_file, &state) {
        crate::cx_eprintln!("{} state set: {e}", cli_app_name());
        return 1;
    }
    state_cache_clear();
    println!("ok");
    0
}

fn print_llm_usage(app_name: &str) {
    crate::cx_eprintln!(
        "Usage: {app_name} llm <show|check [backend]|smoke [prompt]|models <list|add|inspect|remove>|use <codex|ollama|llamacpp|mlx> [model]|unset <backend|model|all>|set-backend <codex|ollama|llamacpp|mlx>|set-model <model>|clear-model>"
    );
}

fn print_models_usage(app_name: &str) {
    crate::cx_eprintln!(
        "Usage: {app_name} llm models <list [--json]|add <alias> --backend <ollama|llamacpp|mlx> --model <model> [--replace] [--json]|inspect <alias-or-id> [--json]|remove <alias-or-id> [--json]>"
    );
}

fn normalize_llm_backend(raw: &str) -> Option<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "codex" => Some("codex".to_string()),
        "ollama" => Some("ollama".to_string()),
        "llamacpp" | "llama.cpp" | "llama_cpp" => Some("llamacpp".to_string()),
        "mlx" => Some("mlx".to_string()),
        _ => None,
    }
}

fn llm_show() -> i32 {
    let backend = llm_backend();
    let model = llm_model();
    let ollama_pref = ollama_model_preference();
    let llama_cpp_pref = llama_cpp_model_preference();
    let mlx_pref = mlx_model_preference();
    println!("llm_backend: {backend}");
    println!(
        "active_model: {}",
        if model.is_empty() { "<unset>" } else { &model }
    );
    println!(
        "ollama_model: {}",
        if ollama_pref.is_empty() {
            "<unset>"
        } else {
            &ollama_pref
        }
    );
    println!(
        "llama_cpp_model: {}",
        if llama_cpp_pref.is_empty() {
            "<unset>"
        } else {
            &llama_cpp_pref
        }
    );
    println!(
        "mlx_model: {}",
        if mlx_pref.is_empty() {
            "<unset>"
        } else {
            &mlx_pref
        }
    );
    0
}

fn emit_quota_probe_notice(backend: &str, model: Option<&str>) {
    let Ok(payload) = quota_probe_for_backend_days(30, backend, model) else {
        crate::cx_eprintln!("quota_probe: unavailable");
        return;
    };
    let backend = payload
        .get("backend")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let service_kind = payload
        .get("service_kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let source = payload
        .get("quota_source")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let remaining = payload.get("quota_remaining_pct").and_then(Value::as_f64);

    if service_kind == "local_unmetered" {
        crate::cx_eprintln!(
            "quota_probe: backend={} service_kind=local_unmetered (provider quota unavailable for local model)",
            backend
        );
        return;
    }

    if let Some(rem) = remaining {
        let remaining_pct = format!("{}%", (rem * 100.0).round() as i64);
        crate::cx_eprintln!(
            "quota_probe: backend={} remaining={} source={}",
            backend,
            remaining_pct,
            source
        );
    } else {
        crate::cx_eprintln!(
            "quota_probe: backend={} remaining=unknown source={} (set quota total or refresh catalog)",
            backend,
            source
        );
    }
}

fn llm_use(app_name: &str, args: &[String]) -> i32 {
    let Some(target) = args.get(1).and_then(|s| normalize_llm_backend(s)) else {
        print_llm_usage(app_name);
        return 2;
    };
    if let Err(e) = set_state_path("preferences.llm_backend", Value::String(target.clone())) {
        crate::cx_eprintln!("{} llm use: {e}", cli_app_name());
        return 1;
    }
    if target == "ollama" {
        if let Some(model) = args.get(2) {
            let m = model.trim();
            if m.is_empty() {
                print_llm_usage(app_name);
                return 2;
            }
            if let Err(e) = set_state_path("preferences.ollama_model", Value::String(m.to_string()))
            {
                crate::cx_eprintln!("{} llm use: {e}", cli_app_name());
                return 1;
            }
        }
        println!("ok");
        println!("llm_backend: ollama");
        let pref = ollama_model_preference();
        println!(
            "ollama_model: {}",
            if pref.is_empty() { "<unset>" } else { &pref }
        );
        state_cache_clear();
        let model_opt = if pref.is_empty() {
            None
        } else {
            Some(pref.as_str())
        };
        emit_quota_probe_notice("ollama", model_opt);
        return 0;
    }
    if target == "llamacpp" {
        if let Some(model) = args.get(2) {
            let m = model.trim();
            if m.is_empty() {
                print_llm_usage(app_name);
                return 2;
            }
            if let Err(e) =
                set_state_path("preferences.llama_cpp_model", Value::String(m.to_string()))
            {
                crate::cx_eprintln!("{} llm use: {e}", cli_app_name());
                return 1;
            }
        }
        println!("ok");
        println!("llm_backend: llamacpp");
        let pref = llama_cpp_model_preference();
        println!(
            "llama_cpp_model: {}",
            if pref.is_empty() { "<unset>" } else { &pref }
        );
        state_cache_clear();
        let model_opt = if pref.is_empty() {
            None
        } else {
            Some(pref.as_str())
        };
        emit_quota_probe_notice("llamacpp", model_opt);
        return 0;
    }
    if target == "mlx" {
        if let Some(model) = args.get(2) {
            let m = model.trim();
            if m.is_empty() {
                print_llm_usage(app_name);
                return 2;
            }
            if let Err(e) = set_state_path("preferences.mlx_model", Value::String(m.to_string())) {
                crate::cx_eprintln!("{} llm use: {e}", cli_app_name());
                return 1;
            }
        }
        println!("ok");
        println!("llm_backend: mlx");
        let pref = mlx_model_preference();
        println!(
            "mlx_model: {}",
            if pref.is_empty() { "<unset>" } else { &pref }
        );
        state_cache_clear();
        let model_opt = if pref.is_empty() {
            None
        } else {
            Some(pref.as_str())
        };
        emit_quota_probe_notice("mlx", model_opt);
        return 0;
    }
    println!("ok");
    println!("llm_backend: codex");
    state_cache_clear();
    emit_quota_probe_notice("codex", None);
    0
}

fn llm_unset(app_name: &str, args: &[String]) -> i32 {
    let target = args.get(1).map(String::as_str).unwrap_or("all");
    match target {
        "backend" => {
            if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                crate::cx_eprintln!("{} llm unset backend: {e}", cli_app_name());
                return 1;
            }
            println!("ok");
            println!("llm_backend: <unset>");
            0
        }
        "model" => {
            let backend = llm_backend();
            let path = match backend.as_str() {
                "llamacpp" => "preferences.llama_cpp_model",
                "mlx" => "preferences.mlx_model",
                _ => "preferences.ollama_model",
            };
            if let Err(e) = set_state_path(path, Value::Null) {
                crate::cx_eprintln!("{} llm unset model: {e}", cli_app_name());
                return 1;
            }
            println!("ok");
            match backend.as_str() {
                "llamacpp" => println!("llama_cpp_model: <unset>"),
                "mlx" => println!("mlx_model: <unset>"),
                _ => println!("ollama_model: <unset>"),
            }
            0
        }
        "all" => {
            if let Err(e) = set_state_path("preferences.llm_backend", Value::Null) {
                crate::cx_eprintln!("{} llm unset all: {e}", cli_app_name());
                return 1;
            }
            if let Err(e) = set_state_path("preferences.ollama_model", Value::Null) {
                crate::cx_eprintln!("{} llm unset all: {e}", cli_app_name());
                return 1;
            }
            if let Err(e) = set_state_path("preferences.llama_cpp_model", Value::Null) {
                crate::cx_eprintln!("{} llm unset all: {e}", cli_app_name());
                return 1;
            }
            if let Err(e) = set_state_path("preferences.mlx_model", Value::Null) {
                crate::cx_eprintln!("{} llm unset all: {e}", cli_app_name());
                return 1;
            }
            println!("ok");
            println!("llm_backend: <unset>");
            println!("ollama_model: <unset>");
            println!("llama_cpp_model: <unset>");
            println!("mlx_model: <unset>");
            0
        }
        _ => {
            print_llm_usage(app_name);
            2
        }
    }
}

fn llm_set_backend(app_name: &str, args: &[String]) -> i32 {
    let Some(v) = args.get(1).and_then(|s| normalize_llm_backend(s)) else {
        print_llm_usage(app_name);
        return 2;
    };
    if let Err(e) = set_state_path("preferences.llm_backend", Value::String(v.clone())) {
        crate::cx_eprintln!("{} llm set-backend: {e}", cli_app_name());
        return 1;
    }
    println!("ok");
    println!("llm_backend: {v}");
    state_cache_clear();
    emit_quota_probe_notice(&v, None);
    0
}

fn llm_set_model(app_name: &str, args: &[String]) -> i32 {
    let Some(model) = args.get(1) else {
        print_llm_usage(app_name);
        return 2;
    };
    if model.trim().is_empty() {
        print_llm_usage(app_name);
        return 2;
    }
    let backend = llm_backend();
    let path = match backend.as_str() {
        "llamacpp" => "preferences.llama_cpp_model",
        "mlx" => "preferences.mlx_model",
        _ => "preferences.ollama_model",
    };
    if let Err(e) = set_state_path(path, Value::String(model.trim().to_string())) {
        crate::cx_eprintln!("{} llm set-model: {e}", cli_app_name());
        return 1;
    }
    println!("ok");
    match backend.as_str() {
        "llamacpp" => println!("llama_cpp_model: {}", model.trim()),
        "mlx" => println!("mlx_model: {}", model.trim()),
        _ => println!("ollama_model: {}", model.trim()),
    }
    state_cache_clear();
    emit_quota_probe_notice(&backend, Some(model.trim()));
    0
}

fn llm_clear_model() -> i32 {
    let backend = llm_backend();
    let path = match backend.as_str() {
        "llamacpp" => "preferences.llama_cpp_model",
        "mlx" => "preferences.mlx_model",
        _ => "preferences.ollama_model",
    };
    if let Err(e) = set_state_path(path, Value::Null) {
        crate::cx_eprintln!("{} llm clear-model: {e}", cli_app_name());
        return 1;
    }
    println!("ok");
    match backend.as_str() {
        "llamacpp" => println!("llama_cpp_model: <unset>"),
        "mlx" => println!("mlx_model: <unset>"),
        _ => println!("ollama_model: <unset>"),
    }
    0
}

fn command_available(bin: &str) -> bool {
    let mut cmd = Command::new("bash");
    cmd.args(["-lc", "command -v \"$1\" >/dev/null 2>&1", "_", bin]);
    run_command_output_with_timeout(cmd, "llm check command")
        .ok()
        .is_some_and(|out| out.status.success())
}

fn mlx_available(python: &str) -> bool {
    let mut cmd = Command::new(python);
    cmd.args(["-c", "import mlx_lm"]);
    run_command_output_with_timeout(cmd, "llm check mlx")
        .ok()
        .is_some_and(|out| out.status.success())
}

fn check_model_for_backend(backend: &str) -> String {
    match backend {
        "ollama" => ollama_model_preference(),
        "llamacpp" => llama_cpp_model_preference(),
        "mlx" => mlx_model_preference(),
        _ => llm_model(),
    }
}

fn llm_check(app_name: &str, args: &[String]) -> i32 {
    let backend = if let Some(raw) = args.get(1) {
        let Some(v) = normalize_llm_backend(raw) else {
            print_llm_usage(app_name);
            return 2;
        };
        v
    } else {
        llm_backend()
    };
    let model = check_model_for_backend(&backend);
    let (runtime_ok, runtime, hint) = match backend.as_str() {
        "ollama" => (
            command_available("ollama"),
            "ollama".to_string(),
            "install Ollama and ensure 'ollama' is on PATH",
        ),
        "llamacpp" => {
            let bin = std::env::var("CX_LLAMA_CPP_BIN")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "llama-cli".to_string());
            (
                command_available(&bin),
                bin,
                "install llama.cpp and ensure llama-cli is on PATH or set CX_LLAMA_CPP_BIN",
            )
        }
        "mlx" => {
            let python = std::env::var("CX_MLX_PYTHON")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "python3".to_string());
            (
                mlx_available(&python),
                python,
                "install mlx-lm in the selected Python environment or set CX_MLX_PYTHON",
            )
        }
        _ => (
            command_available("codex"),
            "codex".to_string(),
            "install Codex CLI and ensure 'codex' is on PATH",
        ),
    };
    let model_required = matches!(backend.as_str(), "ollama" | "llamacpp" | "mlx");
    let model_ok = !model_required || !model.trim().is_empty();
    println!("backend: {backend}");
    println!("runtime: {runtime}");
    println!("runtime_ok: {}", if runtime_ok { "yes" } else { "no" });
    println!(
        "model: {}",
        if model.is_empty() { "<unset>" } else { &model }
    );
    println!("model_ok: {}", if model_ok { "yes" } else { "no" });
    if !runtime_ok {
        println!("runtime_hint: {hint}");
    }
    if !model_ok {
        println!("model_hint: {app_name} llm use {backend} <model>");
    }
    if runtime_ok && model_ok { 0 } else { 1 }
}

fn llm_smoke(args: &[String]) -> i32 {
    let prompt = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        "Respond with OK only.".to_string()
    };
    let backend = llm_backend();
    let model = llm_model();
    let adapter = match resolve_provider_adapter() {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("llm smoke: {e}");
            return 1;
        }
    };
    let text = match adapter.run_plain(&prompt) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("llm smoke: {e}");
            return 1;
        }
    };
    println!("smoke_backend: {backend}");
    println!(
        "smoke_model: {}",
        if model.is_empty() { "<unset>" } else { &model }
    );
    println!("smoke_output:");
    println!("{}", text.trim());
    0
}

fn has_json_flag(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json")
}

fn emit_json(value: &Value) -> i32 {
    match serde_json::to_string_pretty(value) {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!(
                "{} llm models: failed to serialize JSON: {e}",
                cli_app_name()
            );
            1
        }
    }
}

fn option_value(args: &[String], flag: &str) -> Result<Option<String>, String> {
    let mut out = None;
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == flag {
            let Some(v) = args.get(i + 1) else {
                return Err(format!("{flag} requires a value"));
            };
            out = Some(v.clone());
            i += 2;
        } else {
            i += 1;
        }
    }
    Ok(out)
}

fn parse_size_bytes(args: &[String]) -> Result<Option<u64>, String> {
    let Some(raw) = option_value(args, "--size-bytes")? else {
        return Ok(None);
    };
    raw.parse::<u64>()
        .map(Some)
        .map_err(|_| format!("--size-bytes expects an integer, got '{raw}'"))
}

fn parse_model_add(
    alias: &str,
    backend: &str,
    resolved_model: &str,
    args: &[String],
) -> Result<AddLocalModelInput, String> {
    let trust_remote_code =
        option_value(args, "--trust-remote-code")?.unwrap_or_else(|| "unknown".to_string());
    Ok(AddLocalModelInput {
        id: option_value(args, "--id")?,
        alias: alias.to_string(),
        backend: backend.to_string(),
        provider: option_value(args, "--provider")?,
        repo_id: option_value(args, "--repo-id")?,
        revision: option_value(args, "--revision")?,
        resolved_model: resolved_model.to_string(),
        local_path: option_value(args, "--local-path")?,
        quantization: option_value(args, "--quantization")?,
        format: option_value(args, "--format")?,
        size_bytes: parse_size_bytes(args)?,
        cache_path: option_value(args, "--cache-path")?,
        preferred_args: option_value(args, "--preferred-args")?,
        trust_remote_code,
        replace: args.iter().any(|a| a == "--replace"),
    })
}

fn llm_models_list(args: &[String]) -> i32 {
    if has_json_flag(args) {
        return match registry_json() {
            Ok(v) => emit_json(&v),
            Err(e) => {
                crate::cx_eprintln!("{} llm models list: {e}", cli_app_name());
                1
            }
        };
    }
    match list_records() {
        Ok(records) if records.is_empty() => {
            println!("local_models: <empty>");
            0
        }
        Ok(records) => {
            println!("local_models:");
            for record in records {
                println!(
                    "- alias={} backend={} id={} model={}",
                    record.alias, record.backend, record.id, record.resolved_model
                );
            }
            0
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models list: {e}", cli_app_name());
            1
        }
    }
}

fn llm_models_add(app_name: &str, args: &[String]) -> i32 {
    let Some(alias) = args.first() else {
        print_models_usage(app_name);
        return 2;
    };
    let backend = match option_value(args, "--backend") {
        Ok(Some(v)) => v,
        Ok(None) => {
            print_models_usage(app_name);
            return 2;
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models add: {e}", cli_app_name());
            return 2;
        }
    };
    let resolved_model = match option_value(args, "--model") {
        Ok(Some(v)) => v,
        Ok(None) => {
            print_models_usage(app_name);
            return 2;
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models add: {e}", cli_app_name());
            return 2;
        }
    };
    let input = match parse_model_add(alias, &backend, &resolved_model, args) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} llm models add: {e}", cli_app_name());
            return 2;
        }
    };
    match add_record(input) {
        Ok(record) => {
            if has_json_flag(args) {
                emit_json(&record_json(&record))
            } else {
                println!("ok");
                println!("model_alias: {}", record.alias);
                println!("model_backend: {}", record.backend);
                println!("model_id: {}", record.id);
                println!("resolved_model: {}", record.resolved_model);
                0
            }
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models add: {e}", cli_app_name());
            1
        }
    }
}

fn llm_models_inspect(app_name: &str, args: &[String]) -> i32 {
    let Some(query) = args.first() else {
        print_models_usage(app_name);
        return 2;
    };
    match find_record(query) {
        Ok(Some(record)) => {
            if has_json_flag(args) {
                emit_json(&record_json(&record))
            } else {
                println!("model_id: {}", record.id);
                println!("model_alias: {}", record.alias);
                println!("model_backend: {}", record.backend);
                println!("resolved_model: {}", record.resolved_model);
                println!(
                    "local_path: {}",
                    record.local_path.as_deref().unwrap_or("<unset>")
                );
                println!(
                    "cache_path: {}",
                    record.cache_path.as_deref().unwrap_or("<unset>")
                );
                println!("trust_remote_code: {}", record.trust_remote_code);
                0
            }
        }
        Ok(None) => {
            crate::cx_eprintln!(
                "{} llm models inspect: local model not found: {}",
                cli_app_name(),
                query
            );
            1
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models inspect: {e}", cli_app_name());
            1
        }
    }
}

fn llm_models_remove(app_name: &str, args: &[String]) -> i32 {
    let Some(query) = args.first() else {
        print_models_usage(app_name);
        return 2;
    };
    match remove_record(query) {
        Ok(Some(record)) => {
            if has_json_flag(args) {
                emit_json(&record_json(&record))
            } else {
                println!("ok");
                println!("removed_model_alias: {}", record.alias);
                println!("removed_model_id: {}", record.id);
                0
            }
        }
        Ok(None) => {
            crate::cx_eprintln!(
                "{} llm models remove: local model not found: {}",
                cli_app_name(),
                query
            );
            1
        }
        Err(e) => {
            crate::cx_eprintln!("{} llm models remove: {e}", cli_app_name());
            1
        }
    }
}

fn llm_models(app_name: &str, args: &[String]) -> i32 {
    match args.first().map(String::as_str).unwrap_or("list") {
        "list" => llm_models_list(&args[1..]),
        "add" => llm_models_add(app_name, &args[1..]),
        "inspect" => llm_models_inspect(app_name, &args[1..]),
        "remove" | "rm" => llm_models_remove(app_name, &args[1..]),
        _ => {
            print_models_usage(app_name);
            2
        }
    }
}

pub fn cmd_llm(app_name: &str, args: &[String]) -> i32 {
    match args.first().map(String::as_str).unwrap_or("show") {
        "show" => llm_show(),
        "check" => llm_check(app_name, args),
        "smoke" => llm_smoke(args),
        "models" => llm_models(app_name, &args[1..]),
        "use" => llm_use(app_name, args),
        "unset" => llm_unset(app_name, args),
        "set-backend" => llm_set_backend(app_name, args),
        "set-model" => llm_set_model(app_name, args),
        "clear-model" => llm_clear_model(),
        other => {
            crate::cx_eprintln!("{app_name} llm: unknown subcommand '{other}'");
            print_llm_usage(app_name);
            2
        }
    }
}
