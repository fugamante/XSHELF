use std::env;

use serde_json::json;

use crate::capture::budget_config_from_env;
use crate::config::app_config;
use crate::execmeta::toolchain_version_string;
use crate::paths::{resolve_log_file, resolve_quarantine_dir, resolve_state_file};
use crate::provider_adapter::{
    current_provider_capabilities, http_profile, selected_adapter_name,
    selected_http_provider_format, selected_provider_status_kind, selected_tq_caps,
};
use crate::runtime::{llm_backend, llm_model, logging_enabled};
use crate::state::{read_state_value, value_at_path};

fn value_to_display(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

fn state_pref(path: &str) -> String {
    read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, path))
        .map(value_to_display)
        .unwrap_or_else(|| "n/a".to_string())
}

fn print_version_header(
    app_name: &str,
    app_version: &str,
    cwd: &str,
    execution_path: &str,
    source: &str,
) {
    println!("name: {app_name}");
    println!("version: {}", toolchain_version_string(app_version));
    println!("cwd: {cwd}");
    println!("execution_path: {execution_path}");
    println!("source: {source}");
}

fn print_version_paths(log_file: &str, state_file: &str, quarantine_dir: &str) {
    println!("log_file: {log_file}");
    println!("state_file: {state_file}");
    println!("quarantine_dir: {quarantine_dir}");
}

fn print_version_runtime(mode: &str, backend: &str, active_model: &str, schema_relaxed: &str) {
    let adapter_name = selected_adapter_name();
    let provider_status = selected_provider_status_kind().as_str();
    let caps = current_provider_capabilities()
        .unwrap_or_else(|_| crate::provider_adapter::selected_provider_capabilities());
    println!("mode: {mode}");
    println!("llm_backend: {backend}");
    println!("provider_adapter: {adapter_name}");
    println!("provider_transport: {}", caps.transport);
    println!("provider_status: {provider_status}");
    if caps.transport == "http" {
        println!("http_provider_format: {}", selected_http_provider_format());
        let require_https = env::var("CX_HTTP_REQUIRE_HTTPS")
            .ok()
            .map(|v| !matches!(v.trim(), "0" | "false" | "FALSE" | "False"))
            .unwrap_or(true);
        let allow_local_http = env::var("CX_HTTP_ALLOW_LOCAL_HTTP")
            .ok()
            .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "True"))
            .unwrap_or(true);
        println!("http_require_https: {require_https}");
        println!("http_allow_local_http: {allow_local_http}");
        let allowed_hosts = env::var("CX_HTTP_ALLOWED_HOSTS")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "<any>".to_string());
        let pinning = env::var("CX_HTTP_TLS_PINNEDPUBKEY")
            .ok()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        println!("http_allowed_hosts: {allowed_hosts}");
        println!("http_tls_pinning: {}", if pinning { "set" } else { "off" });
    }
    println!("provider_jsonl_native: {}", caps.jsonl_native);
    println!("provider_schema_strict: {}", caps.schema_strict);
    println!("llm_model: {active_model}");
    println!("backend_resolution: backend={backend} model={active_model}");
    println!("schema_relaxed: {schema_relaxed}");
}

fn print_version_capture(capture_provider: &str, native_reduce: &str, prefer_native: &str) {
    println!("capture_provider: {capture_provider}");
    println!("native_reduce: {native_reduce}");
    println!("capture_prefer_native: {prefer_native}");
    println!("capture_external_dependencies: none");
}

fn core_payload(app_version: &str) -> serde_json::Value {
    let runtime_cfg = app_config();
    let mode = runtime_cfg.cx_mode.clone();
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() {
        "<unset>".to_string()
    } else {
        model
    };
    let capture_provider = runtime_cfg.capture_provider.clone();
    let adapter_name = selected_adapter_name();
    let provider_status = selected_provider_status_kind().as_str();
    let caps = current_provider_capabilities()
        .unwrap_or_else(|_| crate::provider_adapter::selected_provider_capabilities());
    let experiment_caps = selected_tq_caps();
    let capture_prefer_native = env::var("CX_CAPTURE_PREFER_NATIVE")
        .ok()
        .map(|v| v.trim() != "0")
        .unwrap_or(true);
    let budget_cfg = budget_config_from_env();
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let bash_fallback = execution_path.contains("bash");

    let mut provider = json!({
        "adapter": adapter_name,
        "transport": caps.transport,
        "status": provider_status,
        "jsonl_native": caps.jsonl_native,
        "schema_strict": caps.schema_strict,
    });
    if caps.transport == "http" {
        let require_https = env::var("CX_HTTP_REQUIRE_HTTPS")
            .ok()
            .map(|v| !matches!(v.trim(), "0" | "false" | "FALSE" | "False"))
            .unwrap_or(true);
        let allow_local_http = env::var("CX_HTTP_ALLOW_LOCAL_HTTP")
            .ok()
            .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "True"))
            .unwrap_or(true);
        let allowed_hosts = env::var("CX_HTTP_ALLOWED_HOSTS")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "<any>".to_string());
        let pinning = env::var("CX_HTTP_TLS_PINNEDPUBKEY")
            .ok()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        provider["http_provider_format"] = json!(selected_http_provider_format());
        provider["http_request_profile"] = json!(http_profile());
        provider["http_require_https"] = json!(require_https);
        provider["http_allow_local_http"] = json!(allow_local_http);
        provider["http_allowed_hosts"] = json!(allowed_hosts);
        provider["http_tls_pinning"] = json!(if pinning { "set" } else { "off" });
    }

    json!({
        "contract_version": "core.v1",
        "version": toolchain_version_string(app_version),
        "execution": {
            "path": execution_path,
            "bash_fallback_used": bash_fallback,
            "mode": mode,
        },
        "backend": {
            "name": backend,
            "active_model": active_model,
        },
        "provider": provider,
        "backend_capabilities": {
            "turboquant": {
                "cx_runtime_support": experiment_caps.turboquant_runtime_support,
                "selected_backend_role": experiment_caps.turboquant_backend_role,
                "memory_metric_kind": experiment_caps.turboquant_metric_kind,
            }
        },
        "capture": {
            "provider": capture_provider,
            "prefer_native": capture_prefer_native,
            "external_dependencies": "none",
        },
        "budget": {
            "chars": budget_cfg.budget_chars,
            "lines": budget_cfg.budget_lines,
            "clip_mode": budget_cfg.clip_mode,
            "clip_footer": budget_cfg.clip_footer,
        },
        "schema_enforcement": true,
        "logging_enabled": logging_enabled(),
        "log_file": log_file,
    })
}

fn print_version_preferences() {
    println!(
        "state.preferences.conventional_commits: {}",
        state_pref("preferences.conventional_commits")
    );
    println!(
        "state.preferences.pr_summary_format: {}",
        state_pref("preferences.pr_summary_format")
    );
}

fn version_payload(app_name: &str, app_version: &str) -> serde_json::Value {
    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    let cfg = app_config();
    let source = env::var("CX_SOURCE_LOCATION").unwrap_or_else(|_| "standalone:cxrs".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let state_file = resolve_state_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let quarantine_dir = resolve_quarantine_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() {
        "<unset>".to_string()
    } else {
        model
    };
    let caps = current_provider_capabilities()
        .unwrap_or_else(|_| crate::provider_adapter::selected_provider_capabilities());
    let provider_status = selected_provider_status_kind().as_str();
    let experiment_caps = selected_tq_caps();
    let native_reduce = env::var("CX_NATIVE_REDUCE").unwrap_or_else(|_| "1".to_string());
    let prefer_native = env::var("CX_CAPTURE_PREFER_NATIVE").unwrap_or_else(|_| "1".to_string());
    let mut provider = json!({
        "adapter": selected_adapter_name(),
        "transport": caps.transport,
        "status": provider_status,
        "jsonl_native": caps.jsonl_native,
        "schema_strict": caps.schema_strict,
    });
    if caps.transport == "http" {
        let require_https = env::var("CX_HTTP_REQUIRE_HTTPS")
            .ok()
            .map(|v| !matches!(v.trim(), "0" | "false" | "FALSE" | "False"))
            .unwrap_or(true);
        let allow_local_http = env::var("CX_HTTP_ALLOW_LOCAL_HTTP")
            .ok()
            .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "True"))
            .unwrap_or(true);
        let allowed_hosts = env::var("CX_HTTP_ALLOWED_HOSTS")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "<any>".to_string());
        let pinning = env::var("CX_HTTP_TLS_PINNEDPUBKEY")
            .ok()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        provider["http_provider_format"] = json!(selected_http_provider_format());
        provider["http_request_profile"] = json!(http_profile());
        provider["http_require_https"] = json!(require_https);
        provider["http_allow_local_http"] = json!(allow_local_http);
        provider["http_allowed_hosts"] = json!(allowed_hosts);
        provider["http_tls_pinning"] = json!(if pinning { "set" } else { "off" });
    }

    json!({
        "contract_version": "version.v1",
        "name": app_name,
        "version": toolchain_version_string(app_version),
        "cwd": cwd,
        "execution_path": execution_path,
        "source": source,
        "paths": {
            "log_file": log_file,
            "state_file": state_file,
            "quarantine_dir": quarantine_dir,
        },
        "runtime": {
            "mode": cfg.cx_mode,
            "backend": backend,
            "active_model": active_model,
            "schema_relaxed": cfg.schema_relaxed,
        },
        "provider": provider,
        "backend_capabilities": {
            "turboquant": {
                "cx_runtime_support": experiment_caps.turboquant_runtime_support,
                "selected_backend_role": experiment_caps.turboquant_backend_role,
                "memory_metric_kind": experiment_caps.turboquant_metric_kind,
            }
        },
        "capture": {
            "provider": cfg.capture_provider,
            "native_reduce": native_reduce,
            "prefer_native": prefer_native,
            "external_dependencies": "none",
        },
        "budget": {
            "chars": cfg.budget_chars,
            "lines": cfg.budget_lines,
            "clip_mode": cfg.clip_mode,
        },
        "cmd_timeout_secs": cfg.cmd_timeout_secs,
        "state_preferences": {
            "conventional_commits": state_pref("preferences.conventional_commits"),
            "pr_summary_format": state_pref("preferences.pr_summary_format"),
        },
    })
}

pub fn print_version(app_name: &str, app_version: &str, args: &[String]) {
    let json_out = args.iter().any(|a| a == "--json");
    if json_out {
        let payload = version_payload(app_name, app_version);
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
        );
        return;
    }

    let cwd = env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    let cfg = app_config();
    let source = env::var("CX_SOURCE_LOCATION").unwrap_or_else(|_| "standalone:cxrs".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let state_file = resolve_state_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let quarantine_dir = resolve_quarantine_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };

    print_version_header(app_name, app_version, &cwd, &execution_path, &source);
    print_version_paths(&log_file, &state_file, &quarantine_dir);
    print_version_runtime(
        &cfg.cx_mode,
        &backend,
        active_model,
        if cfg.schema_relaxed { "1" } else { "0" },
    );

    let native_reduce = env::var("CX_NATIVE_REDUCE").unwrap_or_else(|_| "1".to_string());
    let prefer_native = env::var("CX_CAPTURE_PREFER_NATIVE").unwrap_or_else(|_| "1".to_string());
    print_version_capture(&cfg.capture_provider, &native_reduce, &prefer_native);

    let experiment_caps = selected_tq_caps();
    println!(
        "backend_capability.turboquant_runtime_support: {}",
        experiment_caps.turboquant_runtime_support
    );
    println!(
        "backend_capability.turboquant_backend_role: {}",
        experiment_caps.turboquant_backend_role
    );
    println!(
        "backend_capability.turboquant_metric_kind: {}",
        experiment_caps.turboquant_metric_kind.unwrap_or("n/a")
    );
    println!("budget_chars: {}", cfg.budget_chars);
    println!("budget_lines: {}", cfg.budget_lines);
    println!("cmd_timeout_secs: {}", cfg.cmd_timeout_secs);
    println!("clip_mode: {}", cfg.clip_mode);
    print_version_preferences();
}

pub fn cmd_core(app_version: &str, args: &[String]) -> i32 {
    let json_out = args.iter().any(|a| a == "--json");
    let payload = core_payload(app_version);
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
        );
        return 0;
    }

    let runtime_cfg = app_config();
    let mode = runtime_cfg.cx_mode.clone();
    let backend = llm_backend();
    let model = llm_model();
    let active_model = if model.is_empty() { "<unset>" } else { &model };
    let capture_provider = runtime_cfg.capture_provider.clone();
    let adapter_name = selected_adapter_name();
    let provider_status = selected_provider_status_kind().as_str();
    let caps = current_provider_capabilities()
        .unwrap_or_else(|_| crate::provider_adapter::selected_provider_capabilities());
    let experiment_caps = selected_tq_caps();
    let capture_prefer_native = env::var("CX_CAPTURE_PREFER_NATIVE")
        .ok()
        .map(|v| v.trim() != "0")
        .unwrap_or(true);
    let budget_cfg = budget_config_from_env();
    let log_file = resolve_log_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unresolved>".to_string());
    let execution_path = env::var("CX_EXECUTION_PATH").unwrap_or_else(|_| "rust".to_string());
    let bash_fallback = execution_path.contains("bash");

    println!("== cxcore ==");
    println!("version: {}", toolchain_version_string(app_version));
    println!("execution_path: {execution_path}");
    println!("bash_fallback_used: {bash_fallback}");
    println!("backend: {backend}");
    println!("provider_adapter: {adapter_name}");
    println!("provider_transport: {}", caps.transport);
    println!("provider_status: {provider_status}");
    if caps.transport == "http" {
        println!("http_provider_format: {}", selected_http_provider_format());
        println!("http_request_profile: {}", http_profile());
        let require_https = env::var("CX_HTTP_REQUIRE_HTTPS")
            .ok()
            .map(|v| !matches!(v.trim(), "0" | "false" | "FALSE" | "False"))
            .unwrap_or(true);
        let allow_local_http = env::var("CX_HTTP_ALLOW_LOCAL_HTTP")
            .ok()
            .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "True"))
            .unwrap_or(true);
        println!("http_require_https: {require_https}");
        println!("http_allow_local_http: {allow_local_http}");
        let allowed_hosts = env::var("CX_HTTP_ALLOWED_HOSTS")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "<any>".to_string());
        let pinning = env::var("CX_HTTP_TLS_PINNEDPUBKEY")
            .ok()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        println!("http_allowed_hosts: {allowed_hosts}");
        println!("http_tls_pinning: {}", if pinning { "set" } else { "off" });
    }
    println!("provider_jsonl_native: {}", caps.jsonl_native);
    println!("provider_schema_strict: {}", caps.schema_strict);
    println!(
        "backend_capability.turboquant_runtime_support: {}",
        experiment_caps.turboquant_runtime_support
    );
    println!(
        "backend_capability.turboquant_backend_role: {}",
        experiment_caps.turboquant_backend_role
    );
    println!(
        "backend_capability.turboquant_metric_kind: {}",
        experiment_caps.turboquant_metric_kind.unwrap_or("n/a")
    );
    println!("active_model: {active_model}");
    println!("execution_mode: {mode}");
    println!("capture_provider: {capture_provider}");
    println!("capture_prefer_native: {capture_prefer_native}");
    println!("capture_external_dependencies: none");
    println!("budget_chars: {}", budget_cfg.budget_chars);
    println!("budget_lines: {}", budget_cfg.budget_lines);
    println!("cmd_timeout_secs: {}", runtime_cfg.cmd_timeout_secs);
    println!("clip_mode: {}", budget_cfg.clip_mode);
    println!("clip_footer: {}", budget_cfg.clip_footer);
    println!("schema_enforcement: true");
    println!("logging_enabled: {}", logging_enabled());
    println!("log_file: {log_file}");
    0
}
