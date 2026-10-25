//! The closed `notification` v1 envelope and its consolidation identity.
//!
//! One eligible TI-19 alert becomes exactly one logical notification. The ID is
//! derived from `(tenant, connection, day, mode, ordinal)`, so a consolidated
//! summary keeps its identity across updates and a retry can never mint a
//! second notification. References only: audit IDs, never payload copies.

use crate::{Error, identifier, suspicious};
use agw_audit::contracts::notification::{
    Consolidation, ConsolidationMode, Finding, FindingClass, FindingSide, Notification,
    NotificationAuditRefsItem, NotificationContract, NotificationId, NotificationSeverity,
    NotificationTenant, Occurrences, Scope, ScopeConnection, ScopeOperation, ScopeVendor,
};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::num::NonZeroU64;

/// One serialized envelope, including JSON framing.
pub const NOTIFICATION_BYTES: usize = 8 * 1024;
/// Bounded audit references per logical notification.
pub const AUDIT_REFS: usize = 32;
/// Hard ceiling for occurrence and consolidation counters.
pub const MAX_COUNT: u64 = 1_000_000;
/// Hard ceiling for the UTC epoch day.
pub const MAX_DAY: u64 = 100_000;

/// Everything one eligible alert contributes to the channel-neutral envelope.
#[derive(Debug, Clone, Copy)]
pub struct Alert<'a> {
    pub tenant: &'a str,
    pub connection: &'a str,
    pub vendor: &'a str,
    pub operation: &'a str,
    pub class: FindingClass,
    pub side: FindingSide,
    pub severity: NotificationSeverity,
    /// Total observed occurrences across the represented findings.
    pub occurrences: u64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub audit_refs: &'a [&'a str],
    /// UTC epoch day of the owning TI-19 budget.
    pub day: u64,
    /// TI-19 notification position for the day; a summary keeps the cap slot.
    pub ordinal: u64,
    pub mode: ConsolidationMode,
    /// Findings represented: 1 for an individual alert.
    pub consolidated: u64,
    /// UTC emission time of the logical notification.
    pub ts: DateTime<Utc>,
}

fn check_text(value: &str) -> Result<(), Error> {
    identifier(value)?;
    if suspicious(value) {
        return Err(Error::Invalid("credential-shaped or payload content"));
    }
    Ok(())
}

fn count(value: u64) -> Result<NonZeroU64, Error> {
    if !(1..=MAX_COUNT).contains(&value) {
        return Err(Error::Invalid("notification count out of bounds"));
    }
    NonZeroU64::new(value).ok_or(Error::Invalid("notification count out of bounds"))
}

fn mode(value: ConsolidationMode) -> &'static str {
    match value {
        ConsolidationMode::Individual => "individual",
        ConsolidationMode::Summary => "summary",
    }
}

/// Stable notification identity: `ntf_` + the first 16 SHA-256 bytes.
pub fn notification_id(alert: &Alert<'_>) -> Result<String, Error> {
    check_text(alert.tenant)?;
    check_text(alert.connection)?;
    if alert.day > MAX_DAY || alert.ordinal < 1 || alert.ordinal > MAX_COUNT {
        return Err(Error::Invalid("invalid consolidation identity"));
    }
    let mut digest = Sha256::new();
    digest.update(b"agindow.notification/1");
    let day = alert.day.to_string();
    let ordinal = alert.ordinal.to_string();
    for part in [
        alert.tenant,
        alert.connection,
        day.as_str(),
        mode(alert.mode),
        ordinal.as_str(),
    ] {
        digest.update([0]);
        digest.update(part.as_bytes());
    }
    let mut id = String::from("ntf_");
    for byte in digest.finalize().iter().take(16) {
        write!(id, "{byte:02x}").map_err(|_| Error::Invalid("notification id"))?;
    }
    Ok(id)
}

/// Build and validate one envelope. Nothing is emitted or delivered here.
pub fn build(alert: Alert<'_>) -> Result<Notification, Error> {
    check_text(alert.tenant)?;
    check_text(alert.connection)?;
    check_text(alert.vendor)?;
    check_text(alert.operation)?;
    if alert.audit_refs.len() > AUDIT_REFS {
        return Err(Error::Invalid("too many audit references"));
    }
    for reference in alert.audit_refs {
        check_text(reference)?;
    }
    if alert.first_seen > alert.last_seen || alert.last_seen > alert.ts {
        return Err(Error::Invalid("notification times are not ordered"));
    }
    let id = notification_id(&alert)?;
    let value = Notification {
        audit_refs: alert
            .audit_refs
            .iter()
            .map(|reference| {
                NotificationAuditRefsItem::try_from(*reference)
                    .map_err(|_| Error::Invalid("invalid audit reference"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        consolidation: Consolidation {
            count: count(alert.consolidated)?,
            day: i64::try_from(alert.day).map_err(|_| Error::Invalid("day out of bounds"))?,
            mode: alert.mode,
            ordinal: count(alert.ordinal)?,
        },
        contract: NotificationContract::Notification,
        finding: Finding {
            class: alert.class,
            side: alert.side,
        },
        id: NotificationId::try_from(id.as_str())
            .map_err(|_| Error::Invalid("invalid notification id"))?,
        occurrences: Occurrences {
            count: count(alert.occurrences)?,
            first_seen: alert.first_seen,
            last_seen: alert.last_seen,
        },
        scope: Scope {
            connection: ScopeConnection::try_from(alert.connection)
                .map_err(|_| Error::Invalid("invalid connection"))?,
            operation: ScopeOperation::try_from(alert.operation)
                .map_err(|_| Error::Invalid("invalid operation"))?,
            vendor: ScopeVendor::try_from(alert.vendor)
                .map_err(|_| Error::Invalid("invalid vendor"))?,
        },
        severity: alert.severity,
        tenant: NotificationTenant::try_from(alert.tenant)
            .map_err(|_| Error::Invalid("invalid tenant"))?,
        ts: alert.ts,
        version: 1,
    };
    validate(&value)?;
    Ok(value)
}

/// Update a retained summary in place. The identity and scope never change;
/// counters only grow and `ts` advances to the material update time.
pub fn update_summary(
    notification: &mut Notification,
    occurrences: u64,
    last_seen: DateTime<Utc>,
    ts: DateTime<Utc>,
    consolidated: u64,
) -> Result<(), Error> {
    if notification.consolidation.mode != ConsolidationMode::Summary {
        return Err(Error::Invalid("only a consolidated summary can be updated"));
    }
    let next = count(occurrences)?;
    let next_consolidated = count(consolidated)?;
    if next < notification.occurrences.count
        || next_consolidated < notification.consolidation.count
        || last_seen < notification.occurrences.last_seen
        || ts < notification.ts
    {
        return Err(Error::Invalid("summary updates must advance"));
    }
    notification.occurrences.count = next;
    notification.occurrences.last_seen = last_seen;
    notification.consolidation.count = next_consolidated;
    notification.ts = ts;
    validate(notification)
}

/// Cross-field rules the generated type cannot express.
pub fn validate(value: &Notification) -> Result<(), Error> {
    check_text(value.tenant.as_str())?;
    check_text(value.scope.connection.as_str())?;
    check_text(value.scope.vendor.as_str())?;
    check_text(value.scope.operation.as_str())?;
    let id = value.id.as_str();
    if !id.starts_with("ntf_")
        || id.len() != 36
        || !id[4..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(Error::Invalid("invalid notification id"));
    }
    if value.version != 1 {
        return Err(Error::Invalid("unsupported notification version"));
    }
    if value.occurrences.first_seen > value.occurrences.last_seen
        || value.occurrences.last_seen > value.ts
    {
        return Err(Error::Invalid("notification times are not ordered"));
    }
    if value.audit_refs.len() > AUDIT_REFS {
        return Err(Error::Invalid("too many audit references"));
    }
    for reference in &value.audit_refs {
        check_text(reference.as_str())?;
    }
    for number in [
        value.occurrences.count.get(),
        value.consolidation.count.get(),
        value.consolidation.ordinal.get(),
    ] {
        if number > MAX_COUNT {
            return Err(Error::Invalid("notification count out of bounds"));
        }
    }
    let day = value.consolidation.day;
    if !(0..=i64::try_from(MAX_DAY).map_err(|_| Error::Invalid("day out of bounds"))?)
        .contains(&day)
    {
        return Err(Error::Invalid("day out of bounds"));
    }
    if value.occurrences.count < value.consolidation.count {
        return Err(Error::Invalid("summary exceeds occurrences"));
    }
    if value.consolidation.mode == ConsolidationMode::Individual {
        if value.consolidation.count.get() != 1 {
            return Err(Error::Invalid(
                "an individual notification consolidates exactly one finding",
            ));
        }
        if value.audit_refs.is_empty() {
            return Err(Error::Invalid(
                "an individual notification requires audit evidence",
            ));
        }
    }
    if canonical(value)?.len() > NOTIFICATION_BYTES {
        return Err(Error::Invalid("notification exceeds the byte limit"));
    }
    Ok(())
}

/// The exact serialized bytes compared for retry identity.
pub fn canonical(value: &Notification) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(value).map_err(|_| Error::Invalid("notification does not serialize"))
}
