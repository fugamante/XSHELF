use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::cli_app_name;

use crate::config::app_config;
use crate::contract_versions::POLICY_SHOW_JSON_CONTRACT_VERSION;
use crate::paths::repo_root;

#[derive(Debug, Clone)]
pub enum SafetyDecision {
    Safe,
    Dangerous(String),
}

fn command_tokens(cmd: &str) -> Result<Vec<String>, String> {
    shell_words::split(cmd).map_err(|e| format!("invalid shell quoting: {e}"))
}

fn has_shell_syntax(cmd: &str) -> bool {
    cmd.chars()
        .any(|c| matches!(c, '|' | '&' | ';' | '<' | '>' | '\n' | '\r'))
}

fn is_interpreter(name: &str) -> bool {
    let base = command_name(name);
    matches!(
        base.as_str(),
        "sh" | "bash" | "zsh" | "dash" | "fish" | "ksh" | "perl" | "ruby" | "node" | "php" | "lua"
    ) || base.starts_with("python")
        || base.starts_with("pypy")
}

fn is_launcher(name: &str) -> bool {
    let base = command_name(name);
    matches!(
        base.as_str(),
        "env"
            | "command"
            | "exec"
            | "nice"
            | "nohup"
            | "timeout"
            | "stdbuf"
            | "busybox"
            | "xargs"
            | "find"
    )
}

pub(crate) fn is_env_assignment(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn safe_command_argv(cmd: &str, repo_root: &Path) -> Result<Vec<String>, String> {
    if has_shell_syntax(cmd) {
        return Err("contains shell control or redirection syntax".to_string());
    }
    let argv = command_tokens(cmd)?;
    if argv.is_empty() {
        return Err("empty command".to_string());
    }
    let cwd = env::current_dir()
        .map_err(|error| format!("cannot resolve execution directory: {error}"))?;
    match evaluate_tokens(cmd, repo_root, &cwd, Some(&argv)) {
        SafetyDecision::Safe => Ok(argv),
        SafetyDecision::Dangerous(reason) => Err(reason),
    }
}

fn command_name(program: &str) -> String {
    let name = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    // GNU coreutils may be installed alongside system tools with a g prefix.
    match name {
        "grm" | "gdd" | "gcp" | "gmv" | "ginstall" | "gtouch" | "gmkdir" | "gchmod" | "gchown"
        | "gtee" | "genv" | "gnice" | "gnohup" | "gtimeout" | "gstdbuf" | "gxargs" | "gfind" => {
            name[1..].to_string()
        }
        _ => name.to_string(),
    }
}

fn writes_files(program: &str) -> bool {
    matches!(
        program,
        "tee" | "touch" | "mkdir" | "cp" | "mv" | "install" | "dd" | "chmod" | "chown" | "rm"
    )
}

fn short_target<'a>(token: &'a str, program: &str) -> Option<&'a str> {
    let flags = token
        .strip_prefix('-')
        .filter(|flags| !flags.starts_with('-'))?;
    for (index, flag) in flags.char_indices() {
        if flag == 't' {
            return flags.get(index + 1..).filter(|path| !path.is_empty());
        }
        // These options consume the rest of a cluster as data, not further flags.
        if flag == 'S' || (program == "install" && matches!(flag, 'g' | 'm' | 'o')) {
            break;
        }
    }
    None
}

fn collect_write_candidates(tokens: &[String]) -> Vec<String> {
    let mut candidates = Vec::new();
    let program = tokens
        .first()
        .map(|value| command_name(value))
        .unwrap_or_default();
    let mut operands = false;
    for (index, token) in tokens.iter().enumerate().skip(1) {
        if token == "--" {
            operands = true;
            continue;
        }
        if matches!(token.as_str(), ">" | ">>" | "tee")
            && let Some(next) = tokens.get(index + 1)
        {
            candidates.push(next.clone());
        }
        if let Some(path) = token.strip_prefix("of=") {
            candidates.push(path.to_string());
        }
        if !operands && matches!(program.as_str(), "cp" | "mv" | "install") {
            if let Some(path) = short_target(token, &program) {
                candidates.push(path.to_string());
            }
            if let Some((option, path)) = token
                .strip_prefix("--")
                .and_then(|option| option.split_once('='))
                && !option.is_empty()
                && "target-directory".starts_with(option)
            {
                candidates.push(path.to_string());
            }
        }
        if token.starts_with('/')
            || token.starts_with("~/")
            || token == "~"
            || token.starts_with("$HOME")
            || token.starts_with("${HOME}")
            || (writes_files(&program) && program != "dd" && (operands || !token.starts_with('-')))
        {
            candidates.push(token.clone());
        }
    }
    candidates
}

fn path_is_outside_repo(p: &str, repo_root: &Path, cwd: &Path) -> bool {
    let path = p;
    if path.is_empty() {
        return false;
    }
    if path.contains("..") || path == "~" {
        return true;
    }

    let root_abs = canonical_or_owned(repo_root);
    let candidate = resolve_candidate_path(path, cwd);
    let Some(candidate) = candidate else {
        return true;
    };
    if candidate.exists() {
        let canon = canonical_or_owned(&candidate);
        return !canon_starts_with(&canon, &root_abs);
    }
    // Check the nearest existing ancestor, including symlinks above missing parents.
    if let Some(parent) = candidate.ancestors().skip(1).find(|parent| parent.exists()) {
        let parent_canon = canonical_or_owned(parent);
        if !canon_starts_with(&parent_canon, &root_abs) {
            return true;
        }
    }
    !lexically_inside_root(&candidate, repo_root)
}

fn write_targets_outside_repo(tokens: &[String], repo_root: &Path, cwd: &Path) -> bool {
    collect_write_candidates(tokens)
        .into_iter()
        .any(|p| path_is_outside_repo(&p, repo_root, cwd))
}

fn canonical_or_owned(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn canon_starts_with(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

fn resolve_candidate_path(path: &str, repo_root: &Path) -> Option<PathBuf> {
    if let Some(home) = env::var_os("HOME") {
        if path == "~" {
            return Some(PathBuf::from(home));
        }
        if let Some(rest) = path.strip_prefix("~/") {
            return Some(PathBuf::from(home).join(rest));
        }
        if let Some(rest) = path.strip_prefix("$HOME/") {
            return Some(PathBuf::from(home).join(rest));
        }
        if let Some(rest) = path.strip_prefix("${HOME}/") {
            return Some(PathBuf::from(home).join(rest));
        }
    }
    if path.starts_with('$') {
        return None;
    }
    if path.starts_with('/') {
        return Some(PathBuf::from(path));
    }
    Some(repo_root.join(path))
}

fn lexically_inside_root(candidate: &Path, repo_root: &Path) -> bool {
    let root_s = repo_root.to_string_lossy().to_string();
    let cand = candidate.to_string_lossy().to_string();
    cand == root_s || cand.starts_with(&(root_s + "/"))
}

fn destructive_program(program: &str) -> bool {
    matches!(
        program,
        "reboot"
            | "shutdown"
            | "halt"
            | "poweroff"
            | "fdisk"
            | "sfdisk"
            | "cfdisk"
            | "diskutil"
            | "diskpart"
            | "newfs"
    ) || program.starts_with("mkfs")
        || program.starts_with("newfs_")
}

fn matches_rm_flags(tokens: &[String]) -> bool {
    let mut recursive = false;
    let mut force = false;
    for token in tokens
        .iter()
        .skip(1)
        .take_while(|token| token.as_str() != "--")
    {
        if let Some(option) = token.strip_prefix("--") {
            // GNU rm accepts unambiguous long-option abbreviations.
            recursive |= !option.is_empty() && "recursive".starts_with(option);
            force |= !option.is_empty() && "force".starts_with(option);
        } else if let Some(flags) = token.strip_prefix('-') {
            recursive |= flags.contains('r') || flags.contains('R');
            force |= flags.contains('f');
        }
    }
    recursive && force
}

fn matches_curl_pipe_shell(lower: &str) -> bool {
    lower.contains("curl ")
        && lower.contains('|')
        && (lower.contains("| bash") || lower.contains("| sh") || lower.contains("| zsh"))
}

fn matches_protected_chmod_chown(lower: &str) -> bool {
    (lower.contains("chmod ") || lower.contains("chown "))
        && (lower.contains("/system") || lower.contains("/library") || lower.contains("/usr"))
        && !lower.contains("/usr/local")
}

fn matches_protected_redirect(lower: &str) -> bool {
    let writes_protected = lower.contains("> /system")
        || lower.contains(">> /system")
        || lower.contains("> /library")
        || lower.contains(">> /library")
        || lower.contains("> /usr")
        || lower.contains(">> /usr")
        || (lower.contains("tee ")
            && (lower.contains(" /system")
                || lower.contains(" /library")
                || lower.contains(" /usr")));
    writes_protected && !lower.contains("/usr/local")
}

pub fn evaluate_command_safety(cmd: &str, repo_root: &Path) -> SafetyDecision {
    let cwd = env::current_dir().unwrap_or_else(|_| repo_root.to_path_buf());
    evaluate_tokens(cmd, repo_root, &cwd, None)
}

fn evaluate_tokens(
    cmd: &str,
    repo_root: &Path,
    cwd: &Path,
    parsed_tokens: Option<&[String]>,
) -> SafetyDecision {
    let compact = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = compact.to_lowercase();
    let tokens = match parsed_tokens {
        Some(tokens) => tokens.to_vec(),
        None => match command_tokens(cmd) {
            Ok(tokens) => tokens,
            Err(reason) => return SafetyDecision::Dangerous(reason),
        },
    };
    let Some(program) = tokens.first() else {
        return SafetyDecision::Dangerous("empty command".to_string());
    };
    let name = command_name(program);
    if matches!(name.as_str(), "sudo" | "su" | "doas" | "pkexec") {
        return SafetyDecision::Dangerous("contains sudo or privilege launcher".to_string());
    }
    if is_env_assignment(program) {
        return SafetyDecision::Dangerous(
            "leading environment assignment is unsupported".to_string(),
        );
    }
    if is_launcher(program) || is_interpreter(program) {
        return SafetyDecision::Dangerous(
            "delegates execution through a launcher or interpreter".to_string(),
        );
    }
    if destructive_program(&name) {
        return SafetyDecision::Dangerous("system control or disk management command".to_string());
    }
    if matches!(name.as_str(), "systemctl" | "launchctl")
        && tokens.iter().skip(1).any(|argument| {
            matches!(
                argument.as_str(),
                "reboot" | "poweroff" | "halt" | "kexec" | "soft-reboot"
            )
        })
    {
        return SafetyDecision::Dangerous("system control command".to_string());
    }
    if name == "launchctl"
        && tokens
            .iter()
            .skip(1)
            .any(|argument| matches!(argument.as_str(), "asuser" | "bsexec"))
    {
        return SafetyDecision::Dangerous(
            "delegates execution through a launcher or interpreter".to_string(),
        );
    }
    if name == "rm" && matches_rm_flags(&tokens) {
        return SafetyDecision::Dangerous("contains rm -rf pattern".to_string());
    }
    if matches_curl_pipe_shell(&lower) {
        return SafetyDecision::Dangerous("contains curl pipe shell pattern".to_string());
    }
    if matches!(name.as_str(), "chmod" | "chown")
        && matches_protected_chmod_chown(&format!(
            "{} {}",
            name,
            tokens[1..].join(" ").to_lowercase()
        ))
    {
        return SafetyDecision::Dangerous("chmod/chown on protected system path".to_string());
    }
    if matches_protected_redirect(&lower) {
        return SafetyDecision::Dangerous("write redirection to protected system path".to_string());
    }
    if (writes_files(&name) || lower.contains('>'))
        && write_targets_outside_repo(&tokens, repo_root, cwd)
    {
        return SafetyDecision::Dangerous("write target outside repo root".to_string());
    }
    SafetyDecision::Safe
}

fn handle_policy_check(args: &[String], app_name: &str) -> i32 {
    if args.len() < 2 {
        crate::cx_eprintln!("Usage: {app_name} policy check <command...>");
        return 2;
    }
    let candidate = args[1..].join(" ");
    let root = repo_root()
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    match evaluate_command_safety(&candidate, &root) {
        SafetyDecision::Safe => println!("safe"),
        SafetyDecision::Dangerous(reason) => println!("dangerous: {reason}"),
    }
    0
}

fn policy_rules() -> Vec<&'static str> {
    vec![
        "Block: sudo",
        "Block: privilege launchers",
        "Block: system control and disk management commands",
        "Block: rm -rf family",
        "Block: curl | bash/sh/zsh",
        "Block: shell/interpreter commands and command launchers in fix-run",
        "Block: leading environment assignments in fix-run",
        "Block: chmod/chown on /System,/Library,/usr (except /usr/local)",
        "Block: write operations outside repo root",
    ]
}

fn show_policy_text() {
    let cfg = app_config();
    println!("== {} policy show ==", cli_app_name());
    println!("Active safety rules:");
    for rule in policy_rules() {
        println!("- {rule}");
    }
    println!();
    println!("Unsafe override state:");
    println!(
        "--unsafe / CX_UNSAFE=1: {}",
        if cfg.cx_unsafe { "on" } else { "off" }
    );
    println!(
        "CXFIX_FORCE=1: {}",
        if cfg.cxfix_force { "on" } else { "off" }
    );
}

fn show_policy_json() -> i32 {
    let cfg = app_config();
    let value = serde_json::json!({
        "contract_version": POLICY_SHOW_JSON_CONTRACT_VERSION,
        "rules": policy_rules(),
        "overrides": {
            "unsafe_enabled": cfg.cx_unsafe,
            "cxfix_force_enabled": cfg.cxfix_force
        }
    });
    match serde_json::to_string_pretty(&value) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!("{} policy show: failed to render json: {e}", cli_app_name());
            1
        }
    }
}

fn print_policy_help(app_name: &str) {
    println!("== {} policy ==", cli_app_name());
    println!("Dangerous command patterns blocked by default in fix-run:");
    println!("- sudo (any)");
    println!("- rm -rf / rm -fr forms");
    println!("- curl | bash/sh/zsh");
    println!("- shell/interpreter commands and command launchers");
    println!("- leading environment assignments");
    println!("- chmod/chown on /System, /Library, /usr (except /usr/local)");
    println!("- shell redirection/tee writes to /System, /Library, /usr (except /usr/local)");
    println!();
    println!("Overrides:");
    println!("- --unsafe          allow dangerous execution for current command");
    println!("- CXFIX_RUN=1       execute suggested commands");
    println!("- CXFIX_FORCE=1     allow dangerous commands");
    println!();
    println!("Examples:");
    println!("- {app_name} policy check \"sudo rm -rf /tmp/foo\"");
    println!("- {app_name} policy check \"chmod 755 /usr/local/bin/tool\"");
}

pub fn cmd_policy(args: &[String], app_name: &str) -> i32 {
    let show_json = args.iter().any(|v| v == "--json");
    match args.first().map(String::as_str) {
        Some("check") => handle_policy_check(args, app_name),
        Some("show") | Some("--json") | None => {
            if show_json {
                show_policy_json()
            } else {
                show_policy_text();
                0
            }
        }
        _ => {
            print_policy_help(app_name);
            0
        }
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
