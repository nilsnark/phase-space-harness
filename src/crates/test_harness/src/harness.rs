use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use phase_space_protocol::psip::{
    EntityRecord, EntitySummary, ResponseStatus, ServerEvent, ServerRequest, ServerResponse,
};
use phase_space_protocol::Client;

use crate::config::{EngineConfig, ScenarioConfig};
use crate::error::{HarnessError, HarnessResult};

/// Origin stream for captured log lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
    Event,
}

/// Single captured log line with its source.
#[derive(Debug, Clone)]
pub struct LogLine {
    pub stream: LogStream,
    pub line: String,
}

/// Handle to a running engine process with an active protocol client.
pub struct EngineHarness {
    child: Child,
    client: Client,
    log_buffer: Arc<Mutex<Vec<LogLine>>>,
    event_buffer: Arc<Mutex<Vec<ServerEvent>>>,
    log_collector: thread::JoinHandle<()>,
    event_collector: thread::JoinHandle<()>,
    max_tick: Arc<AtomicU64>,
    tick_wait: Duration,
}

impl EngineHarness {
    /// Spawn the engine process and connect using the synchronous protocol client.
    pub fn spawn(config: EngineConfig) -> HarnessResult<Self> {
        let mut cmd = Command::new(&config.binary_path);
        let mut args = config.extra_args.clone();
        if let Some(path) = &config.scenario_path {
            args.push("--scenario".to_string());
            args.push(path.display().to_string());
        }
        if let Some(seed) = config.world_seed {
            args.push("--seed".to_string());
            args.push(seed.to_string());
        }
        if let Some(plugin) = &config.context_plugin {
            args.push("--context-plugin".to_string());
            args.push(plugin.display().to_string());
        }
        let has_bind_arg = args
            .iter()
            .any(|arg| arg == "--bind-addr" || arg.starts_with("--bind-addr="));
        if !has_bind_arg {
            args.push("--bind-addr".to_string());
            args.push("127.0.0.1:0".to_string());
        }
        cmd.args(args);

        if let Some(dir) = &config.working_directory {
            cmd.current_dir(dir);
        }
        cmd.envs(&config.env);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|err| HarnessError::engine_start(err.to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| HarnessError::engine_start("failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| HarnessError::engine_start("failed to capture stderr"))?;

        let (log_tx, log_rx) = mpsc::channel();
        spawn_log_reader(stdout, LogStream::Stdout, log_tx.clone());
        spawn_log_reader(stderr, LogStream::Stderr, log_tx);

        let log_buffer = Arc::new(Mutex::new(Vec::new()));
        let address =
            wait_for_listen_address(&mut child, &log_rx, &log_buffer, config.startup_timeout)?;
        let log_collector = spawn_log_collector(log_rx, log_buffer.clone());

        let client = Client::connect(address)?;
        let event_rx = client.subscribe();
        let event_buffer = Arc::new(Mutex::new(Vec::new()));
        let max_tick = Arc::new(AtomicU64::new(0));
        let event_collector =
            spawn_event_collector(event_rx, event_buffer.clone(), max_tick.clone());

        Ok(Self {
            child,
            client,
            log_buffer,
            event_buffer,
            log_collector,
            event_collector,
            max_tick,
            tick_wait: config.tick_wait,
        })
    }

    /// Seed the running engine with the provided scenario and return a session handle.
    pub fn run_scenario(self, scenario: ScenarioConfig) -> HarnessResult<Session> {
        let mut entities = Vec::new();

        for spec in scenario.spawns {
            let response = self.client.send(ServerRequest::Spawn {
                entity_type: spec.entity_type.clone(),
                parameters: spec.parameters.clone(),
                dimension: spec.dimension,
            })?;

            match response {
                ServerResponse::Spawned { status, entity } => {
                    if status != ResponseStatus::Ok {
                        return Err(HarnessError::unexpected(format!(
                            "spawn for {} failed with status {status:?}",
                            spec.entity_type
                        )));
                    }
                    entities.push(entity.clone());
                }
                ServerResponse::Error { message, .. } => {
                    return Err(HarnessError::unexpected(message))
                }
                other => {
                    return Err(HarnessError::unexpected(format!(
                        "spawn returned unexpected response: {other:?}"
                    )))
                }
            }
        }

        Ok(self.finish_session(entities))
    }

    /// Connect to a pre-seeded engine (e.g., started with `--scenario`) and list existing entities.
    pub fn attach(self) -> HarnessResult<Session> {
        let response = self.client.send(ServerRequest::List)?;
        let entities = match response {
            ServerResponse::Listed { status, entities } => {
                if status != ResponseStatus::Ok {
                    return Err(HarnessError::unexpected(format!(
                        "list failed with status {status:?}"
                    )));
                }
                entities
            }
            other => {
                return Err(HarnessError::unexpected(format!(
                    "list returned unexpected response: {other:?}"
                )))
            }
        };

        Ok(self.finish_session(entities))
    }

    fn finish_session(self, entities: Vec<EntitySummary>) -> Session {
        let entity_dimensions = entities
            .iter()
            .map(|entity| (entity.entity_id, entity.dimension))
            .collect();

        Session {
            child: self.child,
            client: Some(self.client),
            log_buffer: self.log_buffer,
            event_buffer: self.event_buffer,
            log_collector: Some(self.log_collector),
            event_collector: Some(self.event_collector),
            max_tick: self.max_tick,
            tick_wait: self.tick_wait,
            entity_dimensions,
            entities,
        }
    }
}

/// Active connection to a running engine process plus collected telemetry.
pub struct Session {
    child: Child,
    client: Option<Client>,
    log_buffer: Arc<Mutex<Vec<LogLine>>>,
    event_buffer: Arc<Mutex<Vec<ServerEvent>>>,
    log_collector: Option<thread::JoinHandle<()>>,
    event_collector: Option<thread::JoinHandle<()>>,
    max_tick: Arc<AtomicU64>,
    tick_wait: Duration,
    entity_dimensions: HashMap<u64, u32>,
    entities: Vec<EntitySummary>,
}

impl Session {
    /// Return the entities created during scenario setup.
    pub fn entities(&self) -> &[EntitySummary] {
        &self.entities
    }

    /// Refresh the cached entity list using a list request.
    pub fn refresh_entities(&mut self) -> HarnessResult<&[EntitySummary]> {
        let client = self.client.as_ref().ok_or(HarnessError::ConnectionClosed)?;
        let response = client.send(ServerRequest::List)?;
        let entities = match response {
            ServerResponse::Listed { status, entities } => {
                if status != ResponseStatus::Ok {
                    return Err(HarnessError::unexpected(format!(
                        "list failed with status {status:?}"
                    )));
                }
                entities
            }
            other => {
                return Err(HarnessError::unexpected(format!(
                    "list returned unexpected response: {other:?}"
                )))
            }
        };

        self.entity_dimensions.clear();
        for entity in &entities {
            self.entity_dimensions
                .insert(entity.entity_id, entity.dimension);
        }
        self.entities = entities;

        Ok(&self.entities)
    }

    /// Wait for the engine to progress by a number of ticks.
    ///
    /// If telemetry events are observed, this waits until the requested tick delta
    /// is reached. Otherwise it sleeps for a conservative fallback duration while
    /// ensuring the engine is still alive.
    pub fn advance_ticks(&mut self, ticks: u64) -> HarnessResult<()> {
        if ticks == 0 {
            return Ok(());
        }

        let start_tick = self.max_tick.load(Ordering::SeqCst);
        let target_tick = start_tick.saturating_add(ticks);
        let mut waited = Duration::ZERO;
        let tick_scale = u32::try_from(ticks.max(1)).unwrap_or(u32::MAX);
        let deadline = self.tick_wait.saturating_mul(tick_scale).saturating_mul(2);

        while waited <= deadline {
            if let Some(status) = self.child.try_wait()? {
                return Err(HarnessError::EngineExited(status));
            }

            if self.max_tick.load(Ordering::SeqCst) >= target_tick {
                return Ok(());
            }

            thread::sleep(self.tick_wait);
            waited += self.tick_wait;
        }

        // Fallback when telemetry is silent: still verify the process is running.
        if let Some(status) = self.child.try_wait()? {
            return Err(HarnessError::EngineExited(status));
        }
        if let Some(client) = &self.client {
            if !client.is_connected() {
                return Err(HarnessError::ConnectionClosed);
            }
        } else {
            return Err(HarnessError::ConnectionClosed);
        }

        Ok(())
    }

    /// Fetch the latest telemetry for an entity using an inspect request.
    pub fn telemetry_for(&self, entity_id: u64) -> HarnessResult<Option<EntityRecord>> {
        let dimension = match self.entity_dimensions.get(&entity_id) {
            Some(dimension) => *dimension,
            None => return Ok(None),
        };

        let client = self.client.as_ref().ok_or(HarnessError::ConnectionClosed)?;
        let response = client.send(ServerRequest::Inspect {
            dimension,
            entity_id,
        })?;

        match response {
            ServerResponse::InspectResult { entity, .. } => Ok(entity),
            other => Err(HarnessError::unexpected(format!(
                "inspect returned unexpected response: {other:?}"
            ))),
        }
    }

    /// Return all captured log lines for an entity id (matching telemetry events and stdout).
    pub fn logs_for(&self, entity_id: u64) -> Vec<LogLine> {
        let mut lines = Vec::new();
        let id_text = entity_id.to_string();

        if let Ok(buffer) = self.log_buffer.lock() {
            lines.extend(
                buffer
                    .iter()
                    .filter(|line| line.line.contains(&id_text))
                    .cloned(),
            );
        }

        if let Ok(events) = self.event_buffer.lock() {
            for event in events.iter() {
                match event {
                    ServerEvent::Telemetry {
                        id,
                        tick,
                        ship,
                        message,
                    } if *id == entity_id => {
                        lines.push(LogLine {
                            stream: LogStream::Event,
                            line: format!("tick {tick} [{ship}]: {message}"),
                        });
                    }
                    ServerEvent::Log { message } if message.contains(&id_text) => {
                        lines.push(LogLine {
                            stream: LogStream::Event,
                            line: message.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }

        lines
    }

    /// Return all captured log lines across streams.
    pub fn all_logs(&self) -> Vec<LogLine> {
        let mut lines = Vec::new();
        if let Ok(buffer) = self.log_buffer.lock() {
            lines.extend(buffer.iter().cloned());
        }
        if let Ok(events) = self.event_buffer.lock() {
            lines.extend(events.iter().filter_map(|event| match event {
                ServerEvent::Telemetry {
                    id,
                    tick,
                    ship,
                    message,
                } => Some(LogLine {
                    stream: LogStream::Event,
                    line: format!("entity {id} tick {tick} [{ship}]: {message}"),
                }),
                ServerEvent::Log { message } => Some(LogLine {
                    stream: LogStream::Event,
                    line: message.clone(),
                }),
            }));
        }
        lines
    }

    /// Request a graceful shutdown and wait for the engine process to exit.
    pub fn shutdown(mut self) -> HarnessResult<()> {
        self.request_shutdown()
    }

    fn request_shutdown(&mut self) -> HarnessResult<()> {
        if let Some(client) = &self.client {
            let _ = client.send(ServerRequest::Shutdown);
        }
        let start = Instant::now();
        let timeout = Duration::from_secs(2);
        while start.elapsed() < timeout {
            if let Some(_status) = self.child.try_wait()? {
                self.client.take();
                self.join_workers();
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }

        // Force terminate if graceful shutdown did not complete.
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.client.take();
        self.join_workers();
        Ok(())
    }

    fn join_workers(&mut self) {
        if let Some(handle) = self.log_collector.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.event_collector.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.request_shutdown();
    }
}

fn spawn_log_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    stream: LogStream,
    tx: mpsc::Sender<LogLine>,
) {
    thread::spawn(move || {
        let buf_reader = BufReader::new(reader);
        for line in buf_reader.lines().flatten() {
            let _ = tx.send(LogLine {
                stream,
                line: line.trim().to_string(),
            });
        }
    });
}

fn wait_for_listen_address(
    child: &mut Child,
    log_rx: &mpsc::Receiver<LogLine>,
    log_buffer: &Arc<Mutex<Vec<LogLine>>>,
    timeout: Duration,
) -> HarnessResult<SocketAddr> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(status) = child.try_wait()? {
            return Err(HarnessError::EngineExited(status));
        }

        match log_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
                if let Ok(mut buffer) = log_buffer.lock() {
                    buffer.push(line.clone());
                }
                if let Some(addr) = parse_listen_line(&line.line) {
                    return Ok(addr);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Err(HarnessError::StartupTimeout(timeout))
}

fn parse_listen_line(line: &str) -> Option<SocketAddr> {
    let needle = "listening on";
    let lower = line.to_ascii_lowercase();
    let idx = lower.find(needle)?;
    let after = line[idx + needle.len()..].trim();
    after.parse().ok()
}

fn spawn_log_collector(
    log_rx: mpsc::Receiver<LogLine>,
    buffer: Arc<Mutex<Vec<LogLine>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while let Ok(line) = log_rx.recv() {
            if let Ok(mut guard) = buffer.lock() {
                guard.push(line);
            }
        }
    })
}

fn spawn_event_collector(
    event_rx: mpsc::Receiver<ServerEvent>,
    buffer: Arc<Mutex<Vec<ServerEvent>>>,
    max_tick: Arc<AtomicU64>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while let Ok(event) = event_rx.recv() {
            if let Ok(mut guard) = buffer.lock() {
                guard.push(event.clone());
            }

            if let ServerEvent::Telemetry { tick, .. } = event {
                max_tick.fetch_max(tick, Ordering::SeqCst);
            }
        }
    })
}
