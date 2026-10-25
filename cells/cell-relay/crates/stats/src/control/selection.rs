use super::{Error, text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Request,
    Response,
}
/// Operation identifies the endpoint; every identity dimension is mandatory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope<'a> {
    pub tenant: &'a str,
    pub vendor: &'a str,
    pub operation: &'a str,
    pub side: Side,
    pub tenant_segment: &'a str,
    pub volume_cohort: &'a str,
    pub field: &'a str,
}
impl Scope<'_> {
    pub(super) fn validate(self) -> Result<(), Error> {
        for value in [
            self.tenant,
            self.vendor,
            self.operation,
            self.tenant_segment,
            self.volume_cohort,
            self.field,
        ] {
            text(value)?;
        }
        // JSON Pointer root is represented upstream; non-root fields start '/'.
        if !self.field.starts_with('/') {
            return Err(Error::InvalidInput);
        }
        let mut chars = self.field.chars();
        while let Some(ch) = chars.next() {
            if ch == '~' && !matches!(chars.next(), Some('0' | '1')) {
                return Err(Error::InvalidInput);
            }
        }
        Ok(())
    }
}
/// UTC epoch seconds, with a half-open interval. Data remains caller-owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window<'a> {
    pub scope: Scope<'a>,
    pub start: u64,
    pub end: u64,
    pub complete: bool,
    pub samples: usize,
}
#[derive(Debug, Clone, Copy)]
pub struct Season {
    pub duration: u64,
    pub period: u64,
    pub periods: usize,
    pub min_samples: usize,
}
impl Season {
    fn validate(self) -> Result<(), Error> {
        if self.duration == 0
            || self.period == 0
            || !self.period.is_multiple_of(self.duration)
            || !(1..=52).contains(&self.periods)
            || self.min_samples == 0
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}
/// Exact slots only: no nearest-neighbor, cross-segment or frozen fallback.
/// `None` means unavailable evidence, not a clean detector observation.
pub fn select(
    current: &Window<'_>,
    history: &[Window<'_>],
    season: Season,
) -> Result<Option<Vec<usize>>, Error> {
    if history.len() > 4096 {
        return Err(Error::ResourceLimit);
    }
    season.validate()?;
    current.scope.validate()?;
    if current.end.checked_sub(current.start) != Some(season.duration)
        || !current.start.is_multiple_of(season.duration)
    {
        return Err(Error::InvalidInput);
    }
    if !current.complete || current.samples < season.min_samples {
        return Ok(None);
    }
    let mut selected = Vec::with_capacity(season.periods);
    let mut target = current.start;
    for _ in 0..season.periods {
        let Some(start) = target.checked_sub(season.period) else {
            return Ok(None);
        };
        target = start;
        let mut found = None;
        for (index, candidate) in history.iter().enumerate() {
            if candidate.scope == current.scope && candidate.start == start {
                if found.is_some()
                    || !candidate.complete
                    || candidate.samples < season.min_samples
                    || candidate.end.checked_sub(start) != Some(season.duration)
                {
                    return Ok(None);
                }
                found = Some(index);
            }
        }
        let Some(index) = found else { return Ok(None) };
        selected.push(index);
    }
    Ok(Some(selected))
}
