#![cfg(feature = "test-support")]

use std::path::{Path, PathBuf};

use phase_space_core::context_sdk::DimensionId;
use phase_space_protocol::psip::{EntityParameters, EntityRecord};
use phase_space_test_harness::{EngineConfig, EngineHarness, ScenarioConfig, SpawnSpec};
use tempfile::tempdir;

#[test]
fn arls_plugin_updates_entities_via_harness() {
    let engine = engine_bin_path();
    let plugin = arls_plugin_path();

    let first = run_arls_probe_run(&engine, &plugin, 7);
    let second = run_arls_probe_run(&engine, &plugin, 7);

    assert_eq!(first.dimension, arls_dimension().0);
    assert_eq!(second.dimension, arls_dimension().0);

    let first_mass = first.mass.expect("mass should be present");
    let second_mass = second.mass.expect("mass should be present");
    let first_velocity = first.velocity.expect("velocity should be present");

    assert!(
        first_mass > 0.0 && second_mass > 0.0,
        "engine should report mass via telemetry"
    );
    assert!(
        first_velocity.0 != 0.0 || first_velocity.1 != 0.0,
        "engine should return a populated velocity vector"
    );
    assert_eq!(
        first, second,
        "ARLS telemetry should be deterministic across harness runs with matching seeds"
    );
}

fn run_arls_probe_run(engine: &Path, plugin: &Path, seed: u64) -> EntityRecord {
    let workdir = tempdir().expect("temp workdir");
    let scenario = ScenarioConfig::default().with_spawn(
        SpawnSpec::new("arls_probe")
            .with_parameters(EntityParameters {
                position: Some((0.0, 0.0)),
                velocity: Some((1.0, 0.25)),
                mass: Some(10.0),
            })
            .in_dimension(arls_dimension().0),
    );

    let config = EngineConfig::new(engine)
        .with_context_plugin(plugin)
        .with_world_seed(seed)
        .with_working_directory(workdir.path());

    let harness =
        EngineHarness::spawn(config).expect("engine should launch with ARLS plugin in harness");
    let mut session = harness
        .run_scenario(scenario)
        .expect("scenario should seed entity through PSIP");

    let entity_id = session
        .entities()
        .first()
        .expect("spawned entity should exist")
        .entity_id;

    for _ in 0..8 {
        session
            .advance_ticks(1)
            .expect("engine ticks should advance");
    }

    let record = session
        .telemetry_for(entity_id)
        .expect("inspect should succeed")
        .expect("entity should be present after ticks");

    session.shutdown().expect("shutdown should succeed");
    record
}

fn arls_dimension() -> DimensionId {
    DimensionId(1)
}

fn engine_bin_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_phase-space-engine")
        .or_else(|_| std::env::var("CARGO_BIN_EXE_phase_space_engine"))
    {
        return PathBuf::from(path);
    }

    let mut path = std::env::current_exe().expect("current exe");
    path.pop(); // deps
    path.pop(); // debug or release
    path.push("phase-space-engine");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

fn arls_plugin_path() -> PathBuf {
    if let Ok(path) = std::env::var("PHASE_SPACE_ARLS_PLUGIN_PATH") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return candidate;
        }
    }

    let lib_name = format!(
        "{}phase_space_context_arls{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );

    let target_dir = workspace_target_dir();
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    let candidates = [
        target_dir.join(&profile).join(&lib_name),
        target_dir.join(&profile).join("deps").join(&lib_name),
        target_dir.join("debug").join(&lib_name),
        target_dir.join("debug").join("deps").join(&lib_name),
        target_dir.join("release").join(&lib_name),
        target_dir.join("release").join("deps").join(&lib_name),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return candidate;
        }
    }

    panic!(
        "ARLS plugin library not found; set PHASE_SPACE_ARLS_PLUGIN_PATH or ensure the cdylib is built"
    );
}

fn workspace_target_dir() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(path);
    }

    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop(); // test_harness
    dir.pop(); // crates
    dir.push("target");
    dir
}
