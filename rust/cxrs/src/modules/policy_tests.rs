use super::*;

#[test]
fn blocks_rm_rf() {
    let root = Path::new("/tmp/repo");
    let decision = evaluate_command_safety("rm -rf ./target", root);
    assert!(matches!(decision, SafetyDecision::Dangerous(_)));
}

#[test]
fn blocks_rm_flags() {
    let root = Path::new("/tmp/repo");
    let decision = evaluate_command_safety("rm -r -f ./target", root);
    assert!(matches!(decision, SafetyDecision::Dangerous(_)));
}

#[test]
fn blocks_shell_exec() {
    let root = Path::new("/tmp/repo");
    let result = safe_command_argv("bash -c 'rm -rf /tmp/victim'", root);
    assert!(result.is_err());
}

#[test]
fn blocks_inline_code() {
    let root = Path::new("/tmp/repo");
    let result = safe_command_argv(
        "python3 -c 'import shutil; shutil.rmtree(\"/tmp/victim\")'",
        root,
    );
    assert!(result.is_err());
}

#[test]
fn blocks_attached_code() {
    let root = Path::new("/tmp/repo");
    for command in [
        "python3 -c'print(1)'",
        "python3.12 -c'print(1)'",
        "bash -cecho",
    ] {
        assert!(safe_command_argv(command, root).is_err(), "{command}");
    }
}

#[test]
fn blocks_launcher() {
    let root = Path::new("/tmp/repo");
    for command in [
        "env bash -c 'rm --recursive --force /tmp/victim'",
        "/usr/bin/env bash -c 'echo hi'",
        "busybox sh -c 'echo hi'",
    ] {
        assert!(safe_command_argv(command, root).is_err(), "{command}");
    }
}

#[test]
fn blocks_env_prefix() {
    let root = Path::new("/tmp/repo");
    assert!(safe_command_argv("RUST_BACKTRACE=1 cargo test", root).is_err());
}

#[test]
fn blocks_shell_syntax() {
    let root = Path::new("/tmp/repo");
    let result = safe_command_argv("echo hi; rm -rf /tmp/victim", root);
    assert!(result.is_err());
}

#[test]
fn parses_simple_argv() {
    let root = Path::new("/tmp/repo");
    let argv = safe_command_argv("cargo test --quiet", root).unwrap();
    assert_eq!(argv, vec!["cargo", "test", "--quiet"]);
}

#[test]
fn allows_repo_redirect() {
    let root = env::current_dir().unwrap();
    let command = format!("echo hi > '{}'", root.join("out.txt").display());
    let decision = evaluate_command_safety(&command, &root);
    assert!(matches!(decision, SafetyDecision::Safe));
}

#[test]
fn blocks_external_write() {
    let root = Path::new("/tmp/repo");
    let decision = evaluate_command_safety("echo hi > /etc/out.txt", root);
    assert!(matches!(decision, SafetyDecision::Dangerous(_)));
}

#[test]
fn blocks_external_chmod() {
    let root = Path::new("/tmp/repo");
    let blocked = evaluate_command_safety("chmod 755 /usr/bin/tool", root);
    assert!(matches!(blocked, SafetyDecision::Dangerous(_)));
    let not_protected_rule = evaluate_command_safety("chmod 755 /usr/local/bin/tool", root);
    assert!(matches!(not_protected_rule, SafetyDecision::Dangerous(_)));
}

#[test]
fn allows_repo_write() {
    let root = env::current_dir().unwrap();
    let command = format!("touch '{}'", root.join("output.txt").display());
    let decision = evaluate_command_safety(&command, &root);
    assert!(matches!(decision, SafetyDecision::Safe));
}

#[cfg(unix)]
#[test]
fn blocks_symlink_write() {
    use std::os::unix::fs::symlink;
    let base = std::env::temp_dir().join(format!(
        "cx-policy-test-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    let repo = base.join("repo");
    let outside = base.join("outside");
    let _ = fs::create_dir_all(&repo);
    let _ = fs::create_dir_all(&outside);
    let link = repo.join("link");
    let _ = symlink(&outside, &link);
    let cmd = format!("echo hi > {}/escape.txt", link.display());
    let decision = evaluate_command_safety(&cmd, &repo);
    let _ = fs::remove_dir_all(&base);
    assert!(matches!(decision, SafetyDecision::Dangerous(_)));
}
