//! TI-10: learning resource limits and transactional rejection.
use agw_inspector::inference::{
    BaselineStore, JsonType, Key, LearnError, Limit, Limits, Progress, SpecPrior,
};
use std::num::NonZeroUsize;
#[path = "support/learning.rs"]
mod learning;
use learning::{key, store};

#[test]
fn rejects_incomplete_or_malformed_captures_without_advancing() -> Result<(), LearnError> {
    let mut store = store(1);
    let key = key();
    store.begin(key.clone(), None)?;
    assert_eq!(store.observe(&key, b"{}", true), Err(LearnError::Truncated));
    for bytes in [b"{bad}".as_slice(), b"", b"{} {}", &[0xff]] {
        assert!(matches!(
            store.observe(&key, bytes, false),
            Err(LearnError::Malformed(_))
        ));
    }
    assert!(store.latest(&key).is_none());
    assert_eq!(
        store.observe(&key, b"null", false)?,
        Progress::Established { version: 1 }
    );
    assert_eq!(
        store.observe(&Key::new("absent", "op"), b"null", false),
        Err(LearnError::NotStarted)
    );
    Ok(())
}

#[test]
fn bounds_each_resource_and_preserves_state_on_rejection() -> Result<(), LearnError> {
    for (limits, body, expected) in [
        (
            Limits {
                max_bytes: 3,
                ..Limits::default()
            },
            b"null".as_slice(),
            Limit::Bytes,
        ),
        (
            Limits {
                max_depth: 1,
                ..Limits::default()
            },
            b"[[0]]",
            Limit::Depth,
        ),
        (
            Limits {
                max_nodes: 2,
                ..Limits::default()
            },
            br#"{"a":0,"b":0}"#,
            Limit::Nodes,
        ),
    ] {
        let mut store = BaselineStore::new(NonZeroUsize::MIN, limits);
        let key = key();
        store.begin(key.clone(), None)?;
        assert_eq!(
            store.observe(&key, body, false),
            Err(LearnError::Limit(expected))
        );
        assert!(store.latest(&key).is_none());
        assert_eq!(
            store.observe(&key, b"0", false)?,
            Progress::Established { version: 1 }
        );
    }
    let mut store = BaselineStore::new(
        NonZeroUsize::MIN,
        Limits {
            max_keys: 1,
            max_versions: 1,
            ..Limits::default()
        },
    );
    let key = key();
    store.begin(key.clone(), None)?;
    store.observe(&key, b"0", false)?;
    let original = store.latest(&key).cloned();
    assert_eq!(
        store.begin(Key::new("other", "op"), None),
        Err(LearnError::Limit(Limit::Keys))
    );
    assert_eq!(
        store.begin(key.clone(), None),
        Err(LearnError::Limit(Limit::Versions))
    );
    assert_eq!(store.latest(&key).cloned(), original);
    Ok(())
}

#[test]
fn node_limit_applies_to_merged_history_and_prior_before_mutation() -> Result<(), LearnError> {
    let Some(n) = NonZeroUsize::new(2) else {
        panic!("positive")
    };
    let mut store = BaselineStore::new(
        n,
        Limits {
            max_nodes: 2,
            ..Limits::default()
        },
    );
    let key = key();
    let prior = SpecPrior::object([
        ("a".into(), SpecPrior::scalar(JsonType::String)),
        ("b".into(), SpecPrior::scalar(JsonType::String)),
    ]);
    assert_eq!(
        store.begin(key.clone(), Some(prior)),
        Err(LearnError::Limit(Limit::Nodes))
    );
    assert_eq!(
        store.begin(key.clone(), None)?,
        1,
        "failed start consumes no version"
    );
    store.observe(&key, br#"{"a":0}"#, false)?;
    assert_eq!(
        store.observe(&key, br#"{"b":0}"#, false),
        Err(LearnError::Limit(Limit::Nodes))
    );
    assert_eq!(
        store.observe(&key, br#"{"a":1}"#, false)?,
        Progress::Established { version: 1 }
    );
    let Some(b) = store.latest(&key) else {
        panic!("missing")
    };
    assert!(!b.schema.paths().contains_key("b"));
    Ok(())
}

#[test]
fn prior_depth_and_zero_capacity_fail_without_consuming_versions() -> Result<(), LearnError> {
    let mut store = BaselineStore::new(
        NonZeroUsize::MIN,
        Limits {
            max_depth: 0,
            ..Limits::default()
        },
    );
    let key = key();
    assert_eq!(
        store.begin(
            key.clone(),
            Some(SpecPrior::array(SpecPrior::scalar(JsonType::Null)))
        ),
        Err(LearnError::Limit(Limit::Depth))
    );
    assert_eq!(store.begin(key.clone(), None)?, 1);
    assert_eq!(
        store.observe(&key, b"[0]", false),
        Err(LearnError::Limit(Limit::Depth))
    );
    store.observe(&key, b"{}", false)?;
    for (limits, expected) in [
        (
            Limits {
                max_keys: 0,
                ..Limits::default()
            },
            Limit::Keys,
        ),
        (
            Limits {
                max_versions: 0,
                ..Limits::default()
            },
            Limit::Versions,
        ),
    ] {
        let mut store = BaselineStore::new(NonZeroUsize::MIN, limits);
        assert_eq!(
            store.begin(key.clone(), None),
            Err(LearnError::Limit(expected))
        );
        assert!(store.latest(&key).is_none());
    }
    Ok(())
}

#[test]
fn node_budget_also_limits_repeated_array_work() -> Result<(), LearnError> {
    let mut store = BaselineStore::new(
        NonZeroUsize::MIN,
        Limits {
            max_nodes: 3,
            ..Limits::default()
        },
    );
    let key = key();
    store.begin(key.clone(), None)?;
    assert_eq!(
        store.observe(&key, b"[1,2,3]", false),
        Err(LearnError::Limit(Limit::Nodes))
    );
    store.observe(&key, b"[1,2]", false)?;
    let Some(b) = store.latest(&key) else {
        panic!("missing")
    };
    assert_eq!(b.schema.paths()["$[]"].observations, 1);
    Ok(())
}
