use super::{Error, Label};

/// Truth rows, prediction columns. Callers supply exactly-once audit reviews;
/// these counters do not deduplicate or attest the supplied ground truth.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reviews {
    counts: [[u64; 4]; 4],
    reviewed: u64,
}
impl Reviews {
    pub fn from_counts(counts: [[u64; 4]; 4]) -> Result<Self, Error> {
        let reviewed = counts.iter().flatten().try_fold(0u64, |sum, count| {
            sum.checked_add(*count).ok_or(Error::CounterOverflow)
        })?;
        Ok(Self { counts, reviewed })
    }
    pub fn record(&mut self, predicted: Label, truth: Label) -> Result<(), Error> {
        let total = self.reviewed.checked_add(1).ok_or(Error::CounterOverflow)?;
        let cell = &mut self.counts[index(truth)][index(predicted)];
        let count = cell.checked_add(1).ok_or(Error::CounterOverflow)?;
        *cell = count;
        self.reviewed = total;
        Ok(())
    }
    pub fn count(&self, truth: Label, predicted: Label) -> u64 {
        self.counts[index(truth)][index(predicted)]
    }
    pub fn reviewed(&self) -> u64 {
        self.reviewed
    }
    pub fn accuracy(&self) -> Option<f64> {
        if self.reviewed == 0 {
            return None;
        }
        let correct: u64 = (0..4).map(|i| self.counts[i][i]).sum();
        Some(correct as f64 / self.reviewed as f64)
    }
}
fn index(label: Label) -> usize {
    match label {
        Label::Shape => 0,
        Label::Semantic => 1,
        Label::Invariant => 2,
        Label::Unknown => 3,
    }
}
