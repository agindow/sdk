use agw_stats::control::{Error, Scope, Season, Side, Window, select};

fn scope() -> Scope<'static> {
    Scope {
        tenant: "t",
        vendor: "v",
        operation: "GET /items",
        side: Side::Response,
        tenant_segment: "paid",
        volume_cohort: "high",
        field: "/status",
    }
}
fn window(start: u64) -> Window<'static> {
    Window {
        scope: scope(),
        start,
        end: start + 10,
        complete: true,
        samples: 20,
    }
}
fn season() -> Season {
    Season {
        duration: 10,
        period: 100,
        periods: 2,
        min_samples: 20,
    }
}
#[test]
fn selects_exact_prior_slots_newest_first_and_rolls() -> Result<(), Error> {
    let windows = [window(0), window(100), window(200), window(190)];
    assert_eq!(select(&window(200), &windows, season())?, Some(vec![1, 0]));
    assert_eq!(select(&window(300), &windows, season())?, Some(vec![2, 1]));
    Ok(())
}
#[test]
fn never_falls_back_across_any_scope_dimension() -> Result<(), Error> {
    for dimension in 0..7 {
        let mut foreign = window(100);
        match dimension {
            0 => foreign.scope.tenant = "other",
            1 => foreign.scope.vendor = "other",
            2 => foreign.scope.operation = "POST /items",
            3 => foreign.scope.side = Side::Request,
            4 => foreign.scope.tenant_segment = "other",
            5 => foreign.scope.volume_cohort = "other",
            _ => foreign.scope.field = "/other",
        }
        assert_eq!(select(&window(200), &[window(0), foreign], season())?, None);
    }
    Ok(())
}
#[test]
fn incomplete_sparse_duplicate_and_missing_windows_are_unavailable() -> Result<(), Error> {
    for (complete, samples) in [(false, 20), (true, 19)] {
        let mut bad = window(100);
        bad.complete = complete;
        bad.samples = samples;
        assert_eq!(select(&window(200), &[window(0), bad], season())?, None);
        assert_eq!(select(&bad, &[], season())?, None);
    }
    assert_eq!(
        select(
            &window(200),
            &[window(0), window(100), window(100)],
            season()
        )?,
        None
    );
    assert_eq!(select(&window(200), &[window(0)], season())?, None);
    assert_eq!(select(&window(0), &[], season())?, None);
    Ok(())
}
#[test]
fn rejects_invalid_alignment_configuration_scope_and_resource_limits() {
    let mut bad = season();
    bad.period = 11;
    assert_eq!(select(&window(200), &[], bad), Err(Error::InvalidInput));
    assert_eq!(
        select(&window(201), &[], season()),
        Err(Error::InvalidInput)
    );
    let mut bad = window(200);
    bad.scope.tenant = "";
    assert_eq!(select(&bad, &[], season()), Err(Error::InvalidInput));
    assert_eq!(
        select(&window(200), &vec![window(0); 4097], season()),
        Err(Error::ResourceLimit)
    );
}
#[test]
fn validates_season_bounds_pointer_and_checked_timestamps() -> Result<(), Error> {
    for periods in [0, 53, usize::MAX] {
        assert_eq!(
            select(
                &window(200),
                &[],
                Season {
                    periods,
                    ..season()
                }
            ),
            Err(Error::InvalidInput)
        );
    }
    for duration in [0, 101] {
        assert_eq!(
            select(
                &window(200),
                &[],
                Season {
                    duration,
                    ..season()
                }
            ),
            Err(Error::InvalidInput)
        );
    }
    let mut bad = window(200);
    bad.end = 199;
    assert_eq!(select(&bad, &[], season()), Err(Error::InvalidInput));
    for pointer in ["status", "/bad~", "/bad~2"] {
        bad = window(200);
        bad.scope.field = pointer;
        assert_eq!(select(&bad, &[], season()), Err(Error::InvalidInput));
    }
    let large = "t".repeat(1025);
    bad = window(200);
    bad.scope.tenant = &large;
    assert_eq!(select(&bad, &[], season()), Err(Error::ResourceLimit));
    let mut bad = window(100);
    bad.end = 120;
    assert_eq!(select(&window(200), &[window(0), bad], season())?, None);
    Ok(())
}
