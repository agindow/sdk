//! Native-only, single-owner background spool. Never invoke from a guarded call.
//! All filesystem and sender failures stay here; source staging is acknowledged
//! only after `stage` succeeds. Acks lost after send replay identical bytes.
mod transport;
use agw_audit::contracts::field_profile::FieldProfile;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
pub use transport::HttpsSender;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub windows: usize,
    pub bytes: usize,
    pub attempts: u32,
    pub backoff_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    body: String,
    attempts: u32,
    next: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u8,
    entries: Vec<Entry>,
    losses: u64,
    rejected: u64,
}
impl State {
    // Admission must leave room for retry deadlines and growing telemetry;
    // otherwise a full spool could prevent reserving its very first send.
    fn reserved_bytes(&self) -> std::io::Result<usize> {
        let mut reserved = self.clone();
        reserved.losses = u64::MAX;
        reserved.rejected = u64::MAX;
        for entry in &mut reserved.entries {
            entry.attempts = u32::MAX;
            entry.next = u64::MAX;
        }
        Ok(serde_json::to_vec(&reserved)?.len())
    }
}
/// Caller must assign a unique spool path to exactly one background owner.
/// Bytes include JSON encoding and retry metadata, not only profile payloads.
pub struct Outbox {
    path: PathBuf,
    limits: Limits,
    state: State,
}
fn invalid(message: &str) -> std::io::Error {
    std::io::Error::other(message)
}
impl Outbox {
    pub fn open(path: &Path, limits: Limits) -> std::io::Result<Self> {
        if limits.windows == 0
            || limits.windows > 4096
            || limits.bytes == 0
            || limits.bytes > 16 * 1024 * 1024
            || limits.attempts == 0
            || limits.attempts > 100
            || limits.backoff_ms == 0
            || limits.backoff_ms > 3_600_000
        {
            return Err(invalid("invalid spool limits"));
        }
        let state = match File::open(path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(limits.bytes as u64 + 1).read_to_end(&mut bytes)?;
                if bytes.len() > limits.bytes {
                    return Err(invalid("spool byte cap"));
                }
                let state: State = serde_json::from_slice(&bytes)?;
                if state.version != 1
                    || state.entries.len() > limits.windows
                    || state.reserved_bytes()? > limits.bytes
                {
                    return Err(invalid("spool version/count"));
                }
                for entry in &state.entries {
                    validate(entry.body.as_bytes())?;
                }
                state
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State {
                version: 1,
                ..State::default()
            },
            Err(e) => return Err(e),
        };
        Ok(Self {
            path: path.into(),
            limits,
            state,
        })
    }
    pub fn len(&self) -> usize {
        self.state.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.state.entries.is_empty()
    }
    pub fn rejected(&self) -> u64 {
        self.state.rejected
    }
    pub fn losses(&self) -> u64 {
        self.state.losses
    }
    fn commit(&mut self, state: State) -> std::io::Result<()> {
        let bytes = serde_json::to_vec(&state)?;
        if bytes.len() > self.limits.bytes {
            return Err(invalid("spool byte cap"));
        }
        let mut temp_name = self.path.as_os_str().to_os_string();
        temp_name.push(".ti48.tmp");
        let temp = PathBuf::from(temp_name);
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temp, &self.path)?;
        // A directory fsync makes the rename durable on supported native hosts.
        if let Some(parent) = self.path.parent() {
            // A bare filename has an empty parent, which cannot be opened.
            let parent = if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            };
            File::open(parent)?.sync_all()?;
        }
        self.state = state;
        Ok(())
    }
    /// Project one detached window on the background owner. Invalid projection
    /// is an explicit durable loss; disk/full failures retain the staging front.
    pub fn collect_one(
        &mut self,
        staging: &mut super::Staging,
        cohort: &str,
    ) -> std::io::Result<()> {
        let Some(closed) = staging.front() else {
            return Ok(());
        };
        match super::project(closed, cohort) {
            Ok(bytes) => self.stage(&bytes)?,
            Err(_) => {
                let mut next = self.state.clone();
                next.losses = next.losses.saturating_add(1);
                self.commit(next)?;
            }
        }
        staging.pop();
        Ok(())
    }
    /// Background-only. Caller retains its Closed until this succeeds.
    pub fn stage(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let profile = validate(bytes)?;
        for entry in &self.state.entries {
            let existing = validate(entry.body.as_bytes())?;
            if existing.scope.tenant.as_str() == profile.scope.tenant.as_str()
                && existing.window_id.as_str() == profile.window_id.as_str()
            {
                return if entry.body.as_bytes() == bytes {
                    Ok(())
                } else {
                    Err(invalid("conflicting immutable ID"))
                };
            }
        }
        let mut next = self.state.clone();
        next.entries.push(Entry {
            body: std::str::from_utf8(bytes)
                .map_err(std::io::Error::other)?
                .into(),
            attempts: 0,
            next: 0,
        });
        if next.entries.len() > self.limits.windows || next.reserved_bytes()? > self.limits.bytes {
            let mut lost = self.state.clone();
            lost.rejected = lost.rejected.saturating_add(1);
            self.commit(lost)?;
            return Err(invalid("spool full: reject new"));
        }
        self.commit(next)
    }
    /// At most one send per pump; caller owns scheduling. Persist reservation
    /// before I/O so crashes cannot reset retry budgets. Sender true means Queue
    /// accepted; failures/timeout return false. No unbounded diagnostic buffering.
    pub fn pump(
        &mut self,
        now_ms: u64,
        mut send: impl FnMut(&[u8]) -> bool,
    ) -> std::io::Result<()> {
        let Some(entry) = self.state.entries.first() else {
            return Ok(());
        };
        if now_ms < entry.next {
            return Ok(());
        }
        let mut next = self.state.clone();
        if entry.attempts >= self.limits.attempts {
            next.entries.remove(0);
            next.losses = next.losses.saturating_add(1);
            return self.commit(next);
        }
        let entry = &mut next.entries[0];
        entry.attempts += 1;
        let delay = self
            .limits
            .backoff_ms
            .saturating_mul(1_u64 << entry.attempts.saturating_sub(1).min(16))
            .min(3_600_000);
        entry.next = now_ms.saturating_add(delay);
        self.commit(next)?;
        if send(self.state.entries[0].body.as_bytes()) {
            let mut next = self.state.clone();
            next.entries.remove(0);
            self.commit(next)?;
        }
        Ok(())
    }
}
fn validate(bytes: &[u8]) -> std::io::Result<FieldProfile> {
    if bytes.len() > 65536 {
        return Err(invalid("profile byte cap"));
    }
    let profile: FieldProfile = serde_json::from_slice(bytes)?;
    // Counters and shape are checked by the trusted producer and again in TI-18.
    Ok(profile)
}
