use super::{native_reduce_output_with_metadata, normalize_generic};

#[test]
fn passthrough_metadata() {
    let input = "plain output\nkept as-is\n";
    let metadata = super::reduction_metadata(
        super::ReducerKind::GenericPassthrough,
        super::ReduceProfile::Balanced,
        input,
        input,
    );
    assert_eq!(metadata.reducer_kind, "generic_passthrough");
    assert_eq!(metadata.profile, "balanced");
    assert_eq!(metadata.lossiness_level, "lossless");
    assert_eq!(metadata.omitted_lines, 0);
    assert_eq!(metadata.omitted_chars, 0);
    assert!(metadata.critical_sections_kept.is_empty());
}

#[test]
fn failure_markers() {
    let input = "line 1\nFAIL test_x\nwarning: foo\nline 2\n";
    let out = super::native_reduce_output(&["test".into()], input);
    assert!(out.contains("FAIL test_x"));
    assert!(out.contains("warning: foo"));
}

#[test]
fn test_fallback() {
    let input = include_str!("../../tests/fixtures/phase_x/cargo_unknown_fallback.txt");
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/phase_x/output_fallback_manifest.json"
    ))
    .expect("parse fallback fixture manifest");
    let command = manifest["command"]
        .as_array()
        .expect("command")
        .iter()
        .map(|value| value.as_str().expect("command string").to_string())
        .collect::<Vec<_>>();
    let result = native_reduce_output_with_metadata(&command, input);

    assert_eq!(result.metadata.reducer_kind, "test_output");
    assert_eq!(result.text, normalize_generic(input));
    assert!(!result.text.is_empty());
    for span in manifest["required_spans"]
        .as_array()
        .expect("required spans")
    {
        let span = span.as_str().expect("span string");
        assert!(result.text.contains(span), "missing fallback span: {span}");
    }
}

#[test]
fn test_tail() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/phase_x/output_tail_manifest.json"
    ))
    .expect("parse tail fixture manifest");
    let prefix_lines = manifest["prefix_lines"].as_u64().expect("prefix lines") as usize;
    let mut input = (0..prefix_lines)
        .map(|index| format!("running noisy harness step {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    input.push_str(include_str!(
        "../../tests/fixtures/phase_x/cargo_late_failure.txt"
    ));
    let result = native_reduce_output_with_metadata(&["cargo".into(), "test".into()], &input);

    assert!(result.text.lines().count() <= 400);
    for span in manifest["required_spans"]
        .as_array()
        .expect("required spans")
    {
        let span = span.as_str().expect("span string");
        assert!(result.text.contains(span), "missing late span: {span}");
    }
    assert_eq!(
        result
            .text
            .matches("test result: FAILED. 0 passed; 1 failed")
            .count(),
        1
    );
}

#[test]
fn cargo_omissions() {
    let input = "opaque output\n";
    for command in [
        vec!["cargo", "check"],
        vec!["cargo", "build"],
        vec!["cargo", "clippy"],
        vec!["cargo"],
        Vec::new(),
        vec!["cargo-test"],
    ] {
        let command = command.into_iter().map(String::from).collect::<Vec<_>>();
        let result = native_reduce_output_with_metadata(&command, input);
        assert_eq!(result.metadata.reducer_kind, "generic_passthrough");
        assert_eq!(result.text, input);
    }
}

#[test]
fn warning_flood_tail() {
    let mut input = (0..380)
        .map(|index| format!("running step {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    for index in 0..20_000 {
        input.push_str(&format!("\nwarning: distinct synthetic {index}"));
    }
    input.push_str(
        "\nthread 'test' panicked at synthetic late failure\nassertion `left == right` failed\nleft: 1\nright: 2\ntest result: FAILED. 0 passed; 1 failed\n",
    );

    let (_, retained_warnings) = super::reduce_test_count(&input);
    assert_eq!(retained_warnings, 0);
    let result = native_reduce_output_with_metadata(&["cargo".into(), "test".into()], &input);
    assert_eq!(result.metadata.reducer_kind, "test_output");
    assert!(result.text.lines().count() <= 400);
    assert!(result.text.contains("running step 0"));
    assert!(!result.text.contains("warning: distinct synthetic"));
    for span in [
        "thread 'test' panicked",
        "assertion `left == right` failed",
        "left: 1",
        "right: 2",
        "test result: FAILED. 0 passed; 1 failed",
    ] {
        assert!(result.text.contains(span), "missing late span: {span}");
    }
}

#[test]
fn mixed_panic_context() {
    let mut input = "running step\n".repeat(380);
    input.push_str("thread 'test' panicked with warning: synthetic\n");
    input.push_str("opaque context 1\nopaque context 2\nopaque context 3\n");
    let result = native_reduce_output_with_metadata(&["test".into()], &input);
    assert!(!result.text.contains("panicked with warning"));
    for context in ["opaque context 1", "opaque context 2", "opaque context 3"] {
        assert!(result.text.contains(context), "missing {context}");
    }
}

#[test]
fn long_panic_context() {
    let mut input = "running step\n".repeat(380);
    input.push_str(&format!(
        "thread '{} ' panicked at late failure\n",
        "x".repeat(700)
    ));
    input.push_str("opaque panic context 1\nopaque panic context 2\nopaque panic context 3\n");
    let result = native_reduce_output_with_metadata(&["test".into()], &input);
    assert!(result.text.lines().count() <= 400);
    for context in [
        "opaque panic context 1",
        "opaque panic context 2",
        "opaque panic context 3",
    ] {
        assert!(result.text.contains(context), "missing {context}");
    }
}

#[test]
fn unknown_output_bound() {
    let mut rows = (0..1_000)
        .map(|index| format!("opaque synthetic row {index} {}", "x".repeat(1_100)))
        .collect::<Vec<_>>();
    rows[500] = "é".repeat(900);
    let input = rows.join("\n");
    let result = native_reduce_output_with_metadata(&["cargo".into(), "test".into()], &input);
    assert_eq!(result.text.lines().count(), 400);
    assert_eq!(result.metadata.reducer_version, 2);
    assert_eq!(result.metadata.lossiness_level, "uncertain_fallback");
    assert_eq!(result.metadata.uncertainty, "high");
    assert!(result.metadata.critical_sections_kept.is_empty());
    assert!(result.text.contains("opaque synthetic row 0"));
    assert!(result.text.contains("opaque synthetic row 999"));
    assert!(!result.text.contains("opaque synthetic row 500"));
    assert_eq!(result.metadata.omitted_lines, 600);

    let huge = "opaque synthetic ".repeat(100_000);
    let fallback = super::bounds::bounded_fallback(&huge);
    assert_eq!(fallback.chars().count(), 603);
    assert!(fallback.ends_with("..."));
    let result = native_reduce_output_with_metadata(&["test".into()], &huge);
    assert_eq!(result.text.chars().count(), 604);

    let ordinary = (0..1_000)
        .map(|index| format!("opaque synthetic row {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let result = native_reduce_output_with_metadata(&["test".into()], &ordinary);
    assert_eq!(result.text, normalize_generic(&ordinary));
}
