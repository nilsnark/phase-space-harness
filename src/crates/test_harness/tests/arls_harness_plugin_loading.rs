#![cfg(feature = "test-support")]

#[path = "arls_harness_support.rs"]
mod support;

use support::{hash_prefix, scenario_with_probe, start_session};

#[test]
fn manifest_vs_embedded_same_output() {
    let scenario = scenario_with_probe();
    let manifest = run_with_mode(1337, Some(false), scenario.clone());
    let embedded = run_with_mode(1337, Some(true), scenario);

    assert!(
        !manifest.is_empty() && !embedded.is_empty(),
        "both runs should emit world hash telemetry"
    );
    assert_eq!(manifest, embedded);
}

fn run_with_mode(
    seed: u64,
    embedded: Option<bool>,
    scenario: phase_space_test_harness::ScenarioConfig,
) -> Vec<(u64, String)> {
    let (mut session, _tmp) =
        start_session(seed, Some(scenario), embedded).expect("engine should start");
    session
        .advance_ticks(4)
        .expect("engine ticks should advance");
    let hashes = hash_prefix(&session, 4);
    session.shutdown().expect("shutdown should succeed");
    hashes
}
