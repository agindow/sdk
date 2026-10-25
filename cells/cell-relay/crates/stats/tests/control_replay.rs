use agw_stats::control::replay::{Day, Opportunity, Provenance, run};
use agw_stats::control::{Error, Evidence, Floors, Scope, Season, Side, Signal, Window};

fn current(day: u64) -> Window<'static> {
    Window {
        scope: Scope {
            tenant: "t",
            vendor: "v",
            operation: "GET /items",
            side: Side::Response,
            tenant_segment: "paid",
            volume_cohort: "high",
            field: "/status",
        },
        start: (day + 7) * 86400,
        end: (day + 8) * 86400,
        complete: true,
        samples: 100,
    }
}
fn history(day: u64) -> Window<'static> {
    Window {
        start: day * 86400,
        end: (day + 1) * 86400,
        ..current(day)
    }
}
fn evidence(signal: Signal, effect: f64) -> Evidence {
    Evidence {
        signal,
        effect,
        p_value: 0.001,
        tests: 2,
    }
}
#[test]
fn growing_synthetic_workload_runs_real_controls_but_never_qualifies() -> Result<(), Error> {
    let histories: Vec<_> = (0..30).map(|d| [history(d)]).collect();
    let opportunities: Vec<_> = (0..30)
        .map(|d| {
            vec![
                Opportunity {
                    current: current(d),
                    history: &histories[d as usize],
                    evidence: evidence(Signal::MeanShift, 0.2),
                    actual_drift: false,
                },
                Opportunity {
                    current: current(d),
                    history: &histories[d as usize],
                    evidence: evidence(Signal::EnumNovelty, 0.5),
                    actual_drift: true,
                },
            ]
        })
        .collect();
    let days: Vec<_> = (0..30)
        .map(|d| Day {
            utc_day: d + 7,
            traffic: 100 + d,
            opportunities: &opportunities[d as usize],
        })
        .collect();
    let floors = Floors::new(0.05, 0.1, 0.4, &[])?;
    let season = Season {
        duration: 86400,
        period: 7 * 86400,
        periods: 1,
        min_samples: 20,
    };
    let report = run(&days, season, &floors, 0.01, Provenance::Synthetic)?;
    assert_eq!(report.negatives, 30);
    assert_eq!(report.false_positives, 0);
    assert_eq!(report.positives, 30);
    assert_eq!(report.true_positives, 30);
    assert_eq!(report.unavailable, 0);
    assert_eq!(report.false_positive_rate, 0.0);
    assert!(report.within_threshold);
    assert!(!report.production_qualified);
    // This enum is a caller attestation, not evidence of provenance by itself.
    assert!(
        run(
            &days,
            season,
            &floors,
            0.01,
            Provenance::Production {
                dataset_id: "test-attestation-only"
            }
        )?
        .production_qualified
    );
    Ok(())
}
#[test]
fn false_positives_count_before_budgets_and_threshold_is_strict() -> Result<(), Error> {
    let histories: Vec<_> = (0..30).map(|d| [history(d)]).collect();
    let opportunities: Vec<_> = (0..30)
        .map(|d| {
            [Opportunity {
                current: current(d),
                history: &histories[d as usize],
                evidence: evidence(Signal::MeanShift, if d == 0 { 0.5 } else { 0.1 }),
                actual_drift: false,
            }]
        })
        .collect();
    let days: Vec<_> = (0..30)
        .map(|d| Day {
            utc_day: d + 7,
            traffic: d + 1,
            opportunities: &opportunities[d as usize],
        })
        .collect();
    let floors = Floors::new(0.05, 0.1, 0.4, &[])?;
    let season = Season {
        duration: 86400,
        period: 7 * 86400,
        periods: 1,
        min_samples: 20,
    };
    let report = run(&days, season, &floors, 1.0 / 30.0, Provenance::Synthetic)?;
    assert_eq!(report.false_positives, 1);
    assert!(!report.within_threshold);
    assert!(!report.production_qualified);
    Ok(())
}
#[test]
fn missing_baselines_cannot_dilute_rate_into_qualification() -> Result<(), Error> {
    let opportunities: Vec<_> = (0..30)
        .map(|d| {
            [Opportunity {
                current: current(d),
                history: &[],
                evidence: evidence(Signal::MeanShift, 0.9),
                actual_drift: false,
            }]
        })
        .collect();
    let days: Vec<_> = (0..30)
        .map(|d| Day {
            utc_day: d + 7,
            traffic: d + 1,
            opportunities: &opportunities[d as usize],
        })
        .collect();
    let floors = Floors::new(0.05, 0.1, 0.4, &[])?;
    let season = Season {
        duration: 86400,
        period: 7 * 86400,
        periods: 1,
        min_samples: 20,
    };
    let report = run(
        &days,
        season,
        &floors,
        0.01,
        Provenance::Production { dataset_id: "test" },
    )?;
    assert_eq!(report.unavailable, 30);
    assert!(!report.production_qualified);
    assert!(run(&days[..29], season, &floors, 0.01, Provenance::Synthetic).is_err());
    assert!(run(&days, season, &floors, f64::NAN, Provenance::Synthetic).is_err());
    Ok(())
}
#[test]
fn rejects_gaps_empty_days_non_growth_wrong_timestamps_and_oversized_runs() -> Result<(), Error> {
    let opportunities: Vec<_> = (0..30)
        .map(|d| {
            [Opportunity {
                current: current(d),
                history: &[],
                evidence: evidence(Signal::MeanShift, 0.1),
                actual_drift: false,
            }]
        })
        .collect();
    let days: Vec<_> = (0..30)
        .map(|d| Day {
            utc_day: d + 7,
            traffic: d + 1,
            opportunities: &opportunities[d as usize],
        })
        .collect();
    let floors = Floors::new(0.05, 0.1, 0.4, &[])?;
    let season = Season {
        duration: 86400,
        period: 7 * 86400,
        periods: 1,
        min_samples: 20,
    };
    for mutation in 0..5 {
        let mut bad = days.clone();
        match mutation {
            0 => bad[1].utc_day += 1,
            1 => bad[1].opportunities = &[],
            2 => bad[29].traffic = bad[0].traffic,
            3 => bad[1].traffic = 0,
            _ => bad[0].opportunities = &opportunities[1],
        }
        assert_eq!(
            run(&bad, season, &floors, 0.01, Provenance::Synthetic),
            Err(Error::InvalidInput)
        );
    }
    let huge = vec![opportunities[0][0]; 100_001];
    let mut bad = days.clone();
    bad[0].opportunities = &huge;
    assert_eq!(
        run(&bad, season, &floors, 0.01, Provenance::Synthetic),
        Err(Error::ResourceLimit)
    );
    assert_eq!(
        run(
            &days,
            season,
            &floors,
            0.01,
            Provenance::Production { dataset_id: "" }
        ),
        Err(Error::InvalidInput)
    );
    Ok(())
}
#[test]
fn tracks_missed_positives_and_counts_all_negative_opportunities() -> Result<(), Error> {
    let histories: Vec<_> = (0..30).map(|d| [history(d)]).collect();
    let opportunities: Vec<_> = (0..30)
        .map(|d| {
            vec![
                Opportunity {
                    current: current(d),
                    history: &histories[d as usize],
                    evidence: evidence(Signal::MeanShift, 0.1),
                    actual_drift: false,
                },
                Opportunity {
                    current: current(d),
                    history: &histories[d as usize],
                    evidence: evidence(Signal::MeanShift, 0.5),
                    actual_drift: d != 0,
                },
            ]
        })
        .collect();
    let days: Vec<_> = (0..30)
        .map(|d| Day {
            utc_day: d + 7,
            traffic: 100 + d,
            opportunities: &opportunities[d as usize],
        })
        .collect();
    let floors = Floors::new(0.05, 0.1, 0.6, &[])?;
    let season = Season {
        duration: 86400,
        period: 7 * 86400,
        periods: 1,
        min_samples: 20,
    };
    let report = run(&days, season, &floors, 0.1, Provenance::Synthetic)?;
    assert_eq!(report.negatives, 31);
    assert_eq!(report.positives, 29);
    assert_eq!(report.true_positives, 0);
    let floors = Floors::new(0.05, 0.1, 0.4, &[])?;
    let report = run(&days, season, &floors, 0.1, Provenance::Synthetic)?;
    assert_eq!(report.false_positives, 1);
    assert_eq!(report.true_positives, 29);
    assert_eq!(report.false_positive_rate, 1.0 / 31.0);
    Ok(())
}

#[test]
fn bounds_aggregate_replay_work_before_scanning_seasonal_histories() -> Result<(), Error> {
    let history = vec![history(0); 4096];
    let opportunities: Vec<_> = (0..30)
        .map(|d| {
            vec![
                Opportunity {
                    current: current(d),
                    history: &history,
                    evidence: evidence(Signal::MeanShift, 0.1),
                    actual_drift: false,
                };
                2
            ]
        })
        .collect();
    let days: Vec<_> = (0..30)
        .map(|d| Day {
            utc_day: d + 7,
            traffic: d + 1,
            opportunities: &opportunities[d as usize],
        })
        .collect();
    let floors = Floors::new(0.05, 0.1, 0.4, &[])?;
    let season = Season {
        duration: 86400,
        period: 7 * 86400,
        periods: 52,
        min_samples: 20,
    };
    assert_eq!(
        run(&days, season, &floors, 0.01, Provenance::Synthetic),
        Err(Error::ResourceLimit)
    );
    Ok(())
}
