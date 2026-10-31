use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::evidence_reduce::Summary;

const MAX_RECORD: u64 = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub schema_version: u32,
    pub id: String,
    pub exit_status: i32,
    pub summary: Summary,
    pub output: String,
}

pub fn validate_store(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("store must be an absolute existing private directory".into());
    }
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err("store is not a directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err("store must have private permissions (0700)".into());
        }
    }
    #[cfg(not(unix))]
    return Err("private evidence storage is currently supported on Unix only".into());
    #[cfg(unix)]
    Ok(path)
}

pub fn reserve(store: &Path) -> Result<(String, File), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let id = format!("{:x}-{:x}", std::process::id(), stamp);
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let file = opts
        .open(store.join(format!("{id}.json")))
        .map_err(|e| e.to_string())?;
    Ok((id, file))
}

pub fn save(file: &mut File, record: &Record) -> Result<(), String> {
    let bytes = serde_json::to_vec(record).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECORD {
        return Err("evidence record exceeds 64 MiB; record not saved".into());
    }
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}

pub fn load(store: &Path, id: &str) -> Result<Record, String> {
    if id.is_empty() || id.len() > 80 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err("invalid capture ID".into());
    }
    let path = store.join(format!("{id}.json"));
    let before = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !before.is_file() {
        return Err("record must be a regular file, not a symlink".into());
    }
    let file = File::open(&path).map_err(|e| e.to_string())?;
    let after = file.metadata().map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != after.dev()
            || before.ino() != after.ino()
            || after.mode() & 0o077 != 0
            || after.nlink() != 1
        {
            return Err("record changed during open or is not private".into());
        }
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECORD + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECORD {
        return Err("record exceeds 64 MiB".into());
    }
    let record: Record = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if record.schema_version != 1 || record.id != id {
        return Err("record version or ID mismatch".into());
    }
    Ok(record)
}
