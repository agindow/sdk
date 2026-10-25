//! Administrative local history. Own one store per tenant; no remote dependency.
use crate::{Error, MAX_RULES, Program, Rule, compile, valid_text};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub vendor: String,
    pub operation: String,
    pub contract_version: u64,
}
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub scopes: usize,
    pub versions: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            scopes: 16,
            versions: 16,
        }
    }
}
/// Customer-owned controls, retained with each selected immutable snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Controls {
    pub pin: Option<u64>,
    pub overrides: Vec<Rule>,
    pub disabled: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    bindings: Vec<String>,
    pub scope: Scope,
    pub version: u64,
    pub controls: Controls,
    pub program: Program,
}
impl Snapshot {
    /// Slot order used by the compiled program, retained for safe host projection.
    pub fn bindings(&self) -> &[String] {
        &self.bindings
    }
}
struct Version {
    number: u64,
    rules: Vec<Rule>,
    bindings: Vec<String>,
}
struct History {
    scope: Scope,
    versions: Vec<Version>,
}
pub struct Store {
    limits: Limits,
    histories: Vec<History>,
}
impl Store {
    pub fn new(limits: Limits) -> Result<Self, Error> {
        if limits.scopes == 0
            || limits.versions == 0
            || limits.scopes > 128
            || limits.versions > 128
        {
            return Err(Error::Capacity);
        }
        Ok(Self {
            limits,
            histories: Vec::new(),
        })
    }
    /// All validation precedes mutation; published versions are never replaced.
    pub fn publish(
        &mut self,
        scope: Scope,
        version: u64,
        rules: &[Rule],
        bindings: &[&str],
    ) -> Result<(), Error> {
        if !valid_text(&scope.vendor)
            || !valid_text(&scope.operation)
            || scope.contract_version == 0
            || version == 0
        {
            return Err(Error::Invalid);
        }
        let position = self.histories.iter().position(|h| h.scope == scope);
        if let Some(i) = position {
            let history = &self.histories[i];
            if history.versions.len() >= self.limits.versions {
                return Err(Error::Capacity);
            }
            if history.versions.last().is_some_and(|v| version <= v.number) {
                return Err(Error::Invalid);
            }
        } else if self.histories.len() >= self.limits.scopes {
            return Err(Error::Capacity);
        }
        compile(rules, bindings)?;
        let entry = Version {
            number: version,
            rules: rules.to_vec(),
            bindings: bindings.iter().map(|b| (*b).to_owned()).collect(),
        };
        if let Some(i) = position {
            self.histories[i].versions.push(entry);
        } else {
            self.histories.push(History {
                scope,
                versions: vec![entry],
            });
        }
        Ok(())
    }
    /// Selection is administrative: compile after applying controls, outside the
    /// request path. Runtime callers retain the returned program without store access.
    pub fn select(
        &self,
        scope: &Scope,
        version: u64,
        controls: &Controls,
    ) -> Result<Snapshot, Error> {
        if controls.overrides.len() > MAX_RULES || controls.disabled.len() > MAX_RULES {
            return Err(Error::Capacity);
        }
        let history = self
            .histories
            .iter()
            .find(|h| &h.scope == scope)
            .ok_or(Error::NotFound)?;
        let selected = controls.pin.unwrap_or(version);
        let entry = history
            .versions
            .iter()
            .find(|v| v.number == selected)
            .ok_or(Error::NotFound)?;
        for (i, id) in controls.disabled.iter().enumerate() {
            if !entry.rules.iter().any(|r| &r.id == id) || controls.disabled[..i].contains(id) {
                return Err(Error::Invalid);
            }
        }
        let bindings: Vec<_> = entry.bindings.iter().map(String::as_str).collect();
        // Validate replacement work/size before cloning customer-owned definitions.
        compile(&controls.overrides, &bindings)?;
        let mut rules = entry.rules.clone();
        for (i, replacement) in controls.overrides.iter().enumerate() {
            if controls.overrides[..i]
                .iter()
                .any(|r| r.id == replacement.id)
                || controls.disabled.contains(&replacement.id)
            {
                return Err(Error::Invalid);
            }
            let target = rules
                .iter_mut()
                .find(|r| r.id == replacement.id)
                .ok_or(Error::Invalid)?;
            *target = replacement.clone();
        }
        rules.retain(|r| !controls.disabled.contains(&r.id));
        let program = compile(&rules, &bindings)?;
        Ok(Snapshot {
            bindings: entry.bindings.clone(),
            scope: scope.clone(),
            version: selected,
            controls: controls.clone(),
            program,
        })
    }
}
