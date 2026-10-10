use std::env;
#[cfg(unix)]
use std::fs::{self, File};
use std::io::Write;
use std::process::Command;

#[cfg(not(unix))]
use tempfile::{NamedTempFile, TempDir};

#[cfg(unix)]
use std::io::{Seek, SeekFrom};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub struct CurlValues<'a> {
    pub url: &'a str,
    pub auth: Option<(&'a str, &'a str)>,
    pub pinned: Option<&'a str>,
    pub ca: Option<&'a str>,
    pub cert: Option<&'a str>,
    pub key: Option<&'a str>,
}

pub struct CurlPrivate {
    #[cfg(unix)]
    _file: File,
    #[cfg(not(unix))]
    _file: NamedTempFile,
    #[cfg(not(unix))]
    _dir: TempDir,
}

fn cfg_line(out: &mut String, name: &str, value: &str) -> Result<(), String> {
    if value.chars().any(char::is_control) {
        return Err(format!(
            "http-curl adapter {name} contains a control character"
        ));
    }
    out.push_str(name);
    out.push_str(" = \"");
    for ch in value.chars() {
        if matches!(ch, '\\' | '"') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push_str("\"\n");
    Ok(())
}

pub fn add_private(cmd: &mut Command, values: CurlValues<'_>) -> Result<CurlPrivate, String> {
    let mut config = String::new();
    cfg_line(&mut config, "url", values.url)?;
    if let Some((name, value)) = values.auth {
        cfg_line(&mut config, "header", &format!("{name}: {value}"))?;
    }
    for (name, value) in [
        ("pinnedpubkey", values.pinned),
        ("cacert", values.ca),
        ("cert", values.cert),
        ("key", values.key),
    ] {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            cfg_line(&mut config, name, value)?;
        }
    }

    // Unix keeps the config unnamed, so an abrupt parent exit leaves no path.
    // POST body input remains on stdin while curl reads its inherited descriptor.
    #[cfg(unix)]
    let private = {
        let mut base = tempfile::tempfile()
            .map_err(|e| format!("http-curl adapter could not create private config: {e}"))?;
        base.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("http-curl adapter could not protect private config: {e}"))?;
        base.write_all(config.as_bytes())
            .and_then(|_| base.flush())
            .and_then(|_| base.seek(SeekFrom::Start(0)).map(|_| ()))
            .map_err(|e| format!("http-curl adapter could not write private config: {e}"))?;
        // Reserve a non-stdio descriptor before the runner assigns curl pipes.
        let child_fd = unsafe { libc::fcntl(base.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
        if child_fd == -1 {
            return Err(format!(
                "http-curl adapter could not reserve private descriptor: {}",
                std::io::Error::last_os_error()
            ));
        }
        let file = unsafe { File::from_raw_fd(child_fd) };
        drop(base);
        let fd = file.as_raw_fd();
        // Only the curl child clears close-on-exec; no unrelated child inherits it.
        unsafe {
            cmd.pre_exec(move || {
                if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        cmd.arg("-q").arg("--config").arg(format!("/dev/fd/{fd}"));
        CurlPrivate { _file: file }
    };

    #[cfg(not(unix))]
    let private = {
        let dir = tempfile::Builder::new()
            .prefix("xshelf-http-")
            .tempdir()
            .map_err(|e| format!("http-curl adapter could not create private directory: {e}"))?;
        let mut file = NamedTempFile::new_in(dir.path())
            .map_err(|e| format!("http-curl adapter could not create private config: {e}"))?;
        file.write_all(config.as_bytes())
            .and_then(|_| file.flush())
            .map_err(|e| format!("http-curl adapter could not write private config: {e}"))?;
        cmd.arg("-q").arg("--config").arg(file.path());
        CurlPrivate {
            _file: file,
            _dir: dir,
        }
    };
    // -q is curl's first argument to exclude ambient curlrc options.
    for (name, _) in env::vars_os() {
        if name.to_string_lossy().starts_with("CX_HTTP_") {
            cmd.env_remove(name);
        }
    }
    Ok(private)
}

#[cfg(test)]
mod tests {
    use super::cfg_line;

    #[test]
    fn config_quotes_values() {
        let mut config = String::new();
        cfg_line(&mut config, "header", "X-Key: a\\\"b").expect("quoted header");
        assert_eq!(config, "header = \"X-Key: a\\\\\\\"b\"\n");
    }

    #[test]
    fn config_rejects_newlines() {
        let mut config = String::new();
        assert!(cfg_line(&mut config, "header", "X-Key: ok\nurl = bad").is_err());
    }
}
