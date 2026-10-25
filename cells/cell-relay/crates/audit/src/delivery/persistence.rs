//! Atomic private spool snapshots and process-local path ownership.
use super::{DeliveryMetrics, Limits, invalid, validate};
use crate::contracts::audit_artifact::AuditArtifact;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
static OWNERS: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub id: String,
    pub body: String,
    pub attempts: u32,
    pub next_ms: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    version: u8,
    pub tenant: String,
    pub entries: Vec<Entry>,
    pub metrics: DeliveryMetrics,
}
impl State {
    pub fn reserved_bytes(&self) -> io::Result<usize> {
        let mut next = self.clone();
        next.metrics = DeliveryMetrics::reserved();
        for entry in &mut next.entries {
            entry.attempts = u32::MAX;
            entry.next_ms = u64::MAX;
        }
        Ok(serde_json::to_vec(&next)?.len())
    }
}
pub(super) struct Owner {
    path: PathBuf,
}
impl Owner {
    pub fn acquire(path: &Path) -> io::Result<Self> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let name = path
            .file_name()
            .ok_or_else(|| invalid("spool needs a filename"))?;
        let path = parent.canonicalize()?.join(name);
        // Symlink aliases could evade the ownership fence or overwrite another file.
        if fs::symlink_metadata(&path).is_ok_and(|m| !m.file_type().is_file()) {
            return Err(invalid("spool must be a regular file"));
        }
        if !OWNERS
            .lock()
            .map_err(|_| invalid("spool ownership lock"))?
            .insert(path.clone())
        {
            return Err(invalid("audit spool already open"));
        }
        Ok(Self { path })
    }
    fn temporary(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(".ti57.tmp");
        PathBuf::from(name)
    }
    pub fn load(&self, tenant: &str, limits: Limits) -> io::Result<State> {
        // A crash can leave one uncommitted snapshot. Reclaim that fixed slot
        // under the ownership fence, so repeated crashes cannot accumulate files.
        match fs::remove_file(self.temporary()) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(State {
                    version: 1,
                    tenant: tenant.into(),
                    entries: vec![],
                    metrics: DeliveryMetrics::default(),
                });
            }
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take(limits.bytes as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > limits.bytes {
            return Err(invalid("spool byte limit"));
        }
        let state: State = serde_json::from_slice(&bytes)?;
        if state.version != 1
            || state.tenant != tenant
            || state.entries.len() > limits.entries
            || state.reserved_bytes()? > limits.bytes
        {
            return Err(invalid("spool version, tenant or bounds"));
        }
        let mut ids = BTreeSet::new();
        for entry in &state.entries {
            let artifact: AuditArtifact = serde_json::from_str(&entry.body)?;
            validate(&artifact, tenant)?;
            if crate::text(&artifact.id) != entry.id
                || !ids.insert(&entry.id)
                || entry.attempts > 100
            {
                return Err(invalid("invalid spool entry"));
            }
        }
        Ok(state)
    }
    pub fn save(&self, state: &State, cap: usize) -> io::Result<()> {
        let bytes = serde_json::to_vec(state)?;
        if bytes.len() > cap {
            return Err(invalid("spool byte limit"));
        }
        let temp = self.temporary();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temp, &self.path)?;
            File::open(
                self.path
                    .parent()
                    .ok_or_else(|| invalid("missing parent"))?,
            )?
            .sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if let Ok(mut owners) = OWNERS.lock() {
            owners.remove(&self.path);
        }
    }
}
