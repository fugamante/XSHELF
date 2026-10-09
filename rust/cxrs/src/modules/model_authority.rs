//! Repo-local LLM preferences are data until an operator selects them with an LLM command.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;

use crate::local_models::LocalModelRecord;
use crate::paths::{home_dir, repo_root};
use crate::state::{read_state_checked, read_state_value, set_state_path, value_at_path};

const VERSION: u32 = 1;
const PREFS: [&str; 4] = [
    "preferences.llm_backend",
    "preferences.ollama_model",
    "preferences.llama_cpp_model",
    "preferences.mlx_model",
];

#[derive(Clone, Default, Deserialize, Serialize)]
struct ApprovedPrefs {
    llm_backend: Option<String>,
    ollama_model: Option<String>,
    llama_cpp_model: Option<String>,
    mlx_model: Option<String>,
}

impl ApprovedPrefs {
    fn get(&self, path: &str) -> Option<&str> {
        match path {
            "preferences.llm_backend" => self.llm_backend.as_deref(),
            "preferences.ollama_model" => self.ollama_model.as_deref(),
            "preferences.llama_cpp_model" => self.llama_cpp_model.as_deref(),
            "preferences.mlx_model" => self.mlx_model.as_deref(),
            _ => None,
        }
    }

    fn set(&mut self, path: &str, value: Option<String>) -> Result<(), String> {
        match path {
            "preferences.llm_backend" => self.llm_backend = value,
            "preferences.ollama_model" => self.ollama_model = value,
            "preferences.llama_cpp_model" => self.llama_cpp_model = value,
            "preferences.mlx_model" => self.mlx_model = value,
            _ => return Err(format!("not an LLM authority preference: {path}")),
        }
        Ok(())
    }

    fn as_state(&self) -> Value {
        json!({"preferences": {
            "llm_backend": self.llm_backend,
            "ollama_model": self.ollama_model,
            "llama_cpp_model": self.llama_cpp_model,
            "mlx_model": self.mlx_model,
        }})
    }
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
struct ApprovedModel {
    id: String,
    alias: String,
    backend: String,
    resolved_model: String,
    preferred_args: Option<String>,
}

impl From<&LocalModelRecord> for ApprovedModel {
    fn from(record: &LocalModelRecord) -> Self {
        Self {
            id: record.id.clone(),
            alias: record.alias.clone(),
            backend: record.backend.clone(),
            resolved_model: record.resolved_model.clone(),
            preferred_args: record.preferred_args.clone(),
        }
    }
}

#[derive(Default, Deserialize, Serialize)]
struct Approval {
    version: u32,
    repo_identity: String,
    preferences: ApprovedPrefs,
    models: Vec<ApprovedModel>,
}

fn repo_identity() -> Result<Option<String>, String> {
    let Some(root) = repo_root() else {
        return Ok(None);
    };
    let root = fs::canonicalize(&root).map_err(|e| format!("cannot resolve Git root: {e}"))?;
    let git = fs::symlink_metadata(root.join(".git"))
        .map_err(|e| format!("cannot inspect Git identity: {e}"))?;
    if git.file_type().is_symlink() || !(git.is_dir() || git.is_file()) {
        return Err("Git identity is not a regular file or directory".to_string());
    }
    Ok(Some(format!(
        "{}:{}:{}",
        root.display(),
        git.dev(),
        git.ino()
    )))
}

fn io_err(action: &str, e: impl std::fmt::Display) -> String {
    format!("model authority {action}: {e}")
}

fn secure_dir(create: bool) -> Result<Option<(File, u32)>, String> {
    use rustix::fs::{self as rfs, Mode, OFlags};
    unsafe extern "C" {
        fn geteuid() -> std::os::raw::c_uint;
    }

    let home = home_dir().ok_or_else(|| "model authority HOME is unset".to_string())?;
    let home = fs::canonicalize(home).map_err(|e| io_err("HOME", e))?;
    let home_file = File::from(
        rfs::open(
            &home,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|e| io_err("HOME", e))?,
    );
    let home_meta = home_file
        .metadata()
        .map_err(|e| io_err("HOME metadata", e))?;
    let uid = home_meta.uid();
    if uid != unsafe { geteuid() } || home_meta.mode() & 0o022 != 0 {
        return Err("model authority HOME has unsafe ownership or permissions".to_string());
    }
    let mut parent = home_file;
    for (name, private) in [(".cx", false), ("model_authority", true)] {
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let opened = match rfs::openat(&parent, name, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) if create => {
                match rfs::mkdirat(&parent, name, Mode::from_raw_mode(0o700)) {
                    Ok(()) | Err(rustix::io::Errno::EXIST) => (),
                    Err(e) => return Err(io_err("create directory", e)),
                }
                rfs::openat(&parent, name, flags, Mode::empty())
                    .map_err(|e| io_err("open directory", e))?
            }
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(io_err("open directory", e)),
        };
        parent = File::from(opened);
        let meta = parent
            .metadata()
            .map_err(|e| io_err("directory metadata", e))?;
        if meta.uid() != uid || meta.mode() & if private { 0o077 } else { 0o022 } != 0 {
            return Err(format!(
                "model authority directory {name} has unsafe ownership or permissions"
            ));
        }
    }
    Ok(Some((parent, uid)))
}

fn authority_name(identity: &str) -> String {
    let digest = Sha256::digest(identity.as_bytes());
    format!("{digest:x}.json")
}

fn read_in(dir: &File, name: &str, uid: u32) -> Result<Option<Approval>, String> {
    use rustix::fs::{self as rfs, Mode, OFlags};

    let fd = match rfs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(e) => return Err(io_err("open receipt", e)),
    };
    let file = File::from(fd);
    let meta = file.metadata().map_err(|e| io_err("receipt metadata", e))?;
    if !meta.is_file() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
        return Err(
            "model authority receipt has unsafe type, ownership or permissions".to_string(),
        );
    }
    let mut bytes = String::new();
    const MAX_RECEIPT_BYTES: u64 = 1024 * 1024;
    if meta.len() > MAX_RECEIPT_BYTES {
        return Err("model authority receipt exceeds size limit".to_string());
    }
    file.take(MAX_RECEIPT_BYTES + 1)
        .read_to_string(&mut bytes)
        .map_err(|e| io_err("read receipt", e))?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err("model authority receipt exceeds size limit".to_string());
    }
    let approval: Approval =
        serde_json::from_str(&bytes).map_err(|e| io_err("parse receipt", e))?;
    if approval.version != VERSION {
        return Err("model authority receipt version is unsupported".to_string());
    }
    Ok(Some(approval))
}

fn read_approval(identity: &str) -> Result<Option<Approval>, String> {
    let Some((dir, uid)) = secure_dir(false)? else {
        return Ok(None);
    };
    let receipt = read_in(&dir, &authority_name(identity), uid)?;
    if receipt
        .as_ref()
        .is_some_and(|r| r.repo_identity != identity)
    {
        return Err("model authority receipt belongs to another Git checkout".to_string());
    }
    Ok(receipt)
}

fn mutate_approval(change: impl FnOnce(&mut Approval) -> Result<(), String>) -> Result<(), String> {
    use fs2::FileExt;
    use rustix::fs::{self as rfs, AtFlags, Mode, OFlags};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TMP: AtomicU64 = AtomicU64::new(1);

    let Some(identity) = repo_identity()? else {
        return Ok(());
    };
    let (dir, uid) =
        secure_dir(true)?.ok_or_else(|| "cannot create model authority directory".to_string())?;
    let lock = File::from(
        rfs::openat(
            &dir,
            ".lock",
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|e| io_err("open lock", e))?,
    );
    let meta = lock.metadata().map_err(|e| io_err("lock metadata", e))?;
    if !meta.is_file() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
        return Err("model authority lock has unsafe type, ownership or permissions".to_string());
    }
    lock.lock_exclusive()
        .map_err(|e| io_err("lock receipt", e))?;
    let name = authority_name(&identity);
    let mut approval = read_in(&dir, &name, uid)?.unwrap_or_else(|| Approval {
        version: VERSION,
        repo_identity: identity.clone(),
        ..Approval::default()
    });
    if approval.repo_identity != identity {
        return Err("model authority receipt belongs to another Git checkout".to_string());
    }
    change(&mut approval)?;
    let bytes = serde_json::to_vec_pretty(&approval).map_err(|e| io_err("serialize receipt", e))?;
    let mut staged = None;
    for _ in 0..8 {
        let clock = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| io_err("timestamp receipt", e))?
            .as_nanos();
        let tmp_name = format!(
            ".receipt.tmp.{}.{}.{}",
            std::process::id(),
            clock,
            NEXT_TMP.fetch_add(1, Ordering::Relaxed)
        );
        match rfs::openat(
            &dir,
            &tmp_name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        ) {
            Ok(fd) => {
                staged = Some((tmp_name, File::from(fd)));
                break;
            }
            Err(rustix::io::Errno::EXIST) => continue,
            Err(e) => return Err(io_err("create receipt", e)),
        }
    }
    let (tmp_name, mut tmp) =
        staged.ok_or_else(|| "cannot stage model authority receipt".to_string())?;
    let result = (|| {
        tmp.write_all(&bytes)
            .map_err(|e| io_err("write receipt", e))?;
        tmp.sync_all().map_err(|e| io_err("sync receipt", e))?;
        rfs::renameat(&dir, &tmp_name, &dir, &name).map_err(|e| io_err("save receipt", e))?;
        dir.sync_all().map_err(|e| io_err("sync directory", e))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = rfs::unlinkat(&dir, &tmp_name, AtFlags::empty());
    }
    result
}

/// Only LLM-specific commands use this path. Generic `state set` remains data-only.
pub fn set_approved_pref(path: &str, value: Value) -> Result<(), String> {
    if !PREFS.contains(&path) {
        return Err(format!("not an LLM authority preference: {path}"));
    }
    let selected = value
        .as_str()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    if !value.is_null() && selected.is_none() {
        return Err("LLM authority preference must be a string or null".to_string());
    }
    set_state_path(path, value)?;
    mutate_approval(|a| a.preferences.set(path, selected))
}

pub fn approved_state() -> Result<Option<Value>, String> {
    let Some(identity) = repo_identity()? else {
        return Ok(read_state_value());
    };
    Ok(read_approval(&identity)?.map(|a| a.preferences.as_state()))
}

pub fn approved_pref(path: &str) -> Result<Option<String>, String> {
    Ok(approved_state()?
        .as_ref()
        .and_then(|v| value_at_path(v, path))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned))
}

pub fn approve_model(record: &LocalModelRecord) -> Result<(), String> {
    let candidate = ApprovedModel::from(record);
    mutate_approval(|a| {
        a.models.retain(|m| {
            m.id != candidate.id && !(m.backend == candidate.backend && m.alias == candidate.alias)
        });
        a.models.push(candidate);
        Ok(())
    })
}

pub fn revoke_model(record: &LocalModelRecord) -> Result<(), String> {
    mutate_approval(|a| {
        a.models.retain(|m| m.id != record.id);
        Ok(())
    })
}

pub fn model_is_approved(record: &LocalModelRecord) -> Result<bool, String> {
    let Some(identity) = repo_identity()? else {
        return Ok(true);
    };
    Ok(read_approval(&identity)?.is_some_and(|a| a.models.contains(&ApprovedModel::from(record))))
}

/// A changed or preseeded repo preference must not silently fall through to a provider.
pub fn ensure_execution_authority() -> Result<(), String> {
    ensure_choice_authority(None)
}

/// A backend named by an explicit CLI command is an operator choice for that invocation.
pub fn ensure_cli_authority(backend: &str) -> Result<(), String> {
    ensure_choice_authority(Some(backend))
}

fn ensure_choice_authority(cli_backend: Option<&str>) -> Result<(), String> {
    let Some(identity) = repo_identity()? else {
        return Ok(());
    };
    let chosen_backend = cli_backend.map(str::to_string).or_else(|| {
        std::env::var("CX_LLM_BACKEND")
            .ok()
            .filter(|v| !v.trim().is_empty())
    });
    if let Some(ref backend) = chosen_backend {
        let model_key = match crate::local_models::normalize_backend(backend) {
            Some("ollama") => Some("CX_OLLAMA_MODEL"),
            Some("llamacpp") => Some("CX_LLAMA_CPP_MODEL"),
            Some("mlx") => Some("CX_MLX_MODEL"),
            _ => None,
        };
        if model_key.is_none_or(|key| {
            std::env::var(key)
                .ok()
                .is_some_and(|v| !v.trim().is_empty())
        }) {
            // The complete provider choice came from the process, not repo files.
            return Ok(());
        }
    }
    let state = read_state_checked()?;
    let repo_backend = state.as_ref().and_then(|v| value_at_path(v, PREFS[0]));
    let repo_primary = match repo_backend {
        None | Some(Value::Null) => true,
        Some(Value::String(raw)) => raw.trim().eq_ignore_ascii_case("primary"),
        _ => false,
    };
    if chosen_backend.is_none() && repo_primary && crate::runtime::llm_backend() == "primary" {
        // Primary execution has no repo-selected LLM preference to authorize.
        return Ok(());
    }
    let approval = read_approval(&identity)?;
    let approved = approval.as_ref().map(|a| &a.preferences);
    if chosen_backend.is_none() {
        let repo = state
            .as_ref()
            .and_then(|v| value_at_path(v, PREFS[0]))
            .and_then(Value::as_str);
        let known = approved.and_then(|a| a.get(PREFS[0]));
        if repo != known {
            return Err("repository LLM backend is not approved; select it with 'llm use' or set CX_LLM_BACKEND".to_string());
        }
    }
    let backend = chosen_backend
        .or_else(|| approved.and_then(|a| a.llm_backend.clone()))
        .unwrap_or_else(|| "primary".to_string());
    let (key, path) = match backend.as_str() {
        "ollama" => ("CX_OLLAMA_MODEL", PREFS[1]),
        "llamacpp" | "llama.cpp" | "llama_cpp" => ("CX_LLAMA_CPP_MODEL", PREFS[2]),
        "mlx" => ("CX_MLX_MODEL", PREFS[3]),
        _ => return Ok(()),
    };
    if std::env::var(key)
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
    {
        return Ok(());
    }
    let repo = state
        .as_ref()
        .and_then(|v| value_at_path(v, path))
        .and_then(Value::as_str);
    let known = approved.and_then(|a| a.get(path));
    if repo != known {
        return Err(format!(
            "repository LLM model is not approved; select it with 'llm set-model' or set {key}"
        ));
    }
    Ok(())
}
