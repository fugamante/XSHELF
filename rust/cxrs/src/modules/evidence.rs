//! Opt-in evidence retention; legacy capture and run-log contracts are untouched.
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use serde_json::json;

#[path = "evidence_reduce.rs"]
mod evidence_reduce;
#[path = "evidence_store.rs"]
mod evidence_store;

use evidence_store::{Record, load, reserve, save, validate_store};

const USAGE: &str = "evidence capture --store DIR -- COMMAND [ARGS...]\n\
    evidence import --store DIR --input FILE --profile cargo|xcode|generic --status N\n\
    evidence show --store DIR --id ID [--start N] [--lines N] [--offset N]";

struct Options {
    mode: String,
    store: PathBuf,
    input: Option<PathBuf>,
    profile: Option<String>,
    status: Option<i32>,
    id: Option<String>,
    start: usize,
    lines: usize,
    offset: usize,
    command: Vec<String>,
}

fn number(value: &str, max: usize) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0 && *n <= max)
        .ok_or_else(|| format!("expected an integer in 1..={max}"))
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mode = args
        .first()
        .filter(|s| matches!(s.as_str(), "capture" | "import" | "show"))
        .ok_or(USAGE)?
        .clone();
    let mut opt = Options {
        mode,
        store: PathBuf::new(),
        input: None,
        profile: None,
        status: None,
        id: None,
        start: 1,
        lines: 40,
        offset: 0,
        command: Vec::new(),
    };
    let mut seen = std::collections::HashSet::new();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--" && opt.mode == "capture" {
            opt.command = args[i + 1..].to_vec();
            break;
        }
        let key = args[i].as_str();
        if !seen.insert(key) {
            return Err(format!("duplicate option {key}"));
        }
        let value = args.get(i + 1).ok_or(USAGE)?;
        match (key, opt.mode.as_str()) {
            ("--store", _) => opt.store = PathBuf::from(value),
            ("--input", "import") => opt.input = Some(PathBuf::from(value)),
            ("--profile", "import") if matches!(value.as_str(), "cargo" | "xcode" | "generic") => {
                opt.profile = Some(value.clone())
            }
            ("--status", "import") => {
                opt.status = Some(
                    value
                        .parse::<i32>()
                        .ok()
                        .filter(|n| (0..=255).contains(n))
                        .ok_or("invalid exit status")?,
                )
            }
            ("--id", "show") => opt.id = Some(value.clone()),
            ("--start", "show") => opt.start = number(value, usize::MAX)?,
            ("--offset", "show") => {
                opt.offset = value.parse().map_err(|_| "invalid character offset")?
            }
            ("--lines", "show") => opt.lines = number(value, 200)?,
            _ => return Err(format!("invalid option {key} for {}", opt.mode)),
        }
        i += 2;
    }
    if opt.store.as_os_str().is_empty()
        || (opt.mode == "capture" && opt.command.is_empty())
        || (opt.mode == "import"
            && (opt.input.is_none() || opt.profile.is_none() || opt.status.is_none()))
        || (opt.mode == "show" && opt.id.is_none())
    {
        return Err(USAGE.into());
    }
    Ok(opt)
}

fn source(opt: &Options) -> Result<(String, i32, String), String> {
    if opt.mode == "capture" {
        let (output, status) = crate::capture::run_capture(&opt.command)?;
        return Ok((
            output,
            status,
            evidence_reduce::classify(&opt.command).into(),
        ));
    }
    let file = File::open(opt.input.as_ref().ok_or("missing input")?).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("import exceeds 8 MiB".into());
    }
    Ok((
        String::from_utf8_lossy(&bytes).into_owned(),
        opt.status.unwrap_or(1),
        opt.profile.clone().unwrap_or_default(),
    ))
}

fn ensure_external(store: &std::path::Path) -> Result<(), String> {
    let cwd = std::env::current_dir()
        .and_then(|p| p.canonicalize())
        .map_err(|e| e.to_string())?;
    let root = cwd
        .ancestors()
        .find(|p| p.join(".git").exists())
        .unwrap_or(&cwd);
    if store.starts_with(root) {
        return Err("store must be outside the caller project".into());
    }
    Ok(())
}

fn capture(opt: &Options) -> Result<i32, String> {
    let store = validate_store(&opt.store)?;
    // Reserve private storage before executing; collisions never overwrite evidence.
    ensure_external(&store)?;
    let (id, mut file) = reserve(&store)?;
    let (output, exit_status, profile) = source(opt)?;
    let summary = evidence_reduce::summarize(&profile, &output);
    let record = Record {
        schema_version: 1,
        id,
        exit_status,
        summary,
        output,
    };
    save(&mut file, &record)
        .map_err(|e| format!("child status {exit_status}; {e}; do not automatically rerun"))?;
    println!(
        "{}",
        json!({
            "schema_version": 1, "kind": "summary", "id": record.id,
            "exit_status": exit_status, "summary": record.summary,
            "retention": "explicit_store_until_user_removal"
        })
    );
    // Imports describe a past result; only execution returns the child's status.
    Ok(if opt.mode == "capture" {
        exit_status
    } else {
        0
    })
}

fn show(opt: &Options) -> Result<i32, String> {
    let store = validate_store(&opt.store)?;
    let record = load(&store, opt.id.as_deref().ok_or("missing ID")?)?;
    let total = record.output.lines().count();
    if opt.start > total.max(1) {
        return Err("start exceeds output line count".into());
    }
    let page = record
        .output
        .lines()
        .skip(opt.start - 1)
        .take(opt.lines)
        .collect::<Vec<_>>()
        .join("\n");
    let chars = page.chars().count();
    if opt.offset > chars {
        return Err("offset exceeds selected page".into());
    }
    let text: String = page.chars().skip(opt.offset).take(12000).collect();
    let end = opt.offset + text.chars().count();
    let truncated = end < chars;
    println!(
        "{}",
        json!({
            "schema_version": 1, "kind": "detail", "id": record.id,
            "exit_status": record.exit_status, "start": opt.start, "total_lines": total,
            "returned_lines": text.lines().count(), "truncated": truncated,
            "offset": opt.offset, "next_offset": if truncated { Some(end) } else { None },
            "next_start": if !truncated && opt.lines < total.saturating_sub(opt.start - 1) { Some(opt.start + opt.lines) } else { None },
            "text": text
        })
    );
    Ok(0)
}

pub fn handler(args: &[String]) -> i32 {
    if args == ["--help"] {
        println!("{USAGE}");
        return 0;
    }
    let opt = match parse(args) {
        Ok(opt) => opt,
        Err(e) => {
            crate::cx_eprintln!("evidence: {e}");
            return 2;
        }
    };
    match if opt.mode == "show" {
        show(&opt)
    } else {
        capture(&opt)
    } {
        Ok(status) => status,
        Err(e) => {
            crate::cx_eprintln!("evidence: {e}");
            1
        }
    }
}
