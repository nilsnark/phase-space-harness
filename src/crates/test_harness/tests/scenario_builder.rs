#![cfg(feature = "test-support")]

use std::fs::File;
use std::path::{Path, PathBuf};

use phase_space_harness::{EngineConfig, EngineHarness};
use serde::Serialize;
use tempfile::{tempdir, NamedTempFile};

fn engine_bin_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_phase-space-engine")
        .or_else(|_| std::env::var("CARGO_BIN_EXE_phase_space_engine"))
    {
        return Some(PathBuf::from(path));
    }

    // Fallback to the workspace target directory.
    let mut path = match std::env::current_exe() {
        Ok(current) => current,
        Err(_) => return None,
    };
    path.pop(); // deps
    path.pop(); // debug or release
    path.push("phase-space-engine");
    if cfg!(windows) {
        path.set_extension("exe");
    }

    if path.exists() {
        Some(path)
    } else {
        None
    }
}

#[test]
fn scenario_builder_runs_preseeded_engine() {
    let Some(engine_path) = engine_bin_path() else {
        eprintln!("phase-space-engine binary not found; skipping scenario builder integration test");
        return;
    };

    let scenario = ScenarioBuilder::two_ship_intercept()
        .with_world_seed(7_777)
        .build()
        .expect("scenario should build");

    let temp = NamedTempFile::new().expect("temp file");
    scenario
        .write_json(temp.path())
        .expect("scenario should serialize");

    let _workdir = tempdir().expect("temp workdir");
    let mut config = EngineConfig::new(engine_path.clone())
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

/// Minimal scenario builder used for harness integration tests.
struct ScenarioBuilder {
    dt_seconds: f64,
    total_ticks: u64,
    checkpoint_interval: Option<u64>,
    world_seed: Option<u64>,
    context_plugin: Option<PathBuf>,
}

impl ScenarioBuilder {
    fn two_ship_intercept() -> Self {
        Self {
            dt_seconds: 1.0,
            total_ticks: 6,
            checkpoint_interval: Some(2),
            world_seed: None,
            context_plugin: None,
        }
    }

    fn with_world_seed(mut self, seed: u64) -> Self {
        self.world_seed = Some(seed);
        self
    }

    fn build(self) -> Result<BuiltScenario, ScenarioBuildError> {
        let checkpoints = build_checkpoints(self.total_ticks, self.checkpoint_interval);
        let log = InputLog {
            dt_seconds: self.dt_seconds,
            total_ticks: self.total_ticks,
            checkpoints,
            entities: default_entities(),
        };
        Ok(BuiltScenario {
            log,
            world_seed: self.world_seed,
            context_plugin: self.context_plugin,
        })
    }
}

fn build_checkpoints(total_ticks: u64, interval: Option<u64>) -> Vec<u64> {
    match interval.filter(|value| *value > 0) {
        Some(step) => {
            let mut checkpoints = Vec::new();
            let mut tick = 0;
            while tick <= total_ticks {
                checkpoints.push(tick);
                tick = tick.saturating_add(step);
            }
            if checkpoints.last().copied().unwrap_or_default() != total_ticks {
                checkpoints.push(total_ticks);
            }
            checkpoints
        }
        None => vec![0, total_ticks],
    }
}

fn default_entities() -> Vec<EntitySeed> {
    vec![
        EntitySeed {
            name: "interceptor_a".into(),
            dimension: 0,
            transform: Some(TransformSeed {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            }),
            velocity: Some(VelocitySeed {
                dx: 45.0,
                dy: 0.0,
                dz: 0.0,
            }),
            mass_kg: Some(1_000.0),
            interior_dimension: None,
        },
        EntitySeed {
            name: "interceptor_b".into(),
            dimension: 0,
            transform: Some(TransformSeed {
                x: 10_000.0,
                y: 0.0,
                z: 0.0,
            }),
            velocity: Some(VelocitySeed {
                dx: -35.0,
                dy: 5.0,
                dz: 0.0,
            }),
            mass_kg: Some(900.0),
            interior_dimension: None,
        },
    ]
}

struct BuiltScenario {
    log: InputLog,
    world_seed: Option<u64>,
    context_plugin: Option<PathBuf>,
}

impl BuiltScenario {
    fn world_seed(&self) -> Option<u64> {
        self.world_seed
    }

    fn context_plugin(&self) -> Option<&PathBuf> {
        self.context_plugin.as_ref()
    }

    fn write_json(&self, path: &Path) -> Result<(), ScenarioBuildError> {
        let file = File::create(path).map_err(|err| {
            ScenarioBuildError::msg(format!("create scenario {}: {err}", path.display()))
        })?;
        serde_json::to_writer_pretty(file, &self.log)
            .map_err(|err| ScenarioBuildError::msg(format!("serialize scenario: {err}")))
    }
}

#[derive(Debug)]
struct ScenarioBuildError {
    message: String,
}

impl ScenarioBuildError {
    fn msg(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Serialize)]
struct InputLog {
    dt_seconds: f64,
    total_ticks: u64,
    checkpoints: Vec<u64>,
    entities: Vec<EntitySeed>,
}

#[derive(Serialize)]
struct EntitySeed {
    name: String,
    dimension: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    transform: Option<TransformSeed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    velocity: Option<VelocitySeed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mass_kg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interior_dimension: Option<u32>,
}

#[derive(Default, Serialize)]
struct TransformSeed {
    x: f64,
    y: f64,
    #[serde(default)]
    z: f64,
}

#[derive(Default, Serialize)]
struct VelocitySeed {
    dx: f64,
    dy: f64,
    #[serde(default)]
    dz: f64,
}
