//! Deterministic shape comparison. Semantic metadata comes from callers.
use crate::model::child;
use crate::validate::{Budget, validate};
use crate::{
    Change, DriftClass as C, Evidence, Kind as K, Limits, Node, Reason as R, Report, Side, Snapshot,
};
use agw_inspector::inference::JsonType as T;

pub fn compare(side: Side, before: &Snapshot, after: &Snapshot) -> Report {
    compare_with_limits(side, before, after, Limits::default())
}

pub fn compare_with_limits(
    side: Side,
    before: &Snapshot,
    after: &Snapshot,
    limits: Limits,
) -> Report {
    for snapshot in [before, after] {
        if let Err(skip) = validate(&snapshot.node, 0, &mut Budget::new(limits)) {
            return Report::skip(skip);
        }
    }
    let mut diff = Differ {
        side,
        authoritative: before.evidence == Evidence::Authoritative
            && after.evidence == Evidence::Authoritative,
        observed: after.evidence == Evidence::Observed,
        changes: Vec::new(),
    };
    diff.visit("$", &before.node, &after.node);
    Report {
        changes: diff.changes,
        skipped: None,
    }
}

struct Differ {
    side: Side,
    authoritative: bool,
    observed: bool,
    changes: Vec<Change>,
}
impl Differ {
    fn add(&mut self, path: &str, kind: K, class: C, reason: R) {
        self.changes.push(Change {
            side: self.side,
            path: path.into(),
            to_path: None,
            kind,
            class,
            reason,
        });
    }
    fn structural(&mut self, path: &str, kind: K) {
        let (class, reason) = if self.authoritative {
            (C::Shape, R::DeclaredStructure)
        } else {
            (C::Unknown, R::InsufficientEvidence)
        };
        self.add(path, kind, class, reason);
    }
    fn visit(&mut self, path: &str, a: &Node, b: &Node) {
        // Observations are compatible when their types are a subset of the
        // baseline. Declared contracts also report narrowing as a change.
        let accepts = |expected: &Node, actual: &Node| {
            actual.types.iter().all(|t| {
                expected.types.contains(t)
                    || (*t == T::Integer && expected.types.contains(&T::Number))
            })
        };
        let normalize = |node: &Node, widen: bool| {
            node.types
                .iter()
                .map(|t| {
                    if *t == T::Integer && (widen || node.types.contains(&T::Number)) {
                        T::Number
                    } else {
                        *t
                    }
                })
                .collect::<std::collections::BTreeSet<_>>()
        };
        let same = if self.observed {
            accepts(a, b)
        } else {
            normalize(a, b.types.contains(&T::Number)) == normalize(b, false)
        };
        if !same {
            let numeric = |n: &Node| n.types.contains(&T::Integer) || n.types.contains(&T::Number);
            let non_numeric =
                |n: &Node| n.types.iter().any(|t| !matches!(t, T::Integer | T::Number));
            let only_numeric_difference = a
                .types
                .symmetric_difference(&b.types)
                .all(|t| matches!(t, T::Integer | T::Number));
            if !only_numeric_difference
                && ((numeric(a) && non_numeric(b)) || (non_numeric(a) && numeric(b)))
            {
                self.add(path, K::TypeChanged, C::Semantic, R::NumericCoercion);
            } else if a.types.contains(&T::Null) != b.types.contains(&T::Null) {
                self.add(path, K::TypeChanged, C::Semantic, R::NullabilityChanged);
            } else {
                self.structural(path, K::TypeChanged);
            }
        }
        if !self.observed {
            self.metadata(path, a, b);
        }
        if a.types.contains(&T::Object) && b.types.contains(&T::Object) {
            self.object(path, a, b);
        }
        if a.types.contains(&T::Array) && b.types.contains(&T::Array) {
            match (&a.items, &b.items) {
                (Some(a), Some(b)) => self.visit(&format!("{path}[]"), a, b),
                (None, Some(_)) => self.add(
                    &format!("{path}[]"),
                    K::TypeChanged,
                    C::Unknown,
                    R::InsufficientEvidence,
                ),
                (Some(_), None) if !self.observed => self.add(
                    &format!("{path}[]"),
                    K::TypeChanged,
                    C::Unknown,
                    R::InsufficientEvidence,
                ),
                _ => {}
            }
        }
    }
    fn metadata(&mut self, path: &str, a: &Node, b: &Node) {
        for (old, new, kind, reason) in [
            (&a.units, &b.units, K::UnitsChanged, R::UnitsChanged),
            (
                &a.null_meaning,
                &b.null_meaning,
                K::NullMeaningChanged,
                R::MeaningChanged,
            ),
        ] {
            if old == new {
                continue;
            }
            let known = old.is_some() && new.is_some();
            self.add(
                path,
                kind,
                if known { C::Semantic } else { C::Unknown },
                if known { reason } else { R::MetadataLost },
            );
        }
        if a.enum_values != b.enum_values {
            let expanded = match (&a.enum_values, &b.enum_values) {
                (Some(a), Some(b)) => a.is_subset(b),
                _ => false,
            };
            self.add(
                path,
                if expanded {
                    K::EnumExpanded
                } else {
                    K::EnumChanged
                },
                if expanded { C::Semantic } else { C::Unknown },
                if expanded {
                    R::EnumExpanded
                } else {
                    R::EnumConstraintChanged
                },
            );
        }
    }
    fn identity_count(node: &Node, id: &str) -> usize {
        node.properties
            .values()
            .filter(|n| n.identity.as_deref() == Some(id))
            .count()
    }
    fn ambiguous(a: &Node, b: &Node, node: &Node) -> bool {
        node.identity
            .as_deref()
            .is_some_and(|id| Self::identity_count(a, id) > 1 || Self::identity_count(b, id) > 1)
    }
    fn renamed<'a>(
        &self,
        a: &Node,
        b: &'a Node,
        key: &str,
        old: &Node,
    ) -> Option<(&'a String, &'a Node)> {
        if !self.authoritative || b.properties.contains_key(key) {
            return None;
        }
        let id = old.identity.as_deref()?;
        if Self::identity_count(a, id) != 1 || Self::identity_count(b, id) != 1 {
            return None;
        }
        b.properties
            .iter()
            .find(|(key, n)| !a.properties.contains_key(*key) && n.identity.as_deref() == Some(id))
    }
    fn object(&mut self, path: &str, a: &Node, b: &Node) {
        for (key, old) in &a.properties {
            let next = child(path, key);
            if let Some(new) = b.properties.get(key) {
                self.visit(&next, old, new);
                if a.required.contains(key) != b.required.contains(key) && !self.observed {
                    self.structural(&next, K::RequiredChanged);
                }
            } else if let Some((new_key, new)) = self.renamed(a, b, key, old) {
                let dest = child(path, new_key);
                self.changes.push(Change {
                    side: self.side,
                    path: next,
                    to_path: Some(dest.clone()),
                    kind: K::Renamed,
                    class: C::Shape,
                    reason: R::ConfirmedIdentity,
                });
                self.visit(&dest, old, new);
                if a.required.contains(key) != b.required.contains(new_key) {
                    self.structural(&dest, K::RequiredChanged);
                }
            } else if Self::ambiguous(a, b, old) {
                self.add(&next, K::Removed, C::Unknown, R::AmbiguousIdentity);
            } else {
                self.structural(&next, K::Removed);
            }
        }
        for (key, new) in &b.properties {
            if a.properties.contains_key(key) {
                continue;
            }
            if a.properties.iter().any(|(k, n)| {
                self.renamed(a, b, k, n)
                    .is_some_and(|(dest, _)| dest == key)
            }) {
                continue;
            }
            let next = child(path, key);
            if Self::ambiguous(a, b, new) {
                self.add(&next, K::Added, C::Unknown, R::AmbiguousIdentity);
            } else if !self.authoritative {
                self.structural(&next, K::Added);
            } else if b.required.contains(key) {
                self.structural(&next, K::RequiredAdded);
            } else {
                self.add(&next, K::OptionalAdded, C::Shape, R::OptionalInformation);
            }
        }
    }
}
