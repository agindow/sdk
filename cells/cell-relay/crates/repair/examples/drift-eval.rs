mod drift_eval;
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let baseline = args.next().ok_or("usage: drift-eval BASELINE [REPORT]")?;
    let output = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let report = drift_eval::run()?;
    println!("{}", drift_eval::summary(&report));
    // Write evidence before comparison, so failed runs remain inspectable.
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    let previous = serde_json::from_slice(&fs::read(baseline)?)?;
    drift_eval::compare(&report, &previous)
}
