use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
use std::cell::RefCell;

use crate::paths::ensure_parent_dir;
use crate::types::TaskRecord;

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub(super) enum WriteOutcome {
    Durable,
    PublishedUncertain(String),
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FaultStage {
    AfterApply,
    AfterWrite,
    AfterFileSync,
    BeforeRename,
    AfterRename,
    AfterDirSync,
}

#[cfg(test)]
#[derive(Debug)]
struct FaultSpec {
    stage: FaultStage,
    target: String,
}

#[cfg(test)]
thread_local! {
    static FAULT: RefCell<Option<FaultSpec>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(super) fn set_fault(stage: FaultStage, target: &str) {
    FAULT.with(|slot| {
        *slot.borrow_mut() = Some(FaultSpec {
            stage,
            target: target.to_string(),
        });
    });
}

#[cfg(test)]
pub(super) fn inject_fault(stage: FaultStage, path: &Path) -> Result<(), String> {
    FAULT.with(|slot| {
        let matched = slot
            .borrow()
            .as_ref()
            .is_some_and(|fault| fault.stage == stage && path.ends_with(&fault.target));
        if matched {
            slot.borrow_mut().take();
            Err(format!("injected {stage:?} fault for {}", path.display()))
        } else {
            Ok(())
        }
    })
}

pub(super) fn store_paths(tasks_file: &Path) -> Result<(PathBuf, PathBuf), String> {
    let Some(parent) = tasks_file.parent() else {
        return Err("task store path has no parent".to_string());
    };
    Ok((parent.join("task_ledger"), parent.join("tasks.lock")))
}

pub(super) fn snapshot_meta_path(tasks_file: &Path) -> Result<PathBuf, String> {
    let Some(parent) = tasks_file.parent() else {
        return Err("task snapshot path has no parent".to_string());
    };
    Ok(parent.join("tasks.snapshot.json"))
}

pub(super) fn lock_store(lock_path: &Path) -> Result<File, String> {
    ensure_parent_dir(lock_path)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|e| {
            format!(
                "failed opening task store lock {}: {e}",
                lock_path.display()
            )
        })?;
    lock.lock_exclusive()
        .map_err(|e| format!("failed locking task store {}: {e}", lock_path.display()))?;
    Ok(lock)
}

pub(super) fn lock_store_shared(lock_path: &Path) -> Result<File, String> {
    let lock = OpenOptions::new().read(true).open(lock_path).map_err(|e| {
        format!(
            "failed opening task store lock {}: {e}",
            lock_path.display()
        )
    })?;
    FileExt::lock_shared(&lock)
        .map_err(|e| format!("failed locking task store {}: {e}", lock_path.display()))?;
    Ok(lock)
}

pub(super) fn read_legacy(path: &Path) -> Result<Vec<TaskRecord>, String> {
    if snapshot_meta_path(path)?.exists() {
        return Err(
            "task snapshot metadata exists without authoritative ledger entries; restore the ledger before access".to_string(),
        );
    }
    read_snapshot(path)
}

pub(super) fn read_snapshot(path: &Path) -> Result<Vec<TaskRecord>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut text = String::new();
    File::open(path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?
        .read_to_string(&mut text)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&text).map_err(|e| format!("invalid JSON in {}: {e}", path.display()))
}

pub(super) fn sync_dir(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| format!("failed syncing directory {}: {e}", path.display()))
}

pub(super) fn unique_tmp(path: &Path) -> PathBuf {
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    path.with_extension(format!("tmp.{}.{id}", std::process::id()))
}

pub(super) fn write_atomic_durable(path: &Path, bytes: &[u8]) -> Result<WriteOutcome, String> {
    ensure_parent_dir(path)?;
    let tmp = unique_tmp(path);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)
        .map_err(|e| format!("failed creating {}: {e}", tmp.display()))?;
    let result = (|| {
        file.write_all(bytes)
            .map_err(|e| format!("failed writing {}: {e}", tmp.display()))?;
        #[cfg(test)]
        inject_fault(FaultStage::AfterWrite, path)?;
        file.sync_all()
            .map_err(|e| format!("failed syncing {}: {e}", tmp.display()))?;
        #[cfg(test)]
        inject_fault(FaultStage::AfterFileSync, path)?;
        #[cfg(test)]
        inject_fault(FaultStage::BeforeRename, path)?;
        fs::rename(&tmp, path)
            .map_err(|e| format!("failed moving {} -> {}: {e}", tmp.display(), path.display()))?;
        #[cfg(test)]
        if let Err(error) = inject_fault(FaultStage::AfterRename, path) {
            return Ok(WriteOutcome::PublishedUncertain(error));
        }
        let parent = path
            .parent()
            .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
        if let Err(error) = sync_dir(parent) {
            return Ok(WriteOutcome::PublishedUncertain(error));
        }
        #[cfg(test)]
        if let Err(error) = inject_fault(FaultStage::AfterDirSync, path) {
            return Ok(WriteOutcome::PublishedUncertain(error));
        }
        Ok(WriteOutcome::Durable)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

pub(super) fn write_snapshot(path: &Path, tasks: &[TaskRecord]) -> Result<WriteOutcome, String> {
    let mut bytes =
        serde_json::to_vec_pretty(tasks).map_err(|e| format!("failed to encode tasks: {e}"))?;
    bytes.push(b'\n');
    write_atomic_durable(path, &bytes)
}

pub(super) fn entry_path(ledger_dir: &Path, revision: u64) -> PathBuf {
    ledger_dir.join(format!("{revision:020}.json"))
}

pub(super) fn ledger_entries(ledger_dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !ledger_dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for item in fs::read_dir(ledger_dir)
        .map_err(|e| format!("failed reading task ledger {}: {e}", ledger_dir.display()))?
    {
        let path = item
            .map_err(|e| format!("failed reading task ledger entry: {e}"))?
            .path();
        if path.extension().and_then(|v| v.to_str()) == Some("json") {
            entries.push(path);
        }
    }
    entries.sort();
    Ok(entries)
}
