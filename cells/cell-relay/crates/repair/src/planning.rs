//! Pure confidence/identity/type gates. No payload bytes are changed here.
use crate::path::destination;
use crate::{Assessment, Candidate, JsonType, Mode, Operation, Proposal, Reject, Status, check};
use agw_diff::{Change, DriftClass as C, Kind, Reason};

pub fn evaluate(
    change: &Change,
    candidate: &Candidate,
    from: JsonType,
    to: JsonType,
    mode: Mode,
) -> Assessment {
    let confidence = candidate.evidence.confidence();
    // TI-66: caller-explicit null normalization is the only nonnumeric type
    // exception. Staging additionally requires the actual source to be empty.
    let null_normalization = from == JsonType::String
        && to == JsonType::Null
        && change.class == C::Semantic
        && change.kind == Kind::NullMeaningChanged
        && change.reason == Reason::MeaningChanged;
    let mut class = change.class;
    let rejection = if matches!(candidate.operation, Operation::Replace(_))
        && check(&Proposal {
            path: &change.path,
            from,
            to,
        })
        .is_err()
    {
        class = C::Semantic;
        Some(Reject::NumericCoercion)
    } else if confidence.is_none() {
        Some(Reject::InvalidEvidence)
    } else if class == C::Unknown {
        Some(Reject::UnknownDrift)
    } else {
        match &candidate.operation {
            Operation::Rename { to }
                if change.kind != Kind::Renamed
                    || class != C::Shape
                    || change.reason != Reason::ConfirmedIdentity
                    || change.to_path.is_none()
                    || change.path == "$"
                    || destination(&change.path, to).as_ref() != change.to_path.as_ref() =>
            {
                Some(Reject::UnconfirmedIdentity)
            }
            Operation::Replace(_)
                if (from != to && !null_normalization)
                    || matches!(from, JsonType::Object | JsonType::Array)
                    || !matches!(
                        change.kind,
                        Kind::TypeChanged
                            | Kind::UnitsChanged
                            | Kind::EnumExpanded
                            | Kind::NullMeaningChanged
                    ) =>
            {
                Some(Reject::Unsupported)
            }
            _ => {
                let threshold: f64 = if mode == Mode::Strict || class == C::Semantic {
                    0.99
                } else {
                    0.97
                };
                if confidence.is_some_and(|score| score < threshold) {
                    Some(Reject::BelowThreshold)
                } else {
                    None
                }
            }
        }
    };
    Assessment {
        finding: candidate.finding,
        confidence,
        class,
        status: rejection.map_or(
            if mode == Mode::Observe {
                Status::Observed
            } else {
                Status::Eligible
            },
            Status::Rejected,
        ),
    }
}
