use super::super::{fixtures, score};
use super::{
    Error, Fault, Report,
    facts::{Declaration, verify},
    outcome_metrics,
};

fn validate(report: &Report) -> Result<(), Error> {
    if report.version != 3
        || report.profile != "guarded-v3"
        || report.healthy.len() != 42
        || report.faults.len() != 210
    {
        return Err("incompatible guarded profile/version or missing cases".into());
    }
    super::super::compare(&report.seam, &report.seam)?;
    for (actual, expected) in [
        (&report.metrics, outcome_metrics(&report.healthy)),
        (&report.fault_metrics, outcome_metrics(&report.faults)),
    ] {
        if actual.len() != expected.len()
            || !actual
                .iter()
                .zip(&expected)
                .all(|(a, b)| super::super::regression::same_metrics(a, b))
        {
            return Err("missing or inconsistent guarded outcome metrics".into());
        }
    }
    if report.verification_metrics != super::verification_metrics(&report.healthy)
        || report.fault_verification_metrics != super::verification_metrics(&report.faults)
    {
        return Err("missing or inconsistent verification metrics".into());
    }
    let facts: Declaration = serde_json::from_str(&report.declaration)?;
    if facts.version != 1 || facts.cases.len() != 42 {
        return Err("invalid facts version/count".into());
    }
    let fixtures = fixtures()?;
    for (records, cohort) in [(&report.healthy, false), (&report.faults, true)] {
        for (index, record) in records.iter().enumerate() {
            let fixture = fixtures
                .get(if cohort { index / 5 } else { index })
                .ok_or("case missing")?;
            let expected_fault = if cohort {
                super::FAULTS[index % 5]
            } else {
                Fault::Healthy
            };
            if record.fault != expected_fault || facts.cases.get(&fixture.id) != Some(&record.facts)
            {
                return Err("case facts/cohort mismatch".into());
            }
            let sent = record
                .bytes
                .as_ref()
                .map(|b| serde_json::from_slice(b))
                .transpose()?;
            let scored = score(fixture, "guarded", sent, record.case.trace.clone());
            if record.case != scored
                || record.dispatch_count != usize::from(record.bytes.is_some())
                || record.original_outcome != record.bytes.is_some()
            {
                return Err("dispatch evidence disagrees with whole-write score".into());
            }
            match (&record.verification_input, record.verification.as_slice()) {
                (None, []) => {}
                (Some((bytes, applied)), [result]) if result == "passed" || result == "refused" => {
                    if applied != &[0]
                        || record.bypasses != 0
                        || (*result == "passed")
                            != (record.fault != Fault::Refuse
                                && verify(&fixture.task.input, &record.facts, bytes, applied))
                    {
                        return Err("invalid verification invocation/result".into());
                    }
                }
                (Some((_, applied)), [invoked, fault])
                    if invoked == "invoked"
                        && ["error", "panic", "budget"].contains(&fault.as_str())
                        && applied == &[0]
                        && cohort
                        && record.bypasses == 1 => {}
                _ => return Err("missing or invalid verification evidence".into()),
            }
            if record.bypasses > 0 {
                if !cohort
                    || record.bypasses != 1
                    || !record.original_bytes
                    || record.bytes.as_deref()
                        != Some(serde_json::to_vec(&fixture.task.input)?.as_slice())
                {
                    return Err("invalid availability passthrough".into());
                }
            } else if record.decision == "Repaired" {
                if record.verification != ["passed"]
                    || record.verification_input.as_ref().map(|(bytes, _)| bytes)
                        != record.bytes.as_ref()
                    || !record.bytes.as_ref().is_some_and(|bytes| {
                        verify(&fixture.task.input, &record.facts, bytes, &[0])
                    })
                {
                    return Err("repair missing independent verification evidence".into());
                }
            } else if record.case.recovered || record.verification.iter().any(|s| s == "passed") {
                return Err("recovery without verified repair".into());
            }
        }
    }
    Ok(())
}
pub fn compare(current: &Report, previous: &Report) -> Result<(), Error> {
    if current.declaration != previous.declaration || current.seam.corpus != previous.seam.corpus {
        return Err("incompatible evidence/corpus; explicit migration required".into());
    }
    validate(current)?;
    validate(previous)?;
    super::super::compare(&current.seam, &previous.seam)?;
    for (now, old) in current.healthy.iter().zip(&previous.healthy) {
        if (old.case.recovered && !now.case.recovered)
            || (!old.case.violation && now.case.violation)
        {
            return Err(format!("guarded regression: {}", now.case.id).into());
        }
    }
    Ok(())
}
