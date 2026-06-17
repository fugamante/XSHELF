use std::env;
use std::process::Command;

use crate::process::run_command_output_with_timeout;
use crate::types::CaptureStats;

use super::capture_budget::{
    BudgetConfig, PromptSection, SectionPriority, assemble_sections_with_config,
    budget_config_from_env, clip_text_with_config,
};
use super::capture_reduce::{
    ReductionMetadata, ReductionResult, native_reduce_output_with_metadata,
};

fn run_capture(command: &[String]) -> Result<(String, i32), String> {
    if command.is_empty() {
        return Err("missing command".to_string());
    }
    let mut c = Command::new(&command[0]);
    if command.len() > 1 {
        c.args(&command[1..]);
    }
    let output = run_command_output_with_timeout(c, &format!("system command '{}'", command[0]))?;
    let status = output.status.code().unwrap_or(1);
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !stderr.trim().is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&stderr);
    }
    Ok((combined, status))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShadowAssembly {
    pub text: String,
    pub omitted_section_ids: Vec<String>,
    pub clipped: bool,
}

fn shadow_enabled() -> bool {
    env::var("CX_CAPTURE_ASSEMBLY_SHADOW")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(0)
        == 1
}

fn command_section(cmd: &[String], status: i32) -> String {
    let command = shell_words::join(cmd.iter().map(String::as_str));
    format!("command: {command}\nexit_status: {status}")
}

fn metadata_section(metadata: &ReductionMetadata) -> String {
    format!(
        "reducer_kind: {}\nreducer_version: {}\nprofile: {}\nlossiness_level: {}\nuncertainty: {}\nraw_chars: {}\nreduced_chars: {}\nomitted_lines: {}\nomitted_chars: {}\ncritical_sections_kept: {}",
        metadata.reducer_kind,
        metadata.reducer_version,
        metadata.profile,
        metadata.lossiness_level,
        metadata.uncertainty,
        metadata.raw_chars,
        metadata.reduced_chars,
        metadata.omitted_lines,
        metadata.omitted_chars,
        metadata.critical_sections_kept.join(",")
    )
}

fn passthrough_result(input: String) -> ReductionResult {
    let chars = input.chars().count();
    ReductionResult {
        text: input,
        metadata: ReductionMetadata {
            reducer_kind: "generic_passthrough",
            reducer_version: 1,
            profile: "off",
            lossiness_level: "lossless",
            raw_chars: chars,
            reduced_chars: chars,
            clipped_chars: 0,
            omitted_lines: 0,
            omitted_chars: 0,
            critical_sections_kept: Vec::new(),
            uncertainty: "low",
            replay_pointer: None,
        },
    }
}

pub(crate) fn assemble_capture_shadow(
    cmd: &[String],
    status: i32,
    reduction: &ReductionResult,
    cfg: &BudgetConfig,
) -> ShadowAssembly {
    let command = command_section(cmd, status);
    let metadata = metadata_section(&reduction.metadata);
    let output_priority = if reduction.metadata.uncertainty == "high" {
        SectionPriority::Critical
    } else {
        SectionPriority::High
    };
    let output_id = if reduction.metadata.uncertainty == "high" {
        "output.uncertain_fallback"
    } else {
        "output.reduced"
    };
    let sections = [
        PromptSection {
            id: "command.exit_status",
            priority: SectionPriority::Critical,
            text: &command,
            uncertainty: "low",
        },
        PromptSection {
            id: "reducer.metadata",
            priority: SectionPriority::Medium,
            text: &metadata,
            uncertainty: "low",
        },
        PromptSection {
            id: output_id,
            priority: output_priority,
            text: &reduction.text,
            uncertainty: reduction.metadata.uncertainty,
        },
    ];
    let result = assemble_sections_with_config(&sections, cfg);
    ShadowAssembly {
        text: result.text,
        omitted_section_ids: result
            .omissions
            .iter()
            .map(|omission| omission.id.clone())
            .collect(),
        clipped: result.clipped,
    }
}

fn process_capture(
    cmd: &[String],
    status: i32,
    raw_out: String,
    native_reduce: bool,
    run_shadow: bool,
    cfg: &BudgetConfig,
) -> (String, CaptureStats) {
    let processed = raw_out.clone();
    let reduction = if native_reduce {
        native_reduce_output_with_metadata(cmd, &processed)
    } else {
        passthrough_result(processed)
    };
    if run_shadow {
        let _ = assemble_capture_shadow(cmd, status, &reduction, cfg);
    }
    let (clipped_text, mut stats) = clip_text_with_config(&reduction.text, cfg);
    stats.rtk_used = Some(false);
    stats.capture_provider = Some("native".to_string());
    (clipped_text, stats)
}

pub fn run_system_command_capture(cmd: &[String]) -> Result<(String, i32, CaptureStats), String> {
    if cmd.is_empty() {
        return Err("missing command".to_string());
    }
    let (raw_out, status) = run_capture(cmd)?;
    let native_reduce = env::var("CX_NATIVE_REDUCE")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1)
        == 1;
    let cfg = budget_config_from_env();
    let (clipped_text, stats) =
        process_capture(cmd, status, raw_out, native_reduce, shadow_enabled(), &cfg);
    Ok((clipped_text, status, stats))
}

#[cfg(test)]
mod tests {
    use super::super::capture_reduce::native_reduce_output_with_metadata;
    use super::*;

    fn test_budget(chars: usize, lines: usize) -> BudgetConfig {
        BudgetConfig {
            budget_chars: chars,
            budget_lines: lines,
            clip_mode: "head".to_string(),
            clip_footer: false,
        }
    }

    #[test]
    fn shadow_keeps_core() {
        let cmd = vec!["test".to_string()];
        let input = "running 1 test\ntest api_error ... FAILED\nthread 'api_error' panicked\nerror: root cause\n";
        let reduction = native_reduce_output_with_metadata(&cmd, input);
        let shadow = assemble_capture_shadow(&cmd, 101, &reduction, &test_budget(1000, 80));

        assert!(shadow.text.contains("[critical] command.exit_status"));
        assert!(shadow.text.contains("command: test"));
        assert!(shadow.text.contains("exit_status: 101"));
        assert!(shadow.text.contains("[high] output.reduced"));
        assert!(shadow.text.contains("api_error"));
        assert!(shadow.omitted_section_ids.is_empty());
        assert!(!shadow.clipped);
    }

    #[test]
    fn shadow_promotes_uncertain() {
        let reduction = ReductionResult {
            text: "custom tool emitted ambiguous failure evidence".to_string(),
            metadata: ReductionMetadata {
                reducer_kind: "deep_fallback",
                reducer_version: 1,
                profile: "deep",
                lossiness_level: "uncertain_fallback",
                raw_chars: 128,
                reduced_chars: 43,
                clipped_chars: 0,
                omitted_lines: 3,
                omitted_chars: 85,
                critical_sections_kept: vec!["panic_error_assertion_warning_snippets"],
                uncertainty: "high",
                replay_pointer: None,
            },
        };
        let cmd = vec!["custom-tool".to_string()];
        let shadow = assemble_capture_shadow(&cmd, 2, &reduction, &test_budget(1000, 80));
        let output = shadow
            .text
            .find("[critical] output.uncertain_fallback")
            .expect("uncertain output is critical");
        let metadata = shadow
            .text
            .find("[medium] reducer.metadata")
            .unwrap_or(usize::MAX);

        assert!(output < metadata);
        assert!(shadow.text.contains("uncertainty: high"));
    }

    #[test]
    fn shadow_does_not_change_result() {
        let cmd = vec!["test".to_string()];
        let raw = "running 1 test\ntest api_error ... FAILED\nthread 'api_error' panicked\nerror: root cause\n";
        let cfg = test_budget(1000, 80);
        let without_shadow = process_capture(&cmd, 101, raw.to_string(), true, false, &cfg);
        let with_shadow = process_capture(&cmd, 101, raw.to_string(), true, true, &cfg);

        assert_eq!(without_shadow.0, with_shadow.0);
        assert_eq!(
            format!("{:?}", without_shadow.1),
            format!("{:?}", with_shadow.1)
        );
    }
}
