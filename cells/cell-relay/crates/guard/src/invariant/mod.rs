//! TI-22: frozen local invariant selections in the explicit native guard.
pub(crate) mod activation;
mod audit;
use crate::{availability::EngineFault, statistical::InvalidPlan};
use agw_inspector::{Inspection, Number, Parse, Value};
use agw_invariants::{Expected, Scalar, Verdict, store::Snapshot};

/// Snapshot plus administratively decoded object-only JSON Pointers.
#[derive(Debug, Clone)]
pub struct Selection {
    pub(crate) snapshot: Snapshot,
    paths: Vec<Vec<String>>,
    pub(crate) activation: Option<std::sync::Arc<agw_invariants::activation::Generation>>,
}
impl Selection {
    pub fn new(snapshot: Snapshot) -> Result<Self, InvalidPlan> {
        let mut paths = Vec::new();
        for binding in snapshot.bindings() {
            paths.push(decode_path(binding)?);
        }
        Ok(Self {
            snapshot,
            paths,
            activation: None,
        })
    }
    /// Only a controller-published generation may supply staged authority.
    pub fn activated(
        generation: std::sync::Arc<agw_invariants::activation::Generation>,
    ) -> Result<Self, InvalidPlan> {
        let mut selection = Self::new(generation.snapshot().clone())?;
        selection.activation = Some(generation);
        Ok(selection)
    }
    pub(crate) fn mode(
        &self,
        mode: crate::config::Mode,
        operation: crate::statistical::Operation,
    ) -> crate::config::Mode {
        stage_mode(self.activation.as_deref(), mode, operation)
    }
    pub(crate) fn allowance(&self) -> usize {
        // Includes worst-case object traversal, bounded vocabulary formatting,
        // generated drafts and duplicated metadata/change evidence.
        self.paths.len() * 4096 * 32
            + self.snapshot.program.rules().len()
                * (agw_invariants::MAX_TEXT * agw_invariants::MAX_ENUM * 64 + 4096)
    }
}
pub(crate) fn stage_mode(
    generation: Option<&agw_invariants::activation::Generation>,
    mode: crate::config::Mode,
    operation: crate::statistical::Operation,
) -> crate::config::Mode {
    use agw_invariants::activation::{RolloutMode, Stage};
    if let Some(g) = generation {
        let requested = match mode {
            crate::config::Mode::Observe => RolloutMode::Observe,
            crate::config::Mode::Repair => RolloutMode::Repair,
            crate::config::Mode::Strict => RolloutMode::Strict,
        };
        if !g
            .decision()
            .authorization
            .as_ref()
            .is_some_and(|p| p.modes.contains(&requested))
        {
            return crate::config::Mode::Observe;
        }
    }
    match generation.map(|g| g.stage()) {
        Some(Stage::Shadow) => crate::config::Mode::Observe,
        Some(Stage::Reads) if operation != crate::statistical::Operation::Read => {
            crate::config::Mode::Observe
        }
        _ => mode,
    }
}
pub(crate) fn decode_path(binding: &str) -> Result<Vec<String>, InvalidPlan> {
    if !binding.starts_with('/') || binding.len() > agw_invariants::MAX_TEXT {
        return Err(InvalidPlan);
    }
    let mut path = Vec::new();
    for part in binding[1..].split('/') {
        let mut key = String::new();
        let mut chars = part.chars();
        while let Some(c) = chars.next() {
            key.push(if c == '~' {
                match chars.next() {
                    Some('0') => '~',
                    Some('1') => '/',
                    _ => return Err(InvalidPlan),
                }
            } else {
                c
            });
        }
        path.push(key);
    }
    if path.len() > 32 {
        return Err(InvalidPlan);
    }
    Ok(path)
}
#[derive(Debug, Clone)]
pub struct Record {
    pub id: String,
    pub verdict: Verdict,
    pub actual: Option<String>,
    pub expected: Option<String>,
}
impl Record {
    pub fn class(&self) -> Option<agw_diff::DriftClass> {
        matches!(self.verdict, Verdict::Violation | Verdict::EnumAddition)
            .then_some(agw_diff::DriftClass::Semantic)
    }
}

pub(crate) fn scalar<'a>(root: &'a Value, path: &[String]) -> Scalar<'a> {
    let mut node = root;
    for key in path {
        let Value::Object(entries) = node else {
            return Scalar::Missing;
        };
        let Some((_, value)) = entries.iter().find(|(name, _)| name == key) else {
            return Scalar::Missing;
        };
        node = value;
    }
    match node {
        Value::Number(Number::I64(n)) => Scalar::Integer(*n),
        Value::String(s) if s.len() <= agw_invariants::MAX_TEXT => Scalar::Text(s),
        _ => Scalar::Missing,
    }
}
pub(crate) fn evaluate(
    selection: &Selection,
    inspection: &Inspection,
) -> Result<Vec<Record>, EngineFault> {
    let Parse::Parsed(root) = &inspection.parse else {
        return Err(EngineFault::Internal);
    };
    let mut slots = [Scalar::Missing; agw_invariants::MAX_BINDINGS];
    for (slot, path) in slots.iter_mut().zip(&selection.paths) {
        *slot = scalar(root, path);
    }
    selection
        .snapshot
        .program
        .rules()
        .iter()
        .map(|rule| {
            let verdict = rule.evaluate(&slots[..selection.paths.len()]);
            let evidence = rule.evidence(&slots[..selection.paths.len()]);
            if verdict != Verdict::Unavailable && evidence.is_none() {
                return Err(EngineFault::Internal);
            }
            let (actual, expected) = evidence.map_or((None, None), |e| {
                let actual = match e.actual {
                    Scalar::Integer(n) => n.to_string(),
                    Scalar::Text(s) => s.into(),
                    Scalar::Missing => String::new(),
                };
                let expected = match e.expected {
                    Expected::Numeric {
                        value,
                        relation,
                        tolerance,
                    } => format!("{relation:?} {value}; absolute tolerance {tolerance}"),
                    Expected::Range { min, max } => format!("inclusive [{min}, {max}]"),
                    Expected::Vocabulary(values) => {
                        format!("known vocabulary {values:?}; additions are informational")
                    }
                };
                (Some(actual), Some(expected))
            });
            Ok(Record {
                id: rule.id().into(),
                verdict,
                actual,
                expected,
            })
        })
        .collect()
}

pub(crate) use audit::stamp;
