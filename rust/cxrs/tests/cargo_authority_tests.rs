mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

fn checkout() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("checkout root")
}

#[test]
fn cx_cargo_cwd() {
    let repo = checkout();
    let temp = tempfile::tempdir().expect("tempdir");
    let caller = temp.path().join("caller");
    let mock_bin = temp.path().join("bin");
    fs::create_dir_all(caller.join(".cargo")).expect("cargo config dir");
    fs::create_dir_all(&mock_bin).expect("mock bin dir");
    fs::write(
        caller.join(".cargo/config.toml"),
        "[build]\nrustc-wrapper = '/invalid/synthetic'\n",
    )
    .expect("caller cargo config");
    let cwd_record = temp.path().join("build-cwd");
    let cargo = mock_bin.join("cargo");
    let target_record = temp.path().join("target-dir");
    fs::write(
        &cargo,
        format!(
            "#!/bin/sh\npwd > '{}'\nprintf '%s' \"$CARGO_TARGET_DIR\" > '{}'\nexit 77\n",
            cwd_record.display(),
            target_record.display()
        ),
    )
    .expect("write mock cargo");
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).expect("chmod cargo");
    let path = format!(
        "{}:{}",
        mock_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(repo.join("bin/cx"))
        .arg("version")
        .current_dir(&caller)
        .env("PATH", path)
        .env("CARGO_TARGET_DIR", "fresh-target")
        .output()
        .expect("run bin/cx from untrusted cwd");
    assert!(!out.status.success());
    assert_eq!(
        fs::read_to_string(cwd_record).expect("cargo ran").trim(),
        "/".to_string()
    );
    assert_eq!(
        fs::read_to_string(target_record).expect("target recorded"),
        caller
            .canonicalize()
            .expect("canonical caller")
            .join("fresh-target")
            .display()
            .to_string()
    );
}

#[test]
fn cargo_parent_config() {
    let root = checkout();
    let temp = tempfile::tempdir().expect("tempdir");
    let parent = temp.path().join("untrusted");
    let nested = parent.join("xshelf");
    let caller = temp.path().join("caller");
    fs::create_dir_all(parent.join(".cargo")).expect("parent config dir");
    fs::create_dir_all(nested.join("bin")).expect("nested bin");
    fs::create_dir_all(nested.join("rust/cxrs/src")).expect("nested crate");
    fs::create_dir_all(&caller).expect("caller dir");
    let wrapper = nested.join("bin/cx");
    fs::copy(root.join("bin/cx"), &wrapper).expect("copy entrypoint");
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).expect("chmod entrypoint");
    fs::write(
        nested.join("rust/cxrs/Cargo.toml"),
        "[package]\nname = \"cxrs\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("write manifest");
    fs::write(
        nested.join("rust/cxrs/src/main.rs"),
        "fn main() { if std::env::args().nth(1).as_deref() != Some(\"supports\") { println!(\"{}\", std::env::current_dir().unwrap().display()); } }\n",
    )
    .expect("write source");
    let marker = temp.path().join("ancestor-hook-ran");
    let hook = parent.join("hook");
    fs::write(
        &hook,
        format!(
            "#!/bin/sh\nprintf invoked > '{}'\nexec \"$@\"\n",
            marker.display()
        ),
    )
    .expect("write hook");
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).expect("chmod hook");
    fs::write(
        parent.join(".cargo/config.toml"),
        format!("[build]\nrustc-wrapper = '{}'\n", hook.display()),
    )
    .expect("write parent config");

    let out = Command::new(&wrapper)
        .arg("version")
        .current_dir(&caller)
        .env("CARGO_TARGET_DIR", temp.path().join("target"))
        .output()
        .expect("run nested entrypoint");
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!marker.exists(), "ancestor Cargo hook executed");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        caller
            .canonicalize()
            .expect("canonical caller")
            .display()
            .to_string()
    );
}
