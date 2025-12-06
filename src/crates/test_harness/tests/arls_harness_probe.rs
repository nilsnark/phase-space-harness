#![cfg(feature = "test-support")]

#[path = "arls_harness_support.rs"]
mod support;

use phase_space_protocol::psip::{EntityParameters, EntityRecord};
use phase_space_test_harness::{ScenarioConfig, SpawnSpec};

use support::{arls_dimension, start_session};

#[test]
fn arls_probe_runs_deterministically_via_harness() {
    let scenario = ScenarioConfig::default().with_spawn(
        SpawnSpec::new("arls_probe")
            .with_parameters(EntityParameters {
                position: Some((0.0, 0.0)),
                velocity: Some((1.0, 0.25)),
                mass: Some(10.0),
            })
            .in_dimension(arls_dimension()),
    );

    let first = run_probe_session(11, scenario.clone());
    let second = run_probe_session(11, scenario);

    assert_eq!(first.dimension, arls_dimension());
    assert_eq!(second.dimension, arls_dimension());

    let first_mass = first.mass.expect("mass should be reported");
    let second_mass = second.mass.expect("mass should be reported");
    assert!(
        first_mass > 0.0 && second_mass > 0.0,
        "telemetry should include probe mass"
    );
    assert_eq!(
        first, second,
        "matching seeds should yield identical telemetry via harness"
    );
}

fn run_probe_session(seed: u64, scenario: ScenarioConfig) -> EntityRecord {
    let (mut session, _tmp) =
        start_session(seed, Some(scenario), None).expect("engine should start with ARLS plugin");

    let entity_id = session
        .entities()
        .first()
        .expect("spawned entity should exist")
        .entity_id;

    session
        .advance_ticks(8)
        .expect("engine ticks should advance");

    let record = session
        .telemetry_for(entity_id)
        .expect("inspection should succeed")
        .expect("entity should remain available");

    session.shutdown().expect("shutdown should succeed");
    record
}
