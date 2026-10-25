//! Process-local recovery only; entries expire by bounded FIFO eviction.
use crate::execution::Completed;
use std::{cell::RefCell, collections::VecDeque};
use worker::{Headers, Response, Result};
struct Entry {
    id: String,
    tenant: String,
    connection: String,
    environment: String,
    tool: String,
    token_digest: [u8; 32],
    status: u16,
    dispatched: bool,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    audit: String,
}
thread_local! { static ARCHIVE:RefCell<VecDeque<Entry>>=const{RefCell::new(VecDeque::new())}; }
pub fn retain(
    done: &Completed,
    scope: &super::identity::Scope<'_>,
    token: &str,
    status: u16,
    headers: &Headers,
    body: &[u8],
) -> Result<()> {
    use sha2::{Digest, Sha256};
    let audit = serde_json::to_string(&done.artifact)?;
    let entry = Entry {
        id: done.artifact.id.to_string(),
        tenant: scope.tenant.into(),
        connection: scope.connection.into(),
        environment: scope.environment.into(),
        tool: scope.tool.into(),
        token_digest: Sha256::digest(token.as_bytes()).into(),
        status,
        dispatched: done.dispatched,
        headers: headers.entries().collect(),
        body: body.to_vec(),
        audit,
    };
    ARCHIVE.with(|cell| {
        let mut entries = cell
            .try_borrow_mut()
            .map_err(|_| super::state::internal())?;
        while entries.len() >= 64
            || entries
                .iter()
                .map(size)
                .sum::<usize>()
                .saturating_add(size(&entry))
                > 16 * 1024 * 1024
        {
            if entries.pop_front().is_none() {
                return Err(super::state::internal());
            }
        }
        entries.push_back(entry);
        Ok(())
    })
}
pub fn read(
    id: &str,
    token: &str,
    outcome: bool,
    env: &worker::Env,
    auth: &super::identity::RequestAuth<'_>,
) -> Result<Response> {
    use sha2::{Digest, Sha256};
    use subtle::ConstantTimeEq;
    let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    ARCHIVE.with(|cell| {
        let entries = cell.try_borrow().map_err(|_| super::state::internal())?;
        let Some(entry) = entries.iter().find(|e| {
            e.id == id && bool::from(e.token_digest.ct_eq(&digest)) && !e.tenant.is_empty()
        }) else {
            return Response::error("Not found", 404);
        };
        let scope = super::identity::Scope {
            tenant: &entry.tenant,
            environment: &entry.environment,
            connection: &entry.connection,
            tool: &entry.tool,
        };
        let now = (js_sys::Date::now() / 1000.0) as u64;
        match super::identity::admit(env, &scope, auth, now) {
            Ok(Some(_)) => {}
            Ok(None) => {
                let accepted = env
                    .secret("GUARD_KEYS")
                    .ok()
                    .and_then(|v| crate::auth::Auth::new(&v.to_string()).ok())
                    .is_some_and(|mut a| {
                        a.admit(token, &format!("{}/{}", entry.connection, entry.tool), now)
                            .is_ok_and(|p| p.tenant == entry.tenant)
                    });
                if !accepted {
                    return Response::error("Not found", 404);
                }
            }
            Err(_) => return Response::error("Not found", 404),
        }
        if outcome && !entry.dispatched {
            return Response::error("Not found", 404);
        }
        if outcome {
            let headers = Headers::from_iter(entry.headers.clone());
            headers.set("x-agindow-retained-outcome", "true")?;
            Ok(Response::from_bytes(entry.body.clone())?
                .with_status(entry.status)
                .with_headers(headers))
        } else {
            let headers = Headers::new();
            headers.set("content-type", "application/json")?;
            Ok(Response::ok(entry.audit.clone())?.with_headers(headers))
        }
    })
}

fn size(entry: &Entry) -> usize {
    entry
        .body
        .len()
        .saturating_add(entry.audit.len())
        .saturating_add(entry.id.len())
        .saturating_add(entry.tenant.len())
        .saturating_add(
            entry
                .headers
                .iter()
                .map(|(key, value)| key.len().saturating_add(value.len()))
                .sum::<usize>(),
        )
}
