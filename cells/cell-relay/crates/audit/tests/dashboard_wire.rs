//! TII-39: preserve query filter bounds when adapting typify's generated text.
use agw_audit::contracts::dashboard_query::DashboardQuery;

#[test]
fn dashboard_filter_preserves_nonempty_and_maximum_length() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = include_str!("../../../../../contracts/examples/dashboard_query.valid.json");
    let base: serde_json::Value = serde_json::from_str(fixture)?;
    let _: DashboardQuery = serde_json::from_value(base.clone())?;
    for length in [0, 257] {
        let mut value = base.clone();
        value["filters"]["status"] = serde_json::Value::String("a".repeat(length));
        assert!(serde_json::from_value::<DashboardQuery>(value).is_err());
    }
    let mut maximum = base;
    maximum["filters"]["status"] = serde_json::Value::String("a".repeat(256));
    let _: DashboardQuery = serde_json::from_value(maximum)?;
    Ok(())
}

#[test]
fn dashboard_search_is_a_bounded_non_sensitive_identifier() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = include_str!("../../../../../contracts/examples/dashboard_query.valid.json");
    let base: serde_json::Value = serde_json::from_str(fixture)?;
    for search in ["", "user@example.test", "with space"] {
        let mut value = base.clone();
        value["filters"]["search"] = serde_json::Value::String(search.into());
        assert!(serde_json::from_value::<DashboardQuery>(value).is_err());
    }
    let mut value = base;
    value["filters"]["search"] = serde_json::Value::String("aud_1:part.2".into());
    let _: DashboardQuery = serde_json::from_value(value)?;
    Ok(())
}
