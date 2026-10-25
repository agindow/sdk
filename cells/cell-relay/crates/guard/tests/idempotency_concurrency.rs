#[path = "support/idempotency.rs"]
mod support;
use agw_guard::idempotency::Access;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};
use support::{completed, request, store, wait_for_follower};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn identical_keys_share_one_execution_while_other_keys_proceed() -> Result {
    let cache = store(4, Duration::from_secs(60))?;
    let calls = AtomicUsize::new(0);
    std::thread::scope(|scope| -> Result {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let cache_ref = &cache;
        let calls_ref = &calls;
        let first = scope.spawn(move || {
            cache_ref.execute(request("conn", Some("key")), |_| {
                calls_ref.fetch_add(1, Ordering::SeqCst);
                assert!(entered_tx.send(()).is_ok());
                assert!(release_rx.recv().is_ok());
                Box::new(17)
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(2))?;
        let second = scope.spawn(|| {
            cache.execute(request("conn", Some("key")), |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Box::new(99)
            })
        });
        let (other_tx, other_rx) = mpsc::channel();
        let cache_ref = &cache;
        let other = scope.spawn(move || {
            let result = cache_ref.execute(request("conn", Some("different")), |_| Box::new(31));
            assert!(other_tx.send(result.is_ok()).is_ok());
        });
        let independent = other_rx.recv_timeout(Duration::from_secs(2));
        let waiting = wait_for_follower(&cache);
        release_tx.send(())?;
        assert!(independent?);
        assert!(waiting);
        assert!(other.join().is_ok());
        let one = completed(first.join().map_err(|_| "winner panicked")??)?;
        let two = completed(second.join().map_err(|_| "follower panicked")??)?;
        assert!(Arc::ptr_eq(&one.value, &two.value));
        assert_eq!(**two.value, 17);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        Ok(())
    })
}

#[test]
fn running_writes_never_expire_or_get_evicted() -> Result {
    let cache = store(1, Duration::from_millis(5))?;
    std::thread::scope(|scope| -> Result {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let cache_ref = &cache;
        let first = scope.spawn(move || {
            cache_ref.execute(request("conn", Some("key")), |_| {
                assert!(entered_tx.send(()).is_ok());
                assert!(release_rx.recv().is_ok());
                17
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(2))?;
        std::thread::sleep(Duration::from_millis(10));
        let capacity = cache.execute(request("conn", Some("other")), |_| 99);
        let second = scope.spawn(|| cache.execute(request("conn", Some("key")), |_| 99));
        let waiting = wait_for_follower(&cache);
        release_tx.send(())?;
        assert!(waiting);
        assert_eq!(
            capacity
                .err()
                .ok_or("expected capacity rejection")?
                .http_status(),
            429
        );
        let one = completed(first.join().map_err(|_| "winner panicked")??)?;
        let two = completed(second.join().map_err(|_| "follower panicked")??)?;
        assert!(Arc::ptr_eq(&one.value, &two.value));
        Ok(())
    })
}

#[test]
fn caller_panic_propagates_and_wakes_followers_without_another_write() -> Result {
    let cache = store::<()>(1, Duration::from_secs(60))?;
    let payload = Arc::new(String::from("original caller panic"));
    std::thread::scope(|scope| -> Result {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let cache_ref = &cache;
        let panic_payload = Arc::clone(&payload);
        let first = scope.spawn(move || {
            std::panic::catch_unwind(|| {
                cache_ref.execute(request("conn", Some("key")), |_| {
                    assert!(entered_tx.send(()).is_ok());
                    assert!(release_rx.recv().is_ok());
                    std::panic::panic_any(Arc::clone(&panic_payload));
                })
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(2))?;
        let second = scope.spawn(|| {
            cache.execute(request("conn", Some("key")), |_| {
                panic!("must not repeat uncertain write")
            })
        });
        let waiting = wait_for_follower(&cache);
        release_tx.send(())?;
        assert!(waiting);
        let panic = first
            .join()
            .map_err(|_| "panic escaped catch boundary")?
            .err()
            .ok_or("expected caller panic")?;
        assert!(
            panic
                .downcast_ref::<Arc<String>>()
                .is_some_and(|value| Arc::ptr_eq(value, &payload))
        );
        assert!(matches!(
            second.join().map_err(|_| "follower panicked")??,
            Access::Indeterminate
        ));
        assert!(matches!(
            cache.execute(request("conn", Some("key")), |_| panic!(
                "must stay indeterminate"
            ))?,
            Access::Indeterminate
        ));
        Ok(())
    })
}

#[test]
fn same_thread_reentry_is_explicit_and_does_not_deadlock() -> Result {
    let cache = store(2, Duration::from_secs(60))?;
    let result = completed(cache.execute(request("conn", Some("key")), |_| {
        assert!(matches!(
            cache.execute(request("conn", Some("key")), |_| panic!(
                "recursive duplicate"
            )),
            Ok(Access::InProgress)
        ));
        17
    })?)?;
    assert_eq!(*result.value, 17);
    Ok(())
}
