//! Deterministic procedural fallback for slow/absent tiles (spec §4,
//! ADR-017).
//!
//! When a tile misses its 200 ms deadline ([`FALLBACK_DELAY_MS`]), the
//! sky renders a stand-in from tile metadata — approximate source count
//! plus mean color — distributed on a seeded golden-angle lattice
//! (spec §7 math toolkit) over the tile quad, then swaps seamlessly on
//! catalog arrival. Metadata comes from the cooker manifest
//! ([`ManifestIndex`]) or resident tile headers; tiles with neither get
//! a coarse procedural model ([`model_meta`]) so the sky never blanks.
//! Everything here is a pure function of (master seed, tile, slot):
//! same inputs ⇒ same stand-in on every platform, every session.
//!
//! ```
//! use game_engine::catalog::fallback::{TileMeta, fallback_star};
//!
//! let meta = TileMeta { count: 4, mean_g_mag: 15.0, mean_bp_rp: 1.0 };
//! let a = fallback_star(42, 4, 9, 0, &meta).expect("valid tile");
//! let b = fallback_star(42, 4, 9, 0, &meta).expect("valid tile");
//! assert_eq!((a.ra_rad, a.dec_rad), (b.ra_rad, b.dec_rad));
//! ```

use crate::catalog::format::{DecodedTile, MAX_RECORDS_PER_TILE};
use crate::catalog::healpix::{MAX_ORDER, npix, pix2ang, projected_center, unproj};
use crate::core::SeededRng;
use crate::seeding::ExclusionZone;
use std::collections::HashMap;

/// Deadline after a tile request when the fallback renders (ms).
pub const FALLBACK_DELAY_MS: f64 = 200.0;
/// Golden angle in radians (spec §7 Fibonacci lattice).
pub const GOLDEN_ANGLE_RAD: f64 = 2.399_963_229_728_653;
/// North-galactic-pole J2000 equatorial direction, band analogue
/// (shared with the cooker: single source of truth for the synthetic
/// density tilt; Hipparcos, Perryman et al. 1997).
pub const BAND_NORMAL_RA_DEG: f64 = 192.859_508;
pub const BAND_NORMAL_DEC_DEG: f64 = 27.128_336;
/// Gaussian band width (deg) for the density analogue.
pub const BAND_WIDTH_DEG: f64 = 12.0;
/// Cap on model fallback counts (real dense tiles are cooked and carry
/// metadata; the model covers uncooked sky only).
pub const MODEL_MAX_COUNT: u32 = 20_000;
/// Full-sky-like mean sources per tile the model targets (Gaia DR3
/// ~1.8B over the order-12 grid, scaled per order by pixel area).
const MODEL_MEAN_PER_TILE_ORDER12: f64 = 9.0;

/// Tile summary statistics: the fallback's only data input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileMeta {
    /// Approximate source count (lattice size).
    pub count: u32,
    /// Mean G magnitude (fallback brightness anchor).
    pub mean_g_mag: f32,
    /// Mean BP–RP color (fallback color anchor).
    pub mean_bp_rp: f32,
}

/// [`TileMeta`] from a decoded tile (count = record count).
pub fn meta_from_tile(tile: &DecodedTile) -> TileMeta {
    TileMeta {
        count: tile.stars.len() as u32,
        mean_g_mag: tile.header.mean_g_mag,
        mean_bp_rp: tile.header.mean_bp_rp,
    }
}

/// Band-analogue normal (unit vector, equatorial J2000).
pub fn band_normal() -> [f64; 3] {
    let ra = BAND_NORMAL_RA_DEG.to_radians();
    let dec = BAND_NORMAL_DEC_DEG.to_radians();
    [dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin()]
}

/// One fallback lattice star: sky position plus approximate photometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FallbackStar {
    /// Right ascension (rad).
    pub ra_rad: f64,
    /// Declination (rad).
    pub dec_rad: f64,
    /// Approximate G magnitude (mean ± scatter).
    pub g_mag: f32,
    /// Approximate BP–RP color (mean ± scatter).
    pub bp_rp: f32,
}

/// Deterministic lattice star `slot` of tile (`order`, `pixel`).
/// `slot` must be below `meta.count` (callers generate `count` slots);
/// out-of-range tiles/slots return `None`.
pub fn fallback_star(
    master_seed: u64,
    order: u8,
    pixel: u64,
    slot: u32,
    meta: &TileMeta,
) -> Option<FallbackStar> {
    if order > MAX_ORDER || slot >= meta.count || meta.count == 0 {
        return None;
    }
    let (cx, cy) = projected_center(order, pixel)?;
    let nside = 2u32.pow(order as u32) as f64;
    // Unit-disc Fibonacci point → diamond offset (disc→diamond map
    // keeps |dx|+|dy| ≤ t/√2·… inside the quad; see below).
    let frac = (f64::from(slot) + 0.5) / f64::from(meta.count);
    let radius = frac.sqrt();
    let angle = f64::from(slot) * GOLDEN_ANGLE_RAD;
    let (sang, cang) = angle.sin_cos();
    let half = 0.5 / nside;
    let dx = (radius * cang - radius * sang) * half;
    let dy = (radius * cang + radius * sang) * half;
    let (ra_rad, dec_rad) = unproj(cx + dx, cy + dy)?;
    // Per-slot photometry scatter (own stream: slot-indexable, no
    // sequential dependence).
    let domain = format!("catalog/fallback/v1/{order}/{pixel}/{slot}");
    let mut rng = SeededRng::stream(master_seed, &domain);
    let g_mag = (f64::from(meta.mean_g_mag) + (rng.unit_f64() - 0.5) * 4.0) as f32;
    let bp_rp = (f64::from(meta.mean_bp_rp) + (rng.unit_f64() - 0.5) * 1.0) as f32;
    Some(FallbackStar {
        ra_rad,
        dec_rad,
        g_mag,
        bp_rp: bp_rp.clamp(-0.5, 4.0),
    })
}

/// Procedural model metadata for tiles with no manifest/header data
/// (uncooked sky): full-sky-like mean density modulated by the band
/// analogue, with deterministic jitter. Count 0 is impossible by
/// construction (the sky never blanks); colors default to mid
/// main-sequence.
pub fn model_meta(master_seed: u64, order: u8, pixel: u64) -> TileMeta {
    let count = model_count(master_seed, order, pixel);
    let mut rng = SeededRng::stream(
        master_seed,
        &format!("catalog/fallback-model/v1/{order}/{pixel}"),
    );
    TileMeta {
        count,
        mean_g_mag: (15.0 + (rng.unit_f64() - 0.5) * 6.0) as f32,
        mean_bp_rp: (1.2 + (rng.unit_f64() - 0.5) * 0.8) as f32,
    }
}

fn model_count(master_seed: u64, order: u8, pixel: u64) -> u32 {
    if order > MAX_ORDER || pixel >= npix(order).unwrap_or(0) {
        return 1;
    }
    let (ra, dec) = pix2ang(order, pixel).unwrap_or((0.0, 0.0));
    let normal = band_normal();
    let dir = [dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin()];
    let beta = (dir[0] * normal[0] + dir[1] * normal[1] + dir[2] * normal[2])
        .clamp(-1.0, 1.0)
        .asin();
    let width = BAND_WIDTH_DEG.to_radians();
    let band = 0.3 + 5.0 * (-(beta / width).powi(2)).exp();
    // Area scaling from the order-12 mean.
    let area_ratio = 4u64.pow((12i8 - order as i8).max(0) as u32) as f64;
    let base = MODEL_MEAN_PER_TILE_ORDER12 * area_ratio * band;
    let mut rng = SeededRng::stream(
        master_seed,
        &format!("catalog/fallback-count/v1/{order}/{pixel}"),
    );
    ((base * (0.5 + rng.unit_f64())).floor() as u32).clamp(1, MODEL_MAX_COUNT)
}

/// ADR-019 real-data override hook: bright catalog sources suppress
/// procedural generation around them. Cell semantics (per plan):
/// `RegionId { frame: StellarNeighborhood, cell: [order, pixel, 0] }` —
/// the generator checks these zones *before* its procedural hash fires.
/// Radius follows the brightest source (naked-eye stars clear their
/// neighborhood, faint tiles suppress nothing).
pub fn exclusion_zones(tile: &DecodedTile) -> Vec<ExclusionZone> {
    let brightest = tile
        .stars
        .iter()
        .map(|star| star.g_mag)
        .fold(f32::INFINITY, f32::min);
    let radius_cells = if brightest < 6.0 {
        2
    } else if brightest < 10.0 {
        1
    } else {
        return Vec::new();
    };
    vec![ExclusionZone {
        center: [i64::from(tile.header.order), tile.header.pixel as i64, 0],
        radius_cells,
    }]
}

/// Cooker-manifest tile index: per-tile metadata for the fallback path
/// plus manifest validation (manifests are untrusted asset input).
#[derive(Clone, Debug, PartialEq)]
pub struct ManifestIndex {
    /// Catalog version stamped by the cooker.
    pub catalog_version: String,
    /// HEALPix order of the manifest's tiles.
    pub order: u8,
    /// Total sources across tiles (cross-checked on parse).
    pub total_sources: u64,
    /// Per-tile metadata by (order, pixel).
    pub tiles: HashMap<(u8, u64), TileMeta>,
}

impl ManifestIndex {
    /// Metadata for tile (`order`, `pixel`) when the manifest covers it.
    pub fn get(&self, order: u8, pixel: u64) -> Option<TileMeta> {
        self.tiles.get(&(order, pixel)).copied()
    }
}

/// Manifest rejection reasons (strict subset parser — accepts exactly
/// the cooker writer's shape, nothing else).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestError {
    /// Empty input.
    Empty,
    /// Input ends mid-structure.
    Truncated,
    /// Unexpected byte at an offset.
    UnexpectedChar(usize),
    /// Malformed string at an offset.
    BadString(usize),
    /// Malformed number at an offset.
    BadNumber(usize),
    /// Malformed fnv hash at an offset.
    BadHash(usize),
    /// Pixel out of range for the manifest order.
    BadPixel(u64),
    /// Count above [`MAX_RECORDS_PER_TILE`] (or zero).
    BadCount(u32),
    /// Duplicate tile entry.
    DuplicateTile(u64),
    /// `tile_count` / `total_sources` disagree with the entries.
    CountMismatch,
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Empty => write!(f, "empty manifest"),
            ManifestError::Truncated => write!(f, "truncated manifest"),
            ManifestError::UnexpectedChar(at) => write!(f, "unexpected char at {at}"),
            ManifestError::BadString(at) => write!(f, "bad string at {at}"),
            ManifestError::BadNumber(at) => write!(f, "bad number at {at}"),
            ManifestError::BadHash(at) => write!(f, "bad hash at {at}"),
            ManifestError::BadPixel(p) => write!(f, "pixel {p} out of range"),
            ManifestError::BadCount(c) => write!(f, "count {c} invalid"),
            ManifestError::DuplicateTile(p) => write!(f, "duplicate tile {p}"),
            ManifestError::CountMismatch => write!(f, "tile/total counts disagree"),
        }
    }
}

impl std::error::Error for ManifestError {}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Result<u8, ManifestError> {
        self.bytes
            .get(self.pos)
            .copied()
            .ok_or(ManifestError::Truncated)
    }

    fn expect(&mut self, byte: u8) -> Result<(), ManifestError> {
        if self.peek()? == byte {
            self.pos += 1;
            Ok(())
        } else {
            Err(ManifestError::UnexpectedChar(self.pos))
        }
    }

    fn expect_literal(&mut self, literal: &str) -> Result<(), ManifestError> {
        for byte in literal.bytes() {
            self.expect(byte)?;
        }
        Ok(())
    }

    /// Strict quoted string: printable ASCII except `"`/`\`, plus
    /// `\"` and `\\` escapes. Anything else is rejected.
    fn string(&mut self) -> Result<String, ManifestError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let byte = self.peek()?;
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    match self.peek()? {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        _ => return Err(ManifestError::BadString(self.pos)),
                    }
                    self.pos += 1;
                }
                0x20..=0x7E => {
                    out.push(byte as char);
                    self.pos += 1;
                }
                _ => return Err(ManifestError::BadString(self.pos)),
            }
        }
    }

    /// Unsigned integer digits.
    fn uint(&mut self) -> Result<u64, ManifestError> {
        let start = self.pos;
        while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(ManifestError::BadNumber(start));
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| ManifestError::BadNumber(start))?
            .parse()
            .map_err(|_| ManifestError::BadNumber(start))
    }

    /// `[0-9.+eE-]` float slice parsed via `f32::from_str`.
    fn float(&mut self) -> Result<f32, ManifestError> {
        let start = self.pos;
        while matches!(
            self.bytes.get(self.pos),
            Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
        ) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(ManifestError::BadNumber(start));
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| ManifestError::BadNumber(start))?
            .parse()
            .map_err(|_| ManifestError::BadNumber(start))
    }

    /// 16 lowercase hex chars (fnv hash: charset-validated, content
    /// verified against tile bytes by the caller on decode).
    fn hash(&mut self) -> Result<(), ManifestError> {
        self.expect(b'"')?;
        for _ in 0..16 {
            match self.peek()? {
                b'0'..=b'9' | b'a'..=b'f' => self.pos += 1,
                _ => return Err(ManifestError::BadHash(self.pos)),
            }
        }
        self.expect(b'"')?;
        Ok(())
    }
}

/// Parse a cooker manifest into a validated [`ManifestIndex`].
/// Rejects empty/truncated/malformed input, out-of-range pixels,
/// invalid counts, duplicates, and header/entry count disagreements —
/// all without panicking.
pub fn parse_manifest(text: &str) -> Result<ManifestIndex, ManifestError> {
    if text.is_empty() {
        return Err(ManifestError::Empty);
    }
    let mut p = Parser::new(text);
    p.expect_literal("{\"catalog_version\":")?;
    let catalog_version = p.string()?;
    p.expect_literal(",\"order\":")?;
    let order = p.uint()?;
    if order > u64::from(MAX_ORDER) {
        return Err(ManifestError::BadNumber(p.pos));
    }
    let order = order as u8;
    p.expect_literal(",\"seed\":")?;
    p.uint()?;
    p.expect_literal(",\"tile_count\":")?;
    let tile_count = p.uint()?;
    p.expect_literal(",\"total_sources\":")?;
    let total_sources = p.uint()?;
    p.expect_literal(",\"tiles\":[")?;
    let mut tiles = HashMap::new();
    let mut sources = 0u64;
    if p.peek()? != b']' {
        loop {
            p.expect_literal("{\"pixel\":")?;
            let pixel = p.uint()?;
            if pixel >= npix(order).unwrap_or(0) {
                return Err(ManifestError::BadPixel(pixel));
            }
            p.expect_literal(",\"count\":")?;
            let count = p.uint()?;
            if count == 0 || count > u64::from(MAX_RECORDS_PER_TILE) {
                return Err(ManifestError::BadCount(count as u32));
            }
            p.expect_literal(",\"mean_g\":")?;
            let mean_g_mag = p.float()?;
            p.expect_literal(",\"mean_bp_rp\":")?;
            let mean_bp_rp = p.float()?;
            if !mean_g_mag.is_finite() || !mean_bp_rp.is_finite() {
                return Err(ManifestError::BadNumber(p.pos));
            }
            p.expect_literal(",\"fnv1a64\":")?;
            p.hash()?;
            p.expect(b'}')?;
            if tiles
                .insert(
                    (order, pixel),
                    TileMeta {
                        count: count as u32,
                        mean_g_mag,
                        mean_bp_rp,
                    },
                )
                .is_some()
            {
                return Err(ManifestError::DuplicateTile(pixel));
            }
            sources += count;
            if p.peek()? != b',' {
                break;
            }
            p.pos += 1;
        }
    }
    p.expect_literal("]}")?;
    // Trailing whitespace only past the document.
    while let Some(byte) = p.bytes.get(p.pos) {
        if !byte.is_ascii_whitespace() {
            return Err(ManifestError::UnexpectedChar(p.pos));
        }
        p.pos += 1;
    }
    if tiles.len() as u64 != tile_count || sources != total_sources {
        return Err(ManifestError::CountMismatch);
    }
    Ok(ManifestIndex {
        catalog_version,
        order,
        total_sources,
        tiles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::format::{CATALOG_VERSION_SYNTH, GAIA_EPOCH_YR, StarRecord, TileHeader};
    use crate::catalog::healpix::ang2pix;
    use crate::seeding::suppressed_by_any;

    fn meta() -> TileMeta {
        TileMeta {
            count: 64,
            mean_g_mag: 15.0,
            mean_bp_rp: 1.0,
        }
    }

    #[test]
    fn lattice_is_deterministic_and_inside_the_tile() {
        let meta = meta();
        for slot in 0..64 {
            let a = fallback_star(42, 4, 9, slot, &meta).expect("slot in range");
            let b = fallback_star(42, 4, 9, slot, &meta).expect("slot in range");
            assert_eq!(a, b, "pure function of (seed, tile, slot)");
            // Inside tile 9 at order 4.
            assert_eq!(ang2pix(4, a.ra_rad, a.dec_rad), Some(9));
        }
        // Slots cover the tile (not piled on one point).
        let first = fallback_star(42, 4, 9, 0, &meta).expect("slot");
        let last = fallback_star(42, 4, 9, 63, &meta).expect("slot");
        assert!((first.ra_rad - last.ra_rad).abs() > 1e-4);
        // Out-of-range slots rejected.
        assert_eq!(fallback_star(42, 4, 9, 64, &meta), None);
        assert_eq!(fallback_star(42, 99, 9, 0, &meta), None);
    }

    #[test]
    fn model_meta_never_blanks_and_stays_stable() {
        for (order, pixel) in [(12u8, 0u64), (12, 100_000_000), (13, 5), (4, 9)] {
            let a = model_meta(7, order, pixel);
            let b = model_meta(7, order, pixel);
            assert_eq!((a.count, a.mean_g_mag), (b.count, b.mean_g_mag));
            assert!((1..=MODEL_MAX_COUNT).contains(&a.count));
            // Lattice from model metadata lands in-tile too.
            let star = fallback_star(7, order, pixel, 0, &a).expect("slot 0");
            assert_eq!(ang2pix(order, star.ra_rad, star.dec_rad), Some(pixel));
        }
        // Band analogue: polar tiles are sparser than plane tiles.
        let pole = model_meta(7, 12, ang2pix(12, 0.0, 1.4).expect("coords"));
        let plane = model_meta(7, 12, ang2pix(12, 4.6, 0.0).expect("coords"));
        assert!(
            plane.count > pole.count,
            "plane {} > pole {}",
            plane.count,
            pole.count
        );
    }

    #[test]
    fn exclusion_zones_follow_brightness() {
        let tile = |mags: &[f32]| DecodedTile {
            header: TileHeader {
                order: 4,
                pixel: 9,
                mean_g_mag: 8.0,
                mean_bp_rp: 1.0,
                epoch_yr: GAIA_EPOCH_YR,
                catalog_version: CATALOG_VERSION_SYNTH.to_string(),
            },
            stars: mags
                .iter()
                .enumerate()
                .map(|(i, g)| StarRecord {
                    source_id: i as u64,
                    ra_rad: 0.1,
                    dec_rad: 0.1,
                    pm_ra_mas_yr: 0.0,
                    pm_dec_mas_yr: 0.0,
                    g_mag: *g,
                    bp_rp: 1.0,
                })
                .collect(),
        };
        // Faint tile: no zones.
        assert!(exclusion_zones(&tile(&[12.0, 15.0])).is_empty());
        // Bright tile: own cell suppressed, far cells untouched.
        let zones = exclusion_zones(&tile(&[5.5, 14.0]));
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].radius_cells, 2);
        assert!(suppressed_by_any(&zones, [4, 9, 0]));
        assert!(!suppressed_by_any(&zones, [4, 99, 0]));
        // Mid-bright tile: radius 1.
        let zones = exclusion_zones(&tile(&[8.0]));
        assert_eq!(zones[0].radius_cells, 1);
    }

    #[test]
    fn manifest_round_trip_and_rejections() {
        let text = "{\"catalog_version\":\"gaia-dr3-synth/1.0\",\"order\":3,\
            \"seed\":7,\"tile_count\":2,\"total_sources\":5,\"tiles\":[\
            {\"pixel\":9,\"count\":3,\"mean_g\":15.0,\"mean_bp_rp\":1.0,\"fnv1a64\":\"0123456789abcdef\"},\
            {\"pixel\":10,\"count\":2,\"mean_g\":14.0,\"mean_bp_rp\":0.5,\"fnv1a64\":\"fedcba9876543210\"}]}";
        let index = parse_manifest(text).expect("parses");
        assert_eq!(index.catalog_version, "gaia-dr3-synth/1.0");
        assert_eq!(index.order, 3);
        assert_eq!(index.total_sources, 5);
        let meta = index.get(3, 9).expect("tile 9");
        assert_eq!((meta.count, meta.mean_g_mag), (3, 15.0));
        assert_eq!(index.get(3, 11), None);
        // Rejections: empty, truncated, bad shape, count skew, dupes.
        assert_eq!(parse_manifest(""), Err(ManifestError::Empty));
        assert_eq!(
            parse_manifest("{\"catalog_version\":"),
            Err(ManifestError::Truncated)
        );
        assert!(matches!(
            parse_manifest("{\"nope\":1}"),
            Err(ManifestError::UnexpectedChar(_))
        ));
        let skew = text.replace("\"tile_count\":2", "\"tile_count\":3");
        assert_eq!(parse_manifest(&skew), Err(ManifestError::CountMismatch));
        let dupe = text.replace("\"pixel\":10", "\"pixel\":9");
        assert!(matches!(
            parse_manifest(&dupe),
            Err(ManifestError::DuplicateTile(9))
        ));
        let bad_pixel = text.replace("\"pixel\":9", "\"pixel\":999999");
        assert!(matches!(
            parse_manifest(&bad_pixel),
            Err(ManifestError::BadPixel(_))
        ));
        let trailing = format!("{text} ");
        assert!(parse_manifest(&trailing).is_ok(), "trailing space ok");
        assert!(matches!(
            parse_manifest(&format!("{text}x")),
            Err(ManifestError::UnexpectedChar(_))
        ));
    }
}
