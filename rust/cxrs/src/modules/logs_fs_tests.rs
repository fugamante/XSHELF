use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

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
