use super::*;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};

#[test]
fn retry_recovers_noent() {
    let dir = tempfile::tempdir().unwrap();
    let path = AnchoredPath::open(&dir.path().join("runs.jsonl"), true).unwrap();
    let mut attempts = 0;
    let mut file = File::from(
        retry_append_open(|| {
            attempts += 1;
            if attempts == 1 {
                return Err(rustix::io::Errno::NOENT);
            }
            fs::openat(
                &path.parent,
                &path.leaf,
                OFlags::WRONLY | OFlags::APPEND | OFlags::CREATE | OFlags::NOFOLLOW,
                Mode::from_raw_mode(0o600),
            )
        })
        .unwrap(),
    );
    file.write_all(b"first\n").unwrap();
    let mut reused = path.append_regular().unwrap();
    reused.write_all(b"second\n").unwrap();
    assert_eq!(attempts, 2);
    assert_eq!(std::fs::read(&path.display).unwrap(), b"first\nsecond\n");
    assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn retry_bounds_noent() {
    let mut attempts = 0;
    let error = retry_append_open(|| {
        attempts += 1;
        Err::<(), _>(rustix::io::Errno::NOENT)
    })
    .unwrap_err();
    assert_eq!(attempts, APPEND_OPEN_RETRIES + 1);
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

#[test]
fn retry_rejects_other() {
    let mut attempts = 0;
    let error = retry_append_open(|| {
        attempts += 1;
        Err::<(), _>(rustix::io::Errno::LOOP)
    })
    .unwrap_err();
    assert_eq!(attempts, 1);
    assert_eq!(
        error.raw_os_error(),
        Some(rustix::io::Errno::LOOP.raw_os_error())
    );
}

#[test]
fn retry_rejects_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let victim = dir.path().join("victim.jsonl");
    std::fs::write(&victim, b"sentinel\n").unwrap();
    let path = AnchoredPath::open(&dir.path().join("runs.jsonl"), true).unwrap();
    let mut attempts = 0;
    let error = retry_append_open(|| {
        attempts += 1;
        if attempts == 1 {
            symlink(&victim, &path.display).unwrap();
            return Err(rustix::io::Errno::NOENT);
        }
        fs::openat(
            &path.parent,
            &path.leaf,
            OFlags::WRONLY
                | OFlags::APPEND
                | OFlags::CREATE
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
    })
    .unwrap_err();
    assert_ne!(error.kind(), io::ErrorKind::NotFound);
    assert_eq!(attempts, 2);
    assert_eq!(std::fs::read(victim).unwrap(), b"sentinel\n");
}

#[test]
fn collision_preserves_victim() {
    let dir = tempfile::tempdir().unwrap();
    let victim = dir.path().join("victim");
    std::fs::write(&victim, b"sentinel").unwrap();
    let path = AnchoredPath::open(&dir.path().join("out"), false).unwrap();
    symlink(&victim, dir.path().join("collision")).unwrap();
    assert!(PrivateFile::named(&path, OsString::from("collision")).is_err());
    assert_eq!(std::fs::read(victim).unwrap(), b"sentinel");
    let file = PrivateFile::named(&path, OsString::from("private")).unwrap();
    assert_eq!(
        file.file.metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
    drop(file);
    assert!(!dir.path().join("private").exists());
}

#[test]
fn parent_symlink_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), dir.path().join("redirect")).unwrap();
    assert!(AnchoredPath::open(&dir.path().join("redirect/out"), true).is_err());
    assert!(std::fs::read_dir(outside.path()).unwrap().next().is_none());
}
