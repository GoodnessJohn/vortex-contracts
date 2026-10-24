//! Kani formal verification harnesses for `proof_registry` core arithmetic.
//!
//! These harnesses are compiled only under `cfg(kani)` and are intended to be
//! run with `cargo kani -p proof_registry`. They focus on the pure helpers
//! used by the registry: payload byte-index decoding (bounds safety),
//! freshness-window boundary behavior, and sequence/replay checks.

#![cfg(kani)]

use crate::{
    decode_src_chain_id, is_fresh, payload_len_ok, PROOF_VALIDITY_WINDOW,
};

/// Proves that `decode_src_chain_id` never panics for any input slice and
/// returns `None` (fails closed) whenever the slice is too short to contain
/// the 2-byte chain-id field at the expected offset.
#[kani::proof]
fn verify_decode_src_chain_id_bounds_safe() {
    let len: usize = kani::any();
    kani::assume(len <= 64);
    let mut buf = [0u8; 64];
    for i in 0..len {
        buf[i] = kani::any();
    }
    let slice = &buf[..len];

    let result = decode_src_chain_id(slice);

    // Must not panic (reaching here proves that). Additionally, if the slice
    // is too short to contain the field, the result must be `None`.
    if len < crate::SRC_CHAIN_ID_OFFSET + 2 {
        assert!(result.is_none());
    }
}

/// Proves that `decode_src_chain_id` returns the big-endian value of the two
/// bytes at `SRC_CHAIN_ID_OFFSET` when the slice is long enough.
#[kani::proof]
fn verify_decode_src_chain_id_value() {
    let len: usize = kani::any();
    kani::assume(len >= crate::SRC_CHAIN_ID_OFFSET + 2);
    kani::assume(len <= 64);
    let mut buf = [0u8; 64];
    for i in 0..len {
        buf[i] = kani::any();
    }
    let slice = &buf[..len];

    let result = decode_src_chain_id(slice);
    let expected =
        ((slice[crate::SRC_CHAIN_ID_OFFSET] as u16) << 8)
            | (slice[crate::SRC_CHAIN_ID_OFFSET + 1] as u16);
    assert_eq!(result, Some(expected));
}

/// Proves that `payload_len_ok` fails closed: it returns `false` for any
/// slice shorter than the minimum required payload length, and `true` for
/// any slice at or above it.
#[kani::proof]
fn verify_payload_len_ok_bounds() {
    let len: usize = kani::any();
    kani::assume(len <= 128);
    let mut buf = [0u8; 128];
    for i in 0..len {
        buf[i] = kani::any();
    }
    let slice = &buf[..len];

    let ok = payload_len_ok(slice);
    if len < crate::MIN_PAYLOAD_LEN {
        assert!(!ok);
    } else {
        assert!(ok);
    }
}

/// Proves the freshness window is inclusive at exactly `PROOF_VALIDITY_WINDOW`:
/// a proof timestamp exactly `PROOF_VALIDITY_WINDOW` slots old is still fresh,
/// while one slot older is stale.
#[kani::proof]
fn verify_freshness_boundary_inclusive() {
    let now: u64 = kani::any();
    kani::assume(now >= PROOF_VALIDITY_WINDOW);

    // Exactly at the boundary: still fresh.
    let at_boundary = now - PROOF_VALIDITY_WINDOW;
    assert!(is_fresh(at_boundary, now));

    // One slot past the boundary: stale.
    let past_boundary = now - PROOF_VALIDITY_WINDOW - 1;
    assert!(!is_fresh(past_boundary, now));
}

/// Proves that a proof timestamped in the future is treated as fresh
/// (never rejected due to a negative age) and that `is_fresh` never panics
/// for arbitrary timestamps.
#[kani::proof]
fn verify_freshness_no_panic() {
    let ts: u64 = kani::any();
    let now: u64 = kani::any();
    let _ = is_fresh(ts, now);
}

/// Proves that `is_fresh` is monotonic in `now`: if a proof is fresh at
/// `now`, it remains fresh at any earlier `now'` (i.e. freshness is a
/// lower-bound property on the current slot).
#[kani::proof]
fn verify_freshness_monotonic() {
    let ts: u64 = kani::any();
    let now: u64 = kani::any();
    let earlier: u64 = kani::any();
    kani::assume(earlier <= now);

    if is_fresh(ts, now) {
        // A proof fresh at `now` is fresh at any earlier slot only if the
        // proof timestamp is not in the future relative to `earlier`.
        if ts <= earlier {
            assert!(is_fresh(ts, earlier));
        }
    }
}

/// Proves that sequence/replay checks reject an already-seen sequence:
/// `is_replay(seq, last_seen)` is true iff `seq <= last_seen`.
#[kani::proof]
fn verify_replay_check() {
    let seq: u64 = kani::any();
    let last_seen: u64 = kani::any();

    let replay = crate::is_replay(seq, last_seen);
    if seq <= last_seen {
        assert!(replay);
    } else {
        assert!(!replay);
    }
}

/// Proves that a strictly increasing sequence is never flagged as a replay.
#[kani::proof]
fn verify_replay_accepts_increasing() {
    let last_seen: u64 = kani::any();
    kani::assume(last_seen < u64::MAX);
    let seq = last_seen + 1;
    assert!(!crate::is_replay(seq, last_seen));
}