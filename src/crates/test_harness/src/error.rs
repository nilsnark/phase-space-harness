use std::io;
use std::process::ExitStatus;
use std::time::Duration;

use thiserror::Error;

/// Result alias for harness operations.
pub type HarnessResult<T> = Result<T, HarnessError>;

/// Errors that can occur while spawning or driving the engine process.
#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("failed to spawn engine: {0}")]
    EngineStart(String),
    #[error("engine terminated early with status {0}")]
    EngineExited(ExitStatus),
    #[error("failed to parse listen address from output: {0}")]
    ListenParse(String),
    #[error("engine did not report a listen address within {0:?}")]
    StartupTimeout(Duration),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("protocol error: {0}")]
    Protocol(#[from] phase_space_protocol::ClientError),
    #[error("unexpected server response: {0}")]
    UnexpectedResponse(String),
    #[error("engine connection closed")]
    ConnectionClosed,
}

impl HarnessError {
    pub(crate) fn engine_start(err: impl Into<String>) -> Self {
        HarnessError::EngineStart(err.into())
    }

    pub(crate) fn unexpected(message: impl Into<String>) -> Self {
        HarnessError::UnexpectedResponse(message.into())
    }
}
