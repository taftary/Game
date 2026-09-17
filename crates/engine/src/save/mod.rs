//! ADR-004 binary autosave: envelope codec + atomic store.
//!
//! Procedural content is never saved (ADR-019 determinism) — the payload is
//! the frozen [`crate::flight::ShipSnapshot`] plus metadata. Writes are
//! atomic (temp + rename) with a rotating recovery ring; corrupt saves are
//! quarantined and never boot-loop (persistence contract).

pub mod format;
pub mod store;

pub use format::{
    ENVELOPE_VERSION, MAGIC, MAX_CATALOG_VERSIONS, MAX_VERSION_STAMP_LEN, SaveEnvelope, SaveError,
    SaveMetadata, decode, encode, fnv1a64,
};
pub use store::{AutosaveRing, DEFAULT_SLOTS, LoadedSave, StoreError};
