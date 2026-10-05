mod common;

use common::*;
use serde_json::Value;

#[test]
fn task_output_parity() {
    for mode in ["--text", "--json"] {
        for objective in [
            "capture touch task-executed",
            "llm use ollama unexpected-model",
            "Describe the current task",
            "cxo echo supported-command",
        ] {
            let repo = TempRepo::new("cxrs-task-routing");
            repo.write_mock_primary(
                r#"#!/usr/bin/env bash
cat > "$TASK_PROMPT_FILE"
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"task ok"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":2,"output_tokens":1}}'
"#,
            );
            let prompt_path = repo.root.join("prompt.txt").display().to_string();
            let add = repo.run(&["task", "add", objective]);
            assert!(add.status.success(), "{}", stderr_str(&add));
            let id = stdout_str(&add).trim().to_string();
            let run = repo.run_with_env(
                &["task", "run", &id, mode],
                &[
                    ("CX_LLM_BACKEND", "primary"),
                    ("TASK_PROMPT_FILE", &prompt_path),
                ],
            );
            assert!(run.status.success(), "{}", stderr_str(&run));
            assert!(!repo.root.join("task-executed").exists());
            let prompt = std::fs::read_to_string(&prompt_path).expect("provider received prompt");
            if !objective.starts_with("cxo ") {
                assert!(prompt.contains(objective), "{prompt}");
                assert!(prompt.contains("Task Objective:"), "{prompt}");
            }
            if mode == "--json" {
                let payload: Value = serde_json::from_str(&stdout_str(&run)).expect("task json");
                assert_eq!(payload["contract_version"], "task-run.v1");
                assert_eq!(payload["status"], "complete");
            }
        }
    }
}
