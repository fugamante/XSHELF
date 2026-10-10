use crate::error::{CxError, CxResult};
use crate::paths::ensure_parent_dir;
use crate::types::ExecutionLog;
use serde_json::Value;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

const LOCK_WAIT: Duration = Duration::from_secs(10);

#[path = "logs_cmd.rs"]
mod logs_cmd;
#[cfg(unix)]
#[path = "logs_fs.rs"]
mod logs_fs;
#[path = "logs_migrate.rs"]
mod logs_migrate;
#[path = "logs_offset.rs"]
mod logs_offset;
#[path = "logs_read.rs"]
mod logs_read;

pub use logs_cmd::cmd_logs;
pub use logs_migrate::{migrate_runs_jsonl, migrate_transaction};
pub use logs_offset::latest_value_since;
pub use logs_read::{
    file_len, find_execution_row, find_field_value, load_runs, load_runs_appended, load_values,
    load_values_where, tail_log_lines, validate_runs_jsonl_file,
};
pub(crate) use logs_read::{load_follow_values, load_values_file, read_capped_line_with_end};

pub const MAX_RUN_LOG_ROW_BYTES: usize = logs_read::MAX_ROW_BYTES;

pub fn validate_execution_log_row(row: &ExecutionLog) -> Result<(), String> {
    if row.execution_id.trim().is_empty() {
        return Err("execution log missing execution_id".to_string());
    }
    if row.timestamp.trim().is_empty() {
        return Err("execution log missing timestamp".to_string());
    }
    if row.command.trim().is_empty() {
        return Err("execution log missing command".to_string());
    }
    if row.backend_used.trim().is_empty() {
        return Err("execution log missing backend_used".to_string());
    }
    if row.execution_mode.trim().is_empty() {
        return Err("execution log missing execution_mode".to_string());
    }
    if row.schema_enforced && !row.schema_ok {
        if row
            .schema_reason
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            return Err("schema failure missing schema_reason".to_string());
        }
        if row
            .quarantine_id
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            return Err("schema failure missing quarantine_id".to_string());
        }
    }
    Ok(())
}

pub fn append_jsonl(path: &Path, value: &Value) -> Result<(), String> {
    append_jsonl_cx(path, value, false).map_err(|e| e.to_string())
}

// Only the run log accepts a path explicitly selected through CX_LOG_FILE.
pub fn append_run_jsonl(path: &Path, value: &Value) -> Result<(), String> {
    let explicit = std::env::var_os("CX_LOG_FILE").as_deref() == Some(path.as_os_str());
    append_jsonl_cx(path, value, explicit).map_err(|e| e.to_string())
}

pub(crate) fn open_repo_file(path: &Path) -> std::io::Result<File> {
    #[cfg(unix)]
    {
        logs_fs::AnchoredPath::open(path, false)?.source()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "safe repository log reads require Unix directory descriptors",
        ))
    }
}

fn append_jsonl_cx(path: &Path, value: &Value, explicit: bool) -> CxResult<()> {
    #[cfg(unix)]
    let f = if explicit {
        use rustix::fs::{self, Mode, OFlags};
        ensure_parent_dir(path).map_err(CxError::invalid)?;
        let file = File::from(
            fs::open(
                path,
                OFlags::WRONLY
                    | OFlags::APPEND
                    | OFlags::CREATE
                    | OFlags::NONBLOCK
                    | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )
            .map_err(|e| CxError::io(format!("failed opening {}", path.display()), e.into()))?,
        );
        if !file
            .metadata()
            .map_err(|e| CxError::io("inspect log destination", e))?
            .is_file()
        {
            return Err(CxError::invalid("log destination is not a regular file"));
        }
        file
    } else {
        logs_fs::AnchoredPath::open(path, true)
            .and_then(|target| target.append_regular())
            .map_err(|e| CxError::io(format!("failed opening {}", path.display()), e))?
    };
    #[cfg(not(unix))]
    let f = {
        if !explicit {
            return Err(CxError::invalid(
                "safe repository log writes require Unix directory descriptors",
            ));
        }
        ensure_parent_dir(path).map_err(CxError::invalid)?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| CxError::io(format!("failed opening {}", path.display()), e))?
    };
    let mut line =
        serde_json::to_string(value).map_err(|e| CxError::json("log json serialize", e))?;
    line.push('\n');
    append_record(f, path, line.as_bytes(), Write::write)
}

// Lock the opened inode, not a pathname: explicit CX_LOG_FILE aliases must share
// the same record boundary. Dropping file releases the lock on every return.
fn append_record<F>(mut file: File, path: &Path, line: &[u8], mut write: F) -> CxResult<()>
where
    F: FnMut(&mut File, &[u8]) -> io::Result<usize>,
{
    lock_record(&file, path, LOCK_WAIT)?;
    let before = file
        .metadata()
        .map_err(|e| CxError::io("inspect log destination", e))?
        .len();
    let mut written = 0;
    while written < line.len() {
        let remaining = &line[written..];
        match write(&mut file, remaining) {
            Ok(0) => {
                let error = io::Error::new(io::ErrorKind::WriteZero, "failed to write log row");
                return Err(append_error(&file, path, before, written, error));
            }
            Ok(count) if count <= remaining.len() => written += count,
            Ok(_) => {
                let error = io::Error::new(io::ErrorKind::InvalidData, "invalid write count");
                return Err(append_error(&file, path, before, written, error));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(append_error(&file, path, before, written, error)),
        }
    }
    Ok(())
}

fn lock_record(file: &File, path: &Path, timeout: Duration) -> CxResult<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match fs2::FileExt::try_lock_exclusive(file) {
            Ok(()) => return Ok(()),
            Err(error)
                if error.kind() == io::ErrorKind::Interrupted
                    || error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
            {
                if Instant::now() >= deadline {
                    return Err(CxError::io(
                        format!("timed out locking {}", path.display()),
                        error,
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                return Err(CxError::io(
                    format!("failed locking {}", path.display()),
                    error,
                ));
            }
        }
    }
}

fn append_error(
    file: &File,
    path: &Path,
    before: u64,
    written: usize,
    error: io::Error,
) -> CxError {
    let mut context = format!("failed writing {}", path.display());
    if written > 0 {
        let expected = u64::try_from(written)
            .ok()
            .and_then(|count| before.checked_add(count));
        match (file.metadata(), expected) {
            (Ok(metadata), Some(end)) if metadata.len() == end => {
                if let Err(cleanup) = file.set_len(before) {
                    context.push_str(&format!("; partial row cleanup failed: {cleanup}"));
                }
            }
            (Ok(_), _) => context.push_str("; partial row retained after concurrent file change"),
            (Err(inspect), _) => {
                context.push_str(&format!("; partial row may remain: {inspect}"));
            }
        }
    }
    CxError::io(context, error)
}

#[cfg(test)]
mod append_record_tests {
    use super::{append_record, lock_record};
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, Write};
    use std::path::Path;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn open(path: &Path) -> File {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("owned log")
    }

    #[test]
    fn lock_timeout() {
        let dir = tempfile::tempdir().expect("owned directory");
        let path = dir.path().join("runs.jsonl");
        let first = open(&path);
        fs2::FileExt::lock_exclusive(&first).expect("hold first lock");
        let second = open(&path);
        let error = lock_record(&second, &path, Duration::from_millis(50))
            .expect_err("lock must be bounded");
        assert!(error.to_string().contains("timed out locking"));
        fs2::FileExt::unlock(&first).expect("release first lock");
        lock_record(&second, &path, Duration::from_secs(1)).expect("reuse after lock release");
        fs2::FileExt::unlock(&second).expect("release second lock");
    }

    #[test]
    fn short_writes() {
        let dir = tempfile::tempdir().expect("owned directory");
        let path = dir.path().join("runs.jsonl");
        let row = b"{\"execution_id\":\"synthetic\"}\n";
        append_record(open(&path), &path, row, |file, remaining| {
            file.write(&remaining[..remaining.len().min(3)])
        })
        .expect("complete short writes");
        assert_eq!(fs::read(&path).expect("read log"), row);
    }

    #[test]
    fn malformed_history() {
        let dir = tempfile::tempdir().expect("owned directory");
        let path = dir.path().join("runs.jsonl");
        fs::write(&path, b"not-json\n").expect("existing malformed row");
        let row = b"{\"execution_id\":\"synthetic\"}\n";
        append_record(open(&path), &path, row, Write::write).expect("append after malformed row");
        assert_eq!(
            fs::read(&path).expect("read log"),
            [b"not-json\n".as_slice(), row.as_slice()].concat()
        );
    }

    #[test]
    fn error_cleanup() {
        let dir = tempfile::tempdir().expect("owned directory");
        let path = dir.path().join("runs.jsonl");
        let old = b"{\"execution_id\":\"old\"}\n";
        fs::write(&path, old).expect("existing row");
        let row = b"{\"execution_id\":\"next\"}\n";
        let mut first = true;
        let error = append_record(open(&path), &path, row, |file, remaining| {
            if first {
                first = false;
                file.write(&remaining[..5])
            } else {
                Err(io::Error::other("synthetic EIO"))
            }
        })
        .expect_err("injected failure");
        assert!(error.to_string().contains("synthetic EIO"));
        assert_eq!(fs::read(&path).expect("read log"), old);
        append_record(open(&path), &path, row, Write::write).expect("reuse after error");
        assert_eq!(
            fs::read(&path).expect("read log"),
            [old.as_slice(), row.as_slice()].concat()
        );
    }

    #[test]
    fn foreign_append() {
        let dir = tempfile::tempdir().expect("owned directory");
        let path = dir.path().join("runs.jsonl");
        let mut first = true;
        let error = append_record(open(&path), &path, b"{\"id\":1}\n", |file, remaining| {
            if first {
                first = false;
                file.write(&remaining[..3])
            } else {
                open(&path).write_all(b"foreign\n")?;
                Err(io::Error::other("synthetic EIO"))
            }
        })
        .expect_err("injected failure");
        assert!(error.to_string().contains("concurrent file change"));
        assert_eq!(fs::read(&path).expect("read log"), b"{\"iforeign\n");
    }

    #[test]
    fn alias_lock() {
        let dir = tempfile::tempdir().expect("owned directory");
        let path = dir.path().join("runs.jsonl");
        let alias = dir.path().join("alias.jsonl");
        fs::write(&path, b"").expect("create log");
        fs::hard_link(&path, &alias).expect("alias same inode");
        let (first_tx, first_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (attempt_tx, attempt_rx) = mpsc::channel();
        let (second_tx, second_rx) = mpsc::channel();
        let a_path = path.clone();
        let first = thread::spawn(move || {
            let mut split = true;
            append_record(
                open(&a_path),
                &a_path,
                b"{\"id\":\"first\"}\n",
                |file, remaining| {
                    if split {
                        split = false;
                        let count = file.write(&remaining[..4])?;
                        first_tx.send(()).expect("signal first write");
                        release_rx.recv().expect("release first writer");
                        Ok(count)
                    } else {
                        file.write(remaining)
                    }
                },
            )
        });
        first_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("first writer entered");
        let b_path = alias.clone();
        let second = thread::spawn(move || {
            attempt_tx.send(()).expect("signal lock attempt");
            append_record(
                open(&b_path),
                &b_path,
                b"{\"id\":\"second\"}\n",
                |file, bytes| {
                    second_tx.send(()).expect("signal second writer");
                    file.write(bytes)
                },
            )
        });
        attempt_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("second writer started");
        let blocked = second_rx.recv_timeout(Duration::from_millis(200)).is_err();
        release_tx.send(()).expect("release first writer");
        first.join().expect("first thread").expect("first row");
        second.join().expect("second thread").expect("second row");
        assert!(
            blocked,
            "second alias wrote while the first row was incomplete"
        );
        let data = fs::read_to_string(&path).expect("read log");
        let rows: Vec<_> = data
            .lines()
            .map(serde_json::from_str::<serde_json::Value>)
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].as_ref().expect("first JSON")["id"], "first");
        assert_eq!(rows[1].as_ref().expect("second JSON")["id"], "second");
    }
}

#[cfg(test)]
mod bounded_reader_tests {
    use super::{find_execution_row, find_field_value, latest_value_since, load_values};
    use std::io::{Seek, SeekFrom, Write};

    #[test]
    fn offsets_preserve_selection() {
        let dir = tempfile::tempdir().expect("owned log dir");
        let path = dir.path().join("runs.jsonl");
        let mut file = std::fs::File::create(&path).expect("create log");
        file.write_all(b"{\"execution_id\":\"old\"}\nnot json\n")
            .expect("old rows");
        let boundary = file.stream_position().expect("cursor");
        file.write_all(b"{\"execution_id\":\"new\"}\n{\"execution_id\":\"last\"}")
            .expect("new rows");
        drop(file);

        assert_eq!(
            latest_value_since(&path, boundary, None).unwrap().unwrap()["execution_id"],
            "last"
        );
        assert_eq!(
            latest_value_since(&path, boundary + 8, None)
                .unwrap()
                .unwrap()["execution_id"],
            "last"
        );
        let beyond = std::fs::metadata(&path).expect("log length").len() + 1;
        assert!(latest_value_since(&path, beyond, None).unwrap().is_none());
        assert_eq!(
            find_field_value(&path, "execution_id", "old")
                .unwrap()
                .unwrap()["execution_id"],
            "old"
        );
        let values = load_values(&path, 2).expect("recent values");
        assert_eq!(values[0]["execution_id"], "new");
        assert_eq!(values[1]["execution_id"], "last");

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("reopen");
        file.seek(SeekFrom::Start(0)).expect("seek");
        file.set_len(0).expect("truncate");
        assert!(latest_value_since(&path, boundary, None).unwrap().is_none());
    }

    #[test]
    fn execution_lookup_cap() {
        let dir = tempfile::tempdir().expect("owned log dir");
        let path = dir.path().join("runs.jsonl");
        let mut file = std::fs::File::create(&path).expect("create log");
        file.write_all(b"{\"execution_id\":\"boundary\"}\n")
            .expect("target row");
        let chunk = vec![b'\n'; 1024 * 1024];
        for _ in 0..16 {
            file.write_all(&chunk).expect("fill recent history");
        }
        drop(file);
        assert!(find_execution_row(&path, "boundary").unwrap().is_some());
        assert!(
            find_execution_row(&path, "missing")
                .unwrap_err()
                .contains("scan exceeds")
        );
    }
}
