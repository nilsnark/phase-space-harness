use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use phase_space_protocol::psip::EntityParameters;

/// Process-level configuration for launching the engine binary.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Path to the `phase-space-engine` binary to spawn.
    pub binary_path: PathBuf,
    /// Additional CLI arguments passed through to the engine.
    pub extra_args: Vec<String>,
    /// Optional path to a serialized scenario log (passed via `--scenario`).
    pub scenario_path: Option<PathBuf>,
    /// Optional context plugin to load before ticking.
    pub context_plugin: Option<PathBuf>,
    /// Optional deterministic world seed supplied to the engine.
    pub world_seed: Option<u64>,
    /// Extra environment variables applied to the child process.
    pub env: BTreeMap<String, String>,
    /// Optional working directory override for the child process.
    pub working_directory: Option<PathBuf>,
    /// Upper bound on how long to wait for the engine to announce its listen address.
    pub startup_timeout: Duration,
    /// Expected delay between engine ticks when no telemetry events are available.
    pub tick_wait: Duration,
}

impl EngineConfig {
    /// Create a new config targeting a specific engine binary.
    pub fn new(binary_path: impl Into<PathBuf>) -> Self {
        Self {
            binary_path: binary_path.into(),
            extra_args: Vec::new(),
            scenario_path: None,
            context_plugin: None,
            world_seed: None,
            env: BTreeMap::new(),
            working_directory: None,
            startup_timeout: Duration::from_secs(5),
            tick_wait: Duration::from_millis(10),
        }
    }

    /// Add a passthrough CLI argument.
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    /// Provide a scenario file path to pass through `--scenario`.
    pub fn with_scenario_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.scenario_path = Some(path.into());
        self
    }

    /// Provide a context plugin to load eagerly.
    pub fn with_context_plugin(mut self, path: impl Into<PathBuf>) -> Self {
        self.context_plugin = Some(path.into());
        self
    }

    /// Set a deterministic world seed.
    pub fn with_world_seed(mut self, seed: u64) -> Self {
        self.world_seed = Some(seed);
        self
    }

    /// Add an environment variable override.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Override the working directory for the spawned process.
    pub fn with_working_directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_directory = Some(dir.into());
        self
    }

    /// Override the startup timeout used while waiting for the listen address.
    pub fn with_startup_timeout(mut self, timeout: Duration) -> Self {
        self.startup_timeout = timeout;
        self
    }

    /// Override the expected tick-to-tick wait used when telemetry is absent.
    pub fn with_tick_wait(mut self, wait: Duration) -> Self {
        self.tick_wait = wait;
        self
    }
}

/// Minimal scenario description used to seed entities before ticking.
#[derive(Debug, Clone, Default)]
pub struct ScenarioConfig {
    /// Entities to spawn before returning a session handle.
    pub spawns: Vec<SpawnSpec>,
}

impl ScenarioConfig {
    /// Add a spawn directive to the scenario.
    pub fn with_spawn(mut self, spec: SpawnSpec) -> Self {
        self.spawns.push(spec);
        self
    }
}

/// Entity spawn request issued once the engine is reachable.
#[derive(Debug, Clone)]
pub struct SpawnSpec {
    pub entity_type: String,
    pub parameters: EntityParameters,
    pub dimension: Option<u32>,
}

impl SpawnSpec {
    /// Create a spawn request for a specific entity type.
    pub fn new(entity_type: impl Into<String>) -> Self {
        Self {
            entity_type: entity_type.into(),
            parameters: EntityParameters::default(),
            dimension: None,
        }
    }

    /// Provide initial parameters for the entity.
    pub fn with_parameters(mut self, parameters: EntityParameters) -> Self {
        self.parameters = parameters;
        self
    }

    /// Target a specific dimension for the entity.
    pub fn in_dimension(mut self, dimension: u32) -> Self {
        self.dimension = Some(dimension);
        self
    }
}
