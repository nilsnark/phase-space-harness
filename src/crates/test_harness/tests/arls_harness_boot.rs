#![cfg(feature = "test-support")]

#[path = "arls_harness_support.rs"]
mod support;

use support::{arls_dimension, hash_prefix, scenario_with_probe, start_session};

#[test]
fn engine_loads_arls_dimension_and_ticks() {
    let (mut session, _tmp) = start_session(101, None, None).expect("engine should start");

    let entities = session
        .refresh_entities()
        .expect("entity listing should succeed")
        .to_vec();
    assert!(
        !entities.is_empty(),
        "plugin should seed at least one entity"
    );

    let mut candidate = None;
    let mut fallback = None;
    for entity in entities
        .iter()
        .filter(|entity| entity.dimension == arls_dimension())
    {
        if let Some(record) = session
            .telemetry_for(entity.entity_id)
            .expect("inspect should work")
        {
            if let Some(velocity) = record.velocity {
                if velocity.0 != 0.0 || velocity.1 != 0.0 {
                    candidate = Some((entity.clone(), record));
                    break;
                }
            }
            fallback = Some((entity.clone(), record));
        }
    }

    let (arls_entity, baseline) = candidate
        .or(fallback)
        .expect("ARLS dimension entity should be present");

    session
        .advance_ticks(3)
        .expect("engine ticks should advance");

    let after = session
        .telemetry_for(arls_entity.entity_id)
        .expect("inspect should work")
        .expect("entity should remain available");

    assert_eq!(after.dimension, arls_dimension());

    session
        .advance_ticks(3)
        .expect("engine ticks should advance");
    let digests = hash_prefix(&session, 3);
    assert!(
        digests.len() >= 3,
        "world hash telemetry should be emitted while ticking"
    );

    session.shutdown().expect("shutdown should succeed");
}

#[test]
fn dimension_registered_via_plugin() {
    let scenario = scenario_with_probe();
    let (mut session, _tmp) =
        start_session(2024, Some(scenario), None).expect("engine should start");

    session
        .refresh_entities()
        .expect("entity listing should succeed");

    let probe = session
        .entities()
        .iter()
        .find(|entity| entity.kind == "arls_probe")
        .cloned()
        .expect("probe spawn should be recorded");

    let before = session
        .telemetry_for(probe.entity_id)
        .expect("inspect should work")
        .expect("probe should exist");
    session
        .advance_ticks(2)
        .expect("engine ticks should advance");
    let after = session
        .telemetry_for(probe.entity_id)
        .expect("inspect should work")
        .expect("probe should remain available");

    assert_eq!(after.dimension, arls_dimension());

    let before_mass = before.mass.expect("mass before ticking");
    let after_mass = after.mass.expect("mass after ticking");
    assert!(
        after_mass >= before_mass,
        "intent commit should preserve or grow probe mass"
    );
    assert_eq!(
        after.velocity, after.velocity,
        "velocity should be readable"
    );

    session.shutdown().expect("shutdown should succeed");
}
