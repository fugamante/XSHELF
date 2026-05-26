use serde_json::Value;

use crate::config::cli_app_name;
use crate::local_models::{
    AddLocalModelInput, add_record, find_record, list_records, record_json, registry_json,
    remove_record,
};

fn print_models_usage(app_name: &str) {
    crate::cx_eprintln!(
        "Usage: {app_name} llm models <list [--json]|add <alias> --backend <ollama|llamacpp|mlx> --model <model> [--replace] [--json]|inspect <alias-or-id> [--json]|remove <alias-or-id> [--json]>"
    );
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

pub(super) fn dispatch(app_name: &str, args: &[String]) -> i32 {
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
