use super::learning::key;
use agw_inspector::inference::{BaselineStore, LearnError, Limit, Limits, Progress};
use std::num::NonZeroUsize;

pub fn rejects_without_advancing(
    bytes: &[u8],
    limits: Limits,
    limit: Limit,
) -> Result<(), LearnError> {
    let mut store = BaselineStore::new(NonZeroUsize::MIN, limits);
    let key = key();
    store.begin(key.clone(), None)?;
    assert_eq!(
        store.observe(&key, bytes, false),
        Err(LearnError::Limit(limit))
    );
    assert!(store.latest(&key).is_none());
    assert_eq!(
        store.observe(&key, b"0", false)?,
        Progress::Established { version: 1 }
    );
    Ok(())
}
