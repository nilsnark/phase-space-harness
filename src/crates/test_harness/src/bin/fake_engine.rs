use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use phase_space_protocol::network::NetworkMessage;
use phase_space_protocol::psip::{
    EntityParameters, EntityRecord, EntitySummary, RequestEnvelope, ResponseEnvelope,
    ResponseStatus, ServerEvent, ServerRequest, ServerResponse,
};
use serde::Serialize;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    println!("listening on {addr}");

    let (mut stream, _) = listener.accept()?;
    stream.set_read_timeout(Some(Duration::from_millis(50)))?;
    let writer = stream.try_clone()?;

    let entities: Arc<Mutex<BTreeMap<u64, EntityRecord>>> = Arc::new(Mutex::new(BTreeMap::new()));
    let next_id = Arc::new(AtomicU64::new(1));
    let running = Arc::new(AtomicBool::new(true));
    let tick_counter = Arc::new(AtomicU64::new(0));
    let (event_tx, event_rx) = mpsc::channel();

    let telemetry_handle = spawn_telemetry_thread(
        writer,
        event_rx,
        running.clone(),
        tick_counter.clone(),
        entities.clone(),
    );

    loop {
        match read_frame(&mut stream) {
            Ok(frame) => {
                let message = NetworkMessage::from_bytes(&frame)?;
                let envelope: RequestEnvelope = serde_json::from_slice(&message.payload)?;
                let response = handle_request(
                    envelope,
                    &entities,
                    &next_id,
                    &running,
                    &event_tx,
                    &tick_counter,
                );
                let framed = encode_payload(&response)?;
                write_framed(&mut stream, &framed)?;
                if !running.load(Ordering::SeqCst) {
                    break;
                }
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                continue;
            }
            Err(_) => break,
        }
    }

    running.store(false, Ordering::SeqCst);
    drop(event_tx);
    let _ = telemetry_handle.join();
    Ok(())
}

fn handle_request(
    envelope: RequestEnvelope,
    entities: &Arc<Mutex<BTreeMap<u64, EntityRecord>>>,
    next_id: &Arc<AtomicU64>,
    running: &Arc<AtomicBool>,
    event_tx: &mpsc::Sender<ServerEvent>,
    tick_counter: &Arc<AtomicU64>,
) -> ResponseEnvelope {
    let response = match envelope.payload {
        ServerRequest::Spawn {
            entity_type,
            parameters,
            dimension,
        } => {
            let entity = register_entity(entities, next_id, &entity_type, parameters, dimension);
            ServerResponse::Spawned {
                status: ResponseStatus::Ok,
                entity,
            }
        }
        ServerRequest::List => {
            let list = entities
                .lock()
                .map(|map| {
                    map.values()
                        .map(|record| EntitySummary {
                            dimension: record.dimension,
                            entity_id: record.entity_id,
                            kind: record.kind.clone(),
                            position: record.position,
                        })
                        .collect()
                })
                .unwrap_or_default();

            ServerResponse::Listed {
                status: ResponseStatus::Ok,
                entities: list,
            }
        }
        ServerRequest::Inspect {
            dimension: _,
            entity_id,
        } => {
            let record = entities
                .lock()
                .ok()
                .and_then(|map| map.get(&entity_id).cloned());
            let status = if record.is_some() {
                ResponseStatus::Ok
            } else {
                ResponseStatus::NotFound
            };
            ServerResponse::InspectResult {
                status,
                entity: record,
                message: None,
            }
        }
        ServerRequest::Shutdown => {
            running.store(false, Ordering::SeqCst);
            ServerResponse::Shutdown {
                status: ResponseStatus::Ok,
                message: Some("shutdown requested".to_string()),
            }
        }
    };

    // Send a telemetry event for each request to keep tick counts advancing.
    let tick = tick_counter.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = event_tx.send(build_event(tick, entities));

    ResponseEnvelope {
        id: envelope.id,
        payload: response,
    }
}

fn register_entity(
    entities: &Arc<Mutex<BTreeMap<u64, EntityRecord>>>,
    next_id: &Arc<AtomicU64>,
    entity_type: &str,
    parameters: EntityParameters,
    dimension: Option<u32>,
) -> EntitySummary {
    let id = next_id.fetch_add(1, Ordering::SeqCst);
    let dimension_id = dimension.unwrap_or(0);
    let record = EntityRecord {
        dimension: dimension_id,
        entity_id: id,
        kind: entity_type.to_string(),
        position: parameters.position,
        velocity: parameters.velocity,
        mass: parameters.mass,
    };

    if let Ok(mut map) = entities.lock() {
        map.insert(id, record.clone());
    }

    EntitySummary {
        dimension: dimension_id,
        entity_id: id,
        kind: entity_type.to_string(),
        position: record.position,
    }
}

fn build_event(tick: u64, entities: &Arc<Mutex<BTreeMap<u64, EntityRecord>>>) -> ServerEvent {
    let id = entities
        .lock()
        .ok()
        .and_then(|map| map.keys().next().copied())
        .unwrap_or(0);
    ServerEvent::Telemetry {
        id,
        tick,
        ship: "fake".to_string(),
        message: format!("tick {tick}"),
    }
}

fn spawn_telemetry_thread(
    mut writer: std::net::TcpStream,
    event_rx: mpsc::Receiver<ServerEvent>,
    running: Arc<AtomicBool>,
    tick_counter: Arc<AtomicU64>,
    entities: Arc<Mutex<BTreeMap<u64, EntityRecord>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while running.load(Ordering::SeqCst) {
            match event_rx.recv_timeout(Duration::from_millis(10)) {
                Ok(event) => {
                    if let ServerEvent::Telemetry { tick, .. } = event {
                        tick_counter.fetch_max(tick, Ordering::SeqCst);
                    }
                    if let Ok(bytes) = encode_payload(&event) {
                        if write_framed(&mut writer, &bytes).is_err() {
                            break;
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let tick = tick_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    let event = build_event(tick, &entities);
                    if let Ok(bytes) = encode_payload(&event) {
                        if write_framed(&mut writer, &bytes).is_err() {
                            break;
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    })
}

fn encode_payload<T: Serialize>(payload: &T) -> Result<Vec<u8>, serde_json::Error> {
    let payload_bytes = serde_json::to_vec(payload)?;
    let message = NetworkMessage::new(0, payload_bytes);
    let bytes = message.to_bytes()?;

    let mut framed = Vec::with_capacity(4 + bytes.len());
    framed.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    framed.extend_from_slice(&bytes);
    Ok(framed)
}

fn read_frame(stream: &mut std::net::TcpStream) -> std::io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let msg_len = u32::from_be_bytes(len_buf) as usize;
    let mut msg_buf = vec![0u8; msg_len];
    stream.read_exact(&mut msg_buf)?;
    Ok(msg_buf)
}

fn write_framed(stream: &mut std::net::TcpStream, framed: &[u8]) -> std::io::Result<()> {
    stream.write_all(framed)?;
    stream.flush()
}
