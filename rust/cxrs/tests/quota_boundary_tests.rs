mod common;

use common::{TempRepo, stderr_str, stdout_str};
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

const OUTSIDE_NOTE: &str = "SYNTHETIC_OUTSIDE_CATALOG_NOTE";
const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;

fn catalog_value(note: &str) -> Value {
    json!({
        "version": 1,
        "updated_at": chrono::Utc::now().to_rfc3339(),
        "sources": [],
        "notes": note,
        "entries": [{
            "backend": "primary",
            "tier": "free",
            "limit_type": "hard",
            "quota_total_tokens": 1234,
            "source_url": "https://example.invalid/quota"
        }]
    })
}

fn write_catalog(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).expect("serialize catalog")).expect("write catalog");
}

fn catalog_show(repo: &TempRepo) -> Value {
    let out = repo.run(&["quota", "catalog", "show", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    serde_json::from_str(&stdout_str(&out)).expect("catalog JSON")
}

fn quota_probe(repo: &TempRepo) -> Value {
    let out = repo.run(&["quota", "probe", "30", "--json"]);
    assert!(out.status.success(), "stderr={}", stderr_str(&out));
    serde_json::from_str(&stdout_str(&out)).expect("quota JSON")
}

fn assert_fallback(repo: &TempRepo) {
    let show = catalog_show(repo);
    assert_eq!(show["version"], 1);
    assert_eq!(show["updated_at"], Value::Null);
    assert_eq!(show["entries"], json!([]));
    assert!(!show.to_string().contains(OUTSIDE_NOTE));

    let probe = quota_probe(repo);
    assert_eq!(probe["quota_source"], "unknown");
    assert_eq!(probe["quota_total_tokens"], Value::Null);
}

#[test]
fn catalog_regular_reuse() {
    let repo = TempRepo::new("quota-boundary");
    let catalog = catalog_value("owned catalog");
    write_catalog(&repo.quota_catalog_file(), &catalog);

    assert_eq!(catalog_show(&repo), catalog);
    let probe = quota_probe(&repo);
    assert_eq!(probe["quota_source"], "catalog:primary:free");
    assert_eq!(probe["quota_total_tokens"], 1234);

    let refresh = repo.run(&[
        "quota",
        "catalog",
        "refresh",
        "--if-stale",
        "--max-age-hours",
        "9999",
    ]);
    assert!(refresh.status.success(), "stderr={}", stderr_str(&refresh));
    assert!(stdout_str(&refresh).contains("refreshed: false"));
    assert_eq!(catalog_show(&repo), catalog);
}

#[test]
fn catalog_oversize_fallback() {
    let repo = TempRepo::new("quota-boundary");
    let path = repo.quota_catalog_file();
    let prefix = b"{\"version\":1,\"updated_at\":\"2026-10-10T00:00:00Z\",\"entries\":[{\"backend\":\"primary\",\"tier\":\"free\",\"quota_total_tokens\":4321}],\"notes\":\"";
    let suffix = b"\"}";
    let target = MAX_JSON_BYTES + 1;
    let mut remaining = target - prefix.len() - suffix.len();
    let chunk = vec![b'x'; 1024 * 1024];
    let mut file = File::create(&path).expect("create oversized catalog");
    file.write_all(prefix).expect("write prefix");
    while remaining > 0 {
        let count = remaining.min(chunk.len());
        file.write_all(&chunk[..count]).expect("write body");
        remaining -= count;
    }
    file.write_all(suffix).expect("write suffix");
    drop(file);
    assert_eq!(
        fs::metadata(&path).expect("catalog metadata").len(),
        target as u64
    );

    assert_fallback(&repo);
    let refresh = repo.run(&[
        "quota",
        "catalog",
        "refresh",
        "--if-stale",
        "--max-age-hours",
        "9999",
    ]);
    assert!(refresh.status.success(), "stderr={}", stderr_str(&refresh));
    assert!(stdout_str(&refresh).contains("refreshed: true"));
    assert!(
        catalog_show(&repo)["entries"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
}

#[test]
fn catalog_malformed_fallback() {
    let repo = TempRepo::new("quota-boundary");
    fs::write(repo.quota_catalog_file(), b"{\"entries\":[").expect("write malformed catalog");
    assert_fallback(&repo);

    let refresh = repo.run(&["quota", "catalog", "refresh", "--if-stale"]);
    assert!(refresh.status.success(), "stderr={}", stderr_str(&refresh));
    assert!(stdout_str(&refresh).contains("refreshed: true"));
    assert!(
        catalog_show(&repo)["entries"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
}

#[cfg(unix)]
#[test]
fn catalog_leaf_symlink() {
    use std::os::unix::fs::symlink;

    let repo = TempRepo::new("quota-boundary");
    let outside = repo.home.join("outside.json");
    let mut secret = catalog_value(OUTSIDE_NOTE);
    secret["entries"][0]["quota_total_tokens"] = json!(999_999);
    write_catalog(&outside, &secret);
    let original = fs::read(&outside).expect("read outside catalog");
    symlink(&outside, repo.quota_catalog_file()).expect("link catalog leaf");

    assert_fallback(&repo);
    let selected = repo.run(&["llm", "use", "primary"]);
    assert!(
        selected.status.success(),
        "stderr={}",
        stderr_str(&selected)
    );
    assert!(!stdout_str(&selected).contains(OUTSIDE_NOTE));
    assert!(!stderr_str(&selected).contains(OUTSIDE_NOTE));
    let refresh = repo.run(&["quota", "catalog", "refresh", "--if-stale"]);
    assert!(refresh.status.success(), "stderr={}", stderr_str(&refresh));
    assert!(stdout_str(&refresh).contains("refreshed: true"));
    assert!(
        !fs::symlink_metadata(repo.quota_catalog_file())
            .expect("refreshed catalog metadata")
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&outside).expect("read outside catalog"), original);
}

#[cfg(unix)]
#[test]
fn catalog_parent_symlink() {
    use std::os::unix::fs::symlink;

    let repo = TempRepo::new("quota-boundary");
    let outside = repo.home.join("quota_catalog.json");
    write_catalog(&outside, &catalog_value(OUTSIDE_NOTE));
    let original = fs::read(&outside).expect("read outside catalog");
    fs::rename(repo.root.join(".cx"), repo.root.join(".cx_saved"))
        .expect("move owned catalog directory");
    symlink(&repo.home, repo.root.join(".cx")).expect("link catalog parent");

    assert_fallback(&repo);
    let refresh = repo.run(&["quota", "catalog", "refresh", "--if-stale"]);
    assert_eq!(refresh.status.code(), Some(1));
    assert_eq!(fs::read(&outside).expect("read outside catalog"), original);
}

#[cfg(unix)]
#[test]
fn catalog_swap_race() {
    use std::os::unix::fs::symlink;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let repo = TempRepo::new("quota-boundary");
    let path = repo.quota_catalog_file();
    let outside = repo.home.join("outside.json");
    write_catalog(&outside, &catalog_value(OUTSIDE_NOTE));
    let regular =
        serde_json::to_vec(&catalog_value("owned catalog")).expect("serialize regular catalog");
    fs::write(&path, &regular).expect("write regular catalog");
    let race_path = path.clone();
    let race_outside = outside.clone();
    let running = Arc::new(AtomicBool::new(true));
    let race_running = Arc::clone(&running);
    let swapper = std::thread::spawn(move || {
        let mut i = 0;
        while race_running.load(Ordering::Relaxed) {
            let staged = race_path.with_extension("swap");
            if i % 2 == 0 {
                symlink(&race_outside, &staged).expect("stage symlink");
            } else {
                fs::write(&staged, &regular).expect("stage regular catalog");
            }
            fs::rename(&staged, &race_path).expect("swap catalog");
            i += 1;
            std::thread::yield_now();
        }
    });

    for _ in 0..30 {
        let show = catalog_show(&repo);
        assert!(!show.to_string().contains(OUTSIDE_NOTE));
    }
    running.store(false, Ordering::Relaxed);
    swapper.join().expect("catalog swapper");
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&outside).expect("read outside catalog"))
            .expect("outside JSON")["notes"],
        OUTSIDE_NOTE
    );
}
