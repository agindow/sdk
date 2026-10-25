//! Stage edits against original spans; no source bytes change until all pass.
use crate::spans::{Span, raw_kind};
use crate::{Assessment, Candidate, JsonType, Mode, Operation, Reject, Status, evaluate};
use agw_diff::Change;
use serde_json::value::RawValue;
use std::{ops::Range, sync::Arc};

pub(crate) struct Edit {
    pub range: Range<usize>,
    pub bytes: Arc<[u8]>,
    pub proposal: usize,
    pub field: String,
    pub destination: Option<(usize, String)>,
}

pub(crate) fn stage(
    source: &[u8],
    spans: &[Span],
    change: &Change,
    candidate: &Candidate,
    mode: Mode,
    proposal: usize,
) -> (Assessment, Vec<Edit>) {
    let reject = |reason| {
        (
            Assessment {
                finding: candidate.finding,
                confidence: candidate.evidence.confidence(),
                class: change.class,
                status: Status::Rejected(reason),
            },
            Vec::new(),
        )
    };
    let (replacement, to_type): (Arc<[u8]>, Option<JsonType>) = match &candidate.operation {
        Operation::Rename { to } => match serde_json::to_vec(to) {
            Ok(bytes) => (bytes.into(), None),
            Err(_) => return reject(Reject::InvalidReplacement),
        },
        Operation::Replace(bytes) => {
            let raw: &RawValue = match serde_json::from_slice(bytes) {
                Ok(raw) => raw,
                Err(_) => return reject(Reject::InvalidReplacement),
            };
            if raw.get().len() != bytes.len() {
                return reject(Reject::InvalidReplacement);
            }
            let kind = raw_kind(raw);
            // RawValue accepts unpaired surrogates that inspection rejects.
            // Decode strings for validity, but keep their original patch bytes.
            if kind == JsonType::String && serde_json::from_str::<String>(raw.get()).is_err() {
                return reject(Reject::InvalidReplacement);
            }
            (bytes.as_slice().into(), Some(kind))
        }
    };
    let targets = spans
        .iter()
        .filter(|span| span.schema_path == change.path)
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return reject(Reject::MissingTarget);
    }
    let mut edits = Vec::new();
    let mut assessment = None;
    for target in targets {
        let mut evaluated = evaluate(
            change,
            candidate,
            target.kind,
            to_type.unwrap_or(target.kind),
            mode,
        );
        // A String type alone cannot authorize erasing meaningful text. Check
        // the original raw scalar token before collecting any atomic edits.
        if target.kind == JsonType::String
            && to_type == Some(JsonType::Null)
            && matches!(evaluated.status, Status::Eligible | Status::Observed)
            && source.get(target.token.clone()) != Some(b"\"\"".as_slice())
        {
            evaluated.status = Status::Rejected(Reject::Unsupported);
        }
        // A heterogeneous array can expose a forbidden numeric conversion only
        // on a later element. Inspect all targets before returning a refusal.
        let is_numeric_refusal = evaluated.status == Status::Rejected(Reject::NumericCoercion);
        if assessment.as_ref().is_none_or(|a: &Assessment| {
            !matches!(a.status, Status::Rejected(_)) || is_numeric_refusal
        }) {
            assessment = Some(evaluated.clone());
        }
        if !matches!(evaluated.status, Status::Eligible | Status::Observed) {
            continue;
        }
        let (range, destination) = match &candidate.operation {
            Operation::Rename { to } => {
                let (Some(key), Some(parent), Some(range)) =
                    (&target.key, target.parent, &target.key_token)
                else {
                    return reject(Reject::Unsupported);
                };
                if to == key {
                    continue;
                }
                if spans[parent].keys.contains(to) {
                    return reject(Reject::Collision);
                }
                (range.clone(), Some((parent, to.clone())))
            }
            Operation::Replace(_) => {
                if target.path == "$" {
                    return reject(Reject::Unsupported);
                }
                if source.get(target.token.clone()) == Some(replacement.as_ref()) {
                    continue;
                }
                (target.token.clone(), None)
            }
        };
        edits.push(Edit {
            range,
            bytes: Arc::clone(&replacement),
            proposal,
            field: target.path.clone(),
            destination,
        });
    }
    let mut assessment = match assessment {
        Some(a) => a,
        None => return reject(Reject::MissingTarget),
    };
    if edits.is_empty() && assessment.status == Status::Eligible {
        assessment.status = Status::Noop;
    }
    (assessment, edits)
}
