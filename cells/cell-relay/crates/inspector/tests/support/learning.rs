use agw_inspector::inference::{BaselineStore, Key, Limits};
use std::num::NonZeroUsize;
pub fn store(n: usize) -> BaselineStore {
    let Some(n) = NonZeroUsize::new(n) else {
        panic!("test threshold must be positive")
    };
    BaselineStore::new(n, Limits::default())
}

pub fn key() -> Key {
    Key::new("vendor", "GET /users")
}
