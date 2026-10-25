//! Bounded standalone statistical evidence, significance and alert controls (TI-15–TI-50).
//!
//! TI-19 selects local seasonal windows; callers own snapshots. No traffic collection,
//! I/O, policy execution or automatic repair. TI-45 supplies explicit local windows
//! and immutable snapshots. See runtime contract §10.19–10.26.
pub mod control;
pub mod enumeration;
pub mod field;
pub mod router;
pub mod significance;
pub mod unit;
pub mod windows;

pub mod export;

pub mod refresh;

#[cfg(not(target_arch = "wasm32"))]
pub mod investigation;
