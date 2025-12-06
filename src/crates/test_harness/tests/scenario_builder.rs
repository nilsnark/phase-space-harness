#![cfg(feature = "test-support")]

use std::path::PathBuf;

use phase_space_server::scenario_builder::ScenarioBuilder;
use phase_space_test_harness::{EngineConfig, EngineHarness};
use tempfile::{tempdir, NamedTempFile};

fn engine_bin_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_phase-space-engine")
        .or_else(|_| std::env::var("CARGO_BIN_EXE_phase_space_engine"))
    {
        return PathBuf::from(path);
    }

    // Fallback to the workspace target directory.
    let mut path = std::env::current_exe().expect("current exe");
    path.pop(); // deps
    path.pop(); // debug or release
    path.push("phase-space-engine");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

#[test]
fn scenario_builder_runs_preseeded_engine() {
    let scenario = ScenarioBuilder::two_ship_intercept()
        .with_world_seed(7_777)
        .build()
        .expect("scenario should build");

    let temp = NamedTempFile::new().expect("temp file");
    scenario
        .write_json(temp.path())
        .expect("scenario should serialize");

    let _workdir = tempdir().expect("temp workdir");
    let mut config = EngineConfig::new(engine_bin_path())
        .with_scenario_path(temp.path())
        .with_working_directory(_workdir.path());
    if let Some(seed) = scenario.world_seed() {
        config = config.with_world_seed(seed);
    }
    if let Some(plugin) = scenario.context_plugin() {
        config = config.with_context_plugin(plugin.clone());
    }

    let harness = EngineHarness::spawn(config).expect("engine should launch");
    let mut session = harness
        .attach()
        .expect("scenario should expose initial entities");

    let entities: Vec<_> = session.entities().to_vec();
    assert_eq!(entities.len(), 2, "scenario should seed two ships");
    let names: Vec<_> = entities.iter().map(|entity| entity.kind.as_str()).collect();
    assert!(names.contains(&"interceptor_a"));
    assert!(names.contains(&"interceptor_b"));

    session.advance_ticks(2).expect("ticks should advance");

    for entity in entities {
        let record = session
            .telemetry_for(entity.entity_id)
            .expect("inspect should succeed")
            .expect("entity should exist");
        assert!(
            record.mass.is_some(),
            "entity {} should report mass",
            record.entity_id
        );
    }

    session.shutdown().expect("shutdown should succeed");
}
