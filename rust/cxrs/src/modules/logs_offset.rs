use super::logs_read::{MAX_SCAN_BYTES, open_run_file, read_capped_line};
use serde_json::Value;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;

// Offset-based callers need the newest appended row, not the whole prior log.
pub fn latest_value_since(
    log_file: &Path,
    offset: u64,
    required_field: Option<&str>,
) -> Result<Option<Value>, String> {
    let file = open_run_file(log_file).map_err(|e| format!("cannot open run log: {e}"))?;
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|e| format!("cannot seek run log: {e}"))?;
    let mut scanned = 0u64;
    let mut latest = None;
    while let Some(line) =
        read_capped_line(&mut reader).map_err(|e| format!("cannot read run log: {e}"))?
    {
        scanned = scanned.saturating_add(line.len() as u64 + 1);
        if scanned > MAX_SCAN_BYTES {
            return Err(format!("run log scan exceeds {MAX_SCAN_BYTES} bytes"));
        }
        if let Ok(row) = serde_json::from_slice::<Value>(&line)
            && required_field.is_none_or(|field| {
                row.get(field)
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
            })
        {
            latest = Some(row);
        }
    }
    Ok(latest)
}
