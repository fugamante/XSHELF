use crate::state::{read_state_value, value_at_path};

fn parse_bool_str(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn env_bool(name: &str) -> Option<bool> {
    std::env::var(name).ok().and_then(|v| parse_bool_str(&v))
}

fn state_bool(path: &str) -> Option<bool> {
    let state = read_state_value()?;
    let val = value_at_path(&state, path)?;
    if let Some(b) = val.as_bool() {
        return Some(b);
    }
    if let Some(s) = val.as_str() {
        return parse_bool_str(s);
    }
    if let Some(n) = val.as_i64() {
        return Some(n != 0);
    }
    None
}

pub fn resolve_json_mode(cli: Option<bool>, command_default: bool) -> bool {
    if let Some(v) = cli {
        return v;
    }
    if let Some(v) = env_bool("CX_JSON_DEFAULT") {
        return v;
    }
    if let Some(v) = state_bool("preferences.default_json_output") {
        return v;
    }
    command_default
}
