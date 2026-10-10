use crate::config::cli_app_name;
use crate::error::{CxError, CxResult};
use crate::log_contract::REQUIRED_STRICT_FIELDS;
use crate::quarantine::read_quarantine_record;
use crate::types::RunEntry;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[path = "logs_read_values.rs"]
mod logs_read_values;
pub(crate) use logs_read_values::{load_follow_values, load_values_file};
pub use logs_read_values::{load_values, load_values_where};

static RUNS_PARSE_WARNED: AtomicBool = AtomicBool::new(false);
pub(super) const MAX_ROW_BYTES: usize = 1024 * 1024;
pub(super) const MAX_SCAN_BYTES: u64 = 128 * 1024 * 1024;
const MAX_RESULT_BYTES: usize = 4 * 1024 * 1024;
const MAX_RESULT_ROWS: usize = 50_000;
const MAX_VALIDATION_ISSUES: usize = 10_000;
const REVERSE_CHUNK: usize = 8192;
const REQUIRED_LEGACY_ANY_OF: [(&str, &str); 3] = [
    ("ts", "timestamp"),
    ("tool", "command"),
    ("repo_root", "repo_root"),
];

// Repository-selected logs must not redirect reads through symlinks. An
// explicit CX_LOG_FILE remains an operator-selected path, including aliases.
pub fn open_run_file(path: &Path) -> std::io::Result<File> {
    #[cfg(unix)]
    let file = if std::env::var_os("CX_LOG_FILE").as_deref() == Some(path.as_os_str()) {
        use rustix::fs::{self, Mode, OFlags};
        File::from(
            fs::open(
                path,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
                Mode::empty(),
            )
            .map_err(std::io::Error::from)?,
        )
    } else {
        super::logs_fs::AnchoredPath::open(path, false)?.source()?
    };
    #[cfg(not(unix))]
    let file = if std::env::var_os("CX_LOG_FILE").as_deref() == Some(path.as_os_str()) {
        File::open(path)?
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "safe repository log reads require Unix directory descriptors",
        ));
    };
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "run log is not a regular file",
        ));
    }
    Ok(file)
}
struct RecentLines {
    file: File,
    remaining: u64,
    scanned: u64,
    chunk: [u8; REVERSE_CHUNK],
    index: usize,
    pending: Vec<u8>,
    lines_from_end: usize,
    done: bool,
}

impl RecentLines {
    fn open(path: &Path) -> Result<Self, String> {
        let file =
            open_run_file(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        Self::from_file(file).map_err(|e| format!("cannot inspect {}: {e}", path.display()))
    }

    fn from_file(file: File) -> std::io::Result<Self> {
        let remaining = file.metadata()?.len();
        Ok(Self {
            file,
            remaining,
            scanned: 0,
            chunk: [0; REVERSE_CHUNK],
            index: 0,
            pending: Vec::new(),
            lines_from_end: 0,
            done: false,
        })
    }

    fn next_line(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.done {
            return Ok(None);
        }
        loop {
            if self.index == 0 {
                if self.remaining == 0 {
                    self.done = true;
                    if self.pending.is_empty() {
                        return Ok(None);
                    }
                    self.pending.reverse();
                    return Ok(Some(std::mem::take(&mut self.pending)));
                }
                let len = self.remaining.min(REVERSE_CHUNK as u64) as usize;
                self.remaining -= len as u64;
                self.file
                    .seek(SeekFrom::Start(self.remaining))
                    .and_then(|_| self.file.read_exact(&mut self.chunk[..len]))
                    .map_err(|e| format!("cannot scan run log: {e}"))?;
                self.index = len;
            }
            self.index -= 1;
            self.scanned += 1;
            if self.scanned > MAX_SCAN_BYTES {
                return Err(format!(
                    "run log scan exceeds {MAX_SCAN_BYTES} bytes; narrow the requested window"
                ));
            }
            if self.chunk[self.index] == b'\n' {
                // A terminal delimiter is not an empty row at the end.
                if self.lines_from_end == 0 && self.pending.is_empty() {
                    continue;
                }
                self.lines_from_end += 1;
                self.pending.reverse();
                return Ok(Some(std::mem::take(&mut self.pending)));
            }
            if self.pending.len() == MAX_ROW_BYTES {
                return Err(format!(
                    "run log row {} from end exceeds {MAX_ROW_BYTES} bytes",
                    self.lines_from_end + 1
                ));
            }
            self.pending.push(self.chunk[self.index]);
        }
    }
}

fn check_result_budget(rows: usize, bytes: usize) -> Result<(), String> {
    if rows > MAX_RESULT_ROWS || bytes > MAX_RESULT_BYTES {
        return Err(format!(
            "run log result exceeds {MAX_RESULT_ROWS} rows or {MAX_RESULT_BYTES} bytes; narrow the requested window"
        ));
    }
    Ok(())
}

// A forward scan is necessary for validation and offset-based parity reads.
// Keep a single row bounded even when the source grows while it is open.
pub(super) fn read_capped_line<R: BufRead>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>> {
    read_capped_line_with_end(reader).map(|row| row.map(|(bytes, _)| bytes))
}

pub(crate) fn read_capped_line_with_end<R: BufRead>(
    reader: &mut R,
) -> std::io::Result<Option<(Vec<u8>, bool)>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Ok(Some((line, false)))
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        let data_len = take - usize::from(newline.is_some());
        if line.len().saturating_add(data_len) > MAX_ROW_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("run log row exceeds {MAX_ROW_BYTES} bytes"),
            ));
        }
        line.extend_from_slice(&available[..data_len]);
        reader.consume(take);
        if newline.is_some() {
            return Ok(Some((line, true)));
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct LogValidateOutcome {
    pub total: usize,
    pub legacy_ok: bool,
    pub legacy_lines: usize,
    pub corrupted_lines: BTreeSet<usize>,
    pub invalid_json_lines: usize,
    pub issues: Vec<String>,
}

pub fn validate_runs_jsonl_file(
    log_file: &Path,
    legacy_ok: bool,
) -> Result<LogValidateOutcome, String> {
    validate_runs_jsonl_file_cx(log_file, legacy_ok).map_err(|e| e.to_string())
}

fn validate_runs_jsonl_file_cx(log_file: &Path, legacy_ok: bool) -> CxResult<LogValidateOutcome> {
    let file = open_run_file(log_file)
        .map_err(|e| CxError::io(format!("cannot open {}", log_file.display()), e))?;
    let reader = BufReader::new(file);
    let mut out = LogValidateOutcome {
        legacy_ok,
        ..Default::default()
    };
    let mut reader = reader;
    let mut line_no = 0usize;
    while let Some(bytes) = read_capped_line(&mut reader)
        .map_err(|e| CxError::io(format!("read failed near line {}", line_no + 1), e))?
    {
        line_no += 1;
        let line = match String::from_utf8(bytes) {
            Ok(v) => v,
            Err(e) => {
                out.corrupted_lines.insert(line_no);
                out.invalid_json_lines += 1;
                out.issues
                    .push(format!("line {line_no}: invalid UTF-8: {e}"));
                if out.issues.len() > MAX_VALIDATION_ISSUES {
                    return Err(CxError::invalid("run log has too many validation issues"));
                }
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        out.total += 1;
        let parsed: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                out.corrupted_lines.insert(line_no);
                out.invalid_json_lines += 1;
                let preview: String = line.chars().take(160).collect();
                out.issues.push(
                    CxError::JsonLineParse {
                        file: log_file.to_path_buf(),
                        line: line_no,
                        content_preview: preview,
                        source: e,
                    }
                    .to_string(),
                );
                if out.issues.len() > MAX_VALIDATION_ISSUES {
                    return Err(CxError::invalid("run log has too many validation issues"));
                }
                continue;
            }
        };
        validate_row_fields(&parsed, line_no, legacy_ok, &mut out);
        if out.issues.len() > MAX_VALIDATION_ISSUES {
            return Err(CxError::invalid("run log has too many validation issues"));
        }
    }
    Ok(out)
}

fn validate_row_fields(
    parsed: &Value,
    line_no: usize,
    legacy_ok: bool,
    out: &mut LogValidateOutcome,
) {
    let Some(obj) = parsed.as_object() else {
        out.corrupted_lines.insert(line_no);
        out.issues
            .push(format!("line {line_no}: json is not an object"));
        return;
    };
    if legacy_ok {
        validate_legacy_or_modern_row(obj, line_no, out);
    } else {
        validate_required_fields(obj, line_no, out, true, false);
    }
}

fn validate_legacy_or_modern_row(
    obj: &serde_json::Map<String, Value>,
    line_no: usize,
    out: &mut LogValidateOutcome,
) {
    let is_modern = obj.contains_key("execution_id") && obj.contains_key("timestamp");
    if is_modern {
        validate_required_fields(obj, line_no, out, false, true);
        return;
    }
    let mut legacy_ok = true;
    for (legacy_k, modern_k) in REQUIRED_LEGACY_ANY_OF {
        if !(obj.contains_key(legacy_k) || obj.contains_key(modern_k)) {
            legacy_ok = false;
            out.corrupted_lines.insert(line_no);
            out.issues.push(format!(
                "line {line_no}: missing legacy field '{legacy_k}' (or '{modern_k}')"
            ));
        }
    }
    if legacy_ok {
        out.legacy_lines += 1;
    }
}

fn validate_required_fields(
    obj: &serde_json::Map<String, Value>,
    line_no: usize,
    out: &mut LogValidateOutcome,
    check_links: bool,
    allow_legacy_status: bool,
) {
    for k in REQUIRED_STRICT_FIELDS {
        if !obj.contains_key(k) {
            out.corrupted_lines.insert(line_no);
            out.issues
                .push(format!("line {line_no}: missing required field '{k}'"));
        }
    }
    validate_command_fields(obj, line_no, out, allow_legacy_status);
    if check_links {
        validate_schema_link(obj, line_no, out);
    }
}

fn validate_command_fields(
    obj: &serde_json::Map<String, Value>,
    line_no: usize,
    out: &mut LogValidateOutcome,
    allow_legacy_status: bool,
) {
    let command = obj.get("command").and_then(Value::as_str).unwrap_or("");
    let tool = obj.get("tool").and_then(Value::as_str).unwrap_or("");
    if command != "capture" && tool != "capture" {
        return;
    }
    let Some(status) = obj.get("system_status") else {
        if allow_legacy_status {
            return;
        }
        out.corrupted_lines.insert(line_no);
        out.issues.push(format!(
            "line {line_no}: capture row missing command-provenance field 'system_status'"
        ));
        return;
    };
    if allow_legacy_status && status.is_null() {
        return;
    }
    if status
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .is_none()
    {
        out.corrupted_lines.insert(line_no);
        out.issues.push(format!(
            "line {line_no}: capture row field 'system_status' must be an integer status"
        ));
    }
}

fn validate_schema_link(
    obj: &serde_json::Map<String, Value>,
    line_no: usize,
    out: &mut LogValidateOutcome,
) {
    let enforced = obj
        .get("schema_enforced")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let valid = obj
        .get("schema_valid")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !enforced || valid {
        return;
    }
    let qid = obj
        .get("quarantine_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if qid.is_empty() {
        return;
    }
    if let Err(e) = read_quarantine_record(qid) {
        out.corrupted_lines.insert(line_no);
        out.issues.push(format!(
            "line {line_no}: schema failure quarantine_id '{qid}' is not readable: {e}"
        ));
    }
}

pub fn load_runs(log_file: &Path, limit: usize) -> Result<Vec<RunEntry>, String> {
    load_runs_cx(log_file, limit).map_err(|e| e.to_string())
}

fn load_runs_cx(log_file: &Path, limit: usize) -> CxResult<Vec<RunEntry>> {
    let mut reader = RecentLines::open(log_file).map_err(CxError::invalid)?;
    let mut out: Vec<RunEntry> = Vec::new();
    let mut bytes = 0usize;
    let mut invalid = 0usize;
    let mut sample: Option<String> = None;
    while let Some(line) = reader.next_line().map_err(CxError::invalid)? {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match serde_json::from_slice::<RunEntry>(&line) {
            Ok(v) => {
                bytes = bytes.saturating_add(line.len());
                out.push(v);
                check_result_budget(out.len(), bytes).map_err(CxError::invalid)?;
                if limit > 0 && out.len() >= limit {
                    break;
                }
            }
            Err(e) => {
                invalid += 1;
                if sample.is_none() {
                    sample = Some(format!("recent row from end: {e}"));
                }
            }
        }
    }
    maybe_warn_invalid_lines(log_file, invalid, sample);
    out.reverse();
    Ok(out)
}

pub fn find_execution_row(log_file: &Path, execution_id: &str) -> Result<Option<Value>, String> {
    let mut reader = RecentLines::open(log_file)?;
    while let Some(line) = reader.next_line()? {
        if let Ok(row) = serde_json::from_slice::<Value>(&line)
            && row.get("execution_id").and_then(Value::as_str) == Some(execution_id)
        {
            return Ok(Some(row));
        }
        if reader.scanned > 16 * 1024 * 1024 {
            return Err("execution lookup scan exceeds 16777216 bytes".to_string());
        }
    }
    Ok(None)
}

pub fn find_field_value(
    log_file: &Path,
    field: &str,
    expected: &str,
) -> Result<Option<Value>, String> {
    let mut reader = RecentLines::open(log_file)?;
    while let Some(line) = reader.next_line()? {
        let Ok(row) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        if row.get(field).and_then(Value::as_str) == Some(expected) {
            return Ok(Some(row));
        }
    }
    Ok(None)
}

pub fn tail_log_lines(log_file: &Path, limit: usize) -> Result<Vec<String>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut reader = RecentLines::open(log_file)?;
    let mut out = Vec::new();
    let mut bytes = 0usize;
    while let Some(line) = reader.next_line()? {
        let Ok(text) = String::from_utf8(line) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        bytes = bytes.saturating_add(text.len());
        out.push(text);
        check_result_budget(out.len(), bytes)?;
        if out.len() >= limit {
            break;
        }
    }
    out.reverse();
    Ok(out)
}

pub fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

pub fn load_runs_appended(log_file: &Path, offset: u64) -> Result<Vec<RunEntry>, String> {
    load_runs_appended_cx(log_file, offset).map_err(|e| e.to_string())
}

fn load_runs_appended_cx(log_file: &Path, offset: u64) -> CxResult<Vec<RunEntry>> {
    let file = open_run_file(log_file)
        .map_err(|e| CxError::io(format!("cannot open {}", log_file.display()), e))?;
    let mut reader = BufReader::new(file);
    if offset > 0 {
        reader
            .seek(SeekFrom::Start(offset))
            .map_err(|e| CxError::io(format!("seek failed on {}", log_file.display()), e))?;
    }
    let mut out: Vec<RunEntry> = Vec::new();
    let mut bytes = 0usize;
    let mut invalid = 0usize;
    let mut sample: Option<String> = None;
    let mut line_no = 0usize;
    let mut scanned = 0u64;
    while let Some(line) = read_capped_line(&mut reader)
        .map_err(|e| CxError::io(format!("read failed on {}", log_file.display()), e))?
    {
        scanned = scanned.saturating_add(line.len() as u64 + 1);
        if scanned > MAX_SCAN_BYTES {
            return Err(CxError::invalid(
                "appended run log scan exceeds 134217728 bytes",
            ));
        }
        line_no += 1;
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match serde_json::from_slice::<RunEntry>(&line) {
            Ok(v) => {
                bytes = bytes.saturating_add(line.len());
                out.push(v);
                check_result_budget(out.len(), bytes).map_err(CxError::invalid)?;
            }
            Err(e) => {
                invalid += 1;
                if sample.is_none() {
                    let preview = String::from_utf8_lossy(&line[..line.len().min(160)]).to_string();
                    sample = Some(
                        CxError::JsonLineParse {
                            file: log_file.to_path_buf(),
                            line: line_no,
                            content_preview: preview,
                            source: e,
                        }
                        .to_string(),
                    );
                }
            }
        }
    }
    maybe_warn_invalid_lines(log_file, invalid, sample);
    Ok(out)
}

fn maybe_warn_invalid_lines(log_file: &Path, invalid: usize, sample: Option<String>) {
    if invalid == 0 {
        return;
    }
    if RUNS_PARSE_WARNED.swap(true, Ordering::SeqCst) {
        return;
    }
    let cli = cli_app_name();
    crate::cx_eprintln!(
        "XSHELF warning: skipped {} invalid JSON lines in {} (sample: {}). Run '{} logs validate' for details.",
        invalid,
        log_file.display(),
        sample.unwrap_or_else(|| "n/a".to_string()),
        cli
    );
}
