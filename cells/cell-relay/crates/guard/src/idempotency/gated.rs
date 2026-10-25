use super::{Access, Forwarding, Rejection, Request, Store};
use crate::{
    feedback::Feedback,
    gated::{Body, Call, Outcome, Pipeline},
    proxy::Headers,
};
use agw_audit::contracts::audit_artifact::{AuditArtifact, ProofIdempotencyKey};
use agw_policy::{Decision, Operation};
use std::borrow::Cow;

/// Owned replay record. The upstream outcome is moved intact, not cloned.
/// Hosts own replayable response bytes in `O`; consuming readers cannot be
/// replayed by sharing their exhausted state. Only repaired response bytes need
/// separate storage, never an unmodified inspection capture.
#[derive(Debug)]
pub struct Completion<O> {
    pub outcome: Option<O>,
    pub request: Decision,
    pub response: Option<Decision>,
    pub response_patch: Option<Vec<u8>>,
    pub bypasses: u8,
    pub audit: AuditArtifact,
    pub headers: Headers,
    pub warning: Option<&'static str>,
    pub feedback: Option<Feedback>,
    pub review: Result<Option<crate::review::Evidence>, crate::review::Error>,
}
impl<O> Completion<O> {
    fn from_outcome(mut result: Outcome<'_, O>, key: Option<ProofIdempotencyKey>) -> Self {
        result.audit.proof.idempotency_key = key;
        result.audit.proof.write_integrity = false;
        let mut feedback = Feedback::from_outcome(&result);
        if result.audit.operation != Operation::Read
            && let Some(feedback) = &mut feedback
        {
            feedback.payload.hint.push_str(" This idempotency key is terminal within the replay window; replay returns this same outcome. A new key denotes a new logical write, not an automatic retry.");
        }
        let review = crate::review::capture(&result);
        let response = result.response.as_ref().map(|phase| phase.decision);
        let response_patch = result.response.and_then(|phase| match phase.body {
            Cow::Owned(bytes) => Some(bytes),
            Cow::Borrowed(_) => None,
        });
        Self {
            outcome: result.outcome,
            request: result.request.decision,
            response,
            response_patch,
            bypasses: result.bypasses,
            audit: result.audit,
            headers: result.headers,
            warning: result.warning,
            feedback,
            review,
        }
    }
}
impl Call<'_> {
    /// Explicit TI-27 boundary: validate/reserve before inspection. Replays
    /// return the entire original terminal record and skip all callbacks.
    /// `operation` receives the immutable key/hint binding for every host attempt;
    /// this method never retries. Use an owned bounded response representation.
    pub fn execute_once<'a, O>(
        self,
        store: &Store<Completion<O>>,
        metadata: Request<'_>,
        request: Body<'a>,
        pipeline: &impl Pipeline,
        draft: AuditArtifact,
        operation: impl FnOnce(&[u8], &Forwarding<'_>) -> (O, Body<'a>),
    ) -> Result<Access<Completion<O>>, Rejection> {
        if draft.operation != metadata.operation {
            return Err(Rejection::invalid("operation differs from audit envelope"));
        }
        let proof_key = metadata
            .key
            .map(ProofIdempotencyKey::try_from)
            .transpose()
            .map_err(|_| Rejection::invalid("invalid idempotency_key proof"))?;
        store.execute(metadata, |forwarding| {
            let result = self.execute(request, pipeline, draft, |sent| operation(sent, forwarding));
            let mut completion = Completion::from_outcome(result, proof_key);
            completion.audit.metadata.insert(
                "agindow_idempotency".into(),
                serde_json::json!({
                    "tenant": store.tenant(), "connection": metadata.connection,
                    "durability": forwarding.durability_status,
                    "retention": if metadata.operation == Operation::Read { "not_retained" } else { "process_local" },
                }),
            );
            completion
        })
    }
}
