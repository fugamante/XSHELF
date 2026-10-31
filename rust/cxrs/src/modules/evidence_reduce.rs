use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::capture::{BudgetConfig, clip_text_with_config};

#[derive(Debug, Serialize, Deserialize)]
pub struct Summary {
    pub profile: String,
    pub text: String,
    pub raw_chars: usize,
    pub summary_chars: usize,
    pub raw_lines: usize,
    pub selection_omitted_lines: usize,
    pub truncated: bool,
}

pub fn classify(command: &[String]) -> &'static str {
    let name = command
        .first()
        .and_then(|s| std::path::Path::new(s).file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    match name {
        "cargo"
            if matches!(
                command.get(1).map(String::as_str),
                Some("test" | "build" | "check")
            ) =>
        {
            "cargo"
        }
        "xcodebuild" => "xcode",
        "swift" if matches!(command.get(1).map(String::as_str), Some("test" | "build")) => "xcode",
        _ => "generic",
    }
}

// Diagnostic markers exclude compiler policy flags such as -warnings-as-errors.
fn important(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "error:",
        "error[",
        "warning:",
        "warning[",
        "panicked",
        "assertion",
        "fatal error",
        "undefined symbol",
        "test result:",
        "executed ",
        "** test",
        "** build",
        "build complete",
        "finished ",
        "test suite",
        "testing failed",
        "testing started",
        "passed after",
        "issue recorded",
    ]
    .iter()
    .any(|word| lower.contains(word))
        || lower.split_whitespace().any(|word| {
            matches!(
                word.trim_matches([':', '.', ',', '*']),
                "failed" | "failure" | "failures"
            )
        })
}

fn recognized(profile: &str, input: &str) -> bool {
    match profile {
        "cargo" => input.lines().any(|s| {
            s.contains("test result:")
                || s.trim_start().starts_with("Finished ")
                || s.trim_start().starts_with("error:")
                || s.trim_start().starts_with("error[")
        }),
        "xcode" => input.lines().any(|s| {
            s.contains("Test Suite ")
                || s.contains("** BUILD ")
                || s.contains("** TEST ")
                || s.contains("Build complete!")
                || s.contains("Test run with ")
        }),
        _ => false,
    }
}

fn priority(line: &str) -> u8 {
    let lower = line.to_ascii_lowercase();
    if [
        "test result:",
        "executed ",
        "** test",
        "** build",
        "test run with",
        "build complete",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        0
    } else if important(line) {
        1
    } else {
        2
    }
}

// Outcomes precede diagnostics and context so routine noise cannot bury final counts.
fn select_lines(input: &str) -> (String, usize) {
    let lines: Vec<_> = input.lines().collect();
    let mut selected = vec![false; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        if priority(line) < 2 {
            for keep in &mut selected[i.saturating_sub(2)..(i + 3).min(lines.len())] {
                *keep = true;
            }
        }
    }
    let mut indices: Vec<_> = (0..lines.len()).filter(|i| selected[*i]).collect();
    indices.sort_by_key(|i| (priority(lines[*i]), *i));
    let mut seen = HashSet::new();
    let mut count = 0;
    let mut out = String::new();
    for i in indices {
        if seen.insert(lines[i]) {
            out.push_str(&format!("{}: {}\n", i + 1, lines[i]));
            count += 1;
        }
    }
    (out, count)
}

pub fn summarize(profile: &str, input: &str) -> Summary {
    let semantic = recognized(profile, input);
    let (candidate, selected) = if semantic {
        select_lines(input)
    } else {
        (input.to_string(), input.lines().count())
    };
    let cfg = BudgetConfig {
        budget_chars: 6000,
        budget_lines: 60,
        clip_mode: if semantic { "head" } else { "auto" }.to_string(),
        clip_footer: true,
    };
    let (text, _) = clip_text_with_config(&candidate, &cfg);
    let truncated = candidate.chars().count() > 6000 || candidate.lines().count() > 60;
    Summary {
        profile: if semantic { profile } else { "generic" }.to_string(),
        raw_chars: input.chars().count(),
        summary_chars: text.chars().count(),
        raw_lines: input.lines().count(),
        selection_omitted_lines: input.lines().count().saturating_sub(selected),
        truncated,
        text,
    }
}
