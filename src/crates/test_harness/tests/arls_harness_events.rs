#![cfg(feature = "test-support")]

#[path = "arls_harness_support.rs"]
mod support;

use support::{hash_prefix, scenario_with_probe, start_session};

#[test]
fn scenario_runs_repeatably() {
    let scenario = scenario_with_probe();
    let first = capture_hashes(9090, scenario.clone());
    let second = capture_hashes(9090, scenario);

    assert!(!first.is_empty(), "hash stream should not be empty");
    assert_eq!(first, second);
}

fn capture_hashes(
    seed: u64,
    scenario: phase_space_test_harness::ScenarioConfig,
) -> Vec<(u64, String)> {
    let (mut session, _tmp) =
        start_session(seed, Some(scenario), None).expect("engine should start");
    session
        .advance_ticks(5)
        .expect("engine ticks should advance");
    let hashes = hash_prefix(&session, 5);
    session.shutdown().expect("shutdown should succeed");
    hashes
}
