use crate::error::{CxError, CxResult};
use crate::paths::ensure_parent_dir;
use crate::types::ExecutionLog;
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

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
    append_jsonl_cx(path, value).map_err(|e| e.to_string())
}

fn append_jsonl_cx(path: &Path, value: &Value) -> CxResult<()> {
    ensure_parent_dir(path).map_err(CxError::invalid)?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| CxError::io(format!("failed opening {}", path.display()), e))?;
    let mut line =
        serde_json::to_string(value).map_err(|e| CxError::json("log json serialize", e))?;
    line.push('\n');
    f.write_all(line.as_bytes())
        .map_err(|e| CxError::io(format!("failed writing {}", path.display()), e))
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
