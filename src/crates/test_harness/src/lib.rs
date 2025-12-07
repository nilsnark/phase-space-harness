//! Utilities for spawning the Phase Space engine binary in tests and driving it
//! through the network protocol.
//!
//! Typical usage:
//! ```no_run
//! use phase_space_protocol::psip::EntityParameters;
//! use phase_space_harness::{EngineConfig, EngineHarness, ScenarioConfig, SpawnSpec};
//!
//! let binary = "/path/to/phase-space-engine";
//! let config = EngineConfig::new(binary).with_arg("--test-mode");
//! let scenario = ScenarioConfig::default().with_spawn(
//!     SpawnSpec::new("probe")
//!         .with_parameters(EntityParameters {
//!             position: Some((0.0, 0.0)),
//!             velocity: Some((1.0, 0.0)),
//!             mass: None,
//!         })
//!         .in_dimension(0),
//! );
//!
//! let harness = EngineHarness::spawn(config).expect("engine should launch");
//! let mut session = harness.run_scenario(scenario).expect("scenario should load");
//! session.advance_ticks(4).unwrap();
//! if let Some(state) = session.telemetry_for(session.entities()[0].entity_id).unwrap() {
//!     println!("entity at {:?}", state.position);
//! }
//! ```

mod config;
mod error;
mod harness;

pub use config::{EngineConfig, ScenarioConfig, SpawnSpec};
pub use error::{HarnessError, HarnessResult};
pub use harness::{EngineHarness, LogLine, LogStream, Session};
