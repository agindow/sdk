//! One background refresh against an authorized staging HTTPS bridge. No payload
//! or credential is printed. Selection JSON arrives on stdin, secrets via env.
#[cfg(not(target_arch = "wasm32"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use agw_stats::refresh::{Cache, Limits, Selected, native::HttpsReader};
    use std::{io::Read, time::Instant};
    let endpoint = std::env::var("AGINDOW_PROFILE_ENDPOINT")?;
    let tenant = std::env::var("AGINDOW_PROFILE_TENANT")?;
    let token = std::env::var("AGINDOW_PROFILE_READER_TOKEN")?;
    let mut body = Vec::new();
    std::io::stdin().take(8193).read_to_end(&mut body)?;
    if body.len() > 8192 {
        return Err("selection byte limit".into());
    }
    let selected: Selected = serde_json::from_slice(&body)?;
    let reader = HttpsReader::new(&endpoint, &tenant, &token)?;
    let mut cache = Cache::new(Limits {
        capacity: 1,
        bytes: 262144,
        ttl_ms: 60000,
        refresh_ms: 10000,
        timeout_ms: 5000,
        backoff_ms: 1000,
        recovery_ms: 30000,
        attempts: 3,
    })?;
    let id = cache.select(selected, 0)?;
    let job = cache.begin(0).ok_or("no refresh job")?;
    let started = Instant::now();
    let bytes = reader.fetch(&job);
    let now = u64::try_from(started.elapsed().as_millis())?;
    cache.complete(job, bytes, now)?;
    let snapshot = cache.read(id, now).ok_or("unavailable")?;
    println!(
        "revision={} refresh_ms={} exact_detector_inputs_available={}",
        snapshot.revision(),
        now,
        snapshot.exact_detector_inputs().is_some()
    );
    Ok(())
}
fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    if run().is_err() {
        eprintln!("Snapshot refresh failed; check protected staging configuration and selection.");
        std::process::exit(1);
    }
}
