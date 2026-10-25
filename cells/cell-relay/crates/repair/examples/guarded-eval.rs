#[allow(dead_code)]
mod drift_eval;
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let baseline = args.next().ok_or("usage: guarded-eval BASELINE [REPORT]")?;
    let output = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let report = drift_eval::guarded::run()?;
    println!("{}", drift_eval::summary(&report.seam));
    println!(
        "INTERNAL guarded-v3; supplied findings, caller facts v1, independent TI-24 verification"
    );
    for (cohort, metrics) in [
        ("healthy (42)", &report.metrics),
        ("fault probes (210)", &report.fault_metrics),
    ] {
        for m in metrics {
            println!(
                "{cohort} / {} / {}: repairable recovery {}/{}, blocks {}, rejected {}, unsafe {}/{} dispatched",
                m.class,
                m.label,
                m.repairable_recovered,
                m.repairable_total,
                m.blocked,
                m.rejected,
                m.unsafe_writes,
                m.total - m.blocked
            );
        }
    }
    let diagnosis = drift_eval::guarded::diagnostics::diagnose(&report)?;
    println!("Healthy case causes: {:?}", diagnosis.counts);
    println!(
        "{} total; {} repairable, including {} numeric type transitions refused by contract; {} without unique evidence",
        diagnosis.total,
        diagnosis.repairable_total,
        diagnosis.repairable_numeric_refusals,
        diagnosis.without_unique_evidence
    );
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            path.with_extension("diagnosis.json"),
            serde_json::to_vec_pretty(&diagnosis)?,
        )?;
        fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    let previous = serde_json::from_slice(&fs::read(baseline)?)?;
    drift_eval::guarded::compare(&report, &previous)
}
