//! Opt-in, purely local machine authority. Hosts refresh keys/registrations off path.
//! Successful token validation alone does not authorize a request: verify possession too.
mod cache;
mod wire;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD as B64};
pub use cache::{Counters, KeyCache, PublicKey, Replay, VerificationKey};
use ed25519_dalek::VerifyingKey;
use sha2::{Digest, Sha256};
pub use wire::{Actor, Claims, Confirmation, Context, sign};
pub const CLOCK_SKEW_SECONDS: u64 = 30;
pub const MAX_TTL_SECONDS: u64 = 900;
pub const MAX_TOKEN_BYTES: usize = 16384;
/// Closed audit reasons contain no credential, assertion or private-key bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Denial {
    Malformed,
    Signature,
    Expiry,
    Audience,
    Tenant,
    Scope,
    Unavailable,
    Proof,
    Replay,
    Capacity,
    Registration,
    Delegation(usize),
    Limited,
}
impl std::fmt::Display for Denial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Denial {}
pub(crate) fn encode(bytes: impl AsRef<[u8]>) -> String {
    B64.encode(bytes)
}
pub(crate) fn decode(value: &str) -> Result<Vec<u8>, Denial> {
    B64.decode(value).map_err(|_| Denial::Malformed)
}
pub fn hash(value: &[u8]) -> String {
    encode(Sha256::digest(value))
}
/// RFC 7638 OKP thumbprint. Only public Ed25519 material is serialized.
pub fn thumbprint(key: &VerifyingKey) -> String {
    hash(
        format!(
            "{{\"crv\":\"Ed25519\",\"kty\":\"OKP\",\"x\":\"{}\"}}",
            encode(key.as_bytes())
        )
        .as_bytes(),
    )
}
mod assertion;
mod exchange;
mod proof;
pub use exchange::{Exchange, ExchangeRequest, Registration, SubjectClaims, sign_subject};
pub use proof::{ProofRequest, canonical_target, prove, verify_possession};
mod delegation;
pub use delegation::DelegateRequest;
mod gate;
pub use gate::MachineGate;
mod owner;
pub use owner::OwnerEpoch;
