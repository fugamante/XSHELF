use serde_json::Value;
use std::io::{self, IsTerminal, Write};
use std::process::Command;

use crate::config::{app_config, cli_app_name};
use crate::local_models::{find_record_for_backend, normalize_backend};
use crate::model_authority::{approved_pref, set_approved_pref};
use crate::process::run_command_output_with_timeout;

pub fn llm_backend() -> String {
    // Task execution applies scoped operator overrides after config initialization.
    match std::env::var("CX_LLM_BACKEND") {
        Ok(value) if !value.trim().is_empty() => {
            normalize_backend(&value).unwrap_or("primary").to_string()
        }
        _ => app_config().llm_backend.clone(),
    }
}

pub fn llm_model() -> String {
    let (key, default) = match llm_backend().as_str() {
        "ollama" => ("CX_OLLAMA_MODEL", &app_config().ollama_model),
        "llamacpp" => ("CX_LLAMA_CPP_MODEL", &app_config().llama_cpp_model),
        "mlx" => ("CX_MLX_MODEL", &app_config().mlx_model),
        _ => ("CX_MODEL", &app_config().primary_model),
    };
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.clone())
}

pub fn logging_enabled() -> bool {
    app_config().cxlog_enabled
}

pub fn ollama_model_preference() -> String {
    std::env::var("CX_OLLAMA_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| approved_pref("preferences.ollama_model").ok().flatten())
        .unwrap_or_default()
}

pub fn llama_cpp_model_preference() -> String {
    std::env::var("CX_LLAMA_CPP_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| approved_pref("preferences.llama_cpp_model").ok().flatten())
        .unwrap_or_default()
}

pub fn mlx_model_preference() -> String {
    std::env::var("CX_MLX_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| approved_pref("preferences.mlx_model").ok().flatten())
        .unwrap_or_default()
}

fn is_interactive_tty() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

fn ollama_list_models() -> Vec<String> {
    let mut cmd = Command::new("ollama");
    cmd.arg("list");
    let output = match run_command_output_with_timeout(cmd, "ollama list") {
        Ok(v) if v.status.success() => v,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut out: Vec<String> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i == 0 && line.to_lowercase().contains("name") {
            continue;
        }
        let name = line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if !name.is_empty() {
            out.push(name);
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn resolve_ollama_model_for_run() -> Result<String, String> {
    let model = llm_model();
    if !model.trim().is_empty() {
        return resolve_run_model(model, "ollama", "CX_OLLAMA_MODEL");
    }
    if !is_interactive_tty() {
        return Err(format!(
            "ollama model is unset; set CX_OLLAMA_MODEL or run '{} llm set-model <model>'",
            cli_app_name()
        )
        .to_string());
    }

    let models = ollama_list_models();
    crate::cx_eprintln!("{}: no default Ollama model configured.", cli_app_name());
    if models.is_empty() {
        crate::cx_eprintln!("No local models found from 'ollama list'.");
        crate::cx_eprintln!("Pull one first (example: ollama pull llama3.1) then set it.");
        return Err("ollama model selection aborted".to_string());
    }
    crate::cx_eprintln!("Select a default model (persisted to .cx/state.json):");
    for (idx, m) in models.iter().enumerate() {
        crate::cx_eprintln!("  {}. {}", idx + 1, m);
    }
    eprint!("Enter number or model name: ");
    let _ = io::stderr().flush();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("failed reading selection: {e}"))?;
    let selected_raw = input.trim();
    if selected_raw.is_empty() {
        return Err("no model selected".to_string());
    }
    let selected = if let Ok(n) = selected_raw.parse::<usize>() {
        models
            .get(n.saturating_sub(1))
            .cloned()
            .ok_or_else(|| "invalid model index".to_string())?
    } else {
        selected_raw.to_string()
    };
    set_approved_pref("preferences.ollama_model", Value::String(selected.clone()))?;
    crate::cx_eprintln!(
        "{}: default Ollama model set to '{}'.",
        cli_app_name(),
        selected
    );
    Ok(selected)
}

pub fn resolve_llama_cpp_model_for_run() -> Result<String, String> {
    let model = llm_model();
    if !model.trim().is_empty() {
        return resolve_run_model(model, "llamacpp", "CX_LLAMA_CPP_MODEL");
    }
    Err(format!(
        "llama.cpp model is unset; set CX_LLAMA_CPP_MODEL to a GGUF path or run '{} llm set-model <path.gguf>' while backend is llamacpp",
        cli_app_name()
    ))
}

pub fn resolve_mlx_model_for_run() -> Result<String, String> {
    let model = llm_model();
    if !model.trim().is_empty() {
        return resolve_run_model(model, "mlx", "CX_MLX_MODEL");
    }
    Err(format!(
        "MLX model is unset; set CX_MLX_MODEL or run '{} llm set-model <model>' while backend is mlx",
        cli_app_name()
    ))
}

fn resolve_run_model(model: String, backend: &str, env_key: &str) -> Result<String, String> {
    match find_record_for_backend(&model, backend) {
        Ok(Some(record)) => Ok(record.resolved_model),
        Ok(None) => Ok(model),
        // A broken repository registry cannot cancel an explicit process model.
        Err(_)
            if std::env::var(env_key)
                .ok()
                .is_some_and(|v| !v.trim().is_empty()) =>
        {
            Ok(model)
        }
        Err(e) => Err(e),
    }
}

pub fn llm_bin_name() -> &'static str {
    match llm_backend().as_str() {
        "ollama" => "ollama",
        "llamacpp" => "llama-cli",
        "mlx" => "python3",
        _ => concat!("co", "dex"),
    }
}
