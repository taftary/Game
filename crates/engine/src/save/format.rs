//! ADR-004 save envelope: magic + format version + metadata + frozen
//! [`ShipSnapshot`] payload + FNV-1a 64 checksum trailer.
//!
//! Layout (all integers little-endian):
//!
//! ```text
//! offset  field
//! 0       magic "GSAVE" (5 B)
//! 5       envelope version (u8) = 1
//! 6       master seed (u64)
//! 14      real-world save timestamp, unix seconds (i64, metadata only)
//! 22      simulation timestamp (f64, seconds)
//! 30      session playtime (f64, seconds)
//! 38      catalog version count n (u8, ≤ 16)
//! 39      n × (u8 len + UTF-8 bytes) catalog version IDs (≤ 64 B each)
//! …       payload length (u16; 131 or 204)
//! …       ShipSnapshot payload
//! …       FNV-1a 64 checksum over every preceding byte (u64)
//! ```
//!
//! Decode order (ADR-004 consequence): magic/length → **checksum** →
//! version → parse. Corrupt input fails clean, never panics.

use crate::flight::{ShipSnapshot, SnapshotError};

/// Envelope magic, ASCII `GSAVE`.
pub const MAGIC: &[u8; 5] = b"GSAVE";
/// Format version. Format v1 has no predecessors (ADR-004).
pub const ENVELOPE_VERSION: u8 = 1;
/// Bound on catalog version stamps (untrusted input stays bounded).
pub const MAX_CATALOG_VERSIONS: usize = 16;
/// Bound on one version stamp's UTF-8 length.
pub const MAX_VERSION_STAMP_LEN: usize = 64;
const CHECKSUM_LEN: usize = 8;
const HEADER_LEN: usize = 39; // magic + version + seed + ts + sim + playtime + count

/// Save metadata: identity + timestamps + catalog epoch markers.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveMetadata {
    /// Universe master seed (regeneration key — content itself unsaved).
    pub master_seed: u64,
    /// Real-world save time (unix seconds). Metadata only: excluded from
    /// resume determinism by construction.
    pub real_unix_s: i64,
    /// Simulation timestamp at save (seconds).
    pub sim_time_s: f64,
    /// Accumulated session playtime (seconds).
    pub playtime_s: f64,
    /// Data-catalog version IDs (ADR-020), e.g. `gaia-dr3-synth/1.0`.
    pub catalog_versions: Vec<String>,
}

/// One autosave: metadata + ship/navigation payload.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveEnvelope {
    pub metadata: SaveMetadata,
    pub ship: ShipSnapshot,
}

/// Envelope decode/encode failure. Every variant is a clean rejection —
/// the corrupt-save contract forbids panics on untrusted bytes.
#[derive(Clone, Debug, PartialEq)]
pub enum SaveError {
    /// Magic bytes do not match `GSAVE`.
    BadMagic,
    /// Buffer ends before the declared layout.
    Truncated {
        /// Bytes the layout requires.
        needed: usize,
        /// Bytes actually present.
        have: usize,
    },
    /// Checksum mismatch (computed vs trailer).
    BadChecksum {
        /// Checksum stored in the trailer.
        expected: u64,
        /// Checksum computed over the body.
        actual: u64,
    },
    /// Unknown envelope format version.
    BadVersion(u8),
    /// Metadata failed bounds/finiteness/UTF-8 validation.
    BadMetadata(&'static str),
    /// Ship payload rejected by its own codec.
    BadPayload(SnapshotError),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::BadMagic => write!(f, "not a GSAVE autosave file"),
            SaveError::Truncated { needed, have } => {
                write!(f, "save truncated: need {needed} bytes, have {have}")
            }
            SaveError::BadChecksum { expected, actual } => write!(
                f,
                "save checksum mismatch: stored {expected:016x}, computed {actual:016x}"
            ),
            SaveError::BadVersion(v) => write!(f, "unknown save format version {v}"),
            SaveError::BadMetadata(why) => write!(f, "bad save metadata: {why}"),
            SaveError::BadPayload(e) => write!(f, "bad ship payload: {e}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// FNV-1a 64 (offset `cbf29ce484222325`, prime `100000001b3`) —
/// corruption/truncation integrity, not an adversarial MAC.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn validate_metadata(metadata: &SaveMetadata) -> Result<(), SaveError> {
    if !metadata.sim_time_s.is_finite() {
        return Err(SaveError::BadMetadata("non-finite sim timestamp"));
    }
    if !(metadata.playtime_s.is_finite() && metadata.playtime_s >= 0.0) {
        return Err(SaveError::BadMetadata("non-finite or negative playtime"));
    }
    if metadata.catalog_versions.len() > MAX_CATALOG_VERSIONS {
        return Err(SaveError::BadMetadata("too many catalog versions"));
    }
    for stamp in &metadata.catalog_versions {
        if stamp.is_empty() || stamp.len() > MAX_VERSION_STAMP_LEN {
            return Err(SaveError::BadMetadata("catalog version stamp length"));
        }
    }
    Ok(())
}

/// Encode an envelope. Metadata is validated so every encoder output
/// decodes; the checksum covers the whole body.
pub fn encode(envelope: &SaveEnvelope) -> Result<Vec<u8>, SaveError> {
    validate_metadata(&envelope.metadata)?;
    let payload = envelope.ship.clone().to_bytes();
    let metadata = &envelope.metadata;
    let mut out = Vec::with_capacity(HEADER_LEN + 64 + payload.len() + CHECKSUM_LEN);
    out.extend_from_slice(MAGIC);
    out.push(ENVELOPE_VERSION);
    out.extend_from_slice(&metadata.master_seed.to_le_bytes());
    out.extend_from_slice(&metadata.real_unix_s.to_le_bytes());
    out.extend_from_slice(&metadata.sim_time_s.to_le_bytes());
    out.extend_from_slice(&metadata.playtime_s.to_le_bytes());
    out.push(metadata.catalog_versions.len() as u8);
    for stamp in &metadata.catalog_versions {
        out.push(stamp.len() as u8);
        out.extend_from_slice(stamp.as_bytes());
    }
    out.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    out.extend_from_slice(&payload);
    let checksum = fnv1a64(&out);
    out.extend_from_slice(&checksum.to_le_bytes());
    Ok(out)
}

/// Decode an envelope: magic/length → checksum → version → parse
/// (ADR-004). Unknown versions and corrupt bytes reject cleanly.
pub fn decode(bytes: &[u8]) -> Result<SaveEnvelope, SaveError> {
    if bytes.len() < HEADER_LEN + CHECKSUM_LEN {
        return Err(SaveError::Truncated {
            needed: HEADER_LEN + CHECKSUM_LEN,
            have: bytes.len(),
        });
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err(SaveError::BadMagic);
    }
    let body = &bytes[..bytes.len() - CHECKSUM_LEN];
    let stored = u64::from_le_bytes(
        bytes[bytes.len() - CHECKSUM_LEN..]
            .try_into()
            .expect("length checked"),
    );
    let computed = fnv1a64(body);
    if stored != computed {
        return Err(SaveError::BadChecksum {
            expected: stored,
            actual: computed,
        });
    }
    let version = bytes[MAGIC.len()];
    if version != ENVELOPE_VERSION {
        return Err(SaveError::BadVersion(version));
    }
    let u64_at = |range: std::ops::Range<usize>| {
        u64::from_le_bytes(bytes[range].try_into().expect("header length checked"))
    };
    let master_seed = u64_at(6..14);
    let real_unix_s = u64_at(14..22) as i64;
    let sim_time_s = f64::from_bits(u64_at(22..30));
    let playtime_s = f64::from_bits(u64_at(30..38));
    let count = usize::from(bytes[38]);
    if count > MAX_CATALOG_VERSIONS {
        return Err(SaveError::BadMetadata("too many catalog versions"));
    }
    let mut at = HEADER_LEN;
    let mut catalog_versions = Vec::with_capacity(count);
    for _ in 0..count {
        if at >= body.len() {
            return Err(SaveError::Truncated {
                needed: at + 1,
                have: body.len(),
            });
        }
        let len = usize::from(bytes[at]);
        at += 1;
        if len == 0 || len > MAX_VERSION_STAMP_LEN {
            return Err(SaveError::BadMetadata("catalog version stamp length"));
        }
        if at + len > body.len() {
            return Err(SaveError::Truncated {
                needed: at + len,
                have: body.len(),
            });
        }
        let stamp = std::str::from_utf8(&bytes[at..at + len])
            .map_err(|_| SaveError::BadMetadata("catalog version not UTF-8"))?;
        catalog_versions.push(stamp.to_owned());
        at += len;
    }
    if at + 2 > body.len() {
        return Err(SaveError::Truncated {
            needed: at + 2,
            have: body.len(),
        });
    }
    let payload_len = usize::from(u16::from_le_bytes(
        bytes[at..at + 2].try_into().expect("length checked"),
    ));
    at += 2;
    if at + payload_len != body.len() {
        return Err(SaveError::Truncated {
            needed: at + payload_len,
            have: body.len(),
        });
    }
    let ship =
        ShipSnapshot::from_bytes(&bytes[at..at + payload_len]).map_err(SaveError::BadPayload)?;
    let metadata = SaveMetadata {
        master_seed,
        real_unix_s,
        sim_time_s,
        playtime_s,
        catalog_versions,
    };
    validate_metadata(&metadata)?;
    Ok(SaveEnvelope { metadata, ship })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flight::{FlyToExec, ShipState, Target, plan_fly_to};
    use crate::frames::{FrameChain, FrameId, FrameLink, TransitionReason};
    use crate::time::CompressionClock;
    use glam::{DQuat, DVec3};

    fn ship_at(frame: FrameId, depth_links: usize, x: f64) -> ShipState {
        ShipState {
            chain: FrameChain::new(
                frame,
                DVec3::new(x, 0.5, -0.25),
                DQuat::IDENTITY,
                vec![FrameLink::identity(); depth_links],
            ),
            vel: DVec3::new(0.0, 0.1, 0.0),
            mass_kg: 5_000.0,
            fuel: f64::INFINITY,
        }
    }

    fn metadata() -> SaveMetadata {
        SaveMetadata {
            master_seed: 99,
            real_unix_s: 1_800_000_000,
            sim_time_s: 12.5,
            playtime_s: 61.0,
            catalog_versions: vec!["gaia-dr3-synth/1.0".to_owned()],
        }
    }

    fn envelope_for(ship: &ShipState, clock: &CompressionClock) -> SaveEnvelope {
        SaveEnvelope {
            metadata: metadata(),
            ship: ShipSnapshot::capture(ship, None, clock),
        }
    }

    #[test]
    fn round_trip_mid_flight_is_identical() {
        let ship = ship_at(FrameId::SolarSystem, 4, 2.0);
        let clock = CompressionClock::new();
        let envelope = envelope_for(&ship, &clock);
        let bytes = encode(&envelope).expect("encode");
        assert_eq!(decode(&bytes), Ok(envelope));
    }

    #[test]
    fn round_trip_mid_fly_to_resumes_exactly() {
        let ship = ship_at(FrameId::SolarSystem, 4, 2.0);
        let clock = CompressionClock::new();
        let target = Target::new(FrameId::SolarSystem, DVec3::new(5.0, 0.0, 0.0)).unwrap();
        let plan = plan_fly_to(FrameId::SolarSystem, ship.chain.position(), &target, 3.0).unwrap();
        let mut exec = FlyToExec::new(plan);
        exec.commit().unwrap();
        let envelope = SaveEnvelope {
            metadata: metadata(),
            ship: ShipSnapshot::capture(&ship, Some(exec.plan()), &clock),
        };
        let bytes = encode(&envelope).expect("encode");
        let loaded = decode(&bytes).expect("decode");
        assert_eq!(loaded, envelope);
        // Resume: the restored plan produces the same eased state at any t.
        let restored = loaded.ship.plan.expect("plan saved");
        let mut restored_exec = FlyToExec::new(restored);
        restored_exec.commit().unwrap();
        for t in [3.0, 10.0, 40.0] {
            assert_eq!(restored_exec.update(t), exec.update(t));
        }
    }

    #[test]
    fn round_trip_post_handoff_is_identical() {
        let mut ship = ship_at(FrameId::SolarSystem, 4, 2.0);
        let clock = CompressionClock::new();
        ship.commit_to_parent(TransitionReason::BoundaryCrossing, 7.0)
            .expect("solar -> neighborhood commit");
        assert_eq!(ship.chain.active(), FrameId::StellarNeighborhood);
        let envelope = envelope_for(&ship, &clock);
        let bytes = encode(&envelope).expect("encode");
        assert_eq!(decode(&bytes), Ok(envelope));
    }

    #[test]
    fn corrupt_inputs_reject_cleanly() {
        let ship = ship_at(FrameId::SolarSystem, 4, 2.0);
        let clock = CompressionClock::new();
        let bytes = encode(&envelope_for(&ship, &clock)).expect("encode");
        // Truncation: caught by length or checksum, never a panic.
        for cut in [0, 3, 10, bytes.len() / 2, bytes.len() - 9] {
            assert!(decode(&bytes[..cut]).is_err(), "cut {cut} must reject");
        }
        // Bit flip in the body: checksum mismatch.
        let mut flipped = bytes.clone();
        flipped[20] ^= 0xFF;
        assert!(matches!(
            decode(&flipped),
            Err(SaveError::BadChecksum { .. })
        ));
        // Wrong magic.
        let mut magic = bytes.clone();
        magic[0] = b'X';
        assert_eq!(decode(&magic), Err(SaveError::BadMagic));
        // Wrong version (recompute the checksum so the version check fires).
        let mut versioned = bytes.clone();
        versioned[5] = 99;
        let body_len = versioned.len() - 8;
        let sum = fnv1a64(&versioned[..body_len]);
        versioned[body_len..].copy_from_slice(&sum.to_le_bytes());
        assert_eq!(decode(&versioned), Err(SaveError::BadVersion(99)));
    }

    #[test]
    fn metadata_bounds_hold() {
        let ship = ship_at(FrameId::SolarSystem, 4, 2.0);
        let clock = CompressionClock::new();
        let mut too_many = metadata();
        too_many.catalog_versions = (0..17).map(|i| format!("v{i}")).collect();
        let envelope = SaveEnvelope {
            metadata: too_many,
            ship: ShipSnapshot::capture(&ship, None, &clock),
        };
        assert!(matches!(encode(&envelope), Err(SaveError::BadMetadata(_))));
        let mut negative_playtime = metadata();
        negative_playtime.playtime_s = -1.0;
        let envelope = SaveEnvelope {
            metadata: negative_playtime,
            ship: ShipSnapshot::capture(&ship, None, &clock),
        };
        assert!(matches!(encode(&envelope), Err(SaveError::BadMetadata(_))));
    }
}
