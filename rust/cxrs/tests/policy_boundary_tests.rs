mod common;

use common::*;
use serde_json::json;
use std::process::Command;

fn suggestions(repo: &TempRepo, commands: &[&str]) {
    let response = json!({"analysis":"synthetic policy fixture", "commands":commands}).to_string();
    let item = json!({"type":"item.completed", "item":{"type":"agent_message", "text":response}});
    let turn = json!({"type":"turn.completed", "usage":{"input_tokens":2,"output_tokens":1}});
    repo.write_mock_primary(&format!(
        "#!/usr/bin/env bash\ncat >/dev/null\ncat <<'FIXTURE_JSON'\n{item}\n{turn}\nFIXTURE_JSON\n"
    ));
}

#[test]
fn parsed_command_policy() {
    let repo = TempRepo::new("policy-boundary");
    for command in [
        "r''m -rf ./synthetic",
        "su''do reboot",
        "/usr/bin/sudo reboot",
        "rm '-r''f' ./synthetic",
        "rm ./synthetic -f -R",
        "rm --recursive --force ./synthetic",
        "rm --rec --fo ./synthetic",
        "reboot",
        "shutdown -h now",
        "halt",
        "poweroff",
        "/sbin/reboot",
        "mkfs.ext4 /dev/synthetic",
        "newfs_apfs /dev/synthetic",
        "fdisk -l",
        "diskutil list",
        "to''uch /etc/synthetic",
        "/usr/bin/touch /etc/synthetic",
        "dd of=/dev/synthetic",
        "touch 'unfinished",
        "cp -t/etc input",
        "cp --target=/etc input",
        "mv --t=/etc input",
        "grm -rf ./synthetic",
        "gdd of=/dev/synthetic",
        "gto''uch /etc/synthetic",
        "sudo.exe reboot",
        "bash.exe -c echo",
        "systemctl reboot",
        "systemctl suspend",
        "systemctl sleep",
        "sys''temctl sleep",
        "systemctl hibernate",
        "systemctl hybrid-sleep",
        "systemctl suspend-then-hibernate",
        "sys''temctl suspend",
        "/usr/bin/systemctl.exe hibernate",
        "watch -x rm -rf ./synthetic",
        "wa''tch --exec rm -rf ./synthetic",
        "/usr/bin/watch.exe -n1 rm -rf ./synthetic",
        "launchctl reboot system",
        "cp -at/etc input",
        "cp -vat/etc input",
        "install -vt/etc input",
        "gtimeout 1 rm -rf ./synthetic",
        "gnice reboot",
        "gnohup /sbin/reboot",
        "genv reboot",
        "gstdbuf -o0 reboot",
        "gfind . -exec reboot {}",
        "launchctl asuser 501 /bin/rm -rf ./synthetic",
        "launchctl bsexec 1 /bin/rm -rf ./synthetic",
    ] {
        let out = repo.run(&["policy", "check", command]);
        assert!(out.status.success(), "{}", stderr_str(&out));
        assert!(
            stdout_str(&out).starts_with("dangerous:"),
            "{command}: {}",
            stdout_str(&out)
        );
    }
    for command in [
        "cargo test --quiet",
        "git status",
        "echo sudo rm -rf",
        "touch 'allowed file'",
        "/usr/bin/touch 'allowed file'",
        "rm -f ./file",
        "rm -- -rf",
        "dd if=input of='local image'",
        "grm -f ./file",
        "gdd if=input of='local image'",
        "systemctl status fixture",
        "launchctl list",
        "cp -- -t/etc ./dest",
        "cp -Sat/etc input ./dest",
    ] {
        let out = repo.run(&["policy", "check", command]);
        assert_eq!(
            stdout_str(&out).trim(),
            "safe",
            "{command}: {}",
            stdout_str(&out)
        );
    }
}

#[test]
fn execution_override_matrix() {
    for (run, force, unsafe_env, unsafe_cli, expected) in [
        ("0", "0", "0", false, false),
        ("1", "0", "0", false, false),
        ("1", "1", "0", false, true),
        ("1", "0", "1", false, true),
        ("1", "0", "0", true, true),
    ] {
        let repo = TempRepo::new("policy-execution");
        let sentinel = repo.root.join("mock-execution");
        for program in [
            "rm",
            "sudo",
            "reboot",
            "shutdown",
            "diskutil",
            "watch",
            "systemctl",
        ] {
            repo.write_mock(
                program,
                "#!/usr/bin/env bash\nprintf '%s\\n' \"$0\" >> \"$POLICY_EXECUTED\"\n",
            );
        }
        suggestions(
            &repo,
            &[
                "r''m -rf ./synthetic",
                "su''do reboot",
                "reboot",
                "shutdown -h now",
                "diskutil list",
                "watch -x rm -rf ./synthetic",
                "systemctl suspend",
            ],
        );
        let args = if unsafe_cli {
            vec!["fix-run", "--unsafe", "echo", "fixture"]
        } else {
            vec!["fix-run", "echo", "fixture"]
        };
        let out = repo.run_with_env(
            &args,
            &[
                ("CX_LLM_BACKEND", "primary"),
                ("CXFIX_RUN", run),
                ("CXFIX_FORCE", force),
                ("CX_UNSAFE", unsafe_env),
                ("POLICY_EXECUTED", sentinel.to_str().unwrap()),
            ],
        );
        assert!(out.status.success(), "{}", stderr_str(&out));
        assert_eq!(sentinel.exists(), expected, "{}", stderr_str(&out));
        if expected {
            assert_eq!(
                std::fs::read_to_string(sentinel).unwrap().lines().count(),
                7
            );
        } else if run == "1" {
            let rows = parse_jsonl(&repo.runs_log());
            let row = rows.last().unwrap();
            assert_eq!(row["policy_blocked"], true);
            assert!(row["policy_reason"].as_str().unwrap().contains("rm -rf"));
        }
    }
}

#[test]
fn benign_execution_preserved() {
    let repo = TempRepo::new("policy-benign");
    suggestions(&repo, &["to''uch 'allowed file'"]);
    let out = repo.run_with_env(
        &["fix-run", "echo", "fixture"],
        &[
            ("CX_LLM_BACKEND", "primary"),
            ("CXFIX_RUN", "1"),
            ("CXFIX_FORCE", "0"),
            ("CX_UNSAFE", "0"),
        ],
    );
    assert!(out.status.success(), "{}", stderr_str(&out));
    assert!(repo.root.join("allowed file").is_file());
    assert_eq!(
        parse_jsonl(&repo.runs_log()).last().unwrap()["policy_blocked"],
        false
    );
}

#[cfg(unix)]
#[test]
fn nested_write_containment() {
    let repo = TempRepo::new("policy-nested");
    let nested = repo.root.join("nested");
    std::fs::create_dir(&nested).unwrap();
    std::os::unix::fs::symlink(&repo.home, nested.join("link with spaces")).unwrap();
    let outside = repo.home.join("outside");
    std::fs::write(&outside, "preserved").unwrap();
    std::os::unix::fs::symlink(&outside, nested.join("link ")).unwrap();
    std::os::unix::fs::symlink(&outside, nested.join(" ")).unwrap();
    for command in [
        "touch 'link with spaces/out'",
        "touch 'link with spaces/missing/deeper/out'",
        "cp --target-directory='link with spaces/missing' input",
        "dd of='link with spaces/out'",
        "touch 'link '",
        "dd of='link '",
        "touch ' '",
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_cxrs"))
            .args(["policy", "check", command])
            .current_dir(&nested)
            .env("HOME", &repo.home)
            .output()
            .unwrap();
        assert!(
            stdout_str(&out).starts_with("dangerous:"),
            "{command}: {}",
            stdout_str(&out)
        );
    }
    assert!(!repo.home.join("out").exists());
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "preserved");
}

#[test]
fn copy_operand_boundary() {
    let repo = TempRepo::new("policy-copy");
    let external = repo.home.join("external-copy-input");
    std::fs::write(&external, "synthetic copy input").unwrap();
    std::fs::create_dir(repo.root.join("destination")).unwrap();
    let source = external.display();
    for command in [
        format!("cp '{source}' local-output"),
        format!("gcp '{source}' second-source local-output"),
        format!("cp -t destination '{source}'"),
        format!("cp -atdestination '{source}'"),
        format!("cp -vat destination '{source}'"),
        format!("cp --target-directory=destination '{source}'"),
        format!("cp --target destination '{source}'"),
        format!("cp --t=destination '{source}'"),
        format!("cp --suffix=ignored '{source}' local-output"),
        format!("gcp -S ignored '{source}' local-output"),
        format!("cp --sparse never '{source}' local-output"),
        format!("cp --preserve '{source}' local-output"),
        format!("cp -- '{source}' local-output"),
    ] {
        let out = repo.run(&["policy", "check", &command]);
        assert_eq!(stdout_str(&out).trim(), "safe", "{command}");
    }
    for command in [
        format!("cp local-input '{source}'"),
        format!(
            "cp -t '{}' local-input",
            external.parent().unwrap().display()
        ),
        format!("cp --parents '{source}' destination"),
        format!("mv '{source}' local-output"),
        format!("gcp -l '{source}' local-link"),
        format!("gcp --link '{source}' local-link"),
        format!("gcp --li '{source}' local-link"),
        format!("cp -Sl '{source}' local-link"),
    ] {
        let out = repo.run(&["policy", "check", &command]);
        assert!(stdout_str(&out).starts_with("dangerous:"), "{command}");
    }
    let external_dir = repo.home.join("directory-input");
    std::fs::create_dir(&external_dir).unwrap();
    for command in [
        format!("gcp -R '{}/.' destination", external_dir.display()),
        format!("gcp -R '{}' destination", external_dir.display()),
        format!("cp --archive '{}' destination", external_dir.display()),
    ] {
        let out = repo.run(&["policy", "check", &command]);
        assert!(stdout_str(&out).starts_with("dangerous:"), "{command}");
    }
    suggestions(&repo, &[&format!("cp '{source}' 'allowed copy'")]);
    let out = repo.run_with_env(
        &["fix-run", "echo", "fixture"],
        &[
            ("CX_LLM_BACKEND", "primary"),
            ("CXFIX_RUN", "1"),
            ("CXFIX_FORCE", "0"),
            ("CX_UNSAFE", "0"),
        ],
    );
    assert!(out.status.success(), "{}", stderr_str(&out));
    assert_eq!(
        std::fs::read_to_string(repo.root.join("allowed copy")).unwrap(),
        "synthetic copy input"
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            &external,
            repo.root
                .join("destination")
                .join(external.file_name().unwrap()),
        )
        .unwrap();
        let out = repo.run(&["policy", "check", &format!("cp '{source}' destination")]);
        assert!(stdout_str(&out).starts_with("dangerous:"));
    }
    std::fs::remove_file(&external).unwrap();
}
