use serde_json::Value;
use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;
use std::thread;
use std::time::{Duration, SystemTime};

use crate::config::cli_app_name;
use crate::contract_versions::TASK_EVENTS_JSONL_CONTRACT_VERSION;
use crate::execmeta::utc_now_iso;
use crate::logs::{
    append_jsonl, load_follow_values, load_values_file, open_repo_file, read_capped_line_with_end,
};
use crate::paths::task_events_log;

const FOLLOW_BYTES: usize = 4 * 1024 * 1024;
const FOLLOW_ROWS: usize = 1024;

#[derive(Debug, Clone)]
pub struct TaskEvent<'a> {
    pub event: &'a str,
    pub task_id: Option<&'a str>,
    pub status: Option<&'a str>,
    pub backend: Option<&'a str>,
    pub requested_backend: Option<&'a str>,
    pub execution_id: Option<&'a str>,
    pub failure_class: Option<&'a str>,
    pub queue_ms: Option<u64>,
    pub wave_index: Option<u64>,
    pub wave_mode: Option<&'a str>,
    pub wave_size: Option<u64>,
    pub scheduled: Option<u64>,
    pub complete: Option<u64>,
    pub failed: Option<u64>,
    pub blocked: Option<u64>,
    pub critical_errors: Option<u64>,
    pub halted_remaining: Option<u64>,
}

impl<'a> TaskEvent<'a> {
    pub fn new(event: &'a str) -> Self {
        Self {
            event,
            task_id: None,
            status: None,
            backend: None,
            requested_backend: None,
            execution_id: None,
            failure_class: None,
            queue_ms: None,
            wave_index: None,
            wave_mode: None,
            wave_size: None,
            scheduled: None,
            complete: None,
            failed: None,
            blocked: None,
            critical_errors: None,
            halted_remaining: None,
        }
    }
}

pub fn emit(enabled: bool, event: TaskEvent<'_>) {
    if !enabled {
        return;
    }
    let payload = event_value(event);
    if let Some(path) = task_events_log()
        && let Err(e) = append_jsonl(&path, &payload)
    {
        crate::cx_eprintln!(
            "{} task events: failed to append event: {e}",
            cli_app_name()
        );
    }
    if let Ok(line) = serde_json::to_string(&payload) {
        crate::cx_eprintln!("{line}");
    }
}

fn event_value(event: TaskEvent<'_>) -> Value {
    let mut payload = serde_json::json!({
        "contract_version": TASK_EVENTS_JSONL_CONTRACT_VERSION,
        "event": event.event,
        "at": utc_now_iso()
    });
    let Some(obj) = payload.as_object_mut() else {
        return payload;
    };
    insert_str(obj, "task_id", event.task_id);
    insert_str(obj, "status", event.status);
    insert_str(obj, "backend", event.backend);
    insert_str(obj, "requested_backend", event.requested_backend);
    insert_str(obj, "execution_id", event.execution_id);
    insert_str(obj, "failure_class", event.failure_class);
    insert_u64(obj, "queue_ms", event.queue_ms);
    insert_u64(obj, "wave_index", event.wave_index);
    insert_str(obj, "wave_mode", event.wave_mode);
    insert_u64(obj, "wave_size", event.wave_size);
    insert_u64(obj, "scheduled", event.scheduled);
    insert_u64(obj, "complete", event.complete);
    insert_u64(obj, "failed", event.failed);
    insert_u64(obj, "blocked", event.blocked);
    insert_u64(obj, "critical_errors", event.critical_errors);
    insert_u64(obj, "halted_remaining", event.halted_remaining);
    payload
}

pub fn cmd_task_events(app_name: &str, args: &[String]) -> i32 {
    let usage = format!("Usage: {app_name} task events [--limit N] [--json|--jsonl] [--follow]");
    let mut limit = 50usize;
    let mut as_json = false;
    let mut follow = false;
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" => {
                let Some(v) = args.get(i + 1) else {
                    crate::cx_eprintln!("{usage}");
                    return 2;
                };
                let Ok(n) = v.parse::<usize>() else {
                    crate::cx_eprintln!(
                        "{} task events: --limit must be an integer",
                        cli_app_name()
                    );
                    return 2;
                };
                limit = n;
                i += 2;
            }
            "--json" => {
                as_json = true;
                i += 1;
            }
            "--jsonl" => {
                as_json = false;
                i += 1;
            }
            "--follow" => {
                follow = true;
                i += 1;
            }
            other => {
                crate::cx_eprintln!("{} task events: unknown flag '{other}'", cli_app_name());
                return 2;
            }
        }
    }

    let Some(path) = task_events_log() else {
        crate::cx_eprintln!(
            "{} task events: unable to resolve task event log",
            cli_app_name()
        );
        return 1;
    };
    if as_json && follow {
        crate::cx_eprintln!(
            "{} task events: --follow requires --jsonl output",
            cli_app_name()
        );
        return 2;
    }
    let initial = if follow {
        match open_repo_file(&path) {
            Ok(file) => {
                let identity = match file_identity(&file) {
                    Ok(value) => value,
                    Err(e) => {
                        crate::cx_eprintln!("{} task events: {e}", cli_app_name());
                        return 1;
                    }
                };
                match load_follow_values(file, limit) {
                    Ok((rows, offset)) => Some((rows, offset, Some(identity))),
                    Err(e) => {
                        crate::cx_eprintln!("{} task events: {e}", cli_app_name());
                        return 1;
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                crate::cx_eprintln!("{} task events: {e}", cli_app_name());
                return 1;
            }
        }
    } else {
        match open_repo_file(&path) {
            Ok(file) => match load_values_file(file, limit) {
                Ok(rows) => Some((rows, 0, None)),
                Err(e) => {
                    crate::cx_eprintln!("{} task events: {e}", cli_app_name());
                    return 1;
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                crate::cx_eprintln!("{} task events: {e}", cli_app_name());
                return 1;
            }
        }
    };
    let Some((rows, offset, identity)) = initial else {
        if as_json {
            println!("[]");
        }
        return if follow {
            follow_events(&path, 0, None)
        } else {
            0
        };
    };
    if as_json {
        match serde_json::to_string_pretty(&rows) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                crate::cx_eprintln!("{} task events: failed to render json: {e}", cli_app_name());
                return 1;
            }
        }
    } else {
        for row in &rows {
            match serde_json::to_string(row) {
                Ok(s) => println!("{s}"),
                Err(e) => {
                    crate::cx_eprintln!(
                        "{} task events: failed to render event jsonl: {e}",
                        cli_app_name()
                    );
                    return 1;
                }
            }
        }
    }
    if follow {
        follow_events(&path, offset, identity)
    } else {
        0
    }
}

#[cfg(unix)]
fn file_identity(file: &File) -> std::io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn file_identity(_file: &File) -> std::io::Result<(u64, u64)> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "safe task event following requires Unix directory descriptors",
    ))
}

fn follow_events(path: &Path, mut offset: u64, mut identity: Option<(u64, u64)>) -> i32 {
    let mut observed: Option<(u64, Option<SystemTime>)> = None;
    loop {
        let file = match open_repo_file(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                observed = None;
                thread::sleep(Duration::from_millis(500));
                continue;
            }
            Err(e) => {
                crate::cx_eprintln!(
                    "{} task events: cannot follow {}: {e}",
                    cli_app_name(),
                    path.display()
                );
                return 1;
            }
        };
        let current = match file_identity(&file) {
            Ok(value) => value,
            Err(e) => {
                crate::cx_eprintln!(
                    "{} task events: cannot inspect followed log: {e}",
                    cli_app_name()
                );
                return 1;
            }
        };
        let (length, modified) = match file.metadata() {
            Ok(value) => (value.len(), value.modified().ok()),
            Err(e) => {
                crate::cx_eprintln!(
                    "{} task events: cannot inspect followed log: {e}",
                    cli_app_name()
                );
                return 1;
            }
        };
        if identity == Some(current) && observed == Some((length, modified)) {
            thread::sleep(Duration::from_millis(500));
            continue;
        }
        if identity != Some(current) || length < offset {
            offset = 0;
        }
        identity = Some(current);
        let (rows, pending) = match read_appended_values(file, &mut offset) {
            Ok(result) => result,
            Err(e) => {
                crate::cx_eprintln!("{} task events: {e}", cli_app_name());
                return 1;
            }
        };
        observed = if pending || offset >= length {
            Some((length, modified))
        } else {
            None
        };
        for row in rows {
            match serde_json::to_string(&row) {
                Ok(s) => println!("{s}"),
                Err(e) => {
                    crate::cx_eprintln!(
                        "{} task events: failed to render event jsonl: {e}",
                        cli_app_name()
                    );
                    return 1;
                }
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn read_appended_values(file: File, offset: &mut u64) -> Result<(Vec<Value>, bool), String> {
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(*offset))
        .map_err(|e| format!("cannot seek followed log: {e}"))?;
    let mut out = Vec::new();
    let mut scanned = 0usize;
    let mut pending = false;
    loop {
        let start = reader
            .stream_position()
            .map_err(|e| format!("cannot inspect followed offset: {e}"))?;
        let row = read_capped_line_with_end(&mut reader)
            .map_err(|e| format!("cannot read followed log: {e}"))?;
        let Some((line, complete)) = row else { break };
        if !complete {
            // A writer may still be completing this JSONL row.
            *offset = start;
            pending = true;
            break;
        }
        let end = reader
            .stream_position()
            .map_err(|e| format!("cannot inspect followed offset: {e}"))?;
        *offset = end;
        scanned = scanned.saturating_add((end - start) as usize);
        if !line.iter().all(u8::is_ascii_whitespace)
            && let Ok(v) = serde_json::from_slice::<Value>(&line)
        {
            out.push(v);
        }
        if scanned >= FOLLOW_BYTES || out.len() >= FOLLOW_ROWS {
            break;
        }
    }
    Ok((out, pending))
}

fn insert_str(obj: &mut serde_json::Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        obj.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn insert_u64(obj: &mut serde_json::Map<String, Value>, key: &str, value: Option<u64>) {
    if let Some(value) = value {
        obj.insert(key.to_string(), Value::from(value));
    }
}
