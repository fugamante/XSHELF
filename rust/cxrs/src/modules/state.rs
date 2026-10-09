use crate::paths::resolve_state_file;
use serde_json::{Value, json};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static STATE_CACHE: OnceLock<Mutex<Option<Value>>> = OnceLock::new();

pub fn state_cache_clear() {
    if let Ok(mut g) = STATE_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *g = None;
    }
}

pub fn read_state_value() -> Option<Value> {
    if std::env::var("CX_NO_CACHE").ok().as_deref() != Some("1")
        && let Some(v) = STATE_CACHE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .ok()
            .and_then(|g| g.clone())
    {
        return Some(v);
    }
    let parsed = read_state_checked().ok().flatten()?;
    if std::env::var("CX_NO_CACHE").ok().as_deref() != Some("1")
        && let Ok(mut g) = STATE_CACHE.get_or_init(|| Mutex::new(None)).lock()
    {
        *g = Some(parsed.clone());
    }
    Some(parsed)
}

pub fn read_state_checked() -> Result<Option<Value>, String> {
    let state_file =
        resolve_state_file().ok_or_else(|| "unable to resolve state file".to_string())?;
    let Some(bytes) = read_json_secure(&state_file)? else {
        return Ok(None);
    };
    serde_json::from_str::<Value>(&bytes)
        .map(Some)
        .map_err(|e| format!("invalid JSON in {}: {e}", state_file.display()))
}

fn default_state_value() -> Value {
    json!({
        "preferences": {
            "llm_backend": Value::Null,
            "ollama_model": Value::Null,
            "llama_cpp_model": Value::Null,
            "mlx_model": Value::Null,
            "conventional_commits": Value::Null,
            "pr_summary_format": Value::Null,
            "default_json_output": Value::Null,
            "task_sandbox": {
                "enabled": Value::Null,
                "image": Value::Null
            }
        },
        "runtime": {
            "current_task_id": Value::Null,
            "current_task_parent_id": Value::Null
        },
        "alert_overrides": {},
        "last_model": Value::Null
    })
}

pub fn ensure_state_value() -> Result<(PathBuf, Value), String> {
    let state_file =
        resolve_state_file().ok_or_else(|| "unable to resolve state file".to_string())?;
    if let Some(value) = read_state_checked()? {
        return Ok((state_file, value));
    }
    let initial = default_state_value();
    write_json_atomic(&state_file, &initial)?;
    Ok((state_file, initial))
}

#[cfg(unix)]
fn json_parent(path: &Path, create: bool) -> Result<Option<(File, std::ffi::OsString)>, String> {
    use rustix::fs::{self as rfs, Mode, OFlags};
    use std::path::Component;

    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("cannot resolve current directory: {e}"))?
            .join(path)
    };
    let parent_path = path
        .parent()
        .ok_or_else(|| "JSON path has no parent".to_string())?;
    let leaf = path
        .file_name()
        .ok_or_else(|| "JSON path has no filename".to_string())?
        .to_os_string();
    // The repo and HOME are operator anchors. Every descendant is opened without
    // following symlinks, including an attacker-supplied .cx directory.
    let anchors = [crate::paths::repo_root(), crate::paths::home_dir()];
    let mut selected = None;
    for anchor in anchors.into_iter().flatten() {
        if let Ok(tail) = parent_path.strip_prefix(&anchor) {
            let canonical = std::fs::canonicalize(&anchor)
                .map_err(|e| format!("cannot resolve JSON anchor {}: {e}", anchor.display()))?;
            selected = Some((canonical, tail.to_path_buf()));
            break;
        }
    }
    let (anchor, tail) = match selected {
        Some(v) => v,
        None => {
            // Outside repo/HOME the caller chose the destination; normalize its
            // existing parent so platform aliases such as /var remain usable.
            let canonical = match std::fs::canonicalize(parent_path) {
                Ok(path) => path,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && !create => return Ok(None),
                Err(e) => {
                    return Err(format!(
                        "cannot resolve JSON parent {}: {e}",
                        parent_path.display()
                    ));
                }
            };
            (canonical, PathBuf::new())
        }
    };
    let dir_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut parent = File::from(
        rfs::open(&anchor, dir_flags, Mode::empty())
            .map_err(|e| format!("cannot open JSON anchor {}: {e}", anchor.display()))?,
    );
    for component in tail.components() {
        let Component::Normal(name) = component else {
            return Err("JSON parent contains unsafe path component".to_string());
        };
        let fd = match rfs::openat(&parent, name, dir_flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) if create => {
                match rfs::mkdirat(&parent, name, Mode::from_raw_mode(0o700)) {
                    Ok(()) | Err(rustix::io::Errno::EXIST) => (),
                    Err(e) => return Err(format!("cannot create JSON parent: {e}")),
                }
                rfs::openat(&parent, name, dir_flags, Mode::empty())
                    .map_err(|e| format!("cannot open JSON parent: {e}"))?
            }
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(format!("cannot open JSON parent: {e}")),
        };
        parent = File::from(fd);
    }
    Ok(Some((parent, leaf)))
}

#[cfg(unix)]
pub fn read_json_secure(path: &Path) -> Result<Option<String>, String> {
    use rustix::fs::{self as rfs, Mode, OFlags};

    const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
    let Some((parent, leaf)) = json_parent(path, false)? else {
        return Ok(None);
    };
    let fd = match rfs::openat(
        &parent,
        &leaf,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(e) => return Err(format!("cannot open JSON file {}: {e}", path.display())),
    };
    let file = File::from(fd);
    let meta = file
        .metadata()
        .map_err(|e| format!("cannot inspect JSON file {}: {e}", path.display()))?;
    if !meta.is_file() || meta.len() > MAX_JSON_BYTES {
        return Err(format!(
            "JSON file {} is not a bounded regular file",
            path.display()
        ));
    }
    let mut bytes = String::new();
    file.take(MAX_JSON_BYTES + 1)
        .read_to_string(&mut bytes)
        .map_err(|e| format!("cannot read JSON file {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_JSON_BYTES {
        return Err(format!("JSON file {} exceeds size limit", path.display()));
    }
    Ok(Some(bytes))
}

#[cfg(not(unix))]
pub fn read_json_secure(_path: &Path) -> Result<Option<String>, String> {
    Err("safe JSON reads require Unix directory descriptors".to_string())
}

#[cfg(unix)]
fn write_json_secure(path: &Path, serialized: &[u8]) -> Result<(), String> {
    use rustix::fs::{self as rfs, AtFlags, Mode, OFlags};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TMP: AtomicU64 = AtomicU64::new(1);
    let (parent, leaf) = json_parent(path, true)?
        .ok_or_else(|| format!("cannot open JSON parent for {}", path.display()))?;
    let mut staged = None;
    for _ in 0..8 {
        let clock = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("cannot timestamp JSON write: {e}"))?
            .as_nanos();
        let name = format!(
            ".{}.tmp.{}.{}.{}",
            leaf.to_string_lossy(),
            std::process::id(),
            clock,
            NEXT_TMP.fetch_add(1, Ordering::Relaxed)
        );
        match rfs::openat(
            &parent,
            &name,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        ) {
            Ok(fd) => {
                staged = Some((name, File::from(fd)));
                break;
            }
            Err(rustix::io::Errno::EXIST) => continue,
            Err(e) => return Err(format!("cannot stage JSON write: {e}")),
        }
    }
    let (name, mut file) = staged.ok_or_else(|| "cannot allocate JSON staging file".to_string())?;
    let result = (|| {
        file.write_all(serialized)
            .map_err(|e| format!("cannot write staged JSON: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("cannot sync staged JSON: {e}"))?;
        rfs::renameat(&parent, &name, &parent, &leaf)
            .map_err(|e| format!("cannot publish staged JSON: {e}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = rfs::unlinkat(&parent, &name, AtFlags::empty());
    }
    result
}

#[cfg(not(unix))]
fn write_json_secure(_path: &Path, _serialized: &[u8]) -> Result<(), String> {
    Err("safe JSON writes require Unix directory descriptors".to_string())
}

pub fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let mut serialized = serde_json::to_string_pretty(value)
        .map_err(|e| format!("failed to serialize JSON: {e}"))?;
    serialized.push('\n');
    write_json_secure(path, serialized.as_bytes())?;
    if path.file_name().and_then(|s| s.to_str()) == Some("state.json") {
        state_cache_clear();
    }
    Ok(())
}

pub fn parse_cli_value(raw: &str) -> Value {
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        return v;
    }
    if raw.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if raw.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if raw.eq_ignore_ascii_case("null") {
        return Value::Null;
    }
    if let Ok(v) = raw.parse::<i64>() {
        return json!(v);
    }
    if let Ok(v) = raw.parse::<f64>() {
        return json!(v);
    }
    Value::String(raw.to_string())
}

pub fn value_at_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path.split('.') {
        if seg.is_empty() {
            continue;
        }
        cur = cur.get(seg)?;
    }
    Some(cur)
}

pub fn set_value_at_path(root: &mut Value, path: &str, new_value: Value) -> Result<(), String> {
    let mut segs: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    if segs.is_empty() {
        return Err("key cannot be empty".to_string());
    }
    let last = segs.pop().unwrap_or_default();
    let mut cur = root;
    for seg in segs {
        if !cur.is_object() {
            *cur = json!({});
        }
        let obj = cur
            .as_object_mut()
            .ok_or_else(|| "failed to access state object".to_string())?;
        cur = obj.entry(seg.to_string()).or_insert_with(|| json!({}));
    }
    if !cur.is_object() {
        *cur = json!({});
    }
    let obj = cur
        .as_object_mut()
        .ok_or_else(|| "failed to access final state object".to_string())?;
    obj.insert(last.to_string(), new_value);
    Ok(())
}

pub fn set_state_path(path: &str, value: Value) -> Result<(), String> {
    let (state_file, mut state) = ensure_state_value()?;
    set_value_at_path(&mut state, path, value)?;
    write_json_atomic(&state_file, &state)
}

pub fn current_task_id() -> Option<String> {
    if let Ok(v) = std::env::var("CX_TASK_ID")
        && !v.trim().is_empty()
    {
        return Some(v);
    }
    read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "runtime.current_task_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(all(test, unix))]
mod secure_tests {
    use super::write_json_atomic;
    use serde_json::json;
    use std::fs;
    use std::os::unix::fs::symlink;

    #[test]
    fn temp_symlink_safe() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("outside");
        fs::write(&target, "sentinel").unwrap();
        let path = dir.path().join("state.json");
        symlink(
            &target,
            dir.path().join(format!("state.tmp.{}", std::process::id())),
        )
        .unwrap();
        write_json_atomic(&path, &json!({"safe": true})).unwrap();
        assert_eq!(fs::read_to_string(target).unwrap(), "sentinel");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap(),
            json!({"safe": true})
        );
    }
}

pub fn current_task_parent_id() -> Option<String> {
    if let Ok(v) = std::env::var("CX_TASK_PARENT_ID")
        && !v.trim().is_empty()
    {
        return Some(v);
    }
    read_state_value()
        .as_ref()
        .and_then(|v| value_at_path(v, "runtime.current_task_parent_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_value_handles_primitives() {
        assert_eq!(parse_cli_value("true"), Value::Bool(true));
        assert_eq!(parse_cli_value("42"), json!(42));
        assert_eq!(parse_cli_value("3.5"), json!(3.5));
        assert_eq!(parse_cli_value("null"), Value::Null);
    }

    #[test]
    fn set_and_get_nested_path() {
        let mut v = json!({});
        set_value_at_path(&mut v, "a.b.c", json!(7)).expect("set nested path");
        assert_eq!(value_at_path(&v, "a.b.c"), Some(&json!(7)));
    }
}
