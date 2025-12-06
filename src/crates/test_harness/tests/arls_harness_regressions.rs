#![cfg(feature = "test-support")]

#[path = "arls_harness_support.rs"]
mod support;

use support::{arls_dimension, hash_prefix, scenario_with_probe, start_session};

#[test]
fn single_ship_track_confidence() {
    let scenario = scenario_with_probe();
    let first = capture_hashes(401, Some(scenario.clone()), 6);
    let second = capture_hashes(401, Some(scenario), 6);

    assert_eq!(first, second);
    assert!(
        first.len() >= 5,
        "expected multiple ticks of digest telemetry"
    );
}

#[test]
fn knowledge_sharing() {
    let (mut session, _tmp) = start_session(402, None, None).expect("engine should start");
    let entities = session
        .refresh_entities()
        .expect("entity listing should succeed")
        .to_vec();
    let arls_entities: Vec<_> = entities
        .iter()
        .filter(|entity| entity.dimension == arls_dimension())
        .collect();
    assert!(
        arls_entities.len() > 1,
        "ARLS dimension should expose multiple seeded actors"
    );

    session
        .advance_ticks(6)
        .expect("engine ticks should advance");
    let hashes = hash_prefix(&session, 6);
    session.shutdown().expect("shutdown should succeed");

    let (mut session_again, _tmp2) = start_session(402, None, None).expect("engine should restart");
    session_again
        .advance_ticks(6)
        .expect("engine ticks should advance");
    let hashes_again = hash_prefix(&session_again, 6);
    session_again.shutdown().expect("shutdown should succeed");

    let count = hashes.len().min(hashes_again.len());
    assert_eq!(&hashes[..count], &hashes_again[..count]);
}

#[test]
fn contract_race() {
    let hashes_a = capture_hashes(403, None, 6);
    let hashes_b = capture_hashes(404, Some(scenario_with_probe()), 6);

    assert_eq!(
        hashes_a,
        capture_hashes(403, None, 6),
        "replaying the same seed should yield identical digests"
    );
    assert_ne!(
        hashes_a, hashes_b,
        "different seeds should lead to different digest streams"
    );
}

fn capture_hashes(
    seed: u64,
    scenario: Option<phase_space_test_harness::ScenarioConfig>,
    ticks: u64,
) -> Vec<(u64, String)> {
    let (mut session, _tmp) = start_session(seed, scenario, None).expect("engine should start");
    session
        .advance_ticks(ticks)
        .expect("engine ticks should advance");
    let hashes = hash_prefix(&session, ticks as usize);
    session.shutdown().expect("shutdown should succeed");
    hashes
}
