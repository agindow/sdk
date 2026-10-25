//! Regenerate the cross-language TI-48 fixture; no network or filesystem writes.
#[path = "../tests/support/export.rs"]
mod export;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string_pretty(&export::profiles()?)?);
    Ok(())
}
