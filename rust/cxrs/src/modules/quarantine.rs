use chrono::Utc;
#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::io::Read;
use std::path::PathBuf;

#[cfg(unix)]
use rustix::fs::{self as unix_fs, Dir, Mode, OFlags};
#[cfg(unix)]
use std::io::Write;

use crate::config::cli_app_name;
use crate::execmeta::utc_now_iso;
use crate::types::{QuarantineAttempt, QuarantineRecord};
use crate::util::sha256_hex;

#[cfg(unix)]
const MAX_RECORD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ID_BYTES: usize = 250; // Leaves room for ".json" in a 255-byte filename.
#[cfg(unix)]
const DIR_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

struct QuarantineDir {
    path: PathBuf,
    #[cfg(unix)]
    handle: File,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[cfg(unix)]
fn open_quarantine_dir(create: bool) -> Result<Option<QuarantineDir>, String> {
    let anchor = crate::paths::repo_root()
        .or_else(crate::paths::home_dir)
        .ok_or_else(|| "unable to resolve quarantine directory".to_string())?;
    let path = anchor.join(".cx").join("quarantine");
    // The selected repo or HOME is the trust anchor. Descendants are never
    // resolved through symlinks, including when another process swaps them.
    let selected = std::fs::canonicalize(&anchor)
        .map_err(|e| format!("cannot open quarantine directory {}: {e}", path.display()))?;
    let mut handle = File::from(
        unix_fs::open(&selected, DIR_FLAGS, Mode::empty())
            .map_err(|e| format!("cannot open quarantine directory {}: {e}", path.display()))?,
    );
    for name in [".cx", "quarantine"] {
        match unix_fs::openat(&handle, name, DIR_FLAGS, Mode::empty()) {
            Ok(fd) => handle = File::from(fd),
            Err(rustix::io::Errno::NOENT) if !create => return Ok(None),
            Err(rustix::io::Errno::NOENT) => {
                match unix_fs::mkdirat(&handle, name, Mode::from_raw_mode(0o700)) {
                    Ok(()) | Err(rustix::io::Errno::EXIST) => (),
                    Err(e) => {
                        return Err(format!(
                            "cannot create quarantine directory {}: {e}",
                            path.display()
                        ));
                    }
                }
                handle = File::from(
                    unix_fs::openat(&handle, name, DIR_FLAGS, Mode::empty()).map_err(|e| {
                        format!("cannot open quarantine directory {}: {e}", path.display())
                    })?,
                );
            }
            Err(e) => {
                return Err(format!(
                    "cannot open quarantine directory {}: {e}",
                    path.display()
                ));
            }
        }
    }
    Ok(Some(QuarantineDir { path, handle }))
}

#[cfg(not(unix))]
fn open_quarantine_dir(_create: bool) -> Result<Option<QuarantineDir>, String> {
    Err("quarantine requires secure directory-relative file access".to_string())
}

fn make_quarantine_id(tool: &str) -> String {
    let safe_tool: String = tool
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "{}_{}_{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        safe_tool,
        std::process::id()
    )
}

#[cfg(unix)]
fn store_record(dir: &QuarantineDir, mut rec: QuarantineRecord) -> Result<String, String> {
    let base_id = rec.id.clone();
    for collision in 0..1024 {
        let id = if collision == 0 {
            base_id.clone()
        } else {
            format!("{base_id}_{collision}")
        };
        rec.id.clone_from(&id);
        let serialized = serde_json::to_vec_pretty(&rec)
            .map_err(|e| format!("failed to serialize quarantine record: {e}"))?;
        if serialized.len() as u64 > MAX_RECORD_BYTES {
            return Err("quarantine record exceeds maximum size".to_string());
        }
        let path = dir.path.join(format!("{id}.json"));
        let mut file = match unix_fs::openat(
            &dir.handle,
            format!("{id}.json"),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        ) {
            Ok(fd) => File::from(fd),
            Err(rustix::io::Errno::EXIST) => continue,
            Err(e) => return Err(format!("failed to write {}: {e}", path.display())),
        };
        file.write_all(&serialized)
            .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        return Ok(id);
    }
    Err("failed to create a unique quarantine id".to_string())
}

#[cfg(not(unix))]
fn store_record(_dir: &QuarantineDir, _rec: QuarantineRecord) -> Result<String, String> {
    Err("quarantine requires secure directory-relative file access".to_string())
}

pub fn quarantine_store_with_attempts(
    tool: &str,
    reason: &str,
    raw: &str,
    schema: &str,
    prompt: &str,
    attempts: Vec<QuarantineAttempt>,
) -> Result<String, String> {
    let dir = open_quarantine_dir(true)?
        .ok_or_else(|| "unable to resolve quarantine directory".to_string())?;
    let rec = QuarantineRecord {
        id: make_quarantine_id(tool),
        ts: utc_now_iso(),
        tool: tool.to_string(),
        reason: reason.to_string(),
        schema: schema.to_string(),
        prompt: prompt.to_string(),
        prompt_sha256: sha256_hex(prompt),
        raw_response: raw.to_string(),
        raw_sha256: sha256_hex(raw),
        attempts,
    };
    store_record(&dir, rec)
}

#[allow(dead_code)]
pub fn quarantine_store(
    tool: &str,
    reason: &str,
    raw: &str,
    schema: &str,
    prompt: &str,
) -> Result<String, String> {
    quarantine_store_with_attempts(tool, reason, raw, schema, prompt, Vec::new())
}

fn validate_quarantine_record(rec: &QuarantineRecord, requested_id: &str) -> Result<(), String> {
    if rec.id != requested_id {
        return Err(format!(
            "quarantine id mismatch: requested {requested_id}, record has {}",
            rec.id
        ));
    }
    if rec.prompt_sha256 != sha256_hex(&rec.prompt) {
        return Err(format!(
            "quarantine prompt_sha256 mismatch for id {requested_id}"
        ));
    }
    if rec.raw_sha256 != sha256_hex(&rec.raw_response) {
        return Err(format!(
            "quarantine raw_sha256 mismatch for id {requested_id}"
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn read_record_text(dir: &QuarantineDir, id: &str) -> Result<String, String> {
    let path = dir.path.join(format!("{id}.json"));
    let file = match unix_fs::openat(
        &dir.handle,
        format!("{id}.json"),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    ) {
        Ok(fd) => File::from(fd),
        Err(rustix::io::Errno::NOENT) => return Err(format!("quarantine id not found: {id}")),
        Err(e) => return Err(format!("cannot open {}: {e}", path.display())),
    };
    let metadata = file
        .metadata()
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "quarantine record is not a regular file: {}",
            path.display()
        ));
    }
    if metadata.len() > MAX_RECORD_BYTES {
        return Err(format!(
            "quarantine record exceeds maximum size: {}",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(format!(
            "quarantine record exceeds maximum size: {}",
            path.display()
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn read_record_text(_dir: &QuarantineDir, _id: &str) -> Result<String, String> {
    Err("quarantine requires secure directory-relative file access".to_string())
}

pub fn read_quarantine_record(id: &str) -> Result<QuarantineRecord, String> {
    if !valid_id(id) {
        return Err("invalid quarantine id".to_string());
    }
    let dir =
        open_quarantine_dir(false)?.ok_or_else(|| format!("quarantine id not found: {id}"))?;
    let path = dir.path.join(format!("{id}.json"));
    let text = read_record_text(&dir, id)?;
    let rec: QuarantineRecord = serde_json::from_str(&text)
        .map_err(|e| format!("invalid quarantine JSON {}: {e}", path.display()))?;
    validate_quarantine_record(&rec, id)?;
    Ok(rec)
}

#[cfg(unix)]
fn read_quarantine_rows(dir: &QuarantineDir, n: usize) -> Result<Vec<QuarantineRecord>, String> {
    let entries = Dir::read_from(&dir.handle).map_err(|e| {
        format!(
            "cannot list quarantine directory {}: {e}",
            dir.path.display()
        )
    })?;
    let mut rows = Vec::new();
    for ent in entries.flatten() {
        let Ok(name) = ent.file_name().to_str() else {
            continue;
        };
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if !valid_id(id) {
            continue;
        }
        let Ok(text) = read_record_text(dir, id) else {
            continue;
        };
        if let Ok(rec) = serde_json::from_str::<QuarantineRecord>(&text) {
            rows.push(rec);
        }
    }
    rows.sort_by(|a, b| b.ts.cmp(&a.ts));
    if rows.len() > n {
        rows.truncate(n);
    }
    Ok(rows)
}

#[cfg(not(unix))]
fn read_quarantine_rows(_dir: &QuarantineDir, _n: usize) -> Result<Vec<QuarantineRecord>, String> {
    Err("quarantine requires secure directory-relative file access".to_string())
}

pub fn cmd_quarantine_list(n: usize) -> i32 {
    let dir = match open_quarantine_dir(false) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} quarantine list: {e}", cli_app_name());
            return 1;
        }
    };
    let Some(dir) = dir else {
        let path = crate::paths::resolve_quarantine_dir();
        println!("== {} quarantine list ==", cli_app_name());
        println!("entries: 0");
        if let Some(path) = path {
            println!("quarantine_dir: {}", path.display());
        }
        return 0;
    };
    let rows = match read_quarantine_rows(&dir, n) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} quarantine list: {e}", cli_app_name());
            return 1;
        }
    };
    println!("== {} quarantine list ==", cli_app_name());
    println!("entries: {}", rows.len());
    for rec in rows {
        println!("- {} | {} | {} | {}", rec.id, rec.ts, rec.tool, rec.reason);
    }
    println!("quarantine_dir: {}", dir.path.display());
    0
}

pub fn cmd_quarantine_show(id: &str) -> i32 {
    let rec = match read_quarantine_record(id) {
        Ok(v) => v,
        Err(e) => {
            crate::cx_eprintln!("{} quarantine show: {e}", cli_app_name());
            return 1;
        }
    };
    match serde_json::to_string_pretty(&rec) {
        Ok(v) => {
            println!("{v}");
            0
        }
        Err(e) => {
            crate::cx_eprintln!(
                "{} quarantine show: failed to render JSON: {e}",
                cli_app_name()
            );
            1
        }
    }
}
