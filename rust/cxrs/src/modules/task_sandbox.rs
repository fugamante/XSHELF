use std::env;
use std::path::Path;
use std::process::Command;

use crate::paths::repo_root;
use crate::state::{read_state_value, value_at_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSandboxConfig {
    pub enabled: bool,
    pub image: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TaskSandboxReadiness {
    pub enabled: bool,
    pub active: bool,
    pub image: Option<String>,
    pub ready: bool,
    pub docker_available: bool,
    pub image_available: bool,
    pub repo_mount_writable: bool,
    pub entrypoint_available: bool,
    pub issues: Vec<String>,
    pub recommended_action: Option<String>,
}

pub struct SandboxAuthority {
    pub image: String,
    executable: Option<String>,
    repo_exec: bool,
    shared: Vec<(String, String)>,
}

fn process_grant(name: &str) -> bool {
    env::var(name).is_ok_and(|value| value.trim() == "1")
}

fn process_value(name: &str) -> Result<Option<String>, String> {
    match env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(_) => Err(format!("{name} must contain valid Unicode")),
    }
}

pub fn task_sandbox_config() -> TaskSandboxConfig {
    let state = read_state_value();
    let enabled = env::var("CX_TASK_SANDBOX_ENABLED")
        .ok()
        .and_then(|v| match v.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .or_else(|| {
            state
                .as_ref()
                .and_then(|s| value_at_path(s, "preferences.task_sandbox.enabled")?.as_bool())
        })
        .unwrap_or(false);
    // Preserve requested configuration for show/check; admission never trusts state.
    let image = env::var("CX_TASK_SANDBOX_IMAGE").ok().or_else(|| {
        state.as_ref().and_then(|s| {
            value_at_path(s, "preferences.task_sandbox.image")?
                .as_str()
                .map(ToOwned::to_owned)
        })
    });
    TaskSandboxConfig { enabled, image }
}

pub fn task_sandbox_active() -> bool {
    process_grant("CX_TASK_SANDBOX_ACTIVE")
}

fn image_valid(image: &str) -> bool {
    image.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn reserved_env(name: &str) -> bool {
    name.starts_with("CX_TASK_")
        || name.starts_with("CX_EXECUTION_")
        || name.starts_with("CXFIX_")
        || name.starts_with("LD_")
        || name.starts_with("DYLD_")
        || name.starts_with("DOCKER_")
        || matches!(
            name,
            "PATH"
                | "HOME"
                | "BASH_ENV"
                | "ENV"
                | "SHELLOPTS"
                | "BASHOPTS"
                | "CDPATH"
                | "GLOBIGNORE"
                | "IFS"
                | "PWD"
                | "OLDPWD"
                | "CARGO_TARGET_DIR"
                | "CARGO_NET_OFFLINE"
                | "CX_UNSAFE"
                | "CX_LLM_BACKEND"
                | "CX_REPO_ROOT"
                | "CX_BIN_CX"
                | "CX_CLI_NAME"
                | "CX_SOURCE_LOCATION"
        )
}

fn shared_env() -> Result<Vec<(String, String)>, String> {
    let raw = process_value("CX_TASK_SANDBOX_SHARE_ENV")?.unwrap_or_default();
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let names: Vec<&str> = raw.split(',').collect();
    // Validate every selector before looking up a value or launching Docker.
    for name in &names {
        let mut bytes = name.bytes();
        let valid = bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_');
        if !valid || reserved_env(name) {
            return Err("invalid or reserved CX_TASK_SANDBOX_SHARE_ENV name".to_string());
        }
    }
    names
        .into_iter()
        .map(|name| {
            process_value(name)?
                .map(|value| (name.to_string(), value))
                .ok_or_else(|| format!("explicitly shared variable {name} is unset"))
        })
        .collect()
}

fn sandbox_authority() -> Result<SandboxAuthority, String> {
    if !process_grant("CX_TASK_TRUST_SANDBOX") {
        return Err(
            "review the sandbox and set CX_TASK_TRUST_SANDBOX=1 for this invocation".to_string(),
        );
    }
    let image = process_value("CX_TASK_SANDBOX_IMAGE")?
        .filter(|v| image_valid(v))
        .ok_or_else(|| {
            "CX_TASK_SANDBOX_IMAGE must be an operator-approved full sha256 image ID".to_string()
        })?;
    let executable = process_value("CX_TASK_SANDBOX_EXECUTABLE")?;
    if let Some(path) = &executable
        && (!path.starts_with('/')
            || path == "/"
            || path.starts_with("/work/")
            || path == "/work"
            || path.bytes().any(|b| b.is_ascii_control())
            || path.split('/').skip(1).any(str::is_empty)
            || path.split('/').any(|part| part == "." || part == ".."))
    {
        return Err(
            "CX_TASK_SANDBOX_EXECUTABLE must be an absolute image path outside /work".to_string(),
        );
    }
    Ok(SandboxAuthority {
        image,
        executable,
        repo_exec: process_grant("CX_TASK_TRUST_REPO_EXEC"),
        shared: shared_env()?,
    })
}

fn local_image(authority: &SandboxAuthority) -> bool {
    let mut cmd = Command::new("docker");
    cmd.args([
        "image",
        "inspect",
        "--format",
        "{{.Id}}",
        "--",
        &authority.image,
    ]);
    crate::process::run_command_output_with_timeout(cmd, "task sandbox local image inspect")
        .is_ok_and(|out| {
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == authority.image
        })
}

pub fn task_sandbox_admission() -> Result<Option<SandboxAuthority>, String> {
    if !task_sandbox_config().enabled {
        return Ok(None);
    }
    let authority = sandbox_authority()?;
    // A recursion marker never replaces authority. The parent already checked the
    // immutable image; an admitted inner task must not require a Docker socket.
    if task_sandbox_active() {
        return Ok(None);
    }
    if !local_image(&authority) {
        return Err("approved image is not available locally; no image will be pulled".to_string());
    }
    Ok(Some(authority))
}

fn uid_gid() -> Result<String, String> {
    let mut ids = Vec::new();
    for flag in ["-u", "-g"] {
        let mut cmd = Command::new("id");
        cmd.arg(flag);
        let out =
            crate::process::run_command_output_with_timeout(cmd, "task sandbox user identity")?;
        let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !out.status.success() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err("unable to resolve container user identity".to_string());
        }
        ids.push(value);
    }
    Ok(ids.join(":"))
}

pub fn sandbox_command(
    root: &Path,
    authority: &SandboxAuthority,
    probe: bool,
) -> Result<Command, String> {
    let source = root
        .to_str()
        .filter(|s| !s.contains(',') && !s.chars().any(char::is_control))
        .ok_or_else(|| "repository mount path contains unsupported delimiters".to_string())?;
    let mut cmd = Command::new("docker");
    cmd.args([
        "run",
        "--rm",
        "--pull=never",
        "--no-healthcheck",
        "--entrypoint=/bin/bash",
        "--user",
        &uid_gid()?,
        "--workdir",
        "/work",
    ]);
    if probe {
        cmd.args(["--network=none", "--read-only"]);
    }
    let readonly = if probe { ",readonly" } else { "" };
    cmd.args([
        "--mount",
        &format!("type=bind,source={source},target=/work{readonly}"),
    ]);
    // Fixed shell/path controls prevent image ENV or selected sharing from sourcing
    // repository startup scripts. Secret values stay out of Docker's argv.
    for (key, value) in [
        ("HOME", "/tmp/cx-home"),
        ("BASH_ENV", "/dev/null"),
        ("ENV", "/dev/null"),
        (
            "PATH",
            "/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        ),
    ] {
        cmd.args(["-e", &format!("{key}={value}")]);
    }
    if !probe {
        for (key, value) in &authority.shared {
            cmd.env(key, value).args(["-e", key]);
        }
        for key in ["CX_TASK_TRUST_COMMANDS", "CX_TASK_TRUST_PROVIDER"] {
            cmd.args(["-e", &format!("{key}={}", u8::from(process_grant(key)))]);
        }
        // Preserve the existing numeric boolean parser for deliberate unsafe
        // overrides; the new authority grants use their separate exact-1 rule.
        let cfg = crate::config::app_config();
        for (key, value) in [
            ("CX_UNSAFE", cfg.cx_unsafe),
            ("CXFIX_FORCE", cfg.cxfix_force),
            ("CXFIX_RUN", cfg.cxfix_run),
        ] {
            cmd.args(["-e", &format!("{key}={}", u8::from(value))]);
        }
        for key in [
            "CX_TASK_REPLICA_INDEX",
            "CX_TASK_REPLICA_COUNT",
            "CX_TASK_CONVERGE_MODE",
        ] {
            if let Some(value) = process_value(key)? {
                cmd.env(key, value).args(["-e", key]);
            }
        }
        cmd.args([
            "-e",
            &format!("CX_LLM_BACKEND={}", crate::runtime::llm_backend()),
        ]);
        for (key, value) in [
            ("CARGO_TARGET_DIR", ".cx/task-sandbox/target"),
            ("CARGO_NET_OFFLINE", "true"),
            ("CX_TASK_SANDBOX_ACTIVE", "1"),
            ("CX_TASK_SANDBOX_ENABLED", "1"),
            ("CX_TASK_TRUST_SANDBOX", "1"),
            ("CX_TASK_TRUST_REPO_EXEC", "0"),
            ("CX_TASK_SANDBOX_SHARE_ENV", ""),
            (
                "CX_TASK_SANDBOX_EXECUTABLE",
                authority
                    .executable
                    .as_deref()
                    .unwrap_or("/usr/local/bin/xshelf"),
            ),
            ("CX_TASK_SANDBOX_IMAGE", authority.image.as_str()),
            ("CX_EXECUTION_LANE", "container"),
        ] {
            cmd.args(["-e", &format!("{key}={value}")]);
        }
        cmd.args([
            "-e",
            &format!("CX_EXECUTION_LANE_DETAIL=docker:{}", authority.image),
        ]);
    }
    cmd.arg(&authority.image)
        .args(["--noprofile", "--norc", "-c"]);
    Ok(cmd)
}

pub fn sandbox_script(authority: &SandboxAuthority) -> String {
    if let Some(path) = &authority.executable {
        return format!("app={}; test -x \"$app\"", shell_words::quote(path));
    }
    let repo = if authority.repo_exec {
        "elif [[ -x ./bin/xshelf ]]; then app=./bin/xshelf; elif [[ -x ./bin/cx ]]; then app=./bin/cx; "
    } else {
        ""
    };
    format!(
        "if [[ -x /usr/local/bin/xshelf ]]; then app=/usr/local/bin/xshelf; \
elif [[ -x /usr/local/bin/cx ]]; then app=/usr/local/bin/cx; {repo}\
else echo 'task sandbox: trusted executable unavailable; review image executable or CX_TASK_TRUST_REPO_EXEC=1' >&2; exit 127; fi"
    )
}

pub fn task_sandbox_readiness() -> TaskSandboxReadiness {
    let cfg = task_sandbox_config();
    let mut issues = Vec::new();
    if !cfg.enabled {
        issues.push("sandbox_disabled".to_string());
    }
    if cfg.image.is_none() {
        issues.push("image_unset".to_string());
    }
    let authority = sandbox_authority();
    if cfg.enabled
        && let Err(error) = &authority
    {
        issues.push(error.clone());
    }
    let root = repo_root();
    let repo_mount_writable = root.as_ref().is_some_and(|r| {
        r.join(".cx").is_dir()
            && r.join(".cx")
                .metadata()
                .is_ok_and(|m| !m.permissions().readonly())
    });
    if root.is_none() {
        issues.push("repo_unavailable".to_string());
    } else if !repo_mount_writable {
        issues.push("repo_state_not_writable".to_string());
    }
    // Disabled or denied checks are diagnostics only: no Docker process at all.
    let docker_available = cfg.enabled && authority.is_ok() && {
        let mut cmd = Command::new("docker");
        cmd.arg("--version");
        crate::process::run_command_output_with_timeout(cmd, "docker --version")
            .is_ok_and(|out| out.status.success())
    };
    let image_available = docker_available && authority.as_ref().is_ok_and(local_image);
    let entrypoint_available = image_available && repo_mount_writable && {
        let a = authority.as_ref().unwrap();
        sandbox_command(root.as_ref().unwrap(), a, true).is_ok_and(|mut cmd| {
            cmd.arg(format!("test -d .cx && {}", sandbox_script(a)));
            crate::process::run_command_output_with_timeout(
                cmd,
                "task sandbox readiness docker run",
            )
            .is_ok_and(|out| out.status.success())
        })
    };
    if cfg.enabled && authority.is_ok() {
        if !docker_available {
            issues.push("docker_unavailable".to_string());
        } else if !image_available {
            issues.push("image_unavailable".to_string());
        } else if !entrypoint_available {
            issues.push("entrypoint_unavailable".to_string());
        }
    }
    let ready = cfg.enabled && issues.is_empty() && entrypoint_available;
    let recommended_action = if ready {
        None
    } else if !cfg.enabled {
        Some("run `xshelf task sandbox enable`".to_string())
    } else if cfg.image.is_none() {
        Some("run `xshelf task sandbox set-image <image>`".to_string())
    } else {
        Some(
            "review sandbox authority, approved local image and executable; see execution guidance"
                .to_string(),
        )
    };
    TaskSandboxReadiness {
        enabled: cfg.enabled,
        active: task_sandbox_active(),
        image: cfg.image,
        ready,
        docker_available,
        image_available,
        repo_mount_writable,
        entrypoint_available,
        issues,
        recommended_action,
    }
}
