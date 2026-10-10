mod common;

#[allow(dead_code)]
#[path = "../src/modules/bench_parity_mocks.rs"]
mod bench_parity_mocks;

use bench_parity_mocks::setup_parity_mocks;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn dirs() -> (TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("create owned scratch");
    let repo = root.path().join("repo");
    let temp = root.path().join("parity-temp");
    fs::create_dir_all(repo.join(".cx/schemas")).expect("create schema registry");
    fs::create_dir(&temp).expect("create parity temp");
    (root, repo, temp)
}

fn source(repo: &Path, name: &str) -> PathBuf {
    repo.join(".cx/schemas").join(name)
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    #[test]
    fn temp_private_early() {
        let (root, repo, _unused_temp) = dirs();
        let temp_parent = root.path().join("system-temp");
        fs::create_dir(&temp_parent).expect("create system temp parent");
        fs::set_permissions(&temp_parent, fs::Permissions::from_mode(0o1777))
            .expect("model shared temp parent");
        let mock_bin = root.path().join("mock-bin");
        fs::create_dir(&mock_bin).expect("create mock git directory");
        let marker = root.path().join("git-started");
        let release = root.path().join("git-release");
        let git = mock_bin.join("git");
        fs::write(
            &git,
            "#!/bin/sh\nif [ \"$1\" = init ]; then\n  : > \"$PARITY_READY\"\n  while [ ! -e \"$PARITY_RELEASE\" ]; do sleep 0.05; done\nfi\nexit 0\n",
        )
        .expect("write mock git");
        fs::set_permissions(&git, fs::Permissions::from_mode(0o755))
            .expect("make mock git runnable");
        let path = format!(
            "{}:{}",
            mock_bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut child = Command::new(env!("CARGO_BIN_EXE_cxrs"))
            .arg("parity")
            .current_dir(&repo)
            .env("CX_REPO_ROOT", &repo)
            .env("TMPDIR", &temp_parent)
            .env("PATH", path)
            .env("PARITY_READY", &marker)
            .env("PARITY_RELEASE", &release)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("launch parity");

        let deadline = Instant::now() + Duration::from_secs(5);
        let temp_repo = loop {
            if marker.exists()
                && let Some(dir) = fs::read_dir(&temp_parent)
                    .expect("list temp parent")
                    .flatten()
                    .map(|ent| ent.path())
                    .find(|path| {
                        path.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with("cxparity-"))
                    })
            {
                break dir;
            }
            if Instant::now() >= deadline {
                child.kill().expect("stop stalled parity");
                panic!("parity did not reach mock git init");
            }
            sleep(Duration::from_millis(10));
        };
        let mode = fs::metadata(&temp_repo)
            .expect("temp metadata")
            .permissions()
            .mode()
            & 0o777;
        fs::write(&release, b"continue").expect("release mock git");
        let _ = child.wait().expect("wait for parity cleanup");
        assert_eq!(mode, 0o700, "temp root was exposed before setup");
        assert!(!temp_repo.exists(), "parity did not clean temp root");
    }

    #[test]
    fn private_copy() {
        let (_root, repo, temp) = dirs();
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o755))
            .expect("make initial temp visible");
        let schema = br#"{"$id":"cx://schemas/synthetic","type":"object"}"#;
        fs::write(source(&repo, "synthetic.json"), schema).expect("write source schema");

        let mock_dir = setup_parity_mocks(&repo, &temp).expect("copy regular schema");
        let copied = temp.join(".cx/schemas/synthetic.json");
        assert_eq!(fs::read(&copied).expect("read copied schema"), schema);
        assert_eq!(
            fs::metadata(&temp).expect("temp mode").permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&copied)
                .expect("copy mode")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(mock_dir.join("pbcopy").is_file());
    }

    #[test]
    fn parity_path_safe() {
        for name in [
            "plain-repo",
            "repo'quote",
            "repo'; printf injected > \"$PARITY_MARKER\"; #",
            "repo'$(printf injected > \"$PARITY_MARKER\")'",
        ] {
            let root = tempfile::tempdir().expect("create owned scratch");
            let repo = root.path().join(name);
            fs::create_dir_all(repo.join("lib")).expect("create parity library directory");
            fs::create_dir_all(repo.join(".cx/schemas")).expect("create schema registry");
            fs::write(
                repo.join("lib/cx.sh"),
                "cx() { printf '%s:%s\\n' \"$1\" \"$2\" > \"$PARITY_CALLED\"; }\n",
            )
            .expect("write trusted parity function");
            let marker = root.path().join("injected");
            let called = root.path().join("called");
            let out = Command::new(env!("CARGO_BIN_EXE_cxrs"))
                .arg("parity")
                .current_dir(&repo)
                .env("CX_REPO_ROOT", &repo)
                .env("PARITY_MARKER", &marker)
                .env("PARITY_CALLED", &called)
                .env("TMPDIR", root.path())
                .env("CX_LOG_FILE", root.path().join("runs.jsonl"))
                .env("CXLOG_ENABLED", "0")
                .output()
                .expect("run parity with synthetic repository");
            assert!(
                !marker.exists(),
                "repository path executed shell syntax: {out:?}"
            );
            assert_eq!(
                fs::read_to_string(&called).expect("bash parity function ran"),
                "echo:hi\n",
                "catalog arguments changed: {out:?}"
            );
        }
    }

    #[test]
    fn unsafe_temp_denied() {
        let (root, repo, _unused_temp) = dirs();
        let unsafe_parent = root.path().join("unsafe-temp");
        fs::create_dir(&unsafe_parent).expect("create unsafe parent");
        fs::set_permissions(&unsafe_parent, fs::Permissions::from_mode(0o777))
            .expect("make parent shared without sticky bit");
        let out = Command::new(env!("CARGO_BIN_EXE_cxrs"))
            .arg("parity")
            .current_dir(&repo)
            .env("CX_REPO_ROOT", &repo)
            .env("TMPDIR", &unsafe_parent)
            .output()
            .expect("run parity with unsafe temp parent");
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("unsafe temp parent"));
        assert_eq!(
            fs::read_dir(&unsafe_parent)
                .expect("list unsafe parent")
                .count(),
            0
        );
    }

    #[test]
    fn unsafe_ancestor_denied() {
        let (root, repo, _unused_temp) = dirs();
        let unsafe_upper = root.path().join("unsafe-upper");
        let private_parent = unsafe_upper.join("private-temp");
        fs::create_dir_all(&private_parent).expect("create nested temp parent");
        fs::set_permissions(&unsafe_upper, fs::Permissions::from_mode(0o777))
            .expect("make ancestor shared without sticky bit");
        fs::set_permissions(&private_parent, fs::Permissions::from_mode(0o700))
            .expect("make immediate temp parent private");
        let out = Command::new(env!("CARGO_BIN_EXE_cxrs"))
            .arg("parity")
            .current_dir(&repo)
            .env("CX_REPO_ROOT", &repo)
            .env("TMPDIR", &private_parent)
            .output()
            .expect("run parity with unsafe temp ancestor");
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("unsafe temp ancestor"));
        assert_eq!(
            fs::read_dir(&private_parent)
                .expect("list private parent")
                .count(),
            0
        );
    }

    #[test]
    fn sparse_limit() {
        let (_root, repo, temp) = dirs();
        fs::File::create(source(&repo, "huge.json"))
            .expect("create sparse source")
            .set_len(64 * 1024 * 1024 + 1)
            .expect("extend sparse source");
        let error = setup_parity_mocks(&repo, &temp).expect_err("reject oversized schema");
        assert!(error.contains("exceeds byte limit"), "{error}");
        assert!(!temp.join(".cx/schemas/huge.json").exists());
    }

    #[test]
    fn growth_cleanup() {
        let (_root, repo, temp) = dirs();
        let path = source(&repo, "growing.json");
        fs::write(&path, b"ok").expect("write initial source");
        let mut held = fs::File::open(&path).expect("open source descriptor");
        assert_eq!(held.metadata().expect("initial source size").len(), 2);
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open appender")
            .write_all(b"extra bytes")
            .expect("grow regular source after initial check");
        let target = temp.join("growing.json");
        let error = bench_parity_mocks::copy_limited(&mut held, &target, 2)
            .expect_err("bound growing source");
        assert!(error.contains("exceeds byte limit"), "{error}");
        assert!(!target.exists(), "partial copy remained after overflow");
    }

    #[test]
    fn link_denied() {
        let (root, repo, temp) = dirs();
        let private = root.path().join("private");
        fs::create_dir(&private).expect("create private parent");
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))
            .expect("protect private parent");
        let secret = private.join("synthetic-secret.json");
        fs::write(&secret, b"synthetic-secret-canary").expect("write synthetic secret");
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o644))
            .expect("make file mode representative");
        symlink(&secret, source(&repo, "leak.json")).expect("link schema to synthetic secret");

        let error = setup_parity_mocks(&repo, &temp).expect_err("reject schema symlink");
        assert!(error.contains("unsafe schema entry"), "{error}");
        assert!(!temp.join(".cx/schemas/leak.json").exists());
        assert_eq!(
            fs::read(&secret).expect("source unchanged"),
            b"synthetic-secret-canary"
        );
    }

    #[test]
    fn dir_link_denied() {
        let (root, repo, temp) = dirs();
        let schemas = repo.join(".cx/schemas");
        fs::remove_dir(&schemas).expect("remove empty registry");
        let outside = root.path().join("outside");
        fs::create_dir(&outside).expect("create outside registry");
        symlink(&outside, &schemas).expect("link schema directory");

        let error = setup_parity_mocks(&repo, &temp).expect_err("reject linked registry");
        assert!(error.contains("cxparity: open"), "{error}");
        assert!(!temp.join(".cx/schemas/synthetic.json").exists());
    }

    #[test]
    fn nonfile_denied() {
        let (_root, repo, temp) = dirs();
        fs::create_dir(source(&repo, "directory.json")).expect("create nonregular entry");

        let error = setup_parity_mocks(&repo, &temp).expect_err("reject directory entry");
        assert!(error.contains("not a regular file"), "{error}");
        assert!(!temp.join(".cx/schemas/directory.json").exists());
    }

    #[test]
    fn link_swap_guard() {
        let (root, repo, _unused_temp) = dirs();
        let leaf = source(&repo, "swap.json");
        let outside = root.path().join("private.json");
        let secret = b"SYNTHETIC_PRIVATE_MARKER";
        fs::write(&outside, secret).expect("write synthetic outside file");
        fs::write(&leaf, b"safe").expect("write regular source");

        let running = Arc::new(AtomicBool::new(true));
        let stop = Arc::clone(&running);
        let worker = thread::spawn(move || {
            while stop.load(Ordering::Relaxed) {
                let _ = fs::remove_file(&leaf);
                let _ = symlink(&outside, &leaf);
                thread::yield_now();
                let _ = fs::remove_file(&leaf);
                let _ = fs::write(&leaf, b"safe");
                thread::yield_now();
            }
        });
        for index in 0..40 {
            let temp = root.path().join(format!("swap-{index}"));
            fs::create_dir(&temp).expect("create parity temp");
            let _ = setup_parity_mocks(&repo, &temp);
            let copied = temp.join(".cx/schemas/swap.json");
            if copied.exists() {
                assert_ne!(fs::read(&copied).expect("read copied bytes"), secret);
            }
        }
        running.store(false, Ordering::Relaxed);
        worker.join().expect("join source swap worker");
    }
}
