#![cfg(feature = "test-support")]

use std::path::PathBuf;

use phase_space_harness::{EngineConfig, EngineHarness, ScenarioConfig, SpawnSpec};
use phase_space_protocol::psip::EntityParameters;

fn fake_engine_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_fake_engine") {
        return PathBuf::from(path);
    }

    // Fallback to the workspace target directory.
    let mut path = std::env::current_exe().expect("current exe");
    path.pop(); // deps
    path.pop(); // debug or release
    path.push("fake_engine");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

#[test]
fn drives_fake_engine_end_to_end() {
    let config = EngineConfig::new(fake_engine_path());
    let scenario = ScenarioConfig::default().with_spawn(SpawnSpec::new("probe").with_parameters(
        EntityParameters {
            position: Some((0.0, 0.0)),
            velocity: Some((1.0, 0.0)),
            mass: None,
        },
    ));

    let harness = EngineHarness::spawn(config).expect("engine should launch");
    let mut session = harness
        .run_scenario(scenario)
        .expect("scenario should start");

    let entity_id = session
        .entities()
        .first()
        .expect("spawned entity present")
        .entity_id;

    session.advance_ticks(3).expect("ticks should advance");
    let telemetry = session
        .telemetry_for(entity_id)
        .expect("inspect should succeed")
        .expect("entity should exist");
    assert_eq!(telemetry.entity_id, entity_id);

    let logs = session.logs_for(entity_id);
    assert!(
        !logs.is_empty(),
        "expected telemetry or logs for entity {entity_id}"
    );

    session.shutdown().expect("shutdown should succeed");
}
