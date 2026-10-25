#![allow(dead_code)]
#[path = "gated.rs"]
mod gated;
use agw_diff::Side;
use agw_guard::{
    availability::{Budget, EngineFault},
    gated::{Evidence, Pipeline},
};
pub use gated::{Engine, draft};

pub struct ResponseOnly(pub Engine);
impl Pipeline for ResponseOnly {
    fn detect(
        &self,
        side: Side,
        bytes: &[u8],
        budget: &mut Budget,
    ) -> Result<Evidence, EngineFault> {
        if side == Side::Request {
            Ok(Evidence::default())
        } else {
            self.0.detect(side, bytes, budget)
        }
    }
    fn verify(
        &self,
        side: Side,
        bytes: &[u8],
        findings: &[usize],
        budget: &mut Budget,
    ) -> Result<bool, EngineFault> {
        self.0.verify(side, bytes, findings, budget)
    }
}
