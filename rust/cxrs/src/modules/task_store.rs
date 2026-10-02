use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::execmeta::utc_now_iso;
use crate::paths::resolve_tasks_file;
use crate::types::TaskRecord;

const TASK_COMMAND_VERSION: &str = "task-command.v1";
static NEXT_OPERATION_ID: AtomicU64 = AtomicU64::new(1);

#[path = "task_store_io.rs"]
mod task_store_io;
#[path = "task_store_snapshot.rs"]
mod task_store_snapshot;
#[cfg(test)]
use task_store_io::{FaultStage, inject_fault, set_fault};
use task_store_io::{
    WriteOutcome, entry_path, ledger_entries, lock_store, lock_store_shared, read_legacy,
    read_snapshot, store_paths, sync_dir, write_atomic_durable,
};
use task_store_snapshot::{
    ProjectionToken, capture_projection, check_projection, reconcile_snapshot, validate_snapshot,
    write_snapshot_state,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskPayload {
    tasks: Vec<TaskRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskCommand {
    contract_version: String,
    operation_id: String,
    revision: u64,
    expected_revision: u64,
    transition: String,
    task_id: Option<String>,
    worker_id: Option<String>,
    lease_epoch: u64,
    policy_digest: Option<String>,
    prior_digest: String,
    payload_digest: String,
    result_digest: String,
    prior_command_digest: String,
    command_digest: String,
    at: String,
    payload: TaskPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotMeta {
    contract_version: String,
    revision: u64,
    digest: String,
}

#[derive(Debug, Clone)]
struct LedgerState {
    revision: u64,
    tasks: Vec<TaskRecord>,
    operations: HashMap<String, String>,
    leases: HashMap<String, (u64, String)>,
    revisions_by_digest: HashMap<String, u64>,
    head_command_digest: String,
}

impl Default for LedgerState {
    fn default() -> Self {
        Self {
            revision: 0,
            tasks: Vec::new(),
            operations: HashMap::new(),
            leases: HashMap::new(),
            revisions_by_digest: HashMap::new(),
            head_command_digest: sha256_bytes(&[]),
        }
    }
}

pub struct TaskMutation<T> {
    pub result: T,
    pub task_id: Option<String>,
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn payload_digest(payload: &TaskPayload) -> Result<String, String> {
    serde_json::to_vec(payload)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|e| format!("failed to encode task command payload: {e}"))
}

fn tasks_digest(tasks: &[TaskRecord]) -> Result<String, String> {
    serde_json::to_vec(tasks)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|e| format!("failed to encode task snapshot: {e}"))
}

fn command_digest(command: &TaskCommand) -> Result<String, String> {
    let mut unsigned = command.clone();
    unsigned.command_digest.clear();
    serde_json::to_vec(&unsigned)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|e| format!("failed to encode task command envelope: {e}"))
}

fn operation_digest(command: &TaskCommand) -> Result<String, String> {
    let value = serde_json::json!({
        "transition": command.transition,
        "task_id": command.task_id,
        "worker_id": command.worker_id,
        "lease_epoch": command.lease_epoch,
        "policy_digest": command.policy_digest,
        "payload_digest": command.payload_digest,
    });
    serde_json::to_vec(&value)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|e| format!("failed to encode task operation intent: {e}"))
}

fn task_by_id<'a>(tasks: &'a [TaskRecord], id: &str) -> Option<&'a TaskRecord> {
    tasks.iter().find(|task| task.id == id)
}

fn same_except_status(before: &TaskRecord, after: &TaskRecord) -> Result<bool, String> {
    let mut left = serde_json::to_value(before).map_err(|e| e.to_string())?;
    let mut right = serde_json::to_value(after).map_err(|e| e.to_string())?;
    for value in [&mut left, &mut right] {
        if let Some(obj) = value.as_object_mut() {
            obj.remove("status");
            obj.remove("updated_at");
        }
    }
    Ok(left == right)
}

fn validate_mutation(
    before: &[TaskRecord],
    after: &[TaskRecord],
    transition: &str,
    task_id: Option<&str>,
) -> Result<(), String> {
    let ids: HashSet<&str> = after.iter().map(|task| task.id.as_str()).collect();
    if ids.len() != after.len() {
        return Err("task command would create a duplicate task id".to_string());
    }
    match transition {
        "bootstrap" => {
            if !before.is_empty() {
                return Err("task ledger bootstrap requires empty prior state".to_string());
            }
        }
        "add" => {
            if after.len() != before.len() + 1
                || tasks_digest(before)? != tasks_digest(&after[..before.len()])?
            {
                return Err("task add must append exactly one record".to_string());
            }
            if task_id != after.last().map(|task| task.id.as_str()) {
                return Err("task add command id does not match appended record".to_string());
            }
        }
        "fanout" => {
            if after.len() <= before.len()
                || tasks_digest(before)? != tasks_digest(&after[..before.len()])?
            {
                return Err(
                    "task fanout must append records without changing existing tasks".to_string(),
                );
            }
            if task_id.is_none() {
                return Err("task fanout requires its parent task id".to_string());
            }
        }
        "status" => {
            let id = task_id.ok_or_else(|| "task status command requires task_id".to_string())?;
            if before.len() != after.len() {
                return Err("task status command cannot add or remove tasks".to_string());
            }
            let old = task_by_id(before, id).ok_or_else(|| format!("task not found: {id}"))?;
            let new = task_by_id(after, id).ok_or_else(|| format!("task not found: {id}"))?;
            if !matches!(
                new.status.as_str(),
                "pending" | "in_progress" | "complete" | "failed"
            ) {
                return Err(format!("invalid task status: {}", new.status));
            }
            if !same_except_status(old, new)? {
                return Err(
                    "task status command changed fields outside status metadata".to_string()
                );
            }
            for prior in before.iter().filter(|task| task.id != id) {
                let current = task_by_id(after, &prior.id).ok_or_else(|| {
                    format!("task disappeared during status update: {}", prior.id)
                })?;
                if tasks_digest(std::slice::from_ref(prior))?
                    != tasks_digest(std::slice::from_ref(current))?
                {
                    return Err("task status command changed another task".to_string());
                }
            }
        }
        other => return Err(format!("unknown task ledger transition: {other}")),
    }
    Ok(())
}

fn validate_fence(state: &LedgerState, command: &TaskCommand) -> Result<(), String> {
    if command.lease_epoch == 0 {
        return Ok(());
    }
    if command.transition != "status" {
        return Err("fenced task commands are only valid for status transitions".to_string());
    }
    let id = command
        .task_id
        .as_deref()
        .ok_or_else(|| "fenced task command requires task_id".to_string())?;
    let worker = command
        .worker_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "fenced task command requires worker_id".to_string())?;
    let old_status = task_by_id(&state.tasks, id).map(|task| task.status.as_str());
    let new_status = task_by_id(&command.payload.tasks, id).map(|task| task.status.as_str());
    let current = state.leases.get(id);
    if old_status != Some("in_progress") && new_status == Some("in_progress") {
        let current_epoch = current.map(|(epoch, _)| *epoch).unwrap_or(0);
        if command.lease_epoch <= current_epoch {
            return Err(format!("stale lease epoch for task {id}"));
        }
        return Ok(());
    }
    match current {
        Some((epoch, owner)) if *epoch == command.lease_epoch && owner == worker => Ok(()),
        _ => Err(format!("stale or foreign worker lease for task {id}")),
    }
}

fn apply_command(state: &mut LedgerState, command: &TaskCommand) -> Result<(), String> {
    if command.contract_version != TASK_COMMAND_VERSION {
        return Err(format!(
            "unsupported task ledger contract: {}",
            command.contract_version
        ));
    }
    if command.prior_command_digest != state.head_command_digest {
        return Err(format!(
            "task command chain digest mismatch at revision {}",
            command.revision
        ));
    }
    if command_digest(command)? != command.command_digest {
        return Err(format!(
            "task command envelope digest mismatch at revision {}",
            command.revision
        ));
    }
    if command.expected_revision != state.revision || command.revision != state.revision + 1 {
        return Err(format!(
            "stale task command revision: expected {}, got expected_revision={} revision={}",
            state.revision, command.expected_revision, command.revision
        ));
    }
    if state.operations.contains_key(&command.operation_id) {
        return Err(format!(
            "duplicate task operation id: {}",
            command.operation_id
        ));
    }
    let payload_hash = payload_digest(&command.payload)?;
    if payload_hash != command.payload_digest {
        return Err(format!(
            "task command payload digest mismatch at revision {}",
            command.revision
        ));
    }
    let prior_digest = tasks_digest(&state.tasks)?;
    if prior_digest != command.prior_digest {
        return Err(format!(
            "task command prior digest mismatch at revision {}",
            command.revision
        ));
    }
    if tasks_digest(&command.payload.tasks)? != command.result_digest {
        return Err(format!(
            "task command result digest mismatch at revision {}",
            command.revision
        ));
    }
    validate_mutation(
        &state.tasks,
        &command.payload.tasks,
        &command.transition,
        command.task_id.as_deref(),
    )?;
    validate_fence(state, command)?;
    state
        .revisions_by_digest
        .entry(prior_digest)
        .or_insert(state.revision);
    if command.lease_epoch > 0
        && let (Some(id), Some(worker)) = (&command.task_id, &command.worker_id)
        && task_by_id(&command.payload.tasks, id).map(|task| task.status.as_str())
            == Some("in_progress")
    {
        state
            .leases
            .insert(id.clone(), (command.lease_epoch, worker.clone()));
    }
    state.revision = command.revision;
    state.tasks = command.payload.tasks.clone();
    state
        .revisions_by_digest
        .insert(command.result_digest.clone(), command.revision);
    state.head_command_digest = command.command_digest.clone();
    state
        .operations
        .insert(command.operation_id.clone(), operation_digest(command)?);
    Ok(())
}

fn load_ledger(ledger_dir: &Path) -> Result<LedgerState, String> {
    let mut state = LedgerState::default();
    state.revisions_by_digest.insert(tasks_digest(&[])?, 0);
    for (index, path) in ledger_entries(ledger_dir)?.into_iter().enumerate() {
        let expected_name = format!("{:020}.json", index + 1);
        if path.file_name().and_then(|value| value.to_str()) != Some(expected_name.as_str()) {
            return Err(format!("task ledger revision gap at {}", path.display()));
        }
        let bytes = fs::read(&path)
            .map_err(|e| format!("failed reading task ledger entry {}: {e}", path.display()))?;
        let command: TaskCommand = serde_json::from_slice(&bytes)
            .map_err(|e| format!("invalid task ledger entry {}: {e}", path.display()))?;
        apply_command(&mut state, &command)
            .map_err(|e| format!("invalid task ledger entry {}: {e}", path.display()))?;
    }
    Ok(state)
}

fn operation_id(transition: &str, revision: u64, result_digest: &str) -> String {
    let nonce = NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed);
    let material = format!(
        "{}:{}:{}:{}:{}:{}",
        utc_now_iso(),
        std::process::id(),
        nonce,
        transition,
        revision,
        result_digest
    );
    sha256_bytes(material.as_bytes())
}

struct CommandMeta<'a> {
    operation_id: &'a str,
    transition: &'a str,
    task_id: Option<&'a str>,
    worker_id: Option<&'a str>,
    lease_epoch: u64,
}

#[derive(Debug)]
struct CommitResult {
    warnings: Vec<String>,
}

fn commit_command(
    ledger_dir: &Path,
    state: &mut LedgerState,
    tasks: Vec<TaskRecord>,
    meta: CommandMeta<'_>,
) -> Result<CommitResult, String> {
    let payload = TaskPayload { tasks };
    let payload_hash = payload_digest(&payload)?;
    let prior_hash = tasks_digest(&state.tasks)?;
    let result_hash = tasks_digest(&payload.tasks)?;
    let mut command = TaskCommand {
        contract_version: TASK_COMMAND_VERSION.to_string(),
        operation_id: meta.operation_id.to_string(),
        revision: state.revision + 1,
        expected_revision: state.revision,
        transition: meta.transition.to_string(),
        task_id: meta.task_id.map(ToString::to_string),
        worker_id: meta.worker_id.map(ToString::to_string),
        lease_epoch: meta.lease_epoch,
        policy_digest: None,
        prior_digest: prior_hash,
        payload_digest: payload_hash,
        result_digest: result_hash,
        prior_command_digest: state.head_command_digest.clone(),
        command_digest: String::new(),
        at: utc_now_iso(),
        payload,
    };
    command.command_digest = command_digest(&command)?;
    let intent_hash = operation_digest(&command)?;
    if let Some(existing) = state.operations.get(meta.operation_id) {
        if existing == &intent_hash {
            return Ok(CommitResult {
                warnings: Vec::new(),
            });
        }
        return Err(format!(
            "task operation id reused with different intent: {}",
            meta.operation_id
        ));
    }
    let mut next_state = state.clone();
    apply_command(&mut next_state, &command)?;
    let path = entry_path(ledger_dir, command.revision);
    #[cfg(test)]
    inject_fault(FaultStage::AfterApply, &path)?;
    fs::create_dir_all(ledger_dir)
        .map_err(|e| format!("failed creating task ledger {}: {e}", ledger_dir.display()))?;
    if command.revision == 1
        && let Some(parent) = ledger_dir.parent()
    {
        sync_dir(parent)?;
        #[cfg(test)]
        inject_fault(FaultStage::AfterDirSync, ledger_dir)?;
    }
    if path.exists() {
        return Err(format!(
            "task ledger entry already exists: {}",
            path.display()
        ));
    }
    let mut bytes = serde_json::to_vec_pretty(&command)
        .map_err(|e| format!("failed to encode task command: {e}"))?;
    bytes.push(b'\n');
    let write = write_atomic_durable(&path, &bytes)?;
    *state = next_state;
    let warnings = match write {
        WriteOutcome::Durable => Vec::new(),
        WriteOutcome::PublishedUncertain(error) => vec![format!(
            "operation {} is visible at revision {} but directory durability is uncertain: {error}",
            command.operation_id, command.revision
        )],
    };
    Ok(CommitResult { warnings })
}

pub fn read_tasks() -> Result<Vec<TaskRecord>, String> {
    let tasks_file = resolve_tasks_file()?;
    let (ledger_dir, lock_path) = store_paths(&tasks_file)?;
    let entries = ledger_entries(&ledger_dir)?;
    if !lock_path.exists() && entries.is_empty() {
        let _ = read_snapshot(&tasks_file)?;
        // Preserve the read-only empty-store path, but close the first-writer
        // race by rechecking after the snapshot read.
        if !lock_path.exists() && ledger_entries(&ledger_dir)?.is_empty() {
            return read_legacy(&tasks_file);
        }
    }
    if !lock_path.exists() {
        return Err(format!(
            "task ledger exists without {}; restore the exact lock file before access",
            lock_path.display()
        ));
    }
    let _lock = lock_store_shared(&lock_path)?;
    // Opening the lock file is not a committed transition. A process can die
    // immediately afterward, so an empty ledger still means legacy snapshot
    // authority until the first immutable command is present, unless snapshot
    // metadata proves that authoritative history has been lost.
    if ledger_entries(&ledger_dir)?.is_empty() {
        return read_legacy(&tasks_file);
    }
    let state = load_ledger(&ledger_dir)?;
    if let Err(error) = validate_snapshot(&tasks_file, &state) {
        crate::cx_eprintln!(
            "task ledger: authoritative state loaded at revision {}; derived projection ignored: {error}",
            state.revision
        );
    }
    Ok(state.tasks)
}

fn warn_degraded(operation_id: &str, revision: u64, warning: &str) {
    crate::cx_eprintln!(
        "task ledger: operation {operation_id} committed at revision {revision}; {warning}; inspect task state before retrying"
    );
}

fn project_committed(path: &Path, state: &LedgerState, expected: &ProjectionToken) -> Vec<String> {
    if let Err(error) = check_projection(path, expected) {
        return vec![error];
    }
    match write_snapshot_state(path, state) {
        Ok(warnings) => warnings,
        Err(error) => vec![format!("task projection repair pending: {error}")],
    }
}

pub fn mutate_tasks<T, F>(
    transition: &str,
    worker_id: Option<&str>,
    lease_epoch: u64,
    mutate: F,
) -> Result<T, String>
where
    F: FnOnce(&mut Vec<TaskRecord>) -> Result<TaskMutation<T>, String>,
{
    let tasks_file = resolve_tasks_file()?;
    let (ledger_dir, lock_path) = store_paths(&tasks_file)?;
    let _lock = lock_store(&lock_path)?;
    let entries = ledger_entries(&ledger_dir)?;
    let mut warnings = Vec::new();
    let mut state = if entries.is_empty() {
        LedgerState {
            tasks: read_legacy(&tasks_file)?,
            ..LedgerState::default()
        }
    } else {
        let state = load_ledger(&ledger_dir)?;
        warnings.extend(reconcile_snapshot(&tasks_file, &state)?);
        state
    };
    let projection = capture_projection(&tasks_file)?;
    let original_tasks = state.tasks.clone();
    let mut proposed = state.tasks.clone();
    let mutation = mutate(&mut proposed)?;
    validate_mutation(
        &original_tasks,
        &proposed,
        transition,
        mutation.task_id.as_deref(),
    )?;

    if entries.is_empty() && !state.tasks.is_empty() {
        let bootstrap_id = format!("bootstrap-{}", tasks_digest(&state.tasks)?);
        let bootstrap_tasks = state.tasks.clone();
        let bootstrap = commit_command(
            &ledger_dir,
            &mut LedgerState::default(),
            bootstrap_tasks,
            CommandMeta {
                operation_id: &bootstrap_id,
                transition: "bootstrap",
                task_id: None,
                worker_id: None,
                lease_epoch: 0,
            },
        )?;
        warnings.extend(bootstrap.warnings);
        state = load_ledger(&ledger_dir)?;
    }

    let result_hash = tasks_digest(&proposed)?;
    let op_id = operation_id(transition, state.revision + 1, &result_hash);
    let committed = commit_command(
        &ledger_dir,
        &mut state,
        proposed,
        CommandMeta {
            operation_id: &op_id,
            transition,
            task_id: mutation.task_id.as_deref(),
            worker_id,
            lease_epoch,
        },
    )?;
    warnings.extend(committed.warnings);
    warnings.extend(project_committed(&tasks_file, &state, &projection));
    for warning in warnings {
        warn_degraded(&op_id, state.revision, &warning);
    }
    Ok(mutation.result)
}

#[cfg(test)]
#[path = "task_store_tests.rs"]
mod tests;
