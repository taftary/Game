//! Catalog cooker: Gaia-schema tiles from synthetic generation and/or a
//! small CSV extract (`game_tools catalog`).
//!
//! First real consumer of the `docs/techstack/assets.md` cooker pipeline:
//! deterministic, GPU-free, offline. Same inputs ⇒ byte-identical tiles +
//! manifest (grouping via `BTreeMap`, per-tile sort by coordinate bits,
//! stable source-id assignment). Real full-sky import is later work
//! (spec §10 PO decision); this cooker covers demo extracts and tests.
//!
//! ```text
//! game_tools catalog --out <dir> --order 12 [--seed N]
//!                     [--synthetic COUNT] [--csv PATH]
//!                     [--region RA_DEG DEC_DEG RADIUS_DEG]
//! ```
//!
//! `--region` concentrates synthetic sources uniformly inside a sky cap
//! (dense regional fixtures at high orders without cooking millions of
//! near-empty tiles). CSV rows always keep their own coordinates.
//!
//! CSV schema (numeric extracts only, no quoted fields):
//! `source_id,ra_deg,dec_deg,pmra_mas_yr,pmdec_mas_yr,g_mag,bp_rp`.

use game_engine::catalog::fallback::band_normal;
use game_engine::catalog::format::{
    CATALOG_VERSION_GAIA, CATALOG_VERSION_SYNTH, GAIA_EPOCH_YR, StarRecord, TileHeader,
    decode_tile, encode_tile, verify_tile_placement,
};
use game_engine::catalog::healpix::{MAX_ORDER, ang2pix};
use game_engine::core::SeededRng;
use std::collections::{BTreeMap, HashSet};
use std::f64::consts::PI;

/// Gaussian band width (deg) for the synthetic plane concentration.
const BAND_WIDTH_DEG: f64 = 12.0;
/// Uniform background acceptance floor (keeps high-latitude tiles
/// non-empty for scheduler/fallback tests).
const BAND_FLOOR: f64 = 0.3;

/// Cooker failure modes. Every variant prints to stderr with exit 1 —
/// no partial tile set is left behind (tiles write to a fresh per-run
/// staging check first… see [`run`]: manifest writes last, so an
/// interrupted run has no manifest and the loader ignores it).
#[derive(Debug, PartialEq)]
pub enum CookError {
    /// CLI usage error (message carries the usage text).
    Usage(String),
    /// Order outside 0–[`MAX_ORDER`].
    BadOrder(String),
    /// No `--synthetic` and no `--csv` given.
    NoInput,
    /// Filesystem failure (message).
    Io(String),
    /// CSV ingest failure: line number + reason.
    Csv {
        /// 1-based line number (header = 1).
        line: usize,
        /// Reason.
        reason: String,
    },
    /// A cooked tile fails [`verify_tile_placement`] (cooker bug).
    Placement(u64),
}

impl std::fmt::Display for CookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CookError::Usage(m) => write!(f, "{m}"),
            CookError::BadOrder(m) => write!(f, "bad --order: {m}"),
            CookError::NoInput => write!(f, "need --synthetic COUNT and/or --csv PATH"),
            CookError::Io(m) => write!(f, "io error: {m}"),
            CookError::Csv { line, reason } => write!(f, "csv line {line}: {reason}"),
            CookError::Placement(p) => write!(f, "tile {p} fails placement check"),
        }
    }
}

/// Parsed `catalog` subcommand arguments.
pub struct CatalogArgs {
    /// Output directory (tiles + manifest).
    pub out: String,
    /// HEALPix order for tiling.
    pub order: u8,
    /// Master seed for synthetic generation.
    pub seed: u64,
    /// Synthetic source count (0 = none).
    pub synthetic: u64,
    /// Optional CSV extract path.
    pub csv: Option<String>,
    /// Optional sky cap (ra°, dec°, radius°) concentrating synthetic
    /// sources for dense regional fixtures.
    pub region: Option<(f64, f64, f64)>,
}

pub fn usage() -> &'static str {
    "usage: game_tools catalog --out <dir> [--order 12] [--seed N] [--synthetic COUNT] [--csv PATH] [--region RA DEC RADIUS]"
}

fn parse(args: &[String]) -> Result<CatalogArgs, CookError> {
    let mut out: Option<String> = None;
    let mut order = 12u8;
    let mut seed = 42u64;
    let mut synthetic = 0u64;
    let mut csv: Option<String> = None;
    let mut region: Option<(f64, f64, f64)> = None;
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--out" => {
                out = Some(
                    iter.next()
                        .ok_or_else(|| CookError::Usage(usage().to_owned()))?
                        .to_string(),
                );
            }
            "--order" => {
                let value = iter
                    .next()
                    .ok_or_else(|| CookError::Usage(usage().to_owned()))?;
                order = value
                    .parse()
                    .map_err(|_| CookError::BadOrder(format!("{value:?} not a number")))?;
                if order > MAX_ORDER {
                    return Err(CookError::BadOrder(format!("{order} above {MAX_ORDER}")));
                }
            }
            "--seed" => {
                let value = iter
                    .next()
                    .ok_or_else(|| CookError::Usage(usage().to_owned()))?;
                seed = value
                    .parse()
                    .map_err(|_| CookError::Usage(format!("invalid --seed {value:?}")))?;
            }
            "--synthetic" => {
                let value = iter
                    .next()
                    .ok_or_else(|| CookError::Usage(usage().to_owned()))?;
                synthetic = value
                    .parse()
                    .map_err(|_| CookError::Usage(format!("invalid --synthetic {value:?}")))?;
            }
            "--csv" => {
                csv = Some(
                    iter.next()
                        .ok_or_else(|| CookError::Usage(usage().to_owned()))?
                        .to_string(),
                );
            }
            "--region" => {
                let num = |iter: &mut std::iter::Peekable<std::slice::Iter<'_, String>>,
                           what: &str|
                 -> Result<f64, CookError> {
                    iter.next()
                        .ok_or_else(|| CookError::Usage(usage().to_owned()))?
                        .parse()
                        .map_err(|_| CookError::Usage(format!("invalid --region {what}")))
                };
                let ra = num(&mut iter, "ra")?;
                let dec = num(&mut iter, "dec")?;
                let radius = num(&mut iter, "radius")?;
                if !(0.0..360.0).contains(&ra)
                    || !(-90.0..=90.0).contains(&dec)
                    || !(0.0 < radius && radius <= 90.0)
                {
                    return Err(CookError::Usage(
                        "--region needs RA in [0, 360), DEC in [-90, 90], RADIUS in (0, 90]"
                            .to_string(),
                    ));
                }
                region = Some((ra, dec, radius));
            }
            "--help" | "-h" => return Err(CookError::Usage(usage().to_owned())),
            other => return Err(CookError::Usage(format!("unknown argument {other:?}"))),
        }
    }
    let out = out.ok_or_else(|| CookError::Usage(usage().to_owned()))?;
    if synthetic == 0 && csv.is_none() {
        return Err(CookError::NoInput);
    }
    Ok(CatalogArgs {
        out,
        order,
        seed,
        synthetic,
        csv,
        region,
    })
}

/// One pre-tile source: coordinates plus photometry/astrometry payload.
/// Synthetic ids assign at tiling time; CSV rows carry their own.
struct Draft {
    ra_rad: f64,
    dec_rad: f64,
    pm_ra: f32,
    pm_dec: f32,
    g_mag: f32,
    bp_rp: f32,
    source_id: Option<u64>,
}

/// FNV-1a64 (manifest tile hashes; integer ops only, same kernel
/// family as `core::rng`).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01B3);
    }
    hash
}

/// Deterministic synthetic Gaia-like sources: uniform-sphere
/// background plus a tilted galactic-analogue band (rejection sampled
/// against the band normal), faint-heavy magnitudes, small proper
/// motions. With `region`, sampling concentrates uniformly inside the
/// sky cap instead. Pure function of (`seed`, `count`, `region`).
fn generate_synthetic(seed: u64, count: u64, region: Option<(f64, f64, f64)>) -> Vec<Draft> {
    let mut rng = SeededRng::stream(seed, "catalog/synthetic");
    // Band analogue tilt shared with the engine fallback model
    // (single source of truth in `engine::catalog::fallback`).
    let normal = band_normal();
    let width_rad = BAND_WIDTH_DEG.to_radians();
    // Tangent basis at the region center for uniform-in-cap sampling.
    let cap = region.map(|(ra_deg, dec_deg, radius_deg)| {
        let ra = ra_deg.to_radians();
        let dec = dec_deg.to_radians();
        let c = [dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin()];
        // Any non-parallel reference builds the basis; the pole guard
        // keeps it well-conditioned at dec ±90°.
        let reference = if dec.abs() > 89.0_f64.to_radians() {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let mut t1 = [
            c[1] * reference[2] - c[2] * reference[1],
            c[2] * reference[0] - c[0] * reference[2],
            c[0] * reference[1] - c[1] * reference[0],
        ];
        let norm = (t1[0] * t1[0] + t1[1] * t1[1] + t1[2] * t1[2]).sqrt();
        t1 = [t1[0] / norm, t1[1] / norm, t1[2] / norm];
        let t2 = [
            c[1] * t1[2] - c[2] * t1[1],
            c[2] * t1[0] - c[0] * t1[2],
            c[0] * t1[1] - c[1] * t1[0],
        ];
        (c, t1, t2, radius_deg.to_radians())
    });
    let mut out = Vec::with_capacity(count as usize);
    while out.len() < count as usize {
        let dir = match cap {
            // Uniform inside the cap: radius ∝ √u, azimuth uniform.
            Some((c, t1, t2, radius)) => {
                let ang = radius * rng.unit_f64().sqrt();
                let az = rng.range_f64(0.0, 2.0 * PI);
                let (saz, caz) = az.sin_cos();
                let (sang, cang) = ang.sin_cos();
                [
                    c[0] * cang + (t1[0] * caz + t2[0] * saz) * sang,
                    c[1] * cang + (t1[1] * caz + t2[1] * saz) * sang,
                    c[2] * cang + (t1[2] * caz + t2[2] * saz) * sang,
                ]
            }
            None => {
                // Uniform sphere direction.
                let z = rng.range_f64(-1.0, 1.0);
                let phi = rng.range_f64(0.0, 2.0 * PI);
                let r = (1.0 - z * z).sqrt();
                [r * phi.cos(), r * phi.sin(), z]
            }
        };
        // Band latitude analogue: angle from the band plane (dot
        // clamped: unit-vector roundoff must not NaN the accept test).
        let dot = (dir[0] * normal[0] + dir[1] * normal[1] + dir[2] * normal[2]).clamp(-1.0, 1.0);
        let beta = dot.asin();
        let accept = BAND_FLOOR + (1.0 - BAND_FLOOR) * (-(beta / width_rad).powi(2)).exp();
        if rng.unit_f64() > accept {
            continue;
        }
        let dec = dir[2].clamp(-1.0, 1.0).asin();
        let ra = dir[1].atan2(dir[0]).rem_euclid(2.0 * PI);
        // Faint-heavy magnitudes (G 6–21), color correlated + scatter.
        let g = 6.0 + 15.0 * rng.unit_f64().powi(2);
        let bp_rp = (0.5 + 2.0 * rng.unit_f64() + 0.25 * (g - 13.0) / 8.0) as f32;
        out.push(Draft {
            ra_rad: ra,
            dec_rad: dec,
            pm_ra: ((rng.unit_f64() - 0.5) * 6.0) as f32,
            pm_dec: ((rng.unit_f64() - 0.5) * 6.0) as f32,
            g_mag: g as f32,
            bp_rp: bp_rp.clamp(-0.5, 4.0),
            source_id: None,
        });
    }
    out
}

/// Parse a numeric CSV extract. Header must be exactly
/// `source_id,ra_deg,dec_deg,pmra_mas_yr,pmdec_mas_yr,g_mag,bp_rp`;
/// every field range-checked (degrees → radians here).
fn parse_csv(text: &str) -> Result<Vec<Draft>, CookError> {
    const HEADER: &str = "source_id,ra_deg,dec_deg,pmra_mas_yr,pmdec_mas_yr,g_mag,bp_rp";
    let mut lines = text.lines();
    let header = lines.next().ok_or(CookError::Csv {
        line: 1,
        reason: "empty file".to_string(),
    })?;
    if header.trim() != HEADER {
        return Err(CookError::Csv {
            line: 1,
            reason: format!("bad header {header:?}, want {HEADER:?}"),
        });
    }
    let mut out = Vec::new();
    for (index, line) in lines.enumerate() {
        let line_no = index + 2;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() != 7 {
            return Err(CookError::Csv {
                line: line_no,
                reason: format!("want 7 fields, got {}", fields.len()),
            });
        }
        let num = |i: usize, what: &str| -> Result<f64, CookError> {
            fields[i].trim().parse().map_err(|_| CookError::Csv {
                line: line_no,
                reason: format!("bad {what} {:?}", fields[i]),
            })
        };
        let source_id = num(0, "source_id")? as u64;
        let ra_deg = num(1, "ra_deg")?;
        let dec_deg = num(2, "dec_deg")?;
        let pm_ra = num(3, "pmra")? as f32;
        let pm_dec = num(4, "pmdec")? as f32;
        let g_mag = num(5, "g")? as f32;
        let bp_rp = num(6, "bp_rp")? as f32;
        if !(0.0..=360.0).contains(&ra_deg) {
            return Err(CookError::Csv {
                line: line_no,
                reason: format!("ra_deg {ra_deg} out of [0, 360]"),
            });
        }
        if !(-90.0..=90.0).contains(&dec_deg) {
            return Err(CookError::Csv {
                line: line_no,
                reason: format!("dec_deg {dec_deg} out of [-90, 90]"),
            });
        }
        if !pm_ra.is_finite() || !pm_dec.is_finite() || !g_mag.is_finite() || !bp_rp.is_finite() {
            return Err(CookError::Csv {
                line: line_no,
                reason: "non-finite pm/g/bp_rp".to_string(),
            });
        }
        if !(-5.0..=30.0).contains(&(g_mag as f64)) || !(-1.0..=6.0).contains(&(bp_rp as f64)) {
            return Err(CookError::Csv {
                line: line_no,
                reason: format!("g {g_mag} / bp_rp {bp_rp} outside Gaia ranges"),
            });
        }
        out.push(Draft {
            ra_rad: ra_deg.to_radians(),
            dec_rad: dec_deg.to_radians(),
            pm_ra,
            pm_dec,
            g_mag,
            bp_rp,
            source_id: Some(source_id),
        });
    }
    Ok(out)
}

/// Tile drafts by pixel, sort deterministically, assign stable ids to
/// synthetic sources (`pixel << 24 | index`), validate CSV ids unique,
/// build headers with tile means.
fn tile_drafts(
    order: u8,
    drafts: Vec<Draft>,
    epoch_yr: f64,
    catalog_version: &str,
) -> Result<BTreeMap<u64, (TileHeader, Vec<StarRecord>)>, CookError> {
    // Group by pixel: collect + sort (BTreeMap insert per star is
    // O(log tiles) with a large constant; the cooker handles millions).
    let mut keyed: Vec<(u64, Draft)> = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let pixel = ang2pix(order, draft.ra_rad, draft.dec_rad)
            .expect("draft coordinates are range-checked");
        keyed.push((pixel, draft));
    }
    keyed.sort_by_key(|(pixel, draft)| (*pixel, draft.ra_rad.to_bits(), draft.dec_rad.to_bits()));
    let mut seen_ids = HashSet::new();
    let mut tiles: BTreeMap<u64, (TileHeader, Vec<StarRecord>)> = BTreeMap::new();
    let mut run_start = 0usize;
    while run_start < keyed.len() {
        let pixel = keyed[run_start].0;
        let mut run_end = run_start + 1;
        while run_end < keyed.len() && keyed[run_end].0 == pixel {
            run_end += 1;
        }
        let group = &keyed[run_start..run_end];
        let mut stars = Vec::with_capacity(group.len());
        let (mut sum_g, mut sum_c) = (0.0f64, 0.0f64);
        for (index, (_, draft)) in group.iter().enumerate() {
            let source_id = match draft.source_id {
                Some(id) => {
                    if !seen_ids.insert(id) {
                        return Err(CookError::Csv {
                            line: 0,
                            reason: format!("duplicate source_id {id}"),
                        });
                    }
                    id
                }
                // Unique per (pixel, index): pixel < 2³² at order ≤ 14,
                // index < 2²⁴ (decoder cap is 4M records).
                None => (pixel << 24) | index as u64,
            };
            sum_g += f64::from(draft.g_mag);
            sum_c += f64::from(draft.bp_rp);
            stars.push(StarRecord {
                source_id,
                ra_rad: draft.ra_rad,
                dec_rad: draft.dec_rad,
                pm_ra_mas_yr: draft.pm_ra,
                pm_dec_mas_yr: draft.pm_dec,
                g_mag: draft.g_mag,
                bp_rp: draft.bp_rp,
            });
        }
        let count = stars.len() as f64;
        let header = TileHeader {
            order,
            pixel,
            mean_g_mag: (sum_g / count) as f32,
            mean_bp_rp: (sum_c / count) as f32,
            epoch_yr,
            catalog_version: catalog_version.to_string(),
        };
        tiles.insert(pixel, (header, stars));
        run_start = run_end;
    }
    Ok(tiles)
}

/// Cook tiles + manifest into `args.out`. Returns the exit code (0 ok,
/// 1 on any [`CookError`], already reported to stderr).
pub fn run(args: &[String]) -> i32 {
    match run_inner(args) {
        Ok((tiles, sources)) => {
            println!("catalog cooked: {tiles} tiles, {sources} sources");
            0
        }
        Err(error) => {
            eprintln!("game_tools catalog: {error}");
            1
        }
    }
}

fn run_inner(args: &[String]) -> Result<(usize, u64), CookError> {
    let parsed = parse(args)?;
    let mut drafts = Vec::new();
    let mut version = CATALOG_VERSION_SYNTH;
    if parsed.synthetic > 0 {
        drafts.extend(generate_synthetic(
            parsed.seed,
            parsed.synthetic,
            parsed.region,
        ));
    }
    if let Some(path) = &parsed.csv {
        let text = std::fs::read_to_string(path)
            .map_err(|error| CookError::Io(format!("{path}: {error}")))?;
        drafts.extend(parse_csv(&text)?);
        version = CATALOG_VERSION_GAIA;
    }
    // Mixed sources: synthetic keeps the synth stamp only when no CSV
    // is present; mixing stamps per tile is explicit versioning, not
    // silent blending (CSV rows keep their ids; synthetic ids derive
    // per tile below and cannot collide: high bits hold the pixel).
    let tiles = tile_drafts(parsed.order, drafts, GAIA_EPOCH_YR, version)?;
    let dir = format!("{}/order{}", parsed.out, parsed.order);
    std::fs::create_dir_all(&dir).map_err(|error| CookError::Io(format!("{dir}: {error}")))?;
    let mut total = 0u64;
    let mut manifest_tiles = String::from("\"tiles\":[");
    for (index, (pixel, (header, stars))) in tiles.iter().enumerate() {
        let bytes = encode_tile(header, stars);
        let decoded = decode_tile(&bytes).expect("cooker output re-decodes");
        if !verify_tile_placement(&decoded) {
            return Err(CookError::Placement(*pixel));
        }
        let path = format!("{dir}/{pixel}.tile");
        std::fs::write(&path, &bytes).map_err(|error| CookError::Io(format!("{path}: {error}")))?;
        total += stars.len() as u64;
        if index > 0 {
            manifest_tiles.push(',');
        }
        manifest_tiles.push_str(&format!(
            "{{\"pixel\":{pixel},\"count\":{},\"mean_g\":{},\"mean_bp_rp\":{},\"fnv1a64\":\"{:016x}\"}}",
            stars.len(),
            header.mean_g_mag,
            header.mean_bp_rp,
            fnv1a64(&bytes)
        ));
    }
    manifest_tiles.push(']');
    let manifest = format!(
        "{{\"catalog_version\":\"{version}\",\"order\":{},\"seed\":{},\"tile_count\":{},\"total_sources\":{total},{manifest_tiles}}}",
        parsed.order,
        parsed.seed,
        tiles.len(),
    );
    let manifest_path = format!("{}/manifest.json", parsed.out);
    std::fs::write(&manifest_path, manifest)
        .map_err(|error| CookError::Io(format!("{manifest_path}: {error}")))?;
    Ok((tiles.len(), total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out_dir(name: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("game_catalog_cook_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn cook_argv(out: &std::path::Path, extra: &[&str]) -> Vec<String> {
        let mut argv: Vec<String> = vec![
            "--out".to_string(),
            out.to_string_lossy().into_owned(),
            "--order".to_string(),
            "3".to_string(),
            "--seed".to_string(),
            "7".to_string(),
        ];
        argv.extend(extra.iter().map(|s| s.to_string()));
        argv
    }

    #[test]
    fn synthetic_cook_writes_placed_tiles() {
        let out = out_dir("placed");
        let argv = cook_argv(&out, &["--synthetic", "2000"]);
        let (tiles, total) = run_inner(&argv).expect("cook succeeds");
        assert!(tiles > 10, "band + background spread: {tiles} tiles");
        assert_eq!(total, 2000);
        // Every tile file re-decodes and verifies.
        let dir = out.join("order3");
        for entry in std::fs::read_dir(&dir).expect("read tiles") {
            let bytes = std::fs::read(entry.expect("entry").path()).expect("tile bytes");
            let decoded = decode_tile(&bytes).expect("tile decodes");
            assert!(verify_tile_placement(&decoded));
            assert_eq!(
                decoded.header.catalog_version, CATALOG_VERSION_SYNTH,
                "synthetic stamp"
            );
        }
        assert!(out.join("manifest.json").exists(), "manifest written");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn cook_is_byte_identical_across_runs() {
        let a = out_dir("det_a");
        let b = out_dir("det_b");
        run_inner(&cook_argv(&a, &["--synthetic", "1500"])).expect("cook a");
        run_inner(&cook_argv(&b, &["--synthetic", "1500"])).expect("cook b");
        let files = |dir: &std::path::Path| -> Vec<(String, Vec<u8>)> {
            let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir.join("order3"))
                .expect("read")
                .map(|e| {
                    let e = e.expect("entry");
                    (
                        e.file_name().to_string_lossy().into_owned(),
                        std::fs::read(e.path()).expect("bytes"),
                    )
                })
                .collect();
            v.sort();
            v
        };
        assert_eq!(files(&a), files(&b), "deterministic cook");
        let manifest = |dir: &std::path::Path| {
            std::fs::read_to_string(dir.join("manifest.json")).expect("manifest")
        };
        // Manifests differ only in nothing: same inputs ⇒ same manifest.
        assert_eq!(manifest(&a), manifest(&b));
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn csv_ingest_validates_and_tiles() {
        let csv = "source_id,ra_deg,dec_deg,pmra_mas_yr,pmdec_mas_yr,g_mag,bp_rp\n\
                   1001,10.0,20.0,1.5,-2.0,12.0,0.8\n\
                   1002,10.001,20.001,0.0,0.0,15.5,1.4\n";
        let drafts = parse_csv(csv).expect("parses");
        assert_eq!(drafts.len(), 2);
        assert!((drafts[0].ra_rad - 10.0f64.to_radians()).abs() < 1e-12);
        assert_eq!(drafts[0].source_id, Some(1001));
        // Bad header, bad ranges, duplicates rejected with line info.
        assert!(matches!(
            parse_csv("nope\n"),
            Err(CookError::Csv { line: 1, .. })
        ));
        assert!(matches!(
            parse_csv(
                "source_id,ra_deg,dec_deg,pmra_mas_yr,pmdec_mas_yr,g_mag,bp_rp\n1,400.0,0.0,0,0,12,1\n"
            ),
            Err(CookError::Csv { line: 2, .. })
        ));
        let dup = "source_id,ra_deg,dec_deg,pmra_mas_yr,pmdec_mas_yr,g_mag,bp_rp\n\
                   5,10.0,20.0,0,0,12,1\n\
                   5,11.0,21.0,0,0,12,1\n";
        let drafts = parse_csv(dup).expect("rows parse; dup caught at tiling");
        assert!(matches!(
            tile_drafts(3, drafts, GAIA_EPOCH_YR, CATALOG_VERSION_GAIA),
            Err(CookError::Csv { line: 0, .. })
        ));
        // End-to-end through run_inner: CSV rows land in tiles with the
        // real-extract stamp.
        let dir = out_dir("csv_e2e");
        let csv_path = dir.join("extract.csv");
        std::fs::create_dir_all(&dir).expect("csv dir");
        std::fs::write(&csv_path, csv).expect("csv writes");
        let argv = cook_argv(&dir.join("cooked"), &["--csv", &csv_path.to_string_lossy()]);
        let (tiles, total) = run_inner(&argv).expect("csv cook succeeds");
        assert_eq!((tiles, total), (1, 2));
        let mut entries: Vec<_> = std::fs::read_dir(dir.join("cooked/order3"))
            .expect("order dir")
            .collect();
        assert_eq!(entries.len(), 1);
        let bytes =
            std::fs::read(entries.pop().expect("tile").expect("entry").path()).expect("tile bytes");
        let decoded = decode_tile(&bytes).expect("tile decodes");
        assert_eq!(decoded.header.catalog_version, CATALOG_VERSION_GAIA);
        assert!(verify_tile_placement(&decoded));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn region_concentrates_synthetic_sources() {
        let out = out_dir("region");
        let mut argv = cook_argv(&out, &["--synthetic", "5000"]);
        argv.extend(
            ["--region", "266.4", "-29.0", "1.0"]
                .iter()
                .map(|s| s.to_string()),
        );
        // cook_argv pins --order 3; override to 12 for a dense fixture.
        let order_at = argv.iter().position(|a| a == "--order").expect("order");
        argv[order_at + 1] = "12".to_string();
        let (tiles, total) = run_inner(&argv).expect("region cook succeeds");
        assert_eq!(total, 5000);
        // 5000 sources in a 1° cap at order 12: dozens of tiles, each
        // holding many sources (vs ~1/tile for sparse full-sky cooks).
        assert!(tiles < 5000, "concentrated: {tiles} tiles");
        assert!(tiles > 10, "spread across the cap: {tiles} tiles");
        let dir = out.join("order12");
        let mut biggest = 0usize;
        for entry in std::fs::read_dir(&dir).expect("read tiles") {
            let bytes = std::fs::read(entry.expect("entry").path()).expect("bytes");
            let decoded = decode_tile(&bytes).expect("decodes");
            biggest = biggest.max(decoded.stars.len());
            assert!(verify_tile_placement(&decoded));
            // Every source lands inside the cap (generous margin: the
            // cap edge plus half a tile diagonal).
            for star in &decoded.stars {
                let dra = (star.ra_rad - 266.4f64.to_radians()).abs();
                let ddec = (star.dec_rad - (-29.0f64.to_radians())).abs();
                assert!(dra < 0.03 && ddec < 0.03, "stray source");
            }
        }
        assert!(biggest > 1, "dense tiles exist (biggest {biggest})");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn cli_rejects_bad_input() {
        assert!(matches!(parse(&[]), Err(CookError::Usage(_))));
        assert!(matches!(
            parse(&["--out".to_string(), "x".to_string()]),
            Err(CookError::NoInput)
        ));
        assert!(matches!(
            parse(&[
                "--out".to_string(),
                "x".to_string(),
                "--order".to_string(),
                "99".to_string()
            ]),
            Err(CookError::BadOrder(_))
        ));
        assert!(matches!(
            parse(&[
                "--out".to_string(),
                "x".to_string(),
                "--synthetic".to_string(),
                "10".to_string(),
                "--region".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string()
            ]),
            Err(CookError::Usage(_))
        ));
    }
}
