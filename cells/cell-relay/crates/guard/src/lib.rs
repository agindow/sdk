//! Request interception and mode dispatch for the in-path engine.
//!
//! The forwarding path landed with TI-05: [`proxy::forward`] streams an
//! upstream response to the client byte-for-byte, claiming only the latency
//! header. It never parses the body.
//!
//! TI-09 adds [`config`]: validated mode sources, live updates, and immutable
//! per-call mode snapshots for forwarding and audit artifacts.
//! TI-24 adds [`gated`]: explicit bounded-capture calls connecting verified
//! selective repair and policy. Streaming/SDK wiring remains separate.
//!
//! Inherited constraints, not renegotiable in this crate:
//! - no synchronous dependency on another cell, a database, or an LLM
//!   (`TECH_STACK.md` §2 — the rule the whole delivery plan is governed by);
//! - propagate with `?`; the workspace lints deny `unwrap`/`expect` in library
//!   code (`AGENTS.md` §3.1).

pub mod availability;
pub mod config;
pub mod cross_endpoint;
pub mod feedback;
pub mod gated;
pub mod idempotency;
pub mod invariant;
pub mod malformed;
pub mod proxy;
pub mod readiness;
pub mod review;
pub mod snapshot;
pub mod statistical;

#[cfg(not(target_arch = "wasm32"))]
pub mod peer;

pub mod credential;
