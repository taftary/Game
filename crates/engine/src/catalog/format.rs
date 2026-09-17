//! Binary catalog tile format v1 (spec §4, ADR-017).
//!
//! One HEALPix tile per file: a fixed header (identity, order, pixel,
//! source count, tile means, epoch, catalog version) plus fixed-stride
//! source records. Little-endian throughout; decode validates
//! magic/version/bounds and returns a typed error — corrupt bytes never
//! panic (`TileError`), which is the SECURITY contract for both the
//! runtime loader and the cooker ingest path.
//!
//! Layout (bytes):
//!
//! ```text
//! magic u32 = GSCT | version u16 = 1 | order u8 | pixel u64
//! count u32 | mean_g_mag f32 | mean_bp_rp f32 | epoch_yr f64
//! catalog_version u8 len + UTF-8 bytes (≤ 64)
//! records: source_id u64 | ra_rad f64 | dec_rad f64
//!          pm_ra_mas_yr f32 | pm_dec_mas_yr f32 | g_mag f32 | bp_rp f32
//! ```
//!
//! ```
//! use game_engine::catalog::format::{CATALOG_VERSION_SYNTH, StarRecord, TileHeader, decode_tile, encode_tile};
//!
//! let header = TileHeader {
//!     order: 12,
//!     pixel: 42,
//!     mean_g_mag: 12.5,
//!     mean_bp_rp: 1.0,
//!     epoch_yr: 2016.0,
//!     catalog_version: CATALOG_VERSION_SYNTH.to_string(),
//! };
//! let stars = vec![StarRecord {
//!     source_id: 1,
//!     ra_rad: 0.1,
//!     dec_rad: 0.2,
//!     pm_ra_mas_yr: 3.0,
//!     pm_dec_mas_yr: -1.0,
//!     g_mag: 12.0,
//!     bp_rp: 0.9,
//! }];
//! let bytes = encode_tile(&header, &stars);
//! let decoded = decode_tile(&bytes).expect("round-trips");
//! assert_eq!(decoded.stars, stars);
//! assert_eq!(decoded.header.pixel, 42);
//! ```

use crate::catalog::healpix::{MAX_ORDER, ang2pix};
use crate::physics::ephemeris::Validity;
use crate::physics::{PROPER_MOTION_SPAN_YR, ProperMotion};

/// Gaia DR3 reference epoch (single definition lives in
/// `physics::proper_motion`; re-exported here for tile code).
pub use crate::physics::proper_motion::GAIA_EPOCH_YR;

/// Tile magic `GSCT` (Gaia Streaming Catalog Tile), little-endian.
pub const TILE_MAGIC: u32 = 0x5443_5347;
/// Tile format version stamped by the cooker, checked by the decoder.
pub const TILE_VERSION: u16 = 1;
/// Maximum catalog-version string length (bytes).
pub const MAX_CATALOG_VERSION_LEN: usize = 64;
/// Maximum records per tile accepted by the decoder (defense against
/// corrupt-count allocation blowup; ~4M × 40 B ≈ 160 MB worst case,
/// and the byte-length check binds real allocations to input size).
pub const MAX_RECORDS_PER_TILE: u32 = 4_000_000;
/// Encoded stride of one source record (bytes).
pub const RECORD_STRIDE: usize = 40;
/// Catalog version stamp for cooker-synthesized tiles (spec §10 PO
/// decision: synthetic data fills the Gaia schema until real import).
pub const CATALOG_VERSION_SYNTH: &str = "gaia-dr3-synth/1.0";
/// Catalog version stamp for tiles cooked from a real Gaia DR3 extract.
pub const CATALOG_VERSION_GAIA: &str = "gaia-dr3/1.0";

/// Tile identity + summary statistics (fallback lattice + scheduler
/// metadata derive from these without touching records).
#[derive(Clone, Debug, PartialEq)]
pub struct TileHeader {
    /// HEALPix order of [`pixel`](TileHeader::pixel).
    pub order: u8,
    /// Nested pixel id at [`order`](TileHeader::order).
    pub pixel: u64,
    /// Mean G magnitude of the tile's sources (fallback color anchor).
    pub mean_g_mag: f32,
    /// Mean BP–RP color of the tile's sources (fallback color anchor).
    pub mean_bp_rp: f32,
    /// Reference epoch of the astrometry in years (Gaia DR3: 2016.0).
    pub epoch_yr: f64,
    /// Catalog version stamp ([`CATALOG_VERSION_SYNTH`] or
    /// [`CATALOG_VERSION_GAIA`]); feeds `SeedMetadata::catalog_version`.
    pub catalog_version: String,
}

/// One cataloged source: Gaia DR3 astrometry + photometry subset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarRecord {
    /// Stable catalog identifier (unique per tile by cooker contract).
    pub source_id: u64,
    /// Right ascension at [`epoch`](TileHeader::epoch_yr) (rad).
    pub ra_rad: f64,
    /// Declination at epoch (rad).
    pub dec_rad: f64,
    /// Proper motion in RA (mas/yr, catalog convention: includes cos δ).
    pub pm_ra_mas_yr: f32,
    /// Proper motion in dec (mas/yr).
    pub pm_dec_mas_yr: f32,
    /// Gaia G magnitude.
    pub g_mag: f32,
    /// Gaia BP–RP color index.
    pub bp_rp: f32,
}

impl StarRecord {
    /// Astrometry as the [`ProperMotion`] `scale-physics` consumes
    /// (DoD-3): f32 fields widen losslessly to f64 (exact for finite
    /// magnitudes — no precision claim beyond the catalog's own).
    pub fn proper_motion(&self) -> ProperMotion {
        ProperMotion {
            ra_rad: self.ra_rad,
            dec_rad: self.dec_rad,
            pm_ra_mas_yr: f64::from(self.pm_ra_mas_yr),
            pm_dec_mas_yr: f64::from(self.pm_dec_mas_yr),
        }
    }
}

/// Validity of header-epoch astrometry at `game_year` (ADR-020 ±1000 yr
/// window): out-of-window callers must surface a warning, never fail
/// silently. The player-facing HUD flag lands with `navigation-hud`
/// (v0.3.0); until then `tracing` + the debug overlay carry it —
/// see the debug `CatalogSky` summary path.
pub fn epoch_validity(epoch_yr: f64, game_year: f64) -> Validity {
    if (game_year - epoch_yr).abs() <= PROPER_MOTION_SPAN_YR {
        Validity::InWindow
    } else {
        Validity::OutOfWindow
    }
}

/// A decoded tile: validated header plus its source records.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedTile {
    /// Validated tile header.
    pub header: TileHeader,
    /// Source records in file order (cooker sorts by `source_id`).
    pub stars: Vec<StarRecord>,
}

impl DecodedTile {
    /// Accounted bytes: encoded size (decoded + GPU-resident estimate
    /// both derive from this via the uniform record stride).
    pub fn memory_bytes(&self) -> usize {
        encoded_len(self.stars.len(), self.header.catalog_version.len())
    }
}

/// Fixed header prefix length (everything before the version string;
/// see [`decode_tile`]).
pub const HEADER_FIXED_LEN: usize = 4 + 2 + 1 + 8 + 4 + 4 + 4 + 8 + 1;

/// Encoded byte length of a tile holding `count` records with a
/// `version_len`-byte catalog version string.
pub fn encoded_len(count: usize, version_len: usize) -> usize {
    HEADER_FIXED_LEN + version_len + count * RECORD_STRIDE
}

/// Decode failure modes. Every variant is a clean rejection: no panic,
/// no partial tile, no allocation beyond the input's own size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TileError {
    /// Fewer bytes than the header (or the header's version string).
    TruncatedHeader,
    /// Fewer record bytes than `count × RECORD_STRIDE`, or trailing
    /// bytes past the records.
    TruncatedRecords,
    /// Magic mismatch (not a catalog tile).
    BadMagic(u32),
    /// Version newer than [`TILE_VERSION`] (forward incompatibility is
    /// explicit, never silent).
    UnsupportedVersion(u16),
    /// Order above [`MAX_ORDER`].
    BadOrder(u8),
    /// `count` above [`MAX_RECORDS_PER_TILE`].
    CountExceedsLimit(u32),
    /// Catalog-version bytes are not valid UTF-8.
    BadCatalogVersion,
    /// A record's coordinates fail range checks (NaN or out of range).
    BadRecord {
        /// Record index in the tile.
        index: usize,
    },
}

impl std::fmt::Display for TileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TileError::TruncatedHeader => write!(f, "tile truncated in header"),
            TileError::TruncatedRecords => write!(f, "tile truncated in records"),
            TileError::BadMagic(m) => write!(f, "bad tile magic {m:#x}"),
            TileError::UnsupportedVersion(v) => write!(f, "unsupported tile version {v}"),
            TileError::BadOrder(o) => write!(f, "tile order {o} above maximum"),
            TileError::CountExceedsLimit(c) => write!(f, "tile count {c} exceeds limit"),
            TileError::BadCatalogVersion => write!(f, "tile catalog version is not UTF-8"),
            TileError::BadRecord { index } => write!(f, "tile record {index} out of range"),
        }
    }
}

impl std::error::Error for TileError {}

/// Encode `stars` under `header` (record count comes from the slice).
///
/// # Panics
///
/// Panics on programmer errors only (order above [`MAX_ORDER`],
/// version string above [`MAX_CATALOG_VERSION_LEN`]): the cooker builds
/// headers from validated inputs, and wire bytes always go through
/// [`decode_tile`].
pub fn encode_tile(header: &TileHeader, stars: &[StarRecord]) -> Vec<u8> {
    assert!(
        header.catalog_version.len() <= MAX_CATALOG_VERSION_LEN,
        "catalog version too long"
    );
    assert!(header.order <= MAX_ORDER, "order above maximum");
    let mut out = Vec::with_capacity(encoded_len(stars.len(), header.catalog_version.len()));
    out.extend_from_slice(&TILE_MAGIC.to_le_bytes());
    out.extend_from_slice(&TILE_VERSION.to_le_bytes());
    out.push(header.order);
    out.extend_from_slice(&header.pixel.to_le_bytes());
    out.extend_from_slice(&(stars.len() as u32).to_le_bytes());
    out.extend_from_slice(&header.mean_g_mag.to_le_bytes());
    out.extend_from_slice(&header.mean_bp_rp.to_le_bytes());
    out.extend_from_slice(&header.epoch_yr.to_le_bytes());
    out.push(header.catalog_version.len() as u8);
    out.extend_from_slice(header.catalog_version.as_bytes());
    for star in stars {
        out.extend_from_slice(&star.source_id.to_le_bytes());
        out.extend_from_slice(&star.ra_rad.to_le_bytes());
        out.extend_from_slice(&star.dec_rad.to_le_bytes());
        out.extend_from_slice(&star.pm_ra_mas_yr.to_le_bytes());
        out.extend_from_slice(&star.pm_dec_mas_yr.to_le_bytes());
        out.extend_from_slice(&star.g_mag.to_le_bytes());
        out.extend_from_slice(&star.bp_rp.to_le_bytes());
    }
    out
}

/// Decode and fully validate a tile. `Err` on any anomaly; `Ok` carries
/// a tile the loader can trust without re-checking.
pub fn decode_tile(bytes: &[u8]) -> Result<DecodedTile, TileError> {
    const FIXED: usize = HEADER_FIXED_LEN;
    if bytes.len() < FIXED {
        return Err(TileError::TruncatedHeader);
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().expect("sliced"));
    if magic != TILE_MAGIC {
        return Err(TileError::BadMagic(magic));
    }
    let version = u16::from_le_bytes(bytes[4..6].try_into().expect("sliced"));
    if version != TILE_VERSION {
        return Err(TileError::UnsupportedVersion(version));
    }
    let order = bytes[6];
    if order > MAX_ORDER {
        return Err(TileError::BadOrder(order));
    }
    let pixel = u64::from_le_bytes(bytes[7..15].try_into().expect("sliced"));
    let count = u32::from_le_bytes(bytes[15..19].try_into().expect("sliced"));
    if count > MAX_RECORDS_PER_TILE {
        return Err(TileError::CountExceedsLimit(count));
    }
    let mean_g_mag = f32::from_le_bytes(bytes[19..23].try_into().expect("sliced"));
    let mean_bp_rp = f32::from_le_bytes(bytes[23..27].try_into().expect("sliced"));
    let epoch_yr = f64::from_le_bytes(bytes[27..35].try_into().expect("sliced"));
    let ver_len = bytes[35] as usize;
    if ver_len > MAX_CATALOG_VERSION_LEN || bytes.len() < FIXED + ver_len {
        return Err(TileError::TruncatedHeader);
    }
    let catalog_version = std::str::from_utf8(&bytes[FIXED..FIXED + ver_len])
        .map_err(|_| TileError::BadCatalogVersion)?
        .to_string();
    let body = &bytes[FIXED + ver_len..];
    // Strict length: records must fill the body exactly — no trailing
    // bytes (which would signal a writer/reader version skew).
    if body.len() != count as usize * RECORD_STRIDE {
        return Err(TileError::TruncatedRecords);
    }
    // Allocation is bounded by the input's own byte length (plus the
    // absolute count cap above): corrupt counts cannot blow memory.
    let mut stars = Vec::with_capacity(count as usize);
    for (index, chunk) in body.chunks_exact(RECORD_STRIDE).enumerate() {
        let source_id = u64::from_le_bytes(chunk[0..8].try_into().expect("chunked"));
        let ra_rad = f64::from_le_bytes(chunk[8..16].try_into().expect("chunked"));
        let dec_rad = f64::from_le_bytes(chunk[16..24].try_into().expect("chunked"));
        let pm_ra_mas_yr = f32::from_le_bytes(chunk[24..28].try_into().expect("chunked"));
        let pm_dec_mas_yr = f32::from_le_bytes(chunk[28..32].try_into().expect("chunked"));
        let g_mag = f32::from_le_bytes(chunk[32..36].try_into().expect("chunked"));
        let bp_rp = f32::from_le_bytes(chunk[36..40].try_into().expect("chunked"));
        if !ra_rad.is_finite()
            || !dec_rad.is_finite()
            || !(0.0..=2.0 * std::f64::consts::PI).contains(&ra_rad)
            || !(-std::f64::consts::FRAC_PI_2..=std::f64::consts::FRAC_PI_2).contains(&dec_rad)
            || !pm_ra_mas_yr.is_finite()
            || !pm_dec_mas_yr.is_finite()
            || !g_mag.is_finite()
            || !bp_rp.is_finite()
        {
            return Err(TileError::BadRecord { index });
        }
        stars.push(StarRecord {
            source_id,
            ra_rad,
            dec_rad,
            pm_ra_mas_yr,
            pm_dec_mas_yr,
            g_mag,
            bp_rp,
        });
    }
    Ok(DecodedTile {
        header: TileHeader {
            order,
            pixel,
            mean_g_mag,
            mean_bp_rp,
            epoch_yr,
            catalog_version,
        },
        stars,
    })
}

/// Verify every record of `tile` hashes into (`order`, `pixel`)
/// (cooker self-check; loader trusts but the cooker proves).
pub fn verify_tile_placement(tile: &DecodedTile) -> bool {
    tile.stars.iter().all(|star| {
        ang2pix(tile.header.order, star.ra_rad, star.dec_rad) == Some(tile.header.pixel)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> TileHeader {
        TileHeader {
            order: 12,
            pixel: 42,
            mean_g_mag: 12.5,
            mean_bp_rp: 1.0,
            epoch_yr: GAIA_EPOCH_YR,
            catalog_version: CATALOG_VERSION_SYNTH.to_string(),
        }
    }

    fn sample_stars() -> Vec<StarRecord> {
        vec![
            StarRecord {
                source_id: 7,
                ra_rad: 0.1,
                dec_rad: 0.2,
                pm_ra_mas_yr: 3.0,
                pm_dec_mas_yr: -1.0,
                g_mag: 12.0,
                bp_rp: 0.9,
            },
            StarRecord {
                source_id: 9,
                ra_rad: 5.0,
                dec_rad: -0.4,
                pm_ra_mas_yr: 0.0,
                pm_dec_mas_yr: 0.0,
                g_mag: 18.25,
                bp_rp: 2.1,
            },
        ]
    }

    #[test]
    fn round_trip_preserves_every_field() {
        let header = sample_header();
        let stars = sample_stars();
        let bytes = encode_tile(&header, &stars);
        assert_eq!(bytes.len(), encoded_len(2, header.catalog_version.len()));
        let decoded = decode_tile(&bytes).expect("round-trips");
        assert_eq!(decoded.header, header);
        assert_eq!(decoded.stars, stars);
        assert_eq!(decoded.memory_bytes(), bytes.len());
    }

    #[test]
    fn empty_tile_round_trips() {
        let header = sample_header();
        let bytes = encode_tile(&header, &[]);
        let decoded = decode_tile(&bytes).expect("empty tile decodes");
        assert!(decoded.stars.is_empty());
        assert_eq!(decoded.header.pixel, 42);
    }

    #[test]
    fn corrupt_inputs_fail_cleanly() {
        let bytes = encode_tile(&sample_header(), &sample_stars());
        // Truncated header.
        assert_eq!(decode_tile(&[]), Err(TileError::TruncatedHeader));
        assert_eq!(decode_tile(&bytes[..10]), Err(TileError::TruncatedHeader));
        // Wrong magic.
        let mut bad = bytes.clone();
        bad[0] ^= 0xFF;
        assert!(matches!(decode_tile(&bad), Err(TileError::BadMagic(_))));
        // Wrong version (low byte forced to 0xFF).
        let mut bad = bytes.clone();
        bad[4] = 0xFF;
        assert_eq!(decode_tile(&bad), Err(TileError::UnsupportedVersion(255)));
        // Truncated records + trailing garbage.
        assert_eq!(
            decode_tile(&bytes[..bytes.len() - 7]),
            Err(TileError::TruncatedRecords)
        );
        let mut bad = bytes.clone();
        bad.extend_from_slice(&[0u8; 3]);
        assert_eq!(decode_tile(&bad), Err(TileError::TruncatedRecords));
        // Absurd count: rejected before any big allocation.
        let mut bad = bytes.clone();
        bad[15..19].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            decode_tile(&bad),
            Err(TileError::CountExceedsLimit(u32::MAX))
        );
        // NaN coordinate rejected with its index.
        let mut bad = bytes.clone();
        let rec0 = HEADER_FIXED_LEN + sample_header().catalog_version.len();
        bad[rec0 + 8..rec0 + 16].copy_from_slice(&f64::NAN.to_le_bytes());
        assert_eq!(decode_tile(&bad), Err(TileError::BadRecord { index: 0 }));
        // Non-UTF8 version string.
        let mut bad = encode_tile(&sample_header(), &[]);
        let ver_at = 4 + 2 + 1 + 8 + 4 + 4 + 4 + 8 + 1;
        bad[ver_at] = 0xFF;
        assert_eq!(decode_tile(&bad), Err(TileError::BadCatalogVersion));
    }

    #[test]
    fn placement_verifier_catches_strays() {
        // Records placed by ang2pix verify; a stray does not.
        let order = 4u8;
        let pixel = ang2pix(order, 1.0, 0.2).expect("valid coords");
        let (ra, dec) = crate::catalog::healpix::pix2ang(order, pixel).expect("valid pixel");
        let tile = DecodedTile {
            header: TileHeader {
                order,
                pixel,
                mean_g_mag: 15.0,
                mean_bp_rp: 1.2,
                epoch_yr: GAIA_EPOCH_YR,
                catalog_version: CATALOG_VERSION_SYNTH.to_string(),
            },
            stars: vec![StarRecord {
                source_id: 1,
                ra_rad: ra,
                dec_rad: dec,
                pm_ra_mas_yr: 0.0,
                pm_dec_mas_yr: 0.0,
                g_mag: 15.0,
                bp_rp: 1.2,
            }],
        };
        assert!(verify_tile_placement(&tile));
        let mut stray = tile.clone();
        stray.stars[0].ra_rad = ra + 1.0;
        assert!(!verify_tile_placement(&stray));
    }

    #[test]
    fn version_stamps_feed_save_metadata() {
        // SCS-004: the stamps are the strings saves record.
        let meta = crate::seeding::SeedMetadata::new(42, Some(CATALOG_VERSION_SYNTH.to_string()));
        assert_eq!(meta.catalog_version.as_deref(), Some("gaia-dr3-synth/1.0"));
        assert_ne!(CATALOG_VERSION_SYNTH, CATALOG_VERSION_GAIA);
    }

    #[test]
    fn records_plumb_to_proper_motion_within_tolerance() {
        // SCS-014 / DoD-3: decode → ProperMotion → extrapolate matches
        // rate × time within the 1% PO tolerance; epoch validity flags
        // the ±1000 yr window edges.
        use crate::physics::extrapolate;
        let star = StarRecord {
            source_id: 1,
            ra_rad: 1.0,
            dec_rad: 0.5,
            pm_ra_mas_yr: -800.0,
            pm_dec_mas_yr: 10_300.0,
            g_mag: 12.0,
            bp_rp: 0.9,
        };
        let pm = star.proper_motion();
        assert_eq!(pm.ra_rad, 1.0);
        assert_eq!(pm.pm_ra_mas_yr, -800.0);
        let (ra, dec, validity) = extrapolate(pm, 100.0);
        assert!(validity.in_window());
        let mas_per_rad = crate::physics::proper_motion::MAS_PER_RAD;
        let dra = (ra - 1.0) * mas_per_rad;
        let ddec = (dec - 0.5) * mas_per_rad;
        let got = (dra * dra + ddec * ddec).sqrt();
        let expected = ((800.0f64.powi(2) + 10_300.0f64.powi(2)).sqrt()) * 100.0;
        assert!((got - expected).abs() / expected < 0.01, "got {got} mas");
        // f32→f64 widening is exact for these magnitudes.
        assert_eq!(star.proper_motion().pm_dec_mas_yr, 10_300.0);
        // Validity edges.
        assert!(epoch_validity(GAIA_EPOCH_YR, 2016.0).in_window());
        assert!(epoch_validity(GAIA_EPOCH_YR, 3016.0).in_window());
        assert!(!epoch_validity(GAIA_EPOCH_YR, 3017.0).in_window());
        assert!(!epoch_validity(GAIA_EPOCH_YR, 1015.0).in_window());
    }
}
