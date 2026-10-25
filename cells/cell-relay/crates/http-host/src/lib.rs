//! Explicit HTTP hosts; all guard decisions use the shared Rust engine.
pub mod envelope;
pub use envelope::Envelope;
pub mod auth;
pub mod execution;
#[cfg(all(feature = "worker-host", target_arch = "wasm32"))]
pub mod hosted;
pub mod machine;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native;
