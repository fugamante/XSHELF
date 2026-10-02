use super::task_store_io::{snapshot_meta_path, write_snapshot};
use super::*;
use tempfile::TempDir;

fn task(id: &str, status: &str) -> TaskRecord {
    TaskRecord {
        id: id.to_string(),
        parent_id: None,
        role: "implementer".to_string(),
        objective: "test".to_string(),
        context_ref: String::new(),
        backend: "auto".to_string(),
        model: None,
        profile: "balanced".to_string(),
        converge: "none".to_string(),
        replicas: 1,
        max_concurrency: None,
        run_mode: "sequential".to_string(),
        depends_on: Vec::new(),
        resource_keys: Vec::new(),
        max_retries: None,
        timeout_secs: None,
        status: status.to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn command(
    state: &LedgerState,
    operation_id: &str,
    tasks: Vec<TaskRecord>,
    worker: Option<&str>,
    epoch: u64,
) -> TaskCommand {
    let payload = TaskPayload { tasks };
    let mut command = TaskCommand {
        contract_version: TASK_COMMAND_VERSION.to_string(),
        operation_id: operation_id.to_string(),
        revision: state.revision + 1,
        expected_revision: state.revision,
        transition: "status".to_string(),
        task_id: Some("task_001".to_string()),
        worker_id: worker.map(ToString::to_string),
        lease_epoch: epoch,
        policy_digest: None,
        prior_digest: tasks_digest(&state.tasks).expect("prior digest"),
        payload_digest: payload_digest(&payload).expect("payload digest"),
        result_digest: tasks_digest(&payload.tasks).expect("result digest"),
        prior_command_digest: state.head_command_digest.clone(),
        command_digest: String::new(),
        at: "2026-01-01T00:00:00Z".to_string(),
        payload,
    };
    command.command_digest = command_digest(&command).expect("command digest");
    command
}

#[test]
fn stale_rev_rejected() {
    let mut state = LedgerState {
        tasks: vec![task("task_001", "pending")],
        revision: 2,
        ..LedgerState::default()
    };
    let mut updated = state.tasks.clone();
    updated[0].status = "complete".to_string();
    let mut cmd = command(&state, "stale", updated, None, 0);
    cmd.expected_revision = 1;
    cmd.command_digest = command_digest(&cmd).unwrap();
    assert!(
        apply_command(&mut state, &cmd)
            .unwrap_err()
            .contains("stale task command revision")
    );
}

#[test]
fn duplicate_op_idempotent() {
    let temp = TempDir::new().expect("tempdir");
    let tasks = vec![task("task_001", "pending")];
    commit_command(
        temp.path(),
        &mut LedgerState::default(),
        tasks.clone(),
        CommandMeta {
            operation_id: "same-op",
            transition: "add",
            task_id: Some("task_001"),
            worker_id: None,
            lease_epoch: 0,
        },
    )
    .expect("first commit");
    let mut replayed = load_ledger(temp.path()).expect("replay first commit");
    let revision = replayed.revision;
    commit_command(
        temp.path(),
        &mut replayed,
        tasks.clone(),
        CommandMeta {
            operation_id: "same-op",
            transition: "add",
            task_id: Some("task_001"),
            worker_id: None,
            lease_epoch: 0,
        },
    )
    .expect("same intent is idempotent");
    assert_eq!(replayed.revision, revision);
    assert_eq!(ledger_entries(temp.path()).unwrap().len(), 1);

    assert!(
        commit_command(
            temp.path(),
            &mut replayed,
            tasks,
            CommandMeta {
                operation_id: "same-op",
                transition: "fanout",
                task_id: Some("task_001"),
                worker_id: None,
                lease_epoch: 0,
            },
        )
        .unwrap_err()
        .contains("different intent")
    );
}

#[test]
fn worker_fence_rejected() {
    let mut state = LedgerState {
        tasks: vec![task("task_001", "pending")],
        ..LedgerState::default()
    };
    let mut claimed = state.tasks.clone();
    claimed[0].status = "in_progress".to_string();
    let claim = command(&state, "claim", claimed, Some("worker-a"), 2);
    apply_command(&mut state, &claim).expect("claim accepted");

    let mut completed = state.tasks.clone();
    completed[0].status = "complete".to_string();
    let stale = command(
        &state,
        "stale-finish",
        completed.clone(),
        Some("worker-a"),
        1,
    );
    assert!(
        apply_command(&mut state, &stale)
            .unwrap_err()
            .contains("stale or foreign")
    );
    let foreign = command(&state, "foreign-finish", completed, Some("worker-b"), 2);
    assert!(
        apply_command(&mut state, &foreign)
            .unwrap_err()
            .contains("stale or foreign")
    );
}

#[test]
fn crash_replay_safe() {
    let temp = TempDir::new().expect("tempdir");
    let ledger = temp.path().join("task_ledger");
    let snapshot = temp.path().join("tasks.json");
    let mut state = LedgerState::default();
    let committed = vec![task("task_001", "pending")];
    commit_command(
        &ledger,
        &mut state,
        committed.clone(),
        CommandMeta {
            operation_id: "add-1",
            transition: "add",
            task_id: Some("task_001"),
            worker_id: None,
            lease_epoch: 0,
        },
    )
    .expect("commit ledger entry");

    fs::write(ledger.join("00000000000000000002.tmp.crash"), b"partial")
        .expect("write pre-rename crash artifact");
    write_snapshot(&snapshot, &[]).expect("write stale snapshot");
    let replayed = load_ledger(&ledger).expect("replay committed prefix");
    assert_eq!(
        tasks_digest(&replayed.tasks).unwrap(),
        tasks_digest(&committed).unwrap()
    );
    reconcile_snapshot(&snapshot, &replayed).expect("repair stale snapshot");
    assert_eq!(
        tasks_digest(&read_snapshot(&snapshot).unwrap()).unwrap(),
        tasks_digest(&committed).unwrap()
    );

    let entry = entry_path(&ledger, 1);
    let mut damaged: TaskCommand = serde_json::from_slice(&fs::read(&entry).unwrap()).unwrap();
    damaged.result_digest = "bad".to_string();
    damaged.command_digest = command_digest(&damaged).unwrap();
    fs::write(&entry, serde_json::to_vec_pretty(&damaged).unwrap()).unwrap();
    assert!(
        load_ledger(&ledger)
            .unwrap_err()
            .contains("result digest mismatch")
    );
}

#[test]
fn envelope_digest_rejected() {
    let state = LedgerState {
        tasks: vec![task("task_001", "pending")],
        ..LedgerState::default()
    };
    let mut updated = state.tasks.clone();
    updated[0].status = "complete".to_string();
    let mut command = command(&state, "metadata-corruption", updated, Some("worker-a"), 0);
    command.worker_id = Some("worker-b".to_string());
    assert!(
        apply_command(&mut state.clone(), &command)
            .unwrap_err()
            .contains("envelope digest mismatch")
    );
}

#[test]
fn precommit_faults_uncommitted() {
    let stages = [
        FaultStage::AfterApply,
        FaultStage::AfterWrite,
        FaultStage::AfterFileSync,
        FaultStage::BeforeRename,
    ];
    for stage in stages {
        let temp = TempDir::new().expect("tempdir");
        let ledger = temp.path().join("task_ledger");
        let mut state = LedgerState::default();
        set_fault(stage, "00000000000000000001.json");
        let error = commit_command(
            &ledger,
            &mut state,
            vec![task("task_001", "pending")],
            CommandMeta {
                operation_id: "faulted-add",
                transition: "add",
                task_id: Some("task_001"),
                worker_id: None,
                lease_epoch: 0,
            },
        )
        .unwrap_err();
        assert!(error.contains("injected"), "{stage:?}: {error}");
        assert_eq!(state.revision, 0, "{stage:?}");
        assert!(ledger_entries(&ledger).unwrap().is_empty(), "{stage:?}");
    }

    let temp = TempDir::new().expect("tempdir");
    let ledger = temp.path().join("task_ledger");
    let mut state = LedgerState::default();
    set_fault(FaultStage::AfterDirSync, "task_ledger");
    assert!(
        commit_command(
            &ledger,
            &mut state,
            vec![task("task_001", "pending")],
            CommandMeta {
                operation_id: "directory-fault",
                transition: "add",
                task_id: Some("task_001"),
                worker_id: None,
                lease_epoch: 0,
            },
        )
        .unwrap_err()
        .contains("injected")
    );
    assert_eq!(state.revision, 0);
    assert!(ledger_entries(&ledger).unwrap().is_empty());
}

#[test]
fn postrename_faults_committed() {
    for stage in [FaultStage::AfterRename, FaultStage::AfterDirSync] {
        let temp = TempDir::new().expect("tempdir");
        let ledger = temp.path().join("task_ledger");
        let mut state = LedgerState::default();
        set_fault(stage, "00000000000000000001.json");
        let outcome = commit_command(
            &ledger,
            &mut state,
            vec![task("task_001", "pending")],
            CommandMeta {
                operation_id: "visible-add",
                transition: "add",
                task_id: Some("task_001"),
                worker_id: None,
                lease_epoch: 0,
            },
        )
        .expect("visible entry is committed");
        assert_eq!(outcome.warnings.len(), 1, "{stage:?}");
        assert_eq!(state.revision, 1, "{stage:?}");
        assert_eq!(load_ledger(&ledger).unwrap().revision, 1, "{stage:?}");
    }
}

#[test]
fn bootstrap_fault_prefix() {
    let temp = TempDir::new().expect("tempdir");
    let ledger = temp.path().join("task_ledger");
    let existing = vec![task("task_001", "pending")];
    let mut state = LedgerState::default();
    commit_command(
        &ledger,
        &mut state,
        existing.clone(),
        CommandMeta {
            operation_id: "bootstrap-existing",
            transition: "bootstrap",
            task_id: None,
            worker_id: None,
            lease_epoch: 0,
        },
    )
    .expect("bootstrap commits");

    let mut proposed = existing;
    proposed.push(task("task_002", "pending"));
    set_fault(FaultStage::AfterWrite, "00000000000000000002.json");
    assert!(
        commit_command(
            &ledger,
            &mut state,
            proposed,
            CommandMeta {
                operation_id: "faulted-user-command",
                transition: "add",
                task_id: Some("task_002"),
                worker_id: None,
                lease_epoch: 0,
            },
        )
        .unwrap_err()
        .contains("injected")
    );
    let replayed = load_ledger(&ledger).expect("bootstrap prefix replays");
    assert_eq!(replayed.revision, 1);
    assert_eq!(replayed.tasks.len(), 1);
    assert_eq!(ledger_entries(&ledger).unwrap().len(), 1);
}

#[test]
fn projection_faults_degraded() {
    let stages = [
        FaultStage::AfterWrite,
        FaultStage::AfterFileSync,
        FaultStage::BeforeRename,
        FaultStage::AfterRename,
        FaultStage::AfterDirSync,
    ];
    for target in ["tasks.json", "tasks.snapshot.json"] {
        for stage in stages {
            let temp = TempDir::new().expect("tempdir");
            let ledger = temp.path().join("task_ledger");
            let snapshot = temp.path().join("tasks.json");
            let mut state = LedgerState::default();
            commit_command(
                &ledger,
                &mut state,
                vec![task("task_001", "pending")],
                CommandMeta {
                    operation_id: "projection-add",
                    transition: "add",
                    task_id: Some("task_001"),
                    worker_id: None,
                    lease_epoch: 0,
                },
            )
            .unwrap();
            let token = capture_projection(&snapshot).unwrap();
            set_fault(stage, target);
            let warnings = project_committed(&snapshot, &state, &token);
            assert_eq!(warnings.len(), 1, "{target} {stage:?}");
            assert_eq!(load_ledger(&ledger).unwrap().revision, 1);
            validate_snapshot(&snapshot, &state).expect("ledger remains readable");
        }
    }
}

#[test]
fn projection_drift_preserved() {
    let temp = TempDir::new().expect("tempdir");
    let ledger = temp.path().join("task_ledger");
    let snapshot = temp.path().join("tasks.json");
    let mut state = LedgerState::default();
    commit_command(
        &ledger,
        &mut state,
        vec![task("task_001", "pending")],
        CommandMeta {
            operation_id: "add",
            transition: "add",
            task_id: Some("task_001"),
            worker_id: None,
            lease_epoch: 0,
        },
    )
    .unwrap();
    write_snapshot_state(&snapshot, &state).unwrap();
    let token = capture_projection(&snapshot).unwrap();
    let drift = vec![task("task_999", "pending")];
    write_snapshot(&snapshot, &drift).unwrap();
    let warnings = project_committed(&snapshot, &state, &token);
    assert_eq!(warnings.len(), 1);
    assert_eq!(read_snapshot(&snapshot).unwrap()[0].id, "task_999");
}

#[test]
fn legacy_drift_rejected() {
    let temp = TempDir::new().expect("tempdir");
    let ledger = temp.path().join("task_ledger");
    let snapshot = temp.path().join("tasks.json");
    let mut state = LedgerState::default();
    commit_command(
        &ledger,
        &mut state,
        vec![task("task_001", "pending")],
        CommandMeta {
            operation_id: "add-1",
            transition: "add",
            task_id: Some("task_001"),
            worker_id: None,
            lease_epoch: 0,
        },
    )
    .expect("commit ledger entry");
    write_snapshot_state(&snapshot, &state).expect("write current snapshot");

    fs::remove_file(&snapshot).expect("remove snapshot");
    reconcile_snapshot(&snapshot, &state).expect("repair missing snapshot");
    fs::write(&snapshot, b"{ broken").expect("damage snapshot");
    reconcile_snapshot(&snapshot, &state).expect("repair malformed snapshot");
    fs::write(snapshot_meta_path(&snapshot).unwrap(), b"{ broken")
        .expect("damage snapshot metadata");
    reconcile_snapshot(&snapshot, &state).expect("repair malformed metadata");

    write_snapshot(&snapshot, &[task("task_001", "failed")]).expect("simulate old writer");
    assert!(
        reconcile_snapshot(&snapshot, &state)
            .unwrap_err()
            .contains("changed outside the authoritative task ledger")
    );
}

#[test]
fn future_meta_preserved() {
    let temp = TempDir::new().expect("tempdir");
    let snapshot = temp.path().join("tasks.json");
    let state = LedgerState::default();
    write_snapshot_state(&snapshot, &state).unwrap();
    let meta_path = snapshot_meta_path(&snapshot).unwrap();
    let mut meta: SnapshotMeta = serde_json::from_slice(&fs::read(&meta_path).unwrap()).unwrap();
    meta.contract_version = "task-command.v2".to_string();
    let bytes = serde_json::to_vec(&meta).unwrap();
    fs::write(&meta_path, &bytes).unwrap();
    for damaged in [false, true] {
        if damaged {
            fs::remove_file(&snapshot).unwrap();
        }
        let error = reconcile_snapshot(&snapshot, &state).unwrap_err();
        assert!(error.contains("unsupported task snapshot metadata contract"));
        assert_eq!(fs::read(&meta_path).unwrap(), bytes);
    }
}

#[path = "task_restore_tests.rs"]
mod restore;
