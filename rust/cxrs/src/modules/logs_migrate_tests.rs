use super::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

const INPUT: &[u8] = b"\n{\"ts\":\"2026-01-01\",\"tool\":\"test\"}\nnot-json\n{\"execution_id\":\"e1\",\"timestamp\":\"2026-01-02\",\"command\":\"test\"}\n";

struct Fixture {
    dir: tempfile::TempDir,
    source: PathBuf,
    output: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("runs.jsonl");
        let output = dir.path().join("out.jsonl");
        fs::write(&source, INPUT).unwrap();
        Self {
            dir,
            source,
            output,
        }
    }
}
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        HOOK.with(|hook| *hook.borrow_mut() = None);
    }
}
fn set_hook(callback: impl FnMut(FaultStage) -> CxResult<()> + 'static) -> Reset {
    HOOK.with(|hook| *hook.borrow_mut() = Some(Box::new(callback)));
    Reset
}

#[test]
fn snapshot_matches_backup() {
    let f = Fixture::new();
    let original_path = f.source.clone();
    let _hook = set_hook(move |stage| {
        if stage == FaultStage::Snapshot {
            fs::write(&original_path, b"changed after snapshot").unwrap();
        }
        Ok(())
    });
    let outcome = migrate_transaction(&f.source, &f.output, true).unwrap();
    let backup = outcome.backup.unwrap();
    assert_eq!(fs::read(&backup).unwrap(), INPUT);
    assert_eq!(
        fs::metadata(backup).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(outcome.summary.entries_in, 3);
    assert_eq!(outcome.summary.entries_out, 2);
    assert_eq!(outcome.summary.invalid_json_skipped, 1);
    assert_eq!(outcome.summary.legacy_normalized, 1);
    assert_eq!(outcome.summary.modern_normalized, 1);
    let rows = fs::read_to_string(&f.source).unwrap();
    assert_eq!(rows.lines().count(), 2);
    assert!(!f.output.exists());
}

#[test]
fn prepublish_preserves_files() {
    for stage in [
        FaultStage::BackupWrite,
        FaultStage::BackupSync,
        FaultStage::Snapshot,
        FaultStage::OutputWrite,
        FaultStage::Publish,
    ] {
        let f = Fixture::new();
        fs::write(&f.output, b"prior output").unwrap();
        let _hook = set_hook(move |current| {
            if current == stage {
                Err(CxError::invalid("injected failure"))
            } else {
                Ok(())
            }
        });
        assert!(migrate_transaction(&f.source, &f.output, true).is_err());
        assert_eq!(fs::read(&f.source).unwrap(), INPUT);
        assert_eq!(fs::read(&f.output).unwrap(), b"prior output");
        for entry in fs::read_dir(f.dir.path()).unwrap() {
            assert!(
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp")
            );
        }
    }
}

#[test]
fn published_reports_backup() {
    let f = Fixture::new();
    let _hook = set_hook(|stage| {
        if stage == FaultStage::Published {
            Err(CxError::invalid("injected sync failure"))
        } else {
            Ok(())
        }
    });
    let error = migrate_transaction(&f.source, &f.output, true)
        .err()
        .unwrap();
    assert!(error.contains("published; durability uncertain"), "{error}");
    assert!(error.contains("backup:"), "{error}");
    assert_ne!(fs::read(&f.source).unwrap(), INPUT);
    let backups: Vec<_> = fs::read_dir(f.dir.path())
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".bak.")
                .then_some(p)
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read(&backups[0]).unwrap(), INPUT);
}

#[test]
fn parent_swap_safe() {
    let base = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let parent = base.path().join("logs");
    fs::create_dir(&parent).unwrap();
    let moved = base.path().join("moved");
    let source = parent.join("runs.jsonl");
    let output = parent.join("out.jsonl");
    fs::write(&source, INPUT).unwrap();
    fs::write(outside.path().join("runs.jsonl"), b"victim").unwrap();
    let outside_path = outside.path().to_owned();
    let moved_path = moved.clone();
    let _hook = set_hook(move |stage| {
        if stage == FaultStage::Anchored {
            fs::rename(&parent, &moved_path).unwrap();
            symlink(&outside_path, &parent).unwrap();
        }
        Ok(())
    });
    migrate_transaction(&source, &output, true).unwrap();
    assert_eq!(
        fs::read(outside.path().join("runs.jsonl")).unwrap(),
        b"victim"
    );
    assert_ne!(fs::read(moved.join("runs.jsonl")).unwrap(), INPUT);
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
}

#[test]
fn aliases_output_safe() {
    let f = Fixture::new();
    assert!(migrate_runs_jsonl(&f.source, &f.source).is_err());
    let alias = f.dir.path().join("alias");
    fs::hard_link(&f.source, &alias).unwrap();
    assert!(migrate_runs_jsonl(&f.source, &alias).is_err());
    assert_eq!(fs::read(&f.source).unwrap(), INPUT);
    fs::write(&f.output, b"old output").unwrap();
    migrate_runs_jsonl(&f.source, &f.output).unwrap();
    assert_eq!(fs::read(&f.source).unwrap(), INPUT);
    assert_eq!(
        fs::metadata(&f.output).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let outcome = migrate_transaction(&f.source, &f.source, true).unwrap();
    assert_eq!(fs::read(outcome.backup.unwrap()).unwrap(), INPUT);
}

#[test]
fn invalid_source_safe() {
    let f = Fixture::new();
    let victim = f.dir.path().join("victim");
    fs::write(&victim, INPUT).unwrap();
    symlink(&victim, &f.output).unwrap();
    assert!(migrate_runs_jsonl(&f.source, &f.output).is_err());
    fs::remove_file(&f.source).unwrap();
    symlink(&victim, &f.source).unwrap();
    assert!(migrate_transaction(&f.source, &victim, true).is_err());
    assert_eq!(fs::read(&victim).unwrap(), INPUT);
    fs::remove_file(&f.source).unwrap();
    fs::write(&f.source, b"[]\n").unwrap();
    fs::remove_file(&f.output).unwrap();
    fs::write(&f.output, b"prior output").unwrap();
    assert!(migrate_transaction(&f.source, &f.output, true).is_err());
    assert_eq!(fs::read(&f.source).unwrap(), b"[]\n");
    assert_eq!(fs::read(&f.output).unwrap(), b"prior output");
}

#[test]
fn cross_device_safe() {
    let f = Fixture::new();
    fs::write(&f.output, b"prior output").unwrap();
    let _hook = set_hook(|stage| {
        if stage == FaultStage::Publish {
            Err(CxError::io(
                "publish migration output",
                std::io::Error::from(std::io::ErrorKind::CrossesDevices),
            ))
        } else {
            Ok(())
        }
    });
    assert!(migrate_transaction(&f.source, &f.output, true).is_err());
    assert_eq!(fs::read(&f.source).unwrap(), INPUT);
    assert_eq!(fs::read(&f.output).unwrap(), b"prior output");
}

#[test]
fn long_leaf_safe() {
    let f = Fixture::new();
    let source = f.dir.path().join("s".repeat(255));
    let output = f.dir.path().join("o".repeat(255));
    fs::rename(&f.source, &source).unwrap();
    migrate_runs_jsonl(&source, &output).unwrap();
    assert_eq!(fs::read(&source).unwrap(), INPUT);
    let outcome = migrate_transaction(&source, &output, true).unwrap();
    assert_eq!(fs::read(outcome.backup.unwrap()).unwrap(), INPUT);
    assert_eq!(fs::read(&source).unwrap(), fs::read(&output).unwrap());
}
