use std::borrow::Cow;
use std::collections::VecDeque;

pub(super) const HEAD_LIMIT: usize = 380;
pub(super) const TAIL_LIMIT: usize = 20;
const LINE_LIMIT: usize = 600;
pub(super) const FALLBACK_BYTES: usize = 1024 * 1024;

fn has_ascii(line: &str, needle: &str) -> bool {
    line.as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn starts_ascii(line: &str, needle: &str) -> bool {
    line.as_bytes()
        .get(..needle.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle.as_bytes()))
}

pub(super) struct TestMarkers {
    pub keep: bool,
    pub warning: bool,
    pub panic: bool,
    pub assertion: bool,
}

// Scan raw text so long panic headers still preserve their following context.
pub(super) fn test_markers(line: &str) -> TestMarkers {
    let trimmed = line.trim_start();
    let warning = has_ascii(line, "warning");
    let panic = starts_ascii(trimmed, "thread ") && has_ascii(line, "panicked");
    let assertion = starts_ascii(trimmed, "assertion ");
    let keep = has_ascii(line, "fail")
        || has_ascii(line, "error")
        || panic
        || warning
        || assertion
        || has_ascii(line, "test result")
        || has_ascii(line, "running ")
        || starts_ascii(trimmed, "left:")
        || starts_ascii(trimmed, "right:")
        || starts_ascii(trimmed, "note:")
        || starts_ascii(trimmed, "failures:");
    TestMarkers {
        keep,
        warning,
        panic,
        assertion,
    }
}

// Only retained text needs a copy; normalization will show the same prefix.
pub(super) fn visible_line(line: &str) -> Cow<'_, str> {
    match line.char_indices().nth(LINE_LIMIT) {
        Some((end, _)) => Cow::Owned(format!("{}...", &line[..end])),
        None => Cow::Borrowed(line),
    }
}

// Unknown test output stays lossless at ordinary sizes, then retains both ends.
pub(super) fn bounded_fallback(input: &str) -> String {
    if input.len() <= FALLBACK_BYTES {
        return input.to_string();
    }

    let mut head = Vec::with_capacity(HEAD_LIMIT);
    let mut tail = VecDeque::with_capacity(TAIL_LIMIT);
    for line in input.lines() {
        let shown = visible_line(line).into_owned();
        if head.len() < HEAD_LIMIT {
            head.push(shown);
        } else {
            if tail.len() == TAIL_LIMIT {
                tail.pop_front();
            }
            tail.push_back(shown);
        }
    }
    head.extend(tail);
    let mut output = head.join("\n");
    if input.ends_with('\n') {
        output.push('\n');
    }
    output
}
