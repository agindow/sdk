//! Reviewed scalar preparation. No host authority is inferred from a detector.
use crate::exact::Decimal;
use agw_diff::{Change, DriftClass, Kind, Reason, Report, Side};
use agw_repair::{Candidate, Evidence, Mode, Operation};
use serde::Deserialize;
use serde_json::{json, value::RawValue};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Facts {
    version: u8,
    recipe: String,
    tool: String,
    side: String,
    contract: String,
    migration: String,
    provenance: String,
    expires_at: u64,
    pointer: String,
    mapping: Option<Vec<(String, String)>>,
    equivalence: Option<Vec<(String, String)>>,
    null_meaning: Option<String>,
    source_unit: Option<String>,
    target_unit: Option<String>,
    numerator: Option<String>,
    denominator: Option<String>,
    input_min: Option<String>,
    input_max: Option<String>,
    output_min: Option<String>,
    output_max: Option<String>,
}
fn label(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
impl Facts {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || ![
                &self.tool,
                &self.contract,
                &self.migration,
                &self.provenance,
            ]
            .into_iter()
            .all(|s| label(s))
            || !matches!(self.side.as_str(), "request" | "response")
            || self.expires_at == 0
            || self.expires_at > 9_007_199_254_740_991
            || self.pointer.is_empty()
        {
            return Err("invalid scalar scope".into());
        }
        crate::nested::parse(&self.pointer)?;
        let numeric = [
            &self.source_unit,
            &self.target_unit,
            &self.numerator,
            &self.denominator,
            &self.input_min,
            &self.input_max,
            &self.output_min,
            &self.output_max,
        ];
        match self.recipe.as_str() {
            "string-map" if numeric.iter().all(|v| v.is_none()) && self.null_meaning.is_none() => {
                let pairs = self.mapping.as_ref().ok_or("missing finite mapping")?;
                if pairs.is_empty() || pairs.len() > 64 {
                    return Err("mapping bound".into());
                }
                let mut sources = BTreeSet::new();

                for (source, target) in pairs {
                    if source.len() > 256
                        || target.len() > 256
                        || source == target
                        || !sources.insert(source)
                    {
                        return Err("ambiguous mapping".into());
                    }
                }
                let mut reviews = BTreeMap::new();
                if let Some(equivalence) = &self.equivalence {
                    if equivalence.len() > 64 {
                        return Err("equivalence bound".into());
                    }
                    for (source, provenance) in equivalence {
                        if !sources.contains(source)
                            || !label(provenance)
                            || reviews.insert(source, provenance).is_some()
                        {
                            return Err("invalid equivalence review".into());
                        }
                    }
                }
                for (source, target) in pairs {
                    if pairs.iter().filter(|(_, t)| t == target).count() > 1
                        && !reviews.contains_key(source)
                    {
                        return Err("missing source equivalence review".into());
                    }
                }
            }
            "empty-null"
                if numeric.iter().all(|v| v.is_none())
                    && self.mapping.is_none()
                    && self.equivalence.is_none()
                    && self.null_meaning.as_deref().is_some_and(label) => {}
            "scale"
                if self.mapping.is_none()
                    && self.equivalence.is_none()
                    && self.null_meaning.is_none() =>
            {
                self.ratio().ok_or("unsupported units or ratio")?;
                let bounds = self.bounds().ok_or("missing exact domain bounds")?;
                if bounds[0].compare(bounds[1]).is_none_or(|o| o.is_gt())
                    || bounds[2].compare(bounds[3]).is_none_or(|o| o.is_gt())
                {
                    return Err("invalid domain bounds".into());
                }
            }
            _ => return Err("unsupported scalar recipe".into()),
        }
        Ok(())
    }
    fn ratio(&self) -> Option<(i128, i128)> {
        let ratio = match (self.source_unit.as_deref()?, self.target_unit.as_deref()?) {
            ("cents", "dollars") => (1, 100),
            ("dollars", "cents") => (100, 1),
            ("seconds", "milliseconds") | ("meters", "millimeters") => (1000, 1),
            ("milliseconds", "seconds") | ("millimeters", "meters") => (1, 1000),
            _ => return None,
        };
        if self.numerator.as_ref()? != &ratio.0.to_string()
            || self.denominator.as_ref()? != &ratio.1.to_string()
        {
            return None;
        }
        Some(ratio)
    }
    fn bounds(&self) -> Option<[Decimal; 4]> {
        Some([
            Decimal::parse(self.input_min.as_deref()?)?,
            Decimal::parse(self.input_max.as_deref()?)?,
            Decimal::parse(self.output_min.as_deref()?)?,
            Decimal::parse(self.output_max.as_deref()?)?,
        ])
    }
    fn replacement(&self, raw: &str) -> Option<Vec<u8>> {
        match self.recipe.as_str() {
            "string-map" => {
                let source: String = serde_json::from_str(raw).ok()?;
                let target = &self.mapping.as_ref()?.iter().find(|(s, _)| s == &source)?.1;
                serde_json::to_vec(target).ok()
            }
            "empty-null" => (raw == "\"\"").then(|| b"null".to_vec()),
            "scale" => {
                // Parsing a string as a number cannot pass this lexical fence.
                let _: &RawValue = serde_json::from_str(raw).ok()?;
                let source = Decimal::parse(raw)?;
                let (n, d) = self.ratio()?;
                let target = source.scale(n, d)?;
                let [min, max, out_min, out_max] = self.bounds()?;
                if source.compare(min)?.is_lt()
                    || source.compare(max)?.is_gt()
                    || target.compare(out_min)?.is_lt()
                    || target.compare(out_max)?.is_gt()
                {
                    return None;
                }
                Some(target.render().into_bytes())
            }
            _ => None,
        }
    }
}
fn token<'a>(mut raw: &'a RawValue, parts: &[String]) -> Option<&'a RawValue> {
    for part in parts {
        raw = match raw.get().as_bytes().first()? {
            b'{' => *serde_json::from_str::<BTreeMap<String, &RawValue>>(raw.get())
                .ok()?
                .get(part)?,
            b'[' => {
                let index: usize = part.parse().ok()?;
                if index.to_string() != *part {
                    return None;
                }
                *serde_json::from_str::<Vec<&RawValue>>(raw.get())
                    .ok()?
                    .get(index)?
            }
            _ => return None,
        };
    }
    Some(raw)
}
pub(crate) fn prepare(
    bytes: &[u8],
    facts: &str,
    confidence: f64,
    mode: &str,
    work: u32,
) -> Result<String, String> {
    if facts.len() > 32768 {
        return Err("scalar facts bound".into());
    }
    let facts: Facts = serde_json::from_str(facts).map_err(|_| "invalid scalar facts")?;
    facts.validate()?;
    crate::selective::bounded(bytes, confidence, work)?;
    let mode = match mode {
        "observe" => Mode::Observe,
        "repair" => Mode::Repair,
        "strict" => Mode::Strict,
        _ => return Err("invalid mode".into()),
    };
    let inspection = agw_inspector::inspect(bytes, false, None);
    let agw_inspector::Parse::Parsed(value) = &inspection.parse else {
        return Err("scalar inspection unavailable".into());
    };
    crate::nested::validate(value, 0, &mut 0)?;
    let parts = crate::nested::parse(&facts.pointer)?;
    let root: &RawValue = serde_json::from_slice(bytes).map_err(|_| "invalid scalar JSON")?;
    let refuse = |reason: &str| {
        json!({"matched":true,"applied":false,"halt":mode!=Mode::Observe,"reason":reason,"class":"semantic"}).to_string()
    };
    let Some(raw) = token(root, &parts) else {
        return Ok(refuse("missing-target"));
    };
    let Some(replacement) = facts.replacement(raw.get()) else {
        return Ok(refuse("source-predicate"));
    };
    if replacement == raw.get().as_bytes() {
        return Ok(json!({"matched":false,"applied":false,"body":std::str::from_utf8(bytes).map_err(|_|"UTF-8")?}).to_string());
    }
    let path = crate::nested::resolve(value, &parts).ok_or("invalid scalar pointer")?;
    let kind = match facts.recipe.as_str() {
        "string-map" => Kind::EnumExpanded,
        "empty-null" => Kind::NullMeaningChanged,
        _ => Kind::UnitsChanged,
    };
    let report = Report {
        changes: vec![Change {
            side: Side::Request,
            path: path.clone(),
            to_path: None,
            kind,
            class: DriftClass::Semantic,
            reason: Reason::MeaningChanged,
        }],
        skipped: None,
    };
    let candidate = Candidate {
        finding: 0,
        operation: Operation::Replace(replacement),
        evidence: Evidence {
            detector: confidence,
            applicability: confidence,
        },
    };
    let outcome = agw_repair::repair(
        Side::Request,
        bytes,
        &inspection,
        &report,
        &[candidate],
        mode,
    );
    if outcome.failure.is_some() {
        return Err("scalar repair unavailable".into());
    }
    let applied = outcome.patch().is_some_and(|p| p.applied);
    Ok(json!({"matched":true,"applied":applied,"halt":mode!=Mode::Observe,"class":"semantic","reason":if applied {"verified-candidate"} else {"confidence-or-policy"},
        "body":if applied {Some(std::str::from_utf8(&outcome.body).map_err(|_|"invalid UTF-8")?)} else {None},"changes":[{"path":path,"kind":"replaced"}]}).to_string())
}
