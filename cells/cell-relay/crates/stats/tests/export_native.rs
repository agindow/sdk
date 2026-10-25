#![cfg(not(target_arch = "wasm32"))]
#[path = "support/windows.rs"]
mod support;
use agw_stats::{
    export::{
        StageLimits, Staging,
        native::{Limits, Outbox},
        project,
    },
    windows::Store,
};
use chrono::{TimeZone, Utc};
use support::{limits, record, scope, selection};

fn message() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut store = Store::new(limits())?;
    let key = scope();
    record(&mut store, &[selection(key.clone())], br#"{"value":42}"#, 1)?;
    let mut stage = Staging::new(StageLimits {
        windows: 1,
        bytes: 100_000,
    })?;
    stage.close(
        &mut store,
        &key,
        "close-1",
        Utc.timestamp_opt(0, 0).single().ok_or("time")?,
        Utc.timestamp_opt(10, 0).single().ok_or("time")?,
    )?;
    Ok(project(&stage.pop().ok_or("closed")?, "small")?)
}
fn directory() -> Result<std::path::PathBuf, std::io::Error> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "ti48-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&path)?;
    Ok(path)
}
fn quotas() -> Limits {
    Limits {
        windows: 2,
        bytes: 100_000,
        attempts: 2,
        backoff_ms: 100,
    }
}
#[test]
fn bare_relative_spool_path_supports_staging_restart_and_delivery()
-> Result<(), Box<dyn std::error::Error>> {
    const CHILD: &str = "AGINDOW_TEST_RELATIVE_SPOOL_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // Isolate cwd without changing the process-global directory of parallel tests.
        let dir = directory()?;
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "bare_relative_spool_path_supports_staging_restart_and_delivery",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .current_dir(&dir)
            .output()?;
        std::fs::remove_dir_all(dir)?;
        assert!(
            output.status.success(),
            "relative spool child failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }
    let path = std::path::Path::new("spool.json");
    let body = message()?;
    let mut spool = Outbox::open(path, quotas())?;
    spool.stage(&body)?;
    assert_eq!(spool.len(), 1);
    drop(spool);

    let mut spool = Outbox::open(path, quotas())?;
    assert_eq!(spool.len(), 1);
    let mut sends = 0;
    spool.pump(1000, |bytes| {
        sends += 1;
        assert_eq!(bytes, body);
        true
    })?;
    assert_eq!(sends, 1);
    assert!(spool.is_empty());
    assert!(Outbox::open(path, quotas())?.is_empty());
    Ok(())
}
#[test]
fn restart_replays_exact_content_and_preserves_backoff_and_attempts()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = directory()?;
    let path = dir.join("spool.json");
    let body = message()?;
    let mut box_ = Outbox::open(&path, quotas())?;
    box_.stage(&body)?;
    let mut calls = 0;
    box_.pump(1000, |bytes| {
        calls += 1;
        assert_eq!(bytes, body);
        false
    })?;
    drop(box_);
    let mut box_ = Outbox::open(&path, quotas())?;
    box_.pump(1099, |_| {
        calls += 1;
        true
    })?;
    assert_eq!(calls, 1);
    box_.pump(1100, |bytes| {
        calls += 1;
        assert_eq!(bytes, body);
        true
    })?;
    assert_eq!(calls, 2);
    assert_eq!(box_.len(), 0);
    assert_eq!(Outbox::open(&path, quotas())?.len(), 0);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn attempt_exhaustion_and_full_spool_are_explicit_and_durable()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = directory()?;
    let path = dir.join("spool.json");
    let body = message()?;
    let mut box_ = Outbox::open(
        &path,
        Limits {
            windows: 1,
            ..quotas()
        },
    )?;
    box_.stage(&body)?;
    assert!(box_.stage(&body).is_ok()); // identical pending stage is idempotent
    let mut other: serde_json::Value = serde_json::from_slice(&body)?;
    other["window_id"] = "other".into();
    assert!(box_.stage(&serde_json::to_vec(&other)?).is_err());
    assert_eq!(box_.rejected(), 1);
    assert_eq!(box_.losses(), 0);
    box_.pump(0, |_| false)?;
    box_.pump(100, |_| false)?;
    box_.pump(300, |_| panic!("exhausted retries must not send"))?;
    assert_eq!(box_.len(), 0);
    assert_eq!(box_.losses(), 1);
    assert_eq!(Outbox::open(&path, quotas())?.losses(), 1);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn disk_fault_prevents_transport_and_keeps_pending_content()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = directory()?;
    let path = dir.join("spool.json");
    let mut box_ = Outbox::open(&path, quotas())?;
    box_.stage(&message()?)?;
    std::fs::rename(&dir, dir.with_extension("moved"))?;
    assert!(
        box_.pump(0, |_| panic!("no send before durable attempt"))
            .is_err()
    );
    assert_eq!(box_.len(), 1);
    std::fs::rename(dir.with_extension("moved"), &dir)?;
    box_.pump(0, |_| true)?;
    assert_eq!(box_.len(), 0);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn native_transport_rejects_insecure_endpoints_and_header_injection() {
    use agw_stats::export::native::HttpsSender;
    assert!(HttpsSender::new("http://example/profiles", "tenant-a", "secret", 1000).is_err());
    assert!(
        HttpsSender::new(
            "https://example/profiles",
            "tenant-a",
            "bad\r\nheader",
            1000
        )
        .is_err()
    );
    assert!(
        HttpsSender::new(
            "https://example/profiles",
            "tenant-a",
            "a".repeat(32).as_str(),
            0
        )
        .is_err()
    );
}

#[test]
fn background_collection_acknowledges_only_after_durable_staging()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = directory()?;
    let path = dir.join("spool.json");
    let sibling_temp = path.with_extension("ti48.tmp");
    std::fs::write(&sibling_temp, b"other-owner")?;
    let mut box_ = Outbox::open(&path, quotas())?;
    let mut store = Store::new(limits())?;
    let key = scope();
    record(&mut store, &[selection(key.clone())], br#"{"value":42}"#, 1)?;
    let mut stage = Staging::new(StageLimits {
        windows: 1,
        bytes: 100_000,
    })?;
    stage.close(
        &mut store,
        &key,
        "one",
        Utc.timestamp_opt(0, 0).single().ok_or("time")?,
        Utc.timestamp_opt(10, 0).single().ok_or("time")?,
    )?;
    std::fs::rename(&dir, dir.with_extension("moved"))?;
    assert!(box_.collect_one(&mut stage, "small").is_err());
    assert_eq!(stage.len(), 1);
    std::fs::rename(dir.with_extension("moved"), &dir)?;
    box_.collect_one(&mut stage, "small")?;
    assert_eq!(std::fs::read(&sibling_temp)?, b"other-owner");
    assert!(stage.is_empty());
    assert_eq!(box_.len(), 1);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn byte_admission_reserves_retry_metadata_and_rejects_oversized_checkpoints()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = directory()?;
    let path = dir.join("spool.json");
    let body = message()?;
    let mut wide = Outbox::open(&path, quotas())?;
    wide.stage(&body)?;
    let encoded = std::fs::metadata(&path)?.len() as usize;
    drop(wide);
    std::fs::remove_file(&path)?;
    let tight = Limits {
        bytes: encoded + 4,
        ..quotas()
    };
    let mut box_ = Outbox::open(&path, tight)?;
    assert!(
        box_.stage(&body).is_err(),
        "admission must reserve retry metadata growth"
    );
    assert_eq!(box_.len(), 0);
    assert_eq!(box_.rejected(), 1);
    std::fs::write(&path, vec![b' '; tight.bytes + 1])?;
    assert!(Outbox::open(&path, tight).is_err());
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
