//! A real aborting Rust guest: the host must catch the resulting WASM trap.
#![forbid(unsafe_code)]
fn main() {
    panic!("injected WASM engine panic");
}
