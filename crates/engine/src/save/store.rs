//! Atomic autosave store: temp + rename writes, rotating recovery ring,
//! quarantine-on-corruption loading (ADR-004). Thin fs layer, same
//! precedent as `catalog::io`. No panics on I/O errors — the corrupt-save
//! contract is backup + clean error, never boot-loop.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::format::{SaveEnvelope, decode};

/// Default rotation depth (ADR-004: at least two recovery snapshots).
pub const DEFAULT_SLOTS: usize = 3;

/// Store failure: an I/O error, or no valid snapshot in the ring.
#[derive(Debug)]
pub enum StoreError {
    /// Filesystem failure.
    Io(io::Error),
    /// Every slot is missing or corrupt (all corrupt ones quarantined).
    NoValidSave,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "save store I/O error: {e}"),
            StoreError::NoValidSave => write!(f, "no valid autosave snapshot in the ring"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(error: io::Error) -> Self {
        StoreError::Io(error)
    }
}

/// Rotating autosave ring: `autosave.0.bin` is newest; higher slots are
/// older recovery snapshots. File names are fixed — no caller-controlled
/// path components ever reach the disk.
pub struct AutosaveRing {
    dir: PathBuf,
    slots: usize,
}

/// A successfully loaded snapshot plus recovery diagnostics.
pub struct LoadedSave {
    /// Decoded envelope.
    pub envelope: SaveEnvelope,
    /// Ring slot it came from (0 = newest).
    pub slot: usize,
    /// How many newer-but-corrupt slots were quarantined along the way.
    pub quarantined: usize,
}

impl AutosaveRing {
    /// Ring rooted at `dir` with at least two slots (ADR-004 floor).
    pub fn new(dir: impl Into<PathBuf>, slots: usize) -> Self {
        Self {
            dir: dir.into(),
            slots: slots.max(2),
        }
    }

    /// Ring directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn slot_path(&self, slot: usize) -> PathBuf {
        self.dir.join(format!("autosave.{slot}.bin"))
    }

    /// Persist `bytes` as the newest snapshot, rotating older ones up one
    /// slot (the oldest falls off). The new file lands atomically via
    /// temp + rename; rotation uses plain renames of whole files.
    pub fn write(&self, bytes: &[u8]) -> io::Result<PathBuf> {
        fs::create_dir_all(&self.dir)?;
        for slot in (1..self.slots).rev() {
            let from = self.slot_path(slot - 1);
            if from.exists() {
                let to = self.slot_path(slot);
                if to.exists() {
                    fs::remove_file(&to)?;
                }
                fs::rename(&from, &to)?;
            }
        }
        let target = self.slot_path(0);
        let tmp = self.dir.join("autosave.tmp");
        fs::write(&tmp, bytes)?;
        if target.exists() {
            fs::remove_file(&target)?;
        }
        fs::rename(&tmp, &target)?;
        Ok(target)
    }

    /// Load the newest valid snapshot, newest slot first. Corrupt files
    /// are quarantined aside (never deleted, never boot-looped on) and the
    /// next-older slot is tried. `NoValidSave` only when nothing valid
    /// remains.
    pub fn load_latest(&self) -> Result<LoadedSave, StoreError> {
        let mut quarantined = 0;
        for slot in 0..self.slots {
            let path = self.slot_path(slot);
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(StoreError::Io(error)),
            };
            match decode(&bytes) {
                Ok(envelope) => {
                    return Ok(LoadedSave {
                        envelope,
                        slot,
                        quarantined,
                    });
                }
                Err(_) => {
                    quarantined += 1;
                    // Quarantine is best-effort: a locked file must not
                    // block recovery from older slots.
                    let _ = quarantine(&path);
                }
            }
        }
        Err(StoreError::NoValidSave)
    }
}

/// Move a corrupt file aside: `autosave.N.bin` → `autosave.N.bin.corrupt`
/// (numbered suffix when the plain name is taken).
fn quarantine(path: &Path) -> io::Result<PathBuf> {
    let mut candidate = path.with_extension("bin.corrupt");
    let mut n = 1;
    while candidate.exists() {
        candidate = path.with_extension(format!("bin.corrupt.{n}"));
        n += 1;
    }
    fs::rename(path, &candidate)?;
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flight::{ShipSnapshot, ShipState};
    use crate::frames::{FrameChain, FrameId, FrameLink};
    use crate::save::format::{SaveMetadata, encode};
    use crate::time::CompressionClock;
    use glam::{DQuat, DVec3};

    /// Hand-rolled unique temp dir (no new dependencies).
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "game_save_test_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn envelope(tag: u64) -> Vec<u8> {
        let ship = ShipState {
            chain: FrameChain::new(
                FrameId::SolarSystem,
                DVec3::new(tag as f64, 0.0, 0.0),
                DQuat::IDENTITY,
                vec![FrameLink::identity(); 4],
            ),
            vel: DVec3::ZERO,
            mass_kg: 5_000.0,
            fuel: f64::INFINITY,
        };
        encode(&SaveEnvelope {
            metadata: SaveMetadata {
                master_seed: tag,
                real_unix_s: 1_800_000_000,
                sim_time_s: tag as f64,
                playtime_s: 1.0,
                catalog_versions: vec!["gaia-dr3-synth/1.0".to_owned()],
            },
            ship: ShipSnapshot::capture(&ship, None, &CompressionClock::new()),
        })
        .expect("encode")
    }

    #[test]
    fn rotation_keeps_newest_first_and_drops_oldest() {
        let dir = temp_dir("rotate");
        let ring = AutosaveRing::new(&dir, 3);
        ring.write(&envelope(1)).expect("write 1");
        ring.write(&envelope(2)).expect("write 2");
        ring.write(&envelope(3)).expect("write 3");
        ring.write(&envelope(4)).expect("write 4");
        let latest = ring.load_latest().expect("latest");
        assert_eq!(latest.slot, 0);
        assert_eq!(latest.envelope.metadata.master_seed, 4);
        // Slot 2 is the oldest retained: seed 2 (seed 1 rotated out).
        let oldest = fs::read(dir.join("autosave.2.bin")).expect("slot 2");
        assert_eq!(
            decode(&oldest).expect("decode").metadata.master_seed,
            2,
            "oldest retained snapshot"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_newest_quarantines_and_falls_back() {
        let dir = temp_dir("corrupt");
        let ring = AutosaveRing::new(&dir, 3);
        ring.write(&envelope(1)).expect("write 1");
        ring.write(&envelope(2)).expect("write 2");
        // Truncate the newest slot: checksum/length rejection on load.
        fs::write(dir.join("autosave.0.bin"), &envelope(3)[..10]).expect("corrupt");
        let loaded = ring.load_latest().expect("fallback must succeed");
        assert_eq!(loaded.slot, 1);
        assert_eq!(loaded.quarantined, 1);
        assert_eq!(loaded.envelope.metadata.master_seed, 1);
        assert!(
            dir.join("autosave.0.bin.corrupt").exists(),
            "corrupt file quarantined, not deleted"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn all_corrupt_is_clean_no_valid_save() {
        let dir = temp_dir("all_corrupt");
        let ring = AutosaveRing::new(&dir, 2);
        ring.write(&envelope(1)).expect("write 1");
        ring.write(&envelope(2)).expect("write 2");
        for slot in 0..2 {
            fs::write(dir.join(format!("autosave.{slot}.bin")), b"junk").expect("corrupt");
        }
        assert!(matches!(ring.load_latest(), Err(StoreError::NoValidSave)));
        assert!(dir.join("autosave.0.bin.corrupt").exists());
        assert!(dir.join("autosave.1.bin.corrupt").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_ring_reports_no_valid_save() {
        let dir = temp_dir("empty");
        let ring = AutosaveRing::new(&dir, 3);
        assert!(matches!(ring.load_latest(), Err(StoreError::NoValidSave)));
        fs::remove_dir_all(&dir).ok();
    }
}
