//! Fault probes for skipped inspection cleanup (TI-43).
use agw_guard::availability::{EngineFault, RegistrySnapshot};
use agw_guard::config::CallContext;
use agw_guard::readiness::AvailabilityGate;
use std::panic::panic_any;
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn skipping_gate(call: &CallContext, open: bool) -> AvailabilityGate {
    let gate = AvailabilityGate::new(open);
    if open {
        for _ in 0..3 {
            call.guard_with(
                &gate,
                &(),
                RegistrySnapshot::Ready(&()),
                0,
                |_, _, _| Err(EngineFault::Internal),
                || (),
            );
        }
    }
    gate
}

pub struct Cleanup<'a> {
    pub drops: &'a AtomicUsize,
    pub panics: bool,
    pub payload_panics: bool,
}

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
        if self.payload_panics {
            panic_any(BadPayload);
        }
        if self.panics {
            panic!("injected skipped inspection cleanup panic");
        }
    }
}

struct BadPayload;
impl Drop for BadPayload {
    fn drop(&mut self) {
        panic!("injected cleanup panic payload destructor");
    }
}
