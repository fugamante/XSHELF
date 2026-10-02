use std::fs;
use std::path::Path;

use super::task_store_io::{
    WriteOutcome, read_snapshot, snapshot_meta_path, write_atomic_durable, write_snapshot,
};
use super::{LedgerState, SnapshotMeta, TASK_COMMAND_VERSION, tasks_digest};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ProjectionToken {
    snapshot: Option<Vec<u8>>,
    metadata: Option<Vec<u8>>,
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if !path.exists() {
        return Ok(None);
    }
    fs::read(path)
        .map(Some)
        .map_err(|e| format!("failed reading task projection {}: {e}", path.display()))
}

pub(super) fn capture_projection(path: &Path) -> Result<ProjectionToken, String> {
    Ok(ProjectionToken {
        snapshot: read_optional(path)?,
        metadata: read_optional(&snapshot_meta_path(path)?)?,
    })
}

pub(super) fn check_projection(path: &Path, expected: &ProjectionToken) -> Result<(), String> {
    if &capture_projection(path)? == expected {
        Ok(())
    } else {
        Err(
            "task projection changed during ledger mutation; preserved without overwrite"
                .to_string(),
        )
    }
}

fn read_meta(path: &Path) -> Result<Option<SnapshotMeta>, String> {
    let Some(bytes) = read_optional(path)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| format!("invalid task snapshot metadata {}: {e}", path.display()))
}

fn record_warning(warnings: &mut Vec<String>, outcome: WriteOutcome) {
    if let WriteOutcome::PublishedUncertain(error) = outcome {
        warnings.push(error);
    }
}

pub(super) fn write_snapshot_state(
    path: &Path,
    state: &LedgerState,
) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    record_warning(&mut warnings, write_snapshot(path, &state.tasks)?);
    let meta = SnapshotMeta {
        contract_version: TASK_COMMAND_VERSION.to_string(),
        revision: state.revision,
        digest: tasks_digest(&state.tasks)?,
    };
    let mut bytes = serde_json::to_vec_pretty(&meta)
        .map_err(|e| format!("failed encoding task snapshot metadata: {e}"))?;
    bytes.push(b'\n');
    record_warning(
        &mut warnings,
        write_atomic_durable(&snapshot_meta_path(path)?, &bytes)?,
    );
    Ok(warnings)
}

pub(super) fn validate_snapshot(path: &Path, state: &LedgerState) -> Result<(), String> {
    // An intact future-version marker is a compatibility boundary, even when
    // the derived snapshot matches or needs repair. Never silently downgrade it.
    if let Ok(Some(meta)) = read_meta(&snapshot_meta_path(path)?)
        && meta.contract_version != TASK_COMMAND_VERSION
    {
        return Err(format!(
            "unsupported task snapshot metadata contract: {}",
            meta.contract_version
        ));
    }
    if !path.exists() {
        return Ok(());
    }
    let desired = tasks_digest(&state.tasks)?;
    let Ok(snapshot) = read_snapshot(path) else {
        return Ok(());
    };
    let current = tasks_digest(&snapshot)?;
    if current == desired {
        return Ok(());
    }

    let meta_path = snapshot_meta_path(path)?;
    let meta = read_meta(&meta_path)?;
    if let Some(meta) = &meta {
        if meta.contract_version != TASK_COMMAND_VERSION {
            return Err(format!(
                "unsupported task snapshot metadata contract: {}",
                meta.contract_version
            ));
        }
        if meta.revision == state.revision && meta.digest == desired {
            return Err("tasks.json changed outside the authoritative task ledger".to_string());
        }
        if meta.digest != current
            || state.revisions_by_digest.get(&current).copied() != Some(meta.revision)
        {
            return Err("tasks.json does not match a recorded task ledger revision".to_string());
        }
    } else if !state.revisions_by_digest.contains_key(&current) {
        return Err("tasks.json does not match a recorded task ledger revision".to_string());
    }
    Ok(())
}

pub(super) fn reconcile_snapshot(path: &Path, state: &LedgerState) -> Result<Vec<String>, String> {
    validate_snapshot(path, state)?;
    let desired = tasks_digest(&state.tasks)?;
    let snapshot_current = read_snapshot(path)
        .ok()
        .and_then(|tasks| tasks_digest(&tasks).ok())
        .as_deref()
        == Some(desired.as_str());
    let meta_current = read_meta(&snapshot_meta_path(path)?)
        .ok()
        .flatten()
        .is_some_and(|meta| {
            meta.contract_version == TASK_COMMAND_VERSION
                && meta.revision == state.revision
                && meta.digest == desired
        });
    if snapshot_current && meta_current {
        Ok(Vec::new())
    } else {
        write_snapshot_state(path, state)
    }
}
