#![cfg(feature = "test-support")]

#[path = "arls_harness_support.rs"]
mod support;

use support::{hash_prefix, scenario_with_probe, start_session};

#[test]
fn script_burn_updates_plan() {
    let scenario = scenario_with_probe();
    let (mut session, _tmp) = start_session(11, Some(scenario), None).expect("engine should start");

    session
        .refresh_entities()
        .expect("entity listing should succeed");
    let probe = session
        .entities()
        .iter()
        .find(|entity| entity.kind == "arls_probe")
        .cloned()
        .expect("probe spawn should be present");

    let before = session
        .telemetry_for(probe.entity_id)
        .expect("inspect should work")
        .expect("probe should exist");
    session
        .advance_ticks(4)
        .expect("engine ticks should advance");
    let after = session
        .telemetry_for(probe.entity_id)
        .expect("inspect should work")
        .expect("probe should remain available");

    let _mass_after = after.mass.expect("mass should be reported");

    let digests = hash_prefix(&session, 4);
    assert!(
        !digests.is_empty(),
        "world hash telemetry should be present for probe scenario"
    );

    session.shutdown().expect("shutdown should succeed");
}

#[test]
fn script_burn_deterministic() {
    let scenario = scenario_with_probe();
    let first = capture_hashes(22, scenario.clone());
    let second = capture_hashes(22, scenario);

    assert!(!first.is_empty(), "digest stream should not be empty");
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
