use super::*;

fn restore_fixture() -> (TempDir, LedgerState) {
    let temp = TempDir::new().expect("tempdir");
    let ledger = temp.path().join("task_ledger");
    let snapshot = temp.path().join("tasks.json");
    let mut state = LedgerState::default();
    let pending = vec![task("task_001", "pending")];
    let complete = vec![task("task_001", "complete")];
    // Fixed task timestamps make revisions 2 and 3 exactly identical without
    // relying on two CLI invocations landing in the same wall-clock second.
    let states = [pending, complete.clone(), complete];
    let mut saved = None;
    for (index, tasks) in states.into_iter().enumerate() {
        let operation = format!("restore-{index}");
        commit_command(
            &ledger,
            &mut state,
            tasks,
            CommandMeta {
                operation_id: &operation,
                transition: if index == 0 { "add" } else { "status" },
                task_id: Some("task_001"),
                worker_id: None,
                lease_epoch: 0,
            },
        )
        .unwrap();
        if index == 1 {
            write_snapshot_state(&snapshot, &state).unwrap();
            saved = Some((
                fs::read(&snapshot).unwrap(),
                fs::read(snapshot_meta_path(&snapshot).unwrap()).unwrap(),
            ));
        }
    }
    let mut tasks = state.tasks.clone();
    tasks.push(task("task_002", "pending"));
    commit_command(
        &ledger,
        &mut state,
        tasks,
        CommandMeta {
            operation_id: "restore-next",
            transition: "add",
            task_id: Some("task_002"),
            worker_id: None,
            lease_epoch: 0,
        },
    )
    .unwrap();
    write_snapshot_state(&snapshot, &state).unwrap();
    let (bytes, metadata) = saved.unwrap();
    fs::write(&snapshot, bytes).unwrap();
    fs::write(snapshot_meta_path(&snapshot).unwrap(), metadata).unwrap();
    (temp, load_ledger(&ledger).unwrap())
}

#[test]
fn repeat_restore_safe() {
    let (temp, mut state) = restore_fixture();
    let snapshot = temp.path().join("tasks.json");
    let ledger = temp.path().join("task_ledger");
    let entries = ledger_entries(&ledger).unwrap();
    let bytes: Vec<_> = entries.iter().map(|p| fs::read(p).unwrap()).collect();
    reconcile_snapshot(&snapshot, &state).expect("valid historical revision must repair");
    assert_eq!(read_snapshot(&snapshot).unwrap().len(), 2);
    assert_eq!(
        entries
            .iter()
            .map(|p| fs::read(p).unwrap())
            .collect::<Vec<_>>(),
        bytes
    );
    let mut tasks = state.tasks.clone();
    tasks.push(task("task_003", "pending"));
    commit_command(
        &ledger,
        &mut state,
        tasks,
        CommandMeta {
            operation_id: "restore-final",
            transition: "add",
            task_id: Some("task_003"),
            worker_id: None,
            lease_epoch: 0,
        },
    )
    .unwrap();
    assert_eq!(load_ledger(&ledger).unwrap().tasks.len(), 3);
    assert_eq!(state.revision, 5);
}

#[test]
fn restore_meta_rejected() {
    // Revision 1 has a different state; revision 5 does not yet exist. Neither
    // becomes valid just because the restored digest occurs at revisions 2/3.
    for revision in [1, 5] {
        let (temp, state) = restore_fixture();
        let snapshot = temp.path().join("tasks.json");
        let metadata = snapshot_meta_path(&snapshot).unwrap();
        let mut meta: SnapshotMeta = serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
        meta.revision = revision;
        let bytes = serde_json::to_vec(&meta).unwrap();
        fs::write(&metadata, &bytes).unwrap();
        let old_snapshot = fs::read(&snapshot).unwrap();
        assert!(
            reconcile_snapshot(&snapshot, &state)
                .unwrap_err()
                .contains("does not match a recorded task ledger revision")
        );
        assert_eq!(fs::read(&snapshot).unwrap(), old_snapshot);
        assert_eq!(fs::read(&metadata).unwrap(), bytes);
    }
}
