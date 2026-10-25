use super::{Error, Snapshot};
use crate::{enumeration, field, significance, unit};

/// Attestations about host sampling/comparisons, never inferred from values.
#[derive(Debug, Default, Clone, Copy)]
pub struct Assumptions {
    pub independent_exchangeable_calls: bool,
    pub comparable_populations: bool,
}
/// Available only after explicit host attestations. Reports retain the full
/// declared family and use unchanged TI-50/TI-19 eligibility rules.
pub struct Evidence<'a> {
    pub(super) snapshot: &'a Snapshot,
}
impl Evidence<'_> {
    pub fn completeness(
        &self,
        family: usize,
    ) -> Result<significance::Report<field::Report>, Error> {
        Ok(significance::completeness(
            &self.snapshot.selection.scope.field,
            &self.snapshot.baseline().fields(),
            &self.snapshot.live().fields(),
            family,
        )?)
    }
    pub fn coercion(&self, family: usize) -> Result<significance::Report<field::Report>, Error> {
        Ok(significance::coercion(
            &self.snapshot.selection.scope.field,
            &self.snapshot.baseline().fields(),
            &self.snapshot.live().fields(),
            family,
        )?)
    }
    pub fn charset(&self, family: usize) -> Result<significance::Report<field::Report>, Error> {
        Ok(significance::charset(
            &self.snapshot.selection.scope.field,
            &self.snapshot.baseline().fields(),
            &self.snapshot.live().fields(),
            family,
        )?)
    }
    pub fn enumeration(
        &self,
        family: usize,
    ) -> Result<significance::Report<enumeration::Report>, Error> {
        let baseline = self.snapshot.baseline().enumeration();
        let values: Vec<_> = baseline.iter().map(|o| o.value).collect();
        Ok(significance::enumeration(
            &self.snapshot.selection.scope.field,
            &values,
            &self.snapshot.live().enumeration(),
            family,
        )?)
    }
    pub fn unit(&self, family: usize) -> Result<significance::Report<unit::Detection>, Error> {
        let baseline = self
            .snapshot
            .baseline()
            .unit()
            .ok_or(Error::ProjectionUnavailable)?;
        let live = self
            .snapshot
            .live()
            .unit()
            .ok_or(Error::ProjectionUnavailable)?;
        Ok(significance::unit(
            &self.snapshot.selection.scope.field,
            &baseline,
            &live,
            family,
        )?)
    }
}
