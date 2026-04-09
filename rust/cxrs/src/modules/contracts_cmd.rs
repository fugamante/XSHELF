use serde_json::{Map, Value, json};

use crate::contract_versions::{
    ACTIONS_JSON_CONTRACT_VERSION, DIAG_JSON_CONTRACT_VERSION, OPTIMIZE_JSON_CONTRACT_VERSION,
    SCHEDULER_JSON_CONTRACT_VERSION, TELEMETRY_JSON_CONTRACT_VERSION,
};
use crate::execmeta::utc_now_iso;

#[derive(Clone, Copy)]
struct ContractSpec {
    action: &'static str,
    contract_version: &'static str,
    required_keys: &'static [&'static str],
}

const DIAG_REQUIRED_KEYS: &[&str] = &["contract_version", "retry", "concurrency"];
const SCHED_REQUIRED_KEYS: &[&str] = &["contract_version", "retry", "concurrency"];
const OPT_REQUIRED_KEYS: &[&str] = &["contract_version", "scoreboard"];
const TELE_REQUIRED_KEYS: &[&str] = &["contract_version", "fields", "contract_drift"];
const TASK_CHECK_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "status_filter",
    "selected",
    "waves",
    "blocked_total",
    "can_run",
    "recommended_mode",
];
const TASK_RUN_ALL_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "status_filter",
    "mode",
    "task_readiness",
    "preflight",
    "scheduled",
    "tasks",
];
const TASK_LIST_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "status_filter",
    "count",
    "list_readiness",
    "tasks",
];
const TASK_SHOW_REQUIRED_KEYS: &[&str] = &[
    "contract_version",
    "id",
    "status",
    "latest_run",
    "run_readiness",
];
const TASK_RUN_REQUIRED_KEYS: &[&str] = &["contract_version", "task_id", "status", "execution_id"];

const FULL_SPECS: &[ContractSpec] = &[
    ContractSpec {
        action: "cx.diag",
        contract_version: DIAG_JSON_CONTRACT_VERSION,
        required_keys: DIAG_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.scheduler",
        contract_version: SCHEDULER_JSON_CONTRACT_VERSION,
        required_keys: SCHED_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.optimize",
        contract_version: OPTIMIZE_JSON_CONTRACT_VERSION,
        required_keys: OPT_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.logs.stats",
        contract_version: TELEMETRY_JSON_CONTRACT_VERSION,
        required_keys: TELE_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.task.check",
        contract_version: "task-check.v1",
        required_keys: TASK_CHECK_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.task.run_all",
        contract_version: "task-run-all.v1",
        required_keys: TASK_RUN_ALL_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.task.list",
        contract_version: "task-list.v1",
        required_keys: TASK_LIST_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.task.show",
        contract_version: "task-show.v1",
        required_keys: TASK_SHOW_REQUIRED_KEYS,
    },
    ContractSpec {
        action: "cx.actions",
        contract_version: ACTIONS_JSON_CONTRACT_VERSION,
        required_keys: &["actions_contract_version", "actions"],
    },
    ContractSpec {
        action: "cx.task.run",
        contract_version: "task-run.v1",
        required_keys: TASK_RUN_REQUIRED_KEYS,
    },
];

const EVAL_LAB_ACTIONS: &[&str] = &[
    "cx.diag",
    "cx.scheduler",
    "cx.optimize",
    "cx.logs.stats",
    "cx.task.check",
    "cx.task.run_all",
    "cx.task.list",
    "cx.task.show",
    "cx.task.run",
];

fn profile_specs(profile: &str) -> Option<Vec<ContractSpec>> {
    match profile {
        "full" => Some(FULL_SPECS.to_vec()),
        "eval-lab" => Some(
            FULL_SPECS
                .iter()
                .copied()
                .filter(|spec| EVAL_LAB_ACTIONS.contains(&spec.action))
                .collect(),
        ),
        _ => None,
    }
}

fn contracts_object(specs: &[ContractSpec]) -> Value {
    let mut contracts = Map::new();
    for spec in specs {
        contracts.insert(
            spec.action.to_string(),
            json!({
                "contract_version": spec.contract_version,
                "required_keys": spec.required_keys,
            }),
        );
    }
    Value::Object(contracts)
}

fn bundle_value(app_version: &str, profile: &str, specs: &[ContractSpec]) -> Value {
    json!({
        "bundle_version": "cx-contract-bundle.v1",
        "source_version": app_version,
        "profile": profile,
        "generated_at": utc_now_iso(),
        "contracts": contracts_object(specs)
    })
}

pub fn cmd_contracts(app_name: &str, app_version: &str, args: &[String]) -> i32 {
    let usage = format!("Usage: {app_name} contracts export [--profile eval-lab|full] [--json]");
    let mut sub = "export";
    let mut profile = "full";
    let mut json_out = false;
    let mut i = 0usize;
    if let Some(v) = args.first().map(String::as_str)
        && !v.starts_with("--")
    {
        sub = v;
        i = 1;
    }
    if sub != "export" {
        crate::cx_eprintln!("{usage}");
        return 2;
    }
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json_out = true;
                i += 1;
            }
            "--profile" => {
                let Some(v) = args.get(i + 1).map(String::as_str) else {
                    crate::cx_eprintln!("{usage}");
                    return 2;
                };
                profile = v;
                i += 2;
            }
            other => {
                crate::cx_eprintln!("cxrs contracts: unknown flag '{other}'");
                crate::cx_eprintln!("{usage}");
                return 2;
            }
        }
    }

    let Some(specs) = profile_specs(profile) else {
        crate::cx_eprintln!("cxrs contracts: invalid profile '{profile}'");
        crate::cx_eprintln!("{usage}");
        return 2;
    };
    let bundle = bundle_value(app_version, profile, &specs);
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&bundle).unwrap_or_else(|_| bundle.to_string())
        );
        return 0;
    }

    let contracts = bundle
        .get("contracts")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    println!("bundle_version: cx-contract-bundle.v1");
    println!("source_version: {app_version}");
    println!("profile: {profile}");
    println!("contract_count: {}", contracts.len());
    for (action, spec) in contracts {
        let version = spec
            .get("contract_version")
            .and_then(Value::as_str)
            .unwrap_or("<missing>");
        println!("- {action} [{version}]");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::bundle_value;

    #[test]
    fn eval_lab_bundle_ok() {
        let bundle = bundle_value(
            "0.0.0-test",
            "eval-lab",
            &super::profile_specs("eval-lab").unwrap(),
        );
        let contracts = bundle
            .get("contracts")
            .and_then(serde_json::Value::as_object)
            .unwrap();
        assert!(contracts.contains_key("cx.diag"));
        assert!(contracts.contains_key("cx.scheduler"));
        assert!(contracts.contains_key("cx.task.run_all"));
        assert!(contracts.contains_key("cx.task.run"));
        assert_eq!(
            contracts["cx.task.run"]["contract_version"].as_str(),
            Some("task-run.v1")
        );
    }
}
