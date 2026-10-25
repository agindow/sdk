//! Local fixture timing only; never production qualification or an engine budget.
#[path = "../tests/support/identity.rs"]
mod identity;
use agw_http_host::machine::{ProofRequest, Replay, prove, verify_possession};
use std::{hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut keys = identity::cache()?;
    let token = identity::token(&identity::claims())?;
    let request = ProofRequest {
        method: "POST",
        target: "https://agindow.test/v1/guard",
        token: &token,
    };
    let proofs = (0..1000)
        .map(|n| {
            prove(
                &request,
                &format!("latency-{n}"),
                identity::NOW,
                &identity::key(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut replay = Replay::new(1024)?;
    let mut times = Vec::new();
    for proof in &proofs {
        let started = Instant::now();
        let claims = keys.validate(black_box(&token), &identity::context(), identity::NOW)?;
        verify_possession(
            &claims,
            &request,
            Some(proof),
            None,
            &mut replay,
            identity::NOW,
        )?;
        times.push(started.elapsed().as_nanos());
    }
    times.sort_unstable();
    let mean = times.iter().sum::<u128>() as f64 / times.len() as f64 / 1000.0;
    let p99 = *times.get(989).ok_or("sample missing")? as f64 / 1000.0;
    println!(
        "local native access JWT + DPoP validation: mean={mean:.2}us p99={p99:.2}us n=1000; incremental auth cost, not engine latency"
    );
    Ok(())
}
