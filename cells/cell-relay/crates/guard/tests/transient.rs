#[path = "support/statistical.rs"]
mod support;
use agw_diff::Side;
use agw_inspector::inspect;
use agw_stats::windows::Availability;
#[test]
fn current_scalar_changes_only_transient_window_and_rejects_partial_or_duplicate_bodies()
-> Result<(), Box<dyn std::error::Error>> {
    let field = support::field(Side::Request, "1", "2")?;
    let Availability::Ready(snapshot) = field.snapshot else {
        return Err("ready".into());
    };
    let at = snapshot.live().samples()[0].at();
    let original = snapshot.live().samples().len();
    let limits = agw_stats::windows::Limits {
        samples: 128,
        scopes: 1,
        bytes: 1_000_000,
        batch: 1,
        inspection_nodes: 4096,
        inspection_bytes: 65536,
        minimum: 1,
    };
    let transient = snapshot.with_current(&inspect(b"{\"value\":3}", false, None), at, limits)?;
    assert_eq!(transient.samples().len(), 128);
    assert_eq!(transient.unit().ok_or("numeric")?.last(), Some(&3.0));
    assert_eq!(snapshot.live().samples().len(), original);
    assert_eq!(snapshot.live().unit().ok_or("numeric")?.last(), Some(&2.0));
    assert!(
        snapshot
            .with_current(&inspect(b"{}", true, None), at, limits)
            .is_err()
    );
    assert!(
        snapshot
            .with_current(&inspect(b"{\"x\":1,\"x\":2}", false, None), at, limits)
            .is_err()
    );
    Ok(())
}
