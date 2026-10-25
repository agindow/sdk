//! TI-44: validate every raw string and member before accepting a live sample.
use agw_inspector::inference::{LearnError, Limit, Limits, Progress};
#[path = "support/learning.rs"]
mod learning;
use learning::{key, store};
#[path = "support/validation.rs"]
mod validation;
use validation::rejects_without_advancing;

#[test]
fn invalid_string_values_do_not_advance_learning() -> Result<(), LearnError> {
    for bytes in [
        br#""\uD800""#.as_slice(),
        br#"{"s":"\uDC00"}"#,
        br#"["\uD800x"]"#,
        br#"{"x":"\uD800","x":0}"#,
    ] {
        let mut store = store(2);
        let key = key();
        store.begin(key.clone(), None)?;
        store.observe(&key, b"null", false)?;
        assert!(matches!(
            store.observe(&key, bytes, false),
            Err(LearnError::Malformed(_))
        ));
        assert!(store.latest(&key).is_none());
        assert_eq!(
            store.observe(&key, b"null", false)?,
            Progress::Established { version: 1 }
        );
    }
    Ok(())
}

#[test]
fn overwritten_members_count_toward_the_node_budget() -> Result<(), LearnError> {
    for bytes in [
        br#"{"x":[1,2,3],"x":0}"#.as_slice(),
        br#"{"x":0,"\u0078":1}"#,
    ] {
        rejects_without_advancing(
            bytes,
            Limits {
                max_nodes: 2,
                ..Limits::default()
            },
            Limit::Nodes,
        )?;
    }
    Ok(())
}

#[test]
fn overwritten_members_count_toward_the_depth_budget() -> Result<(), LearnError> {
    rejects_without_advancing(
        br#"{"x":[[0]],"x":0}"#,
        Limits {
            max_depth: 1,
            ..Limits::default()
        },
        Limit::Depth,
    )
}

#[test]
fn valid_strings_and_overwritten_huge_numbers_remain_accepted() -> Result<(), LearnError> {
    let mut store = store(1);
    let key = key();
    store.begin(key.clone(), None)?;
    assert_eq!(
        store.observe(
            &key,
            br#"{"s":"\uD83D\uDE00\n\"","x":1e400,"\u0078":0}"#,
            false
        )?,
        Progress::Established { version: 1 }
    );
    let baseline = store
        .latest(&key)
        .unwrap_or_else(|| panic!("missing baseline"));
    let paths = baseline.schema.paths();
    assert_eq!(
        paths["x"].types,
        [agw_inspector::inference::JsonType::Integer].into()
    );
    Ok(())
}
