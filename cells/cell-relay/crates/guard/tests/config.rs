//! TI-09: resolve once per call, publish changes without restarting the relay.
use agw_guard::config::{Config, Mode, RuntimeConfig};
use agw_inspector::tee::DEFAULT_CAP;
use std::error::Error;
use std::process::Command;

type Result = std::result::Result<(), Box<dyn Error>>;

fn config(mode: Option<&str>) -> Config {
    Config {
        mode: mode.map(str::to_owned),
        ..Config::default()
    }
}

#[test]
fn startup_precedence_matrix() -> Result {
    for explicit in [None, Some("observe"), Some("repair"), Some("strict")] {
        for env in [None, Some("observe"), Some("repair"), Some("strict")] {
            let runtime = RuntimeConfig::from_sources(config(explicit), env)?;
            let expected: Mode = explicit.or(env).unwrap_or("repair").parse()?;
            assert_eq!(runtime.begin_call(None)?.mode(), expected);
            for override_mode in ["observe", "repair", "strict"] {
                assert_eq!(
                    runtime.begin_call(Some(override_mode))?.mode().to_string(),
                    override_mode
                );
                assert_eq!(runtime.begin_call(None)?.mode(), expected);
            }
        }
    }
    Ok(())
}

#[test]
fn invalid_startup_sources_are_errors_even_when_shadowed() {
    for invalid in ["", "Observe", "strict ", " repair", "unknown"] {
        let explicit = RuntimeConfig::from_sources(config(Some(invalid)), Some("repair"));
        assert!(explicit.is_err_and(|error| error.to_string().contains("config.mode")));
        let env = RuntimeConfig::from_sources(config(Some("observe")), Some(invalid));
        assert!(env.is_err_and(|error| error.to_string().contains("AGINDOW_MODE")));
    }
}

#[test]
fn reload_is_shared_but_existing_calls_keep_their_mode() -> Result {
    let runtime = RuntimeConfig::from_sources(config(None), Some("observe"))?;
    let before = runtime.begin_call(None)?;
    let overridden = runtime.begin_call(Some("strict"))?;
    let controller = runtime.clone();
    let updated = std::thread::spawn(move || controller.reload_mode("repair")).join();
    assert!(matches!(updated, Ok(Ok(()))));
    assert_eq!(runtime.begin_call(None)?.mode(), Mode::Repair);
    assert_eq!(before.mode(), Mode::Observe);
    assert_eq!(overridden.mode(), Mode::Strict);
    runtime.reload_mode("strict")?;
    assert_eq!(runtime.begin_call(None)?.mode(), Mode::Strict);
    runtime.reload_mode("observe")?;
    assert_eq!(runtime.begin_call(None)?.mode(), Mode::Observe);
    Ok(())
}

#[test]
fn invalid_reload_and_override_preserve_the_live_mode() -> Result {
    let runtime = RuntimeConfig::from_sources(config(Some("strict")), None)?;
    for invalid in ["", "Observe", "strict ", "garbage"] {
        assert!(
            runtime
                .reload_mode(invalid)
                .is_err_and(|e| e.to_string().contains("reload.mode"))
        );
        assert!(
            runtime
                .begin_call(Some(invalid))
                .is_err_and(|e| e.to_string().contains("request.mode"))
        );
        assert_eq!(runtime.begin_call(None)?.mode(), Mode::Strict);
    }
    Ok(())
}

#[test]
fn cap_defaults_and_bounds() -> Result {
    let runtime = RuntimeConfig::from_sources(Config::default(), None)?;
    assert_eq!(runtime.begin_call(None)?.inspection_cap(), DEFAULT_CAP);
    for cap in [0, 1, 1024, DEFAULT_CAP] {
        let runtime = RuntimeConfig::from_sources(
            Config {
                inspection_cap: Some(cap),
                ..config(None)
            },
            None,
        )?;
        assert_eq!(runtime.begin_call(None)?.inspection_cap(), cap);
    }
    for cap in [DEFAULT_CAP + 1, usize::MAX] {
        assert!(
            RuntimeConfig::from_sources(
                Config {
                    inspection_cap: Some(cap),
                    ..config(None)
                },
                None
            )
            .is_err()
        );
    }
    Ok(())
}

// Exercise the actual process environment without unsafe set_var or races.
#[test]
fn environment_child() -> Result {
    if let Ok(expected) = std::env::var("TI09_EXPECT_MODE") {
        let result = RuntimeConfig::from_env(Config::default());
        if expected == "invalid" {
            assert!(result.is_err_and(|error| error.to_string().contains("AGINDOW_MODE")));
        } else {
            assert_eq!(result?.begin_call(None)?.mode().to_string(), expected);
        }
    }
    Ok(())
}

fn child() -> std::io::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command.args(["--exact", "environment_child", "--nocapture"]);
    command.env_remove("AGINDOW_MODE");
    Ok(command)
}

#[test]
fn environment_startup_in_a_fresh_process() -> Result {
    for (value, expected) in [
        (None, "repair"),
        (Some("observe"), "observe"),
        (Some("repair"), "repair"),
        (Some("strict"), "strict"),
        (Some(""), "invalid"),
        (Some("STRICT"), "invalid"),
    ] {
        let mut command = child()?;
        command.env("TI09_EXPECT_MODE", expected);
        if let Some(value) = value {
            command.env("AGINDOW_MODE", value);
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn non_unicode_environment_is_an_explicit_startup_error() -> Result {
    use std::os::unix::ffi::OsStringExt;
    let output = child()?
        .env("TI09_EXPECT_MODE", "invalid")
        .env("AGINDOW_MODE", std::ffi::OsString::from_vec(vec![0xff]))
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(())
}
