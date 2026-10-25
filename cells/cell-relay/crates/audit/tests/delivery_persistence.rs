mod support;
use agw_audit::delivery::{Limits, Outbox};
use std::{error::Error, fs};
#[test]
fn reopen_reclaims_the_single_crash_temporary_without_touching_committed_evidence()
-> Result<(), Box<dyn Error>> {
    let path = std::env::temp_dir().join(format!("ti57-crash-{}.json", std::process::id()));
    let mut spool = Outbox::open(&path, "acme", Limits::default())?;
    spool.enqueue("acme", &support::fixture()?)?;
    drop(spool);
    let mut temp = path.as_os_str().to_os_string();
    temp.push(".ti57.tmp");
    let temp = std::path::PathBuf::from(temp);
    fs::write(&temp, b"interrupted snapshot")?;
    let spool = Outbox::open(&path, "acme", Limits::default())?;
    assert_eq!(spool.len(), 1);
    assert!(
        !temp.exists(),
        "crash snapshots must not accumulate outside the spool bound"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
    }
    drop(spool);
    fs::remove_file(path)?;
    Ok(())
}
