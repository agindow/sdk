use agw_guard::peer::{PeerPublisher, Recipe, publication_identity};
use agw_invariants::cross_endpoint::{Endpoint, Key, Limits, Scope, Side, Store};
use std::sync::Arc;
fn recipe() -> Recipe {
    Recipe {
        id: "peer-balance".into(),
        revision: 1,
        enabled: true,
        source_url: "https://vendor.example/balance".into(),
        scope: Scope {
            tenant: "tenant-a".into(),
            connection: "connection-a".into(),
            vendor: "vendor".into(),
            workflow: "workflow-a".into(),
            entity: "entity-a".into(),
            revision: "r1".into(),
        },
        key: Key {
            endpoint: Endpoint {
                operation: "balance".into(),
                side: Side::Response,
                field: "/amount".into(),
            },
            concept: "gross".into(),
            unit: "usd-cent".into(),
            contract_version: 1,
        },
        expires_ms: 1792584600000,
    }
}
fn wire(job: &str, sequence: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../contracts/examples/peer_observation.valid.json"
    ))?;
    value["job_id"] = job.into();
    value["sequence"] = sequence.to_string().into();
    value["id"] = publication_identity("tenant-a", job, "peer-balance", 1, sequence)?.into();
    Ok(serde_json::to_vec(&value)?)
}
#[test]
fn publication_is_scoped_exact_idempotent_and_recipe_updates_fence_old_work()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Arc::new(Store::new(Limits::default())?);
    let publisher = PeerPublisher::new(Arc::clone(&store), 4)?;
    let current = recipe();
    let now = 1792584000000;
    publisher.configure(current.clone())?;
    publisher.admit("inv_fixture", "peer-balance", 2, now + 600000, now)?;
    let body = wire("inv_fixture", 2)?;
    assert!(!publisher.publish(&body, now)?.replayed);
    assert!(publisher.publish(&body, now)?.replayed);
    let snapshot = store.snapshot(&current.scope)?;
    assert_eq!(
        snapshot.find(&current.key).and_then(|value| value.value),
        Some(i64::MAX)
    );
    let mut disabled = current.clone();
    disabled.revision = 2;
    disabled.enabled = false;
    publisher.configure(disabled)?;
    assert_eq!(
        store
            .snapshot(&current.scope)?
            .find(&current.key)
            .and_then(|value| value.value),
        None
    );
    assert!(publisher.publish(&body, now).is_err());
    Ok(())
}
#[test]
fn wrong_correlations_conflicts_late_values_and_failed_acquisition_cannot_relabel_success()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Arc::new(Store::new(Limits::default())?);
    let publisher = PeerPublisher::new(Arc::clone(&store), 4)?;
    let current = recipe();
    let now = 1792584000000;
    publisher.configure(current.clone())?;
    for (job, sequence) in [("older", 2), ("newer", 3)] {
        publisher.admit(job, "peer-balance", sequence, now + 600000, now)?;
    }
    publisher.publish(&wire("newer", 3)?, now)?;
    assert!(publisher.publish(&wire("older", 2)?, now).is_err());
    let mut wrong: serde_json::Value = serde_json::from_slice(&wire("newer", 3)?)?;
    wrong["entity"] = "other".into();
    assert!(
        publisher
            .publish(&serde_json::to_vec(&wrong)?, now)
            .is_err()
    );
    publisher.admit("failure", "peer-balance", 4, now + 600000, now)?;
    let mut failed: serde_json::Value = serde_json::from_slice(&wire("failure", 4)?)?;
    failed["value"] = serde_json::Value::Null;
    failed["status"] = "unavailable".into();
    failed["reason"] = "timeout".into();
    publisher.publish(&serde_json::to_vec(&failed)?, now)?;
    assert_eq!(
        store
            .snapshot(&current.scope)?
            .find(&current.key)
            .and_then(|value| value.value),
        None
    );
    assert!(publisher.publish(&wire("newer", 3)?, now + 600001).is_err());
    Ok(())
}
