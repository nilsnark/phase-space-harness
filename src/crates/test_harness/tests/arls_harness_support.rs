#![cfg(feature = "test-support")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use phase_space_core::{engine_rand_u64, RngDomain};
use phase_space_protocol::psip::{EntityParameters, EntitySummary};
use phase_space_test_harness::{
    EngineConfig, EngineHarness, HarnessResult, ScenarioConfig, SpawnSpec,
};
use tempfile::TempDir;

pub fn arls_dimension() -> u32 {
    1
}

pub fn engine_bin_path() -> PathBuf {
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

pub fn arls_plugin_path() -> PathBuf {
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

pub fn scenario_with_probe() -> ScenarioConfig {
    ScenarioConfig::default().with_spawn(
        SpawnSpec::new("arls_probe")
            .with_parameters(EntityParameters {
                position: Some((0.0, 0.0)),
                velocity: Some((0.0, 0.0)),
                mass: Some(5.0),
            })
            .in_dimension(arls_dimension()),
    )
}

pub fn start_session(
    seed: u64,
    scenario: Option<ScenarioConfig>,
    embedded: Option<bool>,
) -> HarnessResult<(phase_space_test_harness::Session, TempDir)> {
    let (config, tempdir) = config_for_seed(seed, embedded);
    let harness = EngineHarness::spawn(config)?;
    let session = match scenario {
        Some(scenario) => harness.run_scenario(scenario)?,
        None => harness.attach()?,
    };
    Ok((session, tempdir))
}

fn config_for_seed(seed: u64, embedded: Option<bool>) -> (EngineConfig, TempDir) {
    let workdir = TempDir::new().expect("temp workdir");
    let mut config = EngineConfig::new(engine_bin_path())
        .with_context_plugin(arls_plugin_path())
        .with_world_seed(seed)
        .with_working_directory(workdir.path())
        .with_env("PHASE_SPACE_STREAM_WORLD_HASHES", "1");

    if let Some(flag) = embedded {
        config = config.with_env("PHASE_SPACE_ARLS_EMBEDDED", if flag { "1" } else { "0" });
    }

    (config, workdir)
}

pub fn world_hashes(session: &phase_space_test_harness::Session) -> Vec<(u64, String)> {
    let mut hashes = Vec::new();
    for line in session.all_logs() {
        if let Some((tick, hash)) = parse_hash_line(&line.line) {
            hashes.push((tick, hash));
        }
    }
    hashes.sort_by_key(|(tick, _)| *tick);
    hashes
}

pub fn hash_prefix(
    session: &phase_space_test_harness::Session,
    count: usize,
) -> Vec<(u64, String)> {
    let mut hashes = world_hashes(session);
    if hashes.len() > count {
        hashes.truncate(count);
    }
    hashes
}

fn parse_hash_line(line: &str) -> Option<(u64, String)> {
    let tick_part = line.split("tick ").nth(1)?;
    let tick_text = tick_part.split_whitespace().next()?;
    let tick = tick_text.parse().ok()?;

    let hash_part = line.split("world_hash=").nth(1)?;
    let hash = hash_part
        .split_whitespace()
        .next()
        .unwrap_or(hash_part)
        .trim_end_matches(',')
        .to_string();

    Some((tick, hash))
}

pub fn phase_traces_for_dimension(
    session: &phase_space_test_harness::Session,
    dimension: u32,
) -> Vec<(u64, Vec<String>)> {
    let mut traces: BTreeMap<u64, Vec<String>> = BTreeMap::new();
    for line in session.all_logs() {
        let phases_text = match line.line.split("phases=").nth(1) {
            Some(text) => text,
            None => continue,
        };

        for entry in phases_text.split('|') {
            let mut parts = entry.split(':');
            let dim: u32 = match parts.next().and_then(|value| value.parse().ok()) {
                Some(dim) => dim,
                None => continue,
            };
            if dim != dimension {
                continue;
            }

            let tick: u64 = match parts.next().and_then(|value| value.parse().ok()) {
                Some(tick) => tick,
                None => continue,
            };

            let phase = parts.next().unwrap_or("").trim();
            if phase.is_empty() {
                continue;
            }

            traces.entry(tick).or_default().push(phase.to_string());
        }
    }

    traces.into_iter().collect()
}

pub fn sensor_delta_for_tick(
    seed: u64,
    entity_id: u64,
    index: usize,
    entities_per_tick: usize,
    tick: u64,
) -> f64 {
    let offset = entities_per_tick.saturating_mul((tick.saturating_sub(1)) as usize) + index;
    let draw = engine_rand_u64(seed, RngDomain::Sensors, entity_id, tick, offset as u64);
    (draw % 5) as f64
}

pub fn sorted_entities_in_dimension(
    entities: &[EntitySummary],
    dimension: u32,
) -> Vec<EntitySummary> {
    let mut filtered: Vec<_> = entities
        .iter()
        .filter(|entity| entity.dimension == dimension)
        .cloned()
        .collect();
    filtered.sort_by_key(|entity| entity.entity_id);
    filtered
}
