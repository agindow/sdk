use super::{
    CLOCK_SKEW_SECONDS, Claims, Confirmation, Denial, Replay, decode, encode, hash, thumbprint,
    wire::{parts, verify},
};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Jwk {
    kty: String,
    crv: String,
    x: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProofHeader {
    alg: String,
    typ: String,
    jwk: Jwk,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    jti: String,
    htm: String,
    htu: String,
    iat: u64,
    ath: String,
}
#[derive(Clone, Copy)]
pub struct ProofRequest<'a> {
    pub method: &'a str,
    pub target: &'a str,
    pub token: &'a str,
}
pub fn canonical_target(target: &str) -> Result<String, Denial> {
    let mut url = url::Url::parse(target).map_err(|_| Denial::Proof)?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Denial::Proof);
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}
/// Generate in the workload boundary; never serialize the private key.
pub fn prove(
    request: &ProofRequest<'_>,
    jti: &str,
    now: u64,
    key: &SigningKey,
) -> Result<String, Denial> {
    let header = ProofHeader {
        alg: "EdDSA".into(),
        typ: "dpop+jwt".into(),
        jwk: Jwk {
            kty: "OKP".into(),
            crv: "Ed25519".into(),
            x: encode(key.verifying_key().as_bytes()),
        },
    };
    let claims = Proof {
        jti: jti.into(),
        htm: request.method.into(),
        htu: canonical_target(request.target)?,
        iat: now,
        ath: hash(request.token.as_bytes()),
    };
    let signed = format!(
        "{}.{}",
        encode(serde_json::to_vec(&header).map_err(|_| Denial::Proof)?),
        encode(serde_json::to_vec(&claims).map_err(|_| Denial::Proof)?)
    );
    Ok(format!(
        "{signed}.{}",
        encode(key.sign(signed.as_bytes()).to_bytes())
    ))
}
/// Call after token validation and before guard evaluation. `tls_peer` is
/// authenticated transport metadata, never a request header or envelope field.
pub fn verify_possession(
    claims: &Claims,
    request: &ProofRequest<'_>,
    proof: Option<&str>,
    tls_peer: Option<&str>,
    replay: &mut Replay,
    now: u64,
) -> Result<(), Denial> {
    match &claims.cnf {
        Confirmation::Certificate(expected) => {
            if tls_peer == Some(expected.as_str()) {
                Ok(())
            } else {
                Err(Denial::Proof)
            }
        }
        Confirmation::Jkt(expected) => {
            let (h, c, s, signed) =
                parts(proof.ok_or(Denial::Proof)?).map_err(|_| Denial::Proof)?;
            let header: ProofHeader =
                serde_json::from_slice(&decode(h).map_err(|_| Denial::Proof)?)
                    .map_err(|_| Denial::Proof)?;
            if header.alg != "EdDSA"
                || header.typ != "dpop+jwt"
                || header.jwk.kty != "OKP"
                || header.jwk.crv != "Ed25519"
            {
                return Err(Denial::Proof);
            }
            let bytes: [u8; 32] = decode(&header.jwk.x)
                .map_err(|_| Denial::Proof)?
                .try_into()
                .map_err(|_| Denial::Proof)?;
            let key = VerifyingKey::from_bytes(&bytes).map_err(|_| Denial::Proof)?;
            if thumbprint(&key) != *expected {
                return Err(Denial::Proof);
            }
            verify(&key, signed, s).map_err(|_| Denial::Proof)?;
            let p: Proof = serde_json::from_slice(&decode(c).map_err(|_| Denial::Proof)?)
                .map_err(|_| Denial::Proof)?;
            if p.htm != request.method
                || p.htu != canonical_target(request.target)?
                || p.ath != hash(request.token.as_bytes())
                || p.iat > now.saturating_add(CLOCK_SKEW_SECONDS)
                || now > p.iat.saturating_add(60 + CLOCK_SKEW_SECONDS)
            {
                return Err(Denial::Proof);
            }
            replay.admit(expected, &p.jti, now.saturating_add(180), now)
        }
    }
}
