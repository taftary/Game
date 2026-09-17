//! Multi-pass depth compositing scheduler
//! (`plans/v0.2.0/log-depth-rendering`, ADR-016).
//!
//! Log-depth ([`crate::render::depth`]) extends one pass, but 26 decades
//! still need several passes composited far→near: distant bands render
//! depth-cleared and skybox-like first, near-field geometry renders last
//! with tight planes. This module plans those passes — pure data in,
//! ordered pass list out — so the ordering, clearing, and sharing rules
//! are headless-tested and the binaries just execute the plan.
//!
//! Bucket rule (the per-pair-of-scales sharing table, executable form):
//! a content layer's home frame depth is compared against the active
//! frame depth — coarser homes share the Far pass, the active home owns
//! the Mid pass, finer homes share the Near pass. There is deliberately
//! no global depth range: every pass carries its own.
//!
//! ```
//! use game_engine::frames::{BodyId, FrameId};
//! use game_engine::render::bands::{BandConfig, ContentLayer, LayerKind, PassBucket, plan_passes};
//!
//! let config = BandConfig::low_defaults();
//! let layers = [
//!     ContentLayer { home: FrameId::Galactocentric, kind: LayerKind::Content },
//!     ContentLayer { home: FrameId::SolarSystem, kind: LayerKind::Content },
//!     ContentLayer { home: FrameId::Planetocentric(BodyId::EARTH), kind: LayerKind::Content },
//! ];
//! let passes = plan_passes(FrameId::SolarSystem, &layers, &config);
//! let buckets: Vec<PassBucket> = passes.iter().map(|p| p.bucket).collect();
//! assert_eq!(buckets, [PassBucket::Far, PassBucket::Mid, PassBucket::Near]);
//! assert!(passes.iter().all(|p| p.clear_depth));
//! ```

use crate::frames::FrameId;

/// What a layer needs from the depth buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayerKind {
    /// Skybox-like backdrop (map points, sky gradient): no depth write,
    /// no depth test, always the leading pass.
    Backdrop,
    /// Depth-tested geometry (planet mesh, chunks, colony instancing).
    Content,
    /// Depth-tested but non-writing overlay (wireframe, markers).
    /// Rides the pass of content at the same home scale.
    Overlay,
    /// Screen-space UI: no depth, always the trailing pass.
    Ui,
}

/// One drawable layer: the frame its content is authored in plus what it
/// needs from depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContentLayer {
    /// Home frame of the layer's content.
    pub home: FrameId,
    /// Depth behavior of the layer.
    pub kind: LayerKind,
}

/// Depth passes, emission order = execution order (far → near).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PassBucket {
    /// Leading skybox-like pass: depth cleared, no depth attachment use.
    Backdrop,
    /// Coarser-than-active content, log-depth (`depth` module).
    Far,
    /// Active-scale content, log-depth.
    Mid,
    /// Finer-than-active content, tight linear depth.
    Near,
    /// Trailing screen-space pass, no depth.
    Ui,
}

/// Depth interpretation of one pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthMode {
    /// No depth attachment (backdrop / UI).
    None,
    /// Log-depth encoding (`depth::LogDepthParams` with the pass far).
    Log,
    /// Classic linear depth with tight planes.
    LinearTight,
}

/// One executable pass: bucket, depth mode, and its own near/far range
/// (`None` where no depth attachment is used).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandPass {
    /// Which pass this is (also its position class in the plan).
    pub bucket: PassBucket,
    /// How depth behaves inside the pass.
    pub mode: DepthMode,
    /// Whether the depth attachment is cleared before this pass.
    /// Always true: every band reinterprets depth, so no band may inherit
    /// another band's values. Binaries clear depth per pass and composite
    /// color with load (preserve).
    pub clear_depth: bool,
    /// `(near, far)` in meters for projection + log normalization, or
    /// `None` for depth-less passes.
    pub range: Option<(f64, f64)>,
}

/// Tunables for pass planning. Exact band boundaries per tier stay open
/// (notion open question, ADR-007 data pending): this ships Low-tier
/// defaults plus explicit overrides, never baked numbers.
///
/// ```
/// use game_engine::render::bands::BandConfig;
///
/// let defaults = BandConfig::low_defaults();
/// assert!(BandConfig::new(1.0e15, 0.1).is_some());
/// assert!(BandConfig::new(0.1, 1.0e15).is_none());
/// assert!(BandConfig::new(1.0, 0.0).is_none());
/// assert_eq!(defaults.log_far, 3.0e26);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandConfig {
    /// Far plane normalizing the Far pass log encoding; must exceed the
    /// active extent (Low default covers the cosmological extent).
    pub log_far: f64,
    /// Projection near plane for Mid/Near passes (Low default 0.1 m).
    pub near_plane: f64,
}

impl BandConfig {
    /// Low-tier defaults: `log_far` covers the full cosmological extent
    /// (`FrameId::Cosmological::extent_meters`), `near_plane` 0.1 m.
    pub fn low_defaults() -> Self {
        Self {
            log_far: 3.0e26,
            near_plane: 0.1,
        }
    }

    /// Validated override: `None` unless `0 < near_plane < log_far`.
    pub fn new(log_far: f64, near_plane: f64) -> Option<Self> {
        if log_far.is_finite() && near_plane.is_finite() && near_plane > 0.0 && near_plane < log_far
        {
            Some(Self {
                log_far,
                near_plane,
            })
        } else {
            None
        }
    }
}

/// Which pass bucket a layer lands in under `active`: the sharing table
/// as a function. Backdrop always shares the Backdrop pass, UI the Ui
/// pass; content/overlay bucket by home-vs-active depth.
pub fn bucket_for(home: FrameId, kind: LayerKind, active: FrameId) -> PassBucket {
    match kind {
        LayerKind::Backdrop => PassBucket::Backdrop,
        LayerKind::Ui => PassBucket::Ui,
        LayerKind::Content | LayerKind::Overlay => {
            use std::cmp::Ordering::*;
            match home.depth().cmp(&active.depth()) {
                Less => PassBucket::Far,
                Equal => PassBucket::Mid,
                Greater => PassBucket::Near,
            }
        }
    }
}

/// Whether content authored at `viewed_a` and `viewed_b` shares one depth
/// pass when `active` is the active frame — the per-pair-of-scales
/// decision, no global range involved.
///
/// ```
/// use game_engine::frames::FrameId;
/// use game_engine::render::bands::shares_depth_pass;
///
/// // Two coarser-than-SolarSystem frames share the Far pass …
/// assert!(shares_depth_pass(
///     FrameId::Galactocentric,
///     FrameId::LocalGroup,
///     FrameId::SolarSystem
/// ));
/// // … but active-scale and near-field content never share.
/// assert!(!shares_depth_pass(
///     FrameId::SolarSystem,
///     FrameId::Planetocentric(game_engine::frames::BodyId::EARTH),
///     FrameId::SolarSystem
/// ));
/// ```
pub fn shares_depth_pass(viewed_a: FrameId, viewed_b: FrameId, active: FrameId) -> bool {
    bucket_for(viewed_a, LayerKind::Content, active)
        == bucket_for(viewed_b, LayerKind::Content, active)
}

/// Plans the pass list for `active` over `layers`: buckets in far→near
/// execution order, empty buckets skipped, every emitted pass
/// depth-cleared with its own range.
///
/// Range rules (meters):
/// - Far: `(extent(active), max(log_far, extent(active)))`.
/// - Mid: `(near_plane, extent(active))`.
/// - Near: `(near_plane, min(finer content extents, extent(active)))` —
///   tight to the nearest actual geometry.
/// - Backdrop / Ui: `None` (no depth attachment).
pub fn plan_passes(active: FrameId, layers: &[ContentLayer], config: &BandConfig) -> Vec<BandPass> {
    const ORDER: [PassBucket; 5] = [
        PassBucket::Backdrop,
        PassBucket::Far,
        PassBucket::Mid,
        PassBucket::Near,
        PassBucket::Ui,
    ];
    let active_extent = active.extent_meters();
    let mut passes = Vec::new();
    for bucket in ORDER {
        let present = layers
            .iter()
            .any(|l| bucket_for(l.home, l.kind, active) == bucket);
        if !present {
            continue;
        }
        let (mode, range) = match bucket {
            PassBucket::Backdrop | PassBucket::Ui => (DepthMode::None, None),
            PassBucket::Far => (
                DepthMode::Log,
                Some((active_extent, config.log_far.max(active_extent))),
            ),
            PassBucket::Mid => (DepthMode::Log, Some((config.near_plane, active_extent))),
            PassBucket::Near => {
                let finest = layers
                    .iter()
                    .filter(|l| bucket_for(l.home, l.kind, active) == PassBucket::Near)
                    .map(|l| l.home.extent_meters())
                    .fold(active_extent, f64::min);
                (DepthMode::LinearTight, Some((config.near_plane, finest)))
            }
        };
        passes.push(BandPass {
            bucket,
            mode,
            clear_depth: true,
            range,
        });
    }
    passes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::BodyId;

    fn content(home: FrameId) -> ContentLayer {
        ContentLayer {
            home,
            kind: LayerKind::Content,
        }
    }

    #[test]
    fn mixed_scene_plans_far_mid_near_in_order() {
        let config = BandConfig::low_defaults();
        let layers = [
            content(FrameId::Galactocentric),
            content(FrameId::SolarSystem),
            content(FrameId::Planetocentric(BodyId::EARTH)),
        ];
        let passes = plan_passes(FrameId::SolarSystem, &layers, &config);
        let buckets: Vec<PassBucket> = passes.iter().map(|p| p.bucket).collect();
        assert_eq!(
            buckets,
            [PassBucket::Far, PassBucket::Mid, PassBucket::Near]
        );
        assert!(passes.iter().all(|p| p.clear_depth));
        assert!(passes.iter().all(|p| p.range.is_some()));
        // Modes: log outside, tight linear inside.
        assert_eq!(passes[0].mode, DepthMode::Log);
        assert_eq!(passes[1].mode, DepthMode::Log);
        assert_eq!(passes[2].mode, DepthMode::LinearTight);
    }

    #[test]
    fn backdrop_leads_and_ui_trails_with_no_range() {
        let config = BandConfig::low_defaults();
        let layers = [
            ContentLayer {
                home: FrameId::Cosmological,
                kind: LayerKind::Backdrop,
            },
            content(FrameId::SolarSystem),
            ContentLayer {
                home: FrameId::SolarSystem,
                kind: LayerKind::Ui,
            },
        ];
        let passes = plan_passes(FrameId::SolarSystem, &layers, &config);
        let buckets: Vec<PassBucket> = passes.iter().map(|p| p.bucket).collect();
        assert_eq!(
            buckets,
            [PassBucket::Backdrop, PassBucket::Mid, PassBucket::Ui]
        );
        assert_eq!(passes[0].mode, DepthMode::None);
        assert_eq!(passes[0].range, None);
        assert_eq!(passes[2].mode, DepthMode::None);
        assert_eq!(passes[2].range, None);
        assert!(passes.iter().all(|p| p.clear_depth));
    }

    #[test]
    fn empty_buckets_are_skipped() {
        let config = BandConfig::low_defaults();
        let passes = plan_passes(
            FrameId::SolarSystem,
            &[content(FrameId::SolarSystem)],
            &config,
        );
        assert_eq!(passes.len(), 1);
        assert_eq!(passes[0].bucket, PassBucket::Mid);
    }

    #[test]
    fn ranges_are_anchored_to_the_active_extent() {
        let config = BandConfig::low_defaults();
        let layers = [
            content(FrameId::StellarNeighborhood),
            content(FrameId::SolarSystem),
            content(FrameId::LocalEnu(BodyId::EARTH)),
        ];
        let passes = plan_passes(FrameId::SolarSystem, &layers, &config);
        let active_extent = FrameId::SolarSystem.extent_meters();
        assert_eq!(passes[0].range, Some((active_extent, config.log_far)));
        assert_eq!(passes[1].range, Some((config.near_plane, active_extent)));
        // Near pass is tight to the finest content (local-enu 1e5 m).
        assert_eq!(passes[2].range, Some((config.near_plane, 1.0e5)));
    }

    #[test]
    fn overlay_rides_its_home_scale_pass() {
        let overlay = ContentLayer {
            home: FrameId::Planetocentric(BodyId::EARTH),
            kind: LayerKind::Overlay,
        };
        assert_eq!(
            bucket_for(overlay.home, overlay.kind, FrameId::SolarSystem),
            PassBucket::Near
        );
        assert_eq!(
            bucket_for(
                overlay.home,
                overlay.kind,
                FrameId::Planetocentric(BodyId::EARTH)
            ),
            PassBucket::Mid
        );
    }

    #[test]
    fn sharing_table_spot_checks() {
        // Same home always shares.
        assert!(shares_depth_pass(
            FrameId::SolarSystem,
            FrameId::SolarSystem,
            FrameId::SolarSystem
        ));
        // Two coarser homes share Far; two finer homes share Near.
        assert!(shares_depth_pass(
            FrameId::Galactocentric,
            FrameId::StellarNeighborhood,
            FrameId::SolarSystem
        ));
        assert!(shares_depth_pass(
            FrameId::Planetocentric(BodyId::EARTH),
            FrameId::LocalEnu(BodyId::EARTH),
            FrameId::SolarSystem
        ));
        // Cross-bucket pairs never share — no global range.
        assert!(!shares_depth_pass(
            FrameId::StellarNeighborhood,
            FrameId::SolarSystem,
            FrameId::SolarSystem
        ));
        assert!(!shares_depth_pass(
            FrameId::SolarSystem,
            FrameId::LocalEnu(BodyId::EARTH),
            FrameId::SolarSystem
        ));
        assert!(!shares_depth_pass(
            FrameId::Cosmological,
            FrameId::LocalEnu(BodyId::EARTH),
            FrameId::StellarNeighborhood
        ));
    }
}
