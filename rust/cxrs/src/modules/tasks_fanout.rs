use std::fs;
use std::process::Command;

use crate::config::{app_config, cli_app_name};
use crate::execmeta::utc_now_iso;
use crate::process::run_command_output_with_timeout;
use crate::types::TaskRecord;

use super::{TaskMutation, mutate_tasks, next_task_id};

fn collect_source_text(source: &str) -> Result<String, i32> {
    let out = match source {
        "staged-diff" => {
            let mut cmd = Command::new("git");
            cmd.args(["diff", "--staged", "--no-color"]);
            run_command_output_with_timeout(cmd, "task fanout staged-diff").ok()
        }
        "worktree" => {
            let mut cmd = Command::new("git");
            cmd.args(["diff", "--no-color"]);
            run_command_output_with_timeout(cmd, "task fanout worktree").ok()
        }
        "log" => {
            let mut cmd = Command::new("git");
            cmd.args(["log", "--oneline", "-n", "200"]);
            run_command_output_with_timeout(cmd, "task fanout log").ok()
        }
        x if x.starts_with("file:") => {
            let p = x.trim_start_matches("file:");
            return Ok(fs::read_to_string(p).unwrap_or_default());
        }
        _ => {
            crate::cx_eprintln!(
                "{} task fanout: unsupported --from source '{source}'",
                cli_app_name()
            );
            return Err(2);
        }
    };
    Ok(out
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).to_string())
            } else {
                None
            }
        })
        .unwrap_or_default())
}

fn count_fanout_groups(input: &str, budget: usize) -> usize {
    let mut groups = 0usize;
    let mut used = 0usize;
    for line in input.lines() {
        let line_chars = line.chars().count().saturating_add(1);
        if used > 0 && line_chars > budget.saturating_sub(used) {
            groups = groups.saturating_add(1);
            used = 0;
        }
        used = used.saturating_add(line_chars);
    }
    groups.saturating_add(usize::from(used > 0))
}

fn make_subtask(
    role: &str,
    index: usize,
    total: usize,
    objective: &str,
    parent_id: &str,
    has_chunks: bool,
    tasks: &[TaskRecord],
) -> TaskRecord {
    let id = next_task_id(tasks);
    let context_ref = if has_chunks {
        format!("diff_chunk_{}/{}", index + 1, total)
    } else {
        format!("objective:{objective}")
    };
    let sub_obj = match role {
        "architect" => format!("Define implementation plan for: {objective}"),
        "implementer" => format!("Implement chunk {} for: {objective}", index + 1),
        "reviewer" => format!(
            "Review chunk {} changes for correctness/safety: {objective}",
            index + 1
        ),
        "tester" => format!("Create/execute tests for chunk {}: {objective}", index + 1),
        _ => format!("Document chunk {} outcomes: {objective}", index + 1),
    };
    TaskRecord {
        id,
        parent_id: Some(parent_id.to_string()),
        role: role.to_string(),
        objective: sub_obj,
        context_ref,
        backend: "auto".to_string(),
        model: None,
        profile: "balanced".to_string(),
        converge: "none".to_string(),
        replicas: 1,
        max_concurrency: None,
        run_mode: "parallel".to_string(),
        depends_on: vec![parent_id.to_string()],
        resource_keys: match role {
            "implementer" => vec!["repo:write".to_string()],
            _ => vec!["repo:read".to_string()],
        },
        max_retries: None,
        timeout_secs: None,
        status: "pending".to_string(),
        created_at: utc_now_iso(),
        updated_at: utc_now_iso(),
    }
}

fn next_task_id_with_created(tasks: &[TaskRecord], created: &[TaskRecord]) -> String {
    let max_n = tasks
        .iter()
        .chain(created.iter())
        .filter_map(|t| t.id.strip_prefix("task_")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    format!("task_{:03}", max_n + 1)
}

fn ensure_min_created(
    created: &mut Vec<TaskRecord>,
    parent_id: &str,
    objective: &str,
    tasks: &[TaskRecord],
) {
    let roles_cycle = ["architect", "implementer", "reviewer", "tester", "doc"];
    while created.len() < 3 {
        let role = roles_cycle[(created.len() + 1) % roles_cycle.len()].to_string();
        let id = next_task_id_with_created(tasks, created);
        let rec = TaskRecord {
            id,
            parent_id: Some(parent_id.to_string()),
            role: role.clone(),
            objective: format!("{} workstream for: {}", role, objective),
            context_ref: "objective".to_string(),
            backend: "auto".to_string(),
            model: None,
            profile: "balanced".to_string(),
            converge: "none".to_string(),
            replicas: 1,
            max_concurrency: None,
            run_mode: "parallel".to_string(),
            depends_on: vec![parent_id.to_string()],
            resource_keys: vec!["repo:read".to_string()],
            max_retries: None,
            timeout_secs: None,
            status: "pending".to_string(),
            created_at: utc_now_iso(),
            updated_at: utc_now_iso(),
        };
        created.push(rec);
    }
}

fn add_fanout_parent(tasks: &mut Vec<TaskRecord>, obj: &str) -> String {
    let parent_id = next_task_id(tasks);
    let now = utc_now_iso();
    tasks.push(TaskRecord {
        id: parent_id.clone(),
        parent_id: None,
        role: "architect".to_string(),
        objective: obj.to_string(),
        context_ref: "fanout_parent".to_string(),
        backend: "auto".to_string(),
        model: None,
        profile: "balanced".to_string(),
        converge: "none".to_string(),
        replicas: 1,
        max_concurrency: None,
        run_mode: "sequential".to_string(),
        depends_on: Vec::new(),
        resource_keys: vec!["repo:write".to_string()],
        max_retries: None,
        timeout_secs: None,
        status: "pending".to_string(),
        created_at: now.clone(),
        updated_at: now,
    });
    parent_id
}

fn create_fanout_children(
    tasks: &mut Vec<TaskRecord>,
    parent_id: &str,
    objective: &str,
    has_chunks: bool,
    chunk_count: usize,
) -> Vec<TaskRecord> {
    let roles_cycle = ["architect", "implementer", "reviewer", "tester", "doc"];
    let mut created: Vec<TaskRecord> = Vec::new();
    for i in 0..chunk_count {
        let role = roles_cycle[(i + 1) % roles_cycle.len()];
        let rec = make_subtask(
            role,
            i,
            chunk_count,
            objective,
            parent_id,
            has_chunks,
            tasks,
        );
        tasks.push(rec.clone());
        created.push(rec);
    }
    ensure_min_created(&mut created, parent_id, objective, tasks);
    if created.len() > 8 {
        created.truncate(8);
    }
    created
}

fn print_fanout_table(parent_id: &str, created: Vec<TaskRecord>) {
    println!("parent: {parent_id}");
    println!("id | role | status | context_ref | objective");
    println!("---|---|---|---|---");
    for t in created {
        println!(
            "{} | {} | {} | {} | {}",
            t.id, t.role, t.status, t.context_ref, t.objective
        );
    }
}

pub fn cmd_task_fanout(app_name: &str, objective: &str, from: Option<&str>) -> i32 {
    let obj = objective.trim();
    if obj.is_empty() {
        crate::cx_eprintln!("Usage: {app_name} task fanout <objective>");
        return 2;
    }
    let budget = app_config().budget_chars;
    if budget == 0 {
        crate::cx_eprintln!("{app_name} task fanout: context budget chars must be > 0");
        return 2;
    }
    let source = from.unwrap_or("worktree");
    let diff = match collect_source_text(source) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let objective = obj.to_string();
    let has_chunks = !diff.trim().is_empty();
    // Fanout stores symbolic refs only. Keep its prior line-group count so
    // splitting an oversized CLI chunk does not create extra provider tasks.
    let chunk_count = if has_chunks {
        count_fanout_groups(&diff, budget).clamp(1, 6)
    } else {
        1
    };
    let (parent_id, created) = match mutate_tasks("fanout", None, 0, move |tasks| {
        let parent_id = add_fanout_parent(tasks, &objective);
        let created =
            create_fanout_children(tasks, &parent_id, &objective, has_chunks, chunk_count);
        Ok(TaskMutation {
            result: (parent_id.clone(), created),
            task_id: Some(parent_id),
        })
    }) {
        Ok(result) => result,
        Err(e) => {
            crate::cx_eprintln!("{} task fanout: {e}", cli_app_name());
            return 1;
        }
    };
    print_fanout_table(&parent_id, created);
    0
}

#[cfg(test)]
mod tests {
    use super::count_fanout_groups;

    #[test]
    fn fanout_keeps_groups() {
        assert_eq!(count_fanout_groups("abcdefghijklmnop\n", 8), 1);
        assert_eq!(count_fanout_groups("one\ntwo\nthree\n", 8), 2);
        assert_eq!(count_fanout_groups("", 8), 0);
    }
}
