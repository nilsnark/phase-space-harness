#![cfg(feature = "test-support")]

#[path = "arls_harness_support.rs"]
mod support;

use support::{
    arls_dimension, hash_prefix, phase_traces_for_dimension, scenario_with_probe, start_session,
};

#[test]
fn phase_sequence_matches_profile() {
    let (mut session, _tmp) =
        start_session(5501, Some(scenario_with_probe()), None).expect("engine should start");

    let entities = session
        .refresh_entities()
        .expect("entity listing should succeed");
    assert!(
        entities
            .iter()
            .any(|entity| entity.dimension == arls_dimension()),
        "ARLS dimension should be registered"
    );

    session
        .advance_ticks(2)
        .expect("engine ticks should advance");

    let traces = phase_traces_for_dimension(&session, arls_dimension());
    if traces.is_empty() {
        for line in session.all_logs() {
            eprintln!("LOG: {}", line.line);
        }
    }
    assert!(
        !traces.is_empty(),
        "phase telemetry should be present for ARLS dimension"
    );

    let (_, phases) = traces
        .first()
        .expect("at least one phase trace should be captured");
    let expected = ["Sensors", "Scripts", "IntentCommit", "Physics"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(
        phases, &expected,
        "ARLS phase order should match plugin profile"
    );

    let digests = hash_prefix(&session, 1);
    assert!(
        !digests.is_empty(),
        "world hash telemetry should be present after ticking"
    );

    session.shutdown().expect("shutdown should succeed");
}
