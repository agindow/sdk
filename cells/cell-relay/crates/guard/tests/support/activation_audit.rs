#![allow(dead_code)]
use super::cross;
#[path = "../../../invariants/tests/support/activation.rs"]
mod fixture;
use agw_invariants::activation::{Candidate, Controller, Family, PeerAttestation, Stage};
pub type Error = Box<dyn std::error::Error>;

pub fn controller(paired: bool) -> Result<(Controller, Candidate), Error> {
    let mut candidate = fixture::candidate();
    candidate.scope.tenant = "tenant".into();
    candidate.scope.connection = "conn".into();
    candidate.scope.vendor = "vendor".into();
    candidate.scope.operation = "endpoint".into();
    candidate.scope.contract = 7;
    let mut policy = fixture::policy();
    policy.scope = candidate.scope.clone();
    if paired {
        candidate.rules.clear();
        candidate.bindings.clear();
        let mut peer = cross::rule();
        peer.right.side = agw_invariants::cross_endpoint::Side::Response;
        candidate.peers = vec![peer.clone()];
        policy.families = vec![Family::CrossEndpoint];
        policy.peer_attestations = vec![PeerAttestation {
            rule: peer,
            revision_semantics: "immutable charge revision".into(),
        }];
    }
    let mut controller = Controller::new(candidate.scope.clone(), 32)?;
    controller.configure(Some(policy), Default::default())?;
    publish(&mut controller, &candidate, Stage::Shadow)?;
    Ok((controller, candidate))
}

pub fn publish(
    controller: &mut Controller,
    candidate: &Candidate,
    stage: Stage,
) -> Result<(), Error> {
    let (id, at) = if stage == Stage::Shadow {
        (1, 10)
    } else {
        (2, 20)
    };
    let mut receipt = fixture::receipt(id, stage);
    receipt.scope = candidate.scope.clone();
    controller.record(receipt)?;
    controller.publish_prepared(candidate.clone(), stage, at)?;
    Ok(())
}
