mod common;

use common::*;
use std::sync::{Mutex, MutexGuard, OnceLock};

fn suite_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("http mtls test lock")
}

#[test]
fn mtls_cov() {
    let _guard = suite_lock();
    let repo = TempRepo::new("cxrs-it");
    repo.write_mock(
        "curl",
        r#"#!/usr/bin/env bash
seen_cert=0
seen_key=0
while [ $# -gt 0 ]; do
  if [ "$1" = "--cert" ]; then
    seen_cert=1
    shift 2
    continue
  fi
  if [ "$1" = "--key" ]; then
    seen_key=1
    shift 2
    continue
  fi
  shift
done
cat >/dev/null
if [ "$seen_cert" != "1" ] || [ "$seen_key" != "1" ]; then
  echo "missing --cert/--key" >&2
  exit 2
fi
printf '%s\n' '{"text":"http mtls ok"}'
"#,
    );
    let out = repo.run_with_env(
        &["cxo", "echo", "http-mtls"],
        &[
            ("CX_PROVIDER_ADAPTER", "http-curl"),
            ("CX_HTTP_PROVIDER_URL", "https://api.example.test/infer"),
            ("CX_HTTP_CLIENT_CERT", "/tmp/test-client.pem"),
            ("CX_HTTP_CLIENT_KEY", "/tmp/test-client.key"),
        ],
    );
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        stdout_str(&out),
        stderr_str(&out)
    );
    assert_eq!(stdout_str(&out).trim(), "http mtls ok");
}
