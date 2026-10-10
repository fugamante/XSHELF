use super::{MAX_ROW_BYTES, RecentLines, check_result_budget};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub fn load_values(log_file: &Path, limit: usize) -> Result<Vec<Value>, String> {
    load_values_where(log_file, limit, |_| true)
}

pub fn load_values_where<F: Fn(&Value) -> bool>(
    log_file: &Path,
    limit: usize,
    accept: F,
) -> Result<Vec<Value>, String> {
    collect_values(RecentLines::open(log_file)?, limit, accept)
}

pub(crate) fn load_values_file(file: File, limit: usize) -> Result<Vec<Value>, String> {
    let reader =
        RecentLines::from_file(file).map_err(|e| format!("cannot inspect task events: {e}"))?;
    collect_values(reader, limit, |_| true)
}

pub(crate) fn load_follow_values(file: File, limit: usize) -> Result<(Vec<Value>, u64), String> {
    let mut reader =
        RecentLines::from_file(file).map_err(|e| format!("cannot inspect task events: {e}"))?;
    let end = reader.remaining;
    let complete = completed_end(&mut reader.file, end)?;
    reader.remaining = complete;
    collect_values(reader, limit, |_| true).map(|rows| (rows, complete))
}

fn completed_end(file: &mut File, end: u64) -> Result<u64, String> {
    let mut pos = end;
    let mut scanned = 0usize;
    let mut chunk = [0u8; 8192];
    while pos > 0 {
        let len = pos.min(chunk.len() as u64) as usize;
        pos -= len as u64;
        file.seek(SeekFrom::Start(pos))
            .and_then(|_| file.read_exact(&mut chunk[..len]))
            .map_err(|e| format!("cannot inspect task event row: {e}"))?;
        if let Some(index) = chunk[..len].iter().rposition(|byte| *byte == b'\n') {
            if end.saturating_sub(pos + index as u64 + 1) > MAX_ROW_BYTES as u64 {
                return Err(format!("task event row exceeds {MAX_ROW_BYTES} bytes"));
            }
            return Ok(pos + index as u64 + 1);
        }
        scanned += len;
        if scanned > MAX_ROW_BYTES {
            return Err(format!("task event row exceeds {MAX_ROW_BYTES} bytes"));
        }
    }
    Ok(0)
}

fn collect_values<F: Fn(&Value) -> bool>(
    mut reader: RecentLines,
    limit: usize,
    accept: F,
) -> Result<Vec<Value>, String> {
    let mut out: Vec<Value> = Vec::new();
    let mut bytes = 0usize;
    let mut seen = 0usize;
    while let Some(line) = reader.next_line()? {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if let Ok(v) = serde_json::from_slice::<Value>(&line) {
            seen += 1;
            if accept(&v) {
                bytes = bytes.saturating_add(line.len());
                out.push(v);
                check_result_budget(out.len(), bytes)?;
            }
            if limit > 0 && seen >= limit {
                break;
            }
        }
    }
    out.reverse();
    Ok(out)
}
