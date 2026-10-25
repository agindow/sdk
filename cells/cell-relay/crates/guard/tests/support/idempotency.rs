#![allow(dead_code)]
use agw_guard::idempotency::{Access, Bindings, Limits, Request, Store};
use agw_policy::Operation;
use std::time::Duration;
pub fn request<'a>(connection: &'a str, key: Option<&'a str>) -> Request<'a> {
    Request {
        connection,
        key,
        operation: Operation::Write,
        durability: None,
    }
}
pub fn store<T>(
    entries: usize,
    window: Duration,
) -> std::result::Result<Store<T>, Box<dyn std::error::Error>> {
    Ok(Store::new(
        "tenant",
        Limits { entries, window },
        Bindings::new("Idempotency-Key", None)?,
    )?)
}
pub fn completed<T>(
    value: Access<T>,
) -> std::result::Result<agw_guard::idempotency::Replay<T>, Box<dyn std::error::Error>> {
    match value {
        Access::Completed(value) => Ok(value),
        _ => Err("expected completed outcome".into()),
    }
}

pub fn wait_for_follower<T>(store: &Store<T>) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while store.waiting() == 0 && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    store.waiting() > 0
}
