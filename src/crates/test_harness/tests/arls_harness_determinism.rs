#![cfg(feature = "test-support")]

#[path = "arls_harness_support.rs"]
mod support;

use support::{hash_prefix, start_session};

#[test]
fn replay_same_seed_yields_same_digest() {
    let first = run_digests(7777, 6);
    let second = run_digests(7777, 6);

    assert!(
        !first.is_empty(),
        "harness should surface per-tick digests from engine telemetry"
    );
    assert_eq!(first, second);
}

fn run_digests(seed: u64, ticks: u64) -> Vec<(u64, String)> {
    let (mut session, _tmp) = start_session(seed, None, None).expect("engine should start");
    session
        .advance_ticks(ticks)
        .expect("engine ticks should advance");
    let hashes = hash_prefix(&session, ticks as usize);
    session.shutdown().expect("shutdown should succeed");
    hashes
}
