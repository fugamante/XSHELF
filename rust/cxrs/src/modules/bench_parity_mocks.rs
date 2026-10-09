#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(unix)]
use rustix::fs::{self as unix_fs, Dir, Mode, OFlags};
#[cfg(unix)]
use std::ffi::OsStr;
#[cfg(unix)]
use std::fs::{File, OpenOptions};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[cfg(unix)]
const DIR_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
#[cfg(unix)]
const MAX_SCHEMA_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(unix)]
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

#[cfg(unix)]
fn write_executable(path: &Path, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|e| format!("cxparity: write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .map_err(|e| format!("cxparity: metadata {}: {e}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .map_err(|e| format!("cxparity: chmod {}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
pub fn setup_parity_mocks(repo: &Path, temp_repo: &Path) -> Result<PathBuf, String> {
    protect_temp_repo(temp_repo)?;
    let mock_dir = temp_repo.join(".cx").join("mockbin");
    fs::create_dir_all(&mock_dir)
        .map_err(|e| format!("cxparity: create {}: {e}", mock_dir.display()))?;
    write_parity_mock_bins(&mock_dir)?;
    copy_schema_registry(repo, temp_repo)?;
    Ok(mock_dir)
}

#[cfg(not(unix))]
pub fn setup_parity_mocks(_repo: &Path, _temp_repo: &Path) -> Result<PathBuf, String> {
    Err("cxparity: secure parity setup requires Unix directory access".to_string())
}

#[cfg(unix)]
fn protect_temp_repo(temp_repo: &Path) -> Result<(), String> {
    // The caller created this directory privately; keep a held descriptor
    // while verifying its permissions before any repository data is copied.
    let dir = unix_fs::open(temp_repo, DIR_FLAGS, Mode::empty())
        .map_err(|e| format!("cxparity: open {}: {e}", temp_repo.display()))?;
    unix_fs::fchmod(&dir, Mode::from_raw_mode(0o700))
        .map_err(|e| format!("cxparity: chmod {}: {e}", temp_repo.display()))?;
    Ok(())
}

#[cfg(unix)]
fn write_parity_mock_bins(mock_dir: &Path) -> Result<(), String> {
    let primary = r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" != "exec" ]]; then exit 2; fi
if [[ "${2:-}" == "--json" && "${3:-}" == "-" ]]; then
  prompt="$(cat)"
  if [[ "$prompt" == *"Generate a commit object from this STAGED diff."* ]]; then
    cat <<'JSON'
{"type":"item.completed","item":{"type":"agent_message","text":"{\"subject\":\"feat: parity commit\",\"body\":[\"align rust and bash parity\"],\"breaking\":false,\"scope\":null,\"tests\":[\"cargo test -q\"]}"}}
{"type":"turn.completed","usage":{"input_tokens":64,"cached_input_tokens":8,"output_tokens":12}}
JSON
  elif [[ "$prompt" == *"Write a PR-ready summary of this diff."* ]]; then
    cat <<'JSON'
{"type":"item.completed","item":{"type":"agent_message","text":"{\"title\":\"Parity test diff summary\",\"summary\":[\"staged file prepared for parity\"],\"risk_edge_cases\":[\"none identified\"],\"suggested_tests\":[\"cargo test -q\"]}"}}
{"type":"turn.completed","usage":{"input_tokens":64,"cached_input_tokens":8,"output_tokens":12}}
JSON
  elif [[ "$prompt" == *"Based on the terminal command output below, propose the NEXT shell commands to run."* ]]; then
    cat <<'JSON'
{"type":"item.completed","item":{"type":"agent_message","text":"{\"commands\":[\"git status --short\",\"cargo test -q\"]}"}}
{"type":"turn.completed","usage":{"input_tokens":64,"cached_input_tokens":8,"output_tokens":12}}
JSON
  else
    cat <<'JSON'
{"type":"item.completed","item":{"type":"agent_message","text":"parity ok"}}
{"type":"turn.completed","usage":{"input_tokens":64,"cached_input_tokens":8,"output_tokens":12}}
JSON
  fi
  exit 0
fi
if [[ "${2:-}" == "-" ]]; then
  cat >/dev/null
  printf '%s\n' "parity plain output"
  exit 0
fi
exit 2
"#;
    let pbcopy = r#"#!/usr/bin/env bash
cat >/dev/null
exit 0
"#;
    write_executable(&mock_dir.join(concat!("co", "dex")), primary)?;
    write_executable(&mock_dir.join("pbcopy"), pbcopy)
}

#[cfg(unix)]
pub(crate) fn copy_limited<R: Read>(
    source: &mut R,
    target: &Path,
    limit: u64,
) -> Result<u64, String> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(target)
        .map_err(|e| format!("cxparity: create {}: {e}", target.display()))?;
    let result = io::copy(&mut source.take(limit + 1), &mut output);
    drop(output);
    match result {
        Ok(bytes) if bytes <= limit => Ok(bytes),
        Ok(_) => {
            fs::remove_file(target)
                .map_err(|e| format!("cxparity: remove partial {}: {e}", target.display()))?;
            Err(format!(
                "cxparity: schema copy exceeds byte limit: {}",
                target.display()
            ))
        }
        Err(error) => {
            fs::remove_file(target)
                .map_err(|e| format!("cxparity: remove partial {}: {e}", target.display()))?;
            Err(format!("cxparity: copy {}: {error}", target.display()))
        }
    }
}

#[cfg(unix)]
fn copy_schema_registry(repo: &Path, temp_repo: &Path) -> Result<(), String> {
    let src_schema = repo.join(".cx").join("schemas");
    let dst_schema = temp_repo.join(".cx").join("schemas");
    fs::create_dir_all(&dst_schema)
        .map_err(|e| format!("cxparity: create {}: {e}", dst_schema.display()))?;
    // Hold each directory and file descriptor. Pathname checks followed by
    // fs::copy would allow a symlink swap between validation and reading.
    let root =
        fs::canonicalize(repo).map_err(|e| format!("cxparity: resolve {}: {e}", repo.display()))?;
    let root_dir = unix_fs::open(&root, DIR_FLAGS, Mode::empty())
        .map_err(|e| format!("cxparity: open {}: {e}", root.display()))?;
    let cx_dir = unix_fs::openat(&root_dir, ".cx", DIR_FLAGS, Mode::empty())
        .map_err(|e| format!("cxparity: open {}: {e}", repo.join(".cx").display()))?;
    let schema_dir = unix_fs::openat(&cx_dir, "schemas", DIR_FLAGS, Mode::empty())
        .map_err(|e| format!("cxparity: open {}: {e}", src_schema.display()))?;
    let entries = Dir::read_from(&schema_dir)
        .map_err(|e| format!("cxparity: read {}: {e}", src_schema.display()))?;
    let mut total = 0u64;
    for ent in entries {
        let ent = ent.map_err(|e| format!("cxparity: schema entry error: {e}"))?;
        let fname = OsStr::from_bytes(ent.file_name().to_bytes());
        if Path::new(fname).extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let path = src_schema.join(fname);
        let mut source = File::from(
            unix_fs::openat(
                &schema_dir,
                ent.file_name(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                Mode::empty(),
            )
            .map_err(|e| format!("cxparity: unsafe schema entry {}: {e}", path.display()))?,
        );
        let metadata = source
            .metadata()
            .map_err(|e| format!("cxparity: metadata {}: {e}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "cxparity: unsafe schema entry {}: not a regular file",
                path.display()
            ));
        }
        // Both the preflight size and streamed copy are bounded: a file
        // can grow after fstat, and a sparse file may claim huge length.
        let remaining = MAX_TOTAL_BYTES.saturating_sub(total);
        let limit = MAX_SCHEMA_BYTES.min(remaining);
        if metadata.len() > limit {
            return Err(format!(
                "cxparity: schema copy exceeds byte limit: {}",
                path.display()
            ));
        }
        let target = dst_schema.join(fname);
        total += copy_limited(&mut source, &target, limit)?;
    }
    Ok(())
}

pub fn with_parity_env(cmd: &mut Command, mock_dir: &Path, temp_repo: &Path) {
    let path = std::env::var("PATH").unwrap_or_default();
    let prefixed = format!("{}:{}", mock_dir.display(), path);
    cmd.current_dir(temp_repo)
        .env("PATH", prefixed)
        .env("CX_CAPTURE_PROVIDER", "native")
        .env("CX_NATIVE_REDUCE", "0");
}
