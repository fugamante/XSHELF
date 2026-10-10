mod common;

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run_chunk(input: &str, budget: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cxrs"))
        .arg("chunk")
        .env("CX_CLI_NAME", "xshelf")
        .env("CX_CONTEXT_BUDGET_CHARS", budget)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn chunk");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write input");
    child.wait_with_output().expect("wait chunk")
}

#[test]
fn chunk_cli_bounds() {
    let out = run_chunk("abcdefghijklmnop\n", "8");
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).expect("stdout"),
        "----- xshelf chunk 1/3 -----\nabcdefgh\n----- xshelf chunk 2/3 -----\nijklmnop\n----- xshelf chunk 3/3 -----\n\n"
    );
}

#[test]
fn chunk_cli_zero() {
    let out = run_chunk("x\n", "0");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("context budget chars must be > 0"));
}

#[test]
fn fanout_rejects_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_cxrs"))
        .args(["task", "fanout", "synthetic objective"])
        .env("CX_CLI_NAME", "xshelf")
        .env("CX_CONTEXT_BUDGET_CHARS", "0")
        .current_dir(dir.path())
        .output()
        .expect("run fanout");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("context budget chars must be > 0"));
    assert_eq!(std::fs::read_dir(dir.path()).expect("read dir").count(), 0);
}
