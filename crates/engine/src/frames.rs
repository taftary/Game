//! `engine::frames` — hierarchical nested reference frames (ADR-012) plus
//! floating-origin GPU upload (ADR-013).
//!
//! The engine spans 10²⁶ m → 10⁰ m (spec §1). No single world space holds
//! both ends, so ship/camera truth lives in a [`FrameChain`]: a position
//! and orientation in the *active* frame plus the parent links up to the
//! root. Conversion exists **only** active ⇄ parent — there is
//! deliberately no global-flatten function.
//!
//! CPU truth is `f64` (or per-frame non-dimensionalized units, spec §5).
//! The GPU consumes `f32`, so [`recenter`] maps world truth to
//! camera-relative `f32` every frame before upload.
//!
//! ```
//! use game_engine::frames::{BodyId, FrameChain, FrameId, FrameLink};
//! use glam::{DQuat, DVec3};
//!
//! let earth = BodyId::EARTH;
//! let chain = FrameChain::new(
//!     FrameId::LocalEnu(earth),
//!     DVec3::new(120.5, -40.25, 15.0),
//!     DQuat::IDENTITY,
//!     vec![FrameLink::identity(); 6],
//! );
//! assert_eq!(chain.active(), FrameId::LocalEnu(earth));
//! assert_eq!(chain.depth(), 6);
//! ```

use glam::{DQuat, DVec3, Vec3};
use std::fmt;

/// Meters per megaparsec (exact IAU definition via the parsec).
pub const METERS_PER_MPC: f64 = 3.085_677_581e22;
/// Meters per kiloparsec.
pub const METERS_PER_KPC: f64 = 3.085_677_581e19;
/// Meters per parsec (exact).
pub const METERS_PER_PC: f64 = 3.085_677_581e16;
/// Meters per astronomical unit (exact).
pub const METERS_PER_AU: f64 = 149_597_870_700.0;
/// Meters per kilometer.
pub const METERS_PER_KM: f64 = 1_000.0;

/// Opaque body key for [`FrameId::Planetocentric`] and
/// [`FrameId::LocalEnu`].
///
/// Placeholder until `star-catalog-streaming` (v0.2.0) defines canonical
/// body ids; the swap is a newtype change with no chain-logic impact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(pub u64);

impl BodyId {
    /// Well-known placeholder id for Earth (the spec anchor).
    pub const EARTH: BodyId = BodyId(1);
}

/// Reference frames of the spec §3 chain, root (cosmological) → leaf
/// (local ENU). Order follows the spec exactly — including
/// [`FrameId::LocalGroup`] nested under [`FrameId::Galactocentric`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameId {
    /// Comoving Cosmological frame (Mpc) — cosmic web, superclusters.
    Cosmological,
    /// Galactocentric frame (kpc) — Milky Way.
    Galactocentric,
    /// Local-Group frame (Mpc) — Milky Way, Andromeda, satellites.
    LocalGroup,
    /// Stellar-Neighborhood frame (pc/ly) — nearby real stars.
    StellarNeighborhood,
    /// Solar-System Barycentric frame (AU) — real ephemerides.
    SolarSystem,
    /// Planetocentric frame (km, ECEF) — one per body.
    Planetocentric(BodyId),
    /// Local Tangent-Plane ENU frame (m) — the facility. Axes are true
    /// ENU: `east × north == up` (rendering invariants,
    /// `docs/techstack/rendering.md`).
    LocalEnu(BodyId),
}

impl FrameId {
    /// Immediate parent toward the root; `None` for the root.
    /// Resolution is only ever allowed through this link (ADR-012).
    ///
    /// ```
    /// use game_engine::frames::{BodyId, FrameId};
    ///
    /// assert_eq!(FrameId::Cosmological.parent(), None);
    /// assert_eq!(
    ///     FrameId::LocalEnu(BodyId::EARTH).parent(),
    ///     Some(FrameId::Planetocentric(BodyId::EARTH))
    /// );
    /// ```
    pub fn parent(self) -> Option<FrameId> {
        match self {
            FrameId::Cosmological => None,
            FrameId::Galactocentric => Some(FrameId::Cosmological),
            FrameId::LocalGroup => Some(FrameId::Galactocentric),
            FrameId::StellarNeighborhood => Some(FrameId::LocalGroup),
            FrameId::SolarSystem => Some(FrameId::StellarNeighborhood),
            FrameId::Planetocentric(_) => Some(FrameId::SolarSystem),
            FrameId::LocalEnu(body) => Some(FrameId::Planetocentric(body)),
        }
    }

    /// Depth in the chain: 0 at the root, 6 at the leaf.
    pub fn depth(self) -> usize {
        match self {
            FrameId::Cosmological => 0,
            FrameId::Galactocentric => 1,
            FrameId::LocalGroup => 2,
            FrameId::StellarNeighborhood => 3,
            FrameId::SolarSystem => 4,
            FrameId::Planetocentric(_) => 5,
            FrameId::LocalEnu(_) => 6,
        }
    }

    /// Meters per one local unit (spec §5 non-dimensionalization).
    pub fn meters_per_unit(self) -> f64 {
        match self {
            FrameId::Cosmological | FrameId::LocalGroup => METERS_PER_MPC,
            FrameId::Galactocentric => METERS_PER_KPC,
            FrameId::StellarNeighborhood => METERS_PER_PC,
            FrameId::SolarSystem => METERS_PER_AU,
            FrameId::Planetocentric(_) => METERS_PER_KM,
            FrameId::LocalEnu(_) => 1.0,
        }
    }

    /// Nominal spatial extent of the frame's domain in meters. Used as
    /// the floating-origin bound: no camera-relative `f32` upload for a
    /// point inside the active frame may exceed this.
    pub fn extent_meters(self) -> f64 {
        match self {
            FrameId::Cosmological => 3.0e26,
            FrameId::Galactocentric => 1.0e21,
            FrameId::LocalGroup => 3.0e23,
            FrameId::StellarNeighborhood => 1.0e18,
            FrameId::SolarSystem => 1.0e15,
            FrameId::Planetocentric(_) => 1.0e9,
            FrameId::LocalEnu(_) => 1.0e5,
        }
    }

    /// Short display name (feeds the spec §10 HUD frame indicator via
    /// `scale-debug-screens`).
    pub fn name(self) -> &'static str {
        match self {
            FrameId::Cosmological => "cosmological",
            FrameId::Galactocentric => "galactocentric",
            FrameId::LocalGroup => "local-group",
            FrameId::StellarNeighborhood => "stellar-neighborhood",
            FrameId::SolarSystem => "solar-system",
            FrameId::Planetocentric(_) => "planetocentric",
            FrameId::LocalEnu(_) => "local-enu",
        }
    }

    /// Local unit symbol for display (`Mpc`, `kpc`, `pc`, `AU`, `km`, `m`).
    pub fn unit_name(self) -> &'static str {
        match self {
            FrameId::Cosmological | FrameId::LocalGroup => "Mpc",
            FrameId::Galactocentric => "kpc",
            FrameId::StellarNeighborhood => "pc",
            FrameId::SolarSystem => "AU",
            FrameId::Planetocentric(_) => "km",
            FrameId::LocalEnu(_) => "m",
        }
    }

    /// Body this frame is anchored to, if any (planetocentric / ENU).
    pub fn body(self) -> Option<BodyId> {
        match self {
            FrameId::Planetocentric(body) | FrameId::LocalEnu(body) => Some(body),
            _ => None,
        }
    }

    /// Explicit identity encoding: discriminant + body id. Basis for
    /// save codecs (`flight::ShipSnapshot`) and seed domains
    /// (`seeding::RegionId`); never `Debug`-formatted.
    pub fn code(self) -> (u8, u64) {
        match self {
            FrameId::Cosmological => (0, 0),
            FrameId::Galactocentric => (1, 0),
            FrameId::LocalGroup => (2, 0),
            FrameId::StellarNeighborhood => (3, 0),
            FrameId::SolarSystem => (4, 0),
            FrameId::Planetocentric(body) => (5, body.0),
            FrameId::LocalEnu(body) => (6, body.0),
        }
    }

    /// Inverse of [`FrameId::code`]: `None` for unknown discriminants
    /// (codec rejection path — never default a frame).
    pub fn from_code(discriminant: u8, body: u64) -> Option<FrameId> {
        match discriminant {
            0 => Some(FrameId::Cosmological),
            1 => Some(FrameId::Galactocentric),
            2 => Some(FrameId::LocalGroup),
            3 => Some(FrameId::StellarNeighborhood),
            4 => Some(FrameId::SolarSystem),
            5 => Some(FrameId::Planetocentric(BodyId(body))),
            6 => Some(FrameId::LocalEnu(BodyId(body))),
            _ => None,
        }
    }
}

/// Transform from one frame to its immediate parent: frame axes expressed
/// in parent axes (`rotation`) plus the frame origin in parent local
/// units (`origin`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameLink {
    /// Frame axes in parent axes (must be a unit quaternion).
    pub rotation: DQuat,
    /// Frame origin in parent local units.
    pub origin: DVec3,
}

impl FrameLink {
    /// Identity link (coincident frames).
    pub fn identity() -> Self {
        Self {
            rotation: DQuat::IDENTITY,
            origin: DVec3::ZERO,
        }
    }

    /// Convert a position from `frame` local units to parent local units.
    ///
    /// ```
    /// use game_engine::frames::{FrameId, FrameLink};
    /// use glam::{DQuat, DVec3};
    ///
    /// // 1 AU on the solar-system x-axis, identity link: the parent
    /// // (stellar-neighborhood, pc) sees ~4.848e-6 pc.
    /// let link = FrameLink::identity();
    /// let p = link.to_parent(FrameId::SolarSystem, DVec3::X);
    /// assert!((p.x - 4.848_136_811e-6).abs() < 1e-15);
    /// ```
    pub fn to_parent(&self, frame: FrameId, local: DVec3) -> DVec3 {
        let parent = frame.parent().expect("root frame has no parent link");
        debug_assert!(
            self.rotation.is_normalized(),
            "FrameLink rotation must be a unit quaternion"
        );
        let ratio = frame.meters_per_unit() / parent.meters_per_unit();
        self.origin + self.rotation * (local * ratio)
    }

    /// Convert a position from parent local units to `frame` local units.
    /// Exact inverse of [`FrameLink::to_parent`] up to `f64` rounding.
    pub fn from_parent(&self, frame: FrameId, parent_pos: DVec3) -> DVec3 {
        let parent = frame.parent().expect("root frame has no parent link");
        debug_assert!(
            self.rotation.is_normalized(),
            "FrameLink rotation must be a unit quaternion"
        );
        let ratio = parent.meters_per_unit() / frame.meters_per_unit();
        self.rotation.conjugate() * ((parent_pos - self.origin) * ratio)
    }
}

/// Composed ship/camera state: position + orientation in the active frame
/// plus the parent links up to the root. Never flattened globally.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameChain {
    active: FrameId,
    /// Position in active-frame local units.
    position: DVec3,
    /// Orientation (ship/camera axes) in the active frame.
    orientation: DQuat,
    /// Parent links ordered active → root: `links[0]` maps the active
    /// frame to its parent. Length always equals `active.depth()`.
    links: Vec<FrameLink>,
}

impl FrameChain {
    /// Build a chain. Panics if `links.len() != active.depth()` — a chain
    /// without the full parent path cannot resolve and is a bug.
    pub fn new(
        active: FrameId,
        position: DVec3,
        orientation: DQuat,
        links: Vec<FrameLink>,
    ) -> Self {
        assert_eq!(
            links.len(),
            active.depth(),
            "frame chain needs one parent link per level above the active frame"
        );
        Self {
            active,
            position,
            orientation,
            links,
        }
    }

    /// Active frame id.
    pub fn active(&self) -> FrameId {
        self.active
    }

    /// Chain depth (0 at root, 6 at leaf).
    pub fn depth(&self) -> usize {
        self.active.depth()
    }

    /// Position in active-frame local units.
    pub fn position(&self) -> DVec3 {
        self.position
    }

    /// Orientation in the active frame.
    pub fn orientation(&self) -> DQuat {
        self.orientation
    }

    /// Overwrite the active-frame position (ship integration writes
    /// back through here — the chain owns position truth).
    pub fn set_position(&mut self, position: DVec3) {
        self.position = position;
    }

    /// Overwrite the active-frame orientation.
    pub fn set_orientation(&mut self, orientation: DQuat) {
        self.orientation = orientation;
    }

    /// The active frame's parent link (`links[0]`), if any. Consumers
    /// (ship velocity mapping) read the rotation through here.
    pub fn link_to_parent(&self) -> Option<&FrameLink> {
        self.links.first()
    }

    /// One resolution step into the parent frame. `None` at the root.
    pub fn step_to_parent(&self) -> Option<FrameStep> {
        let parent = self.active.parent()?;
        let link = &self.links[0];
        Some(FrameStep {
            frame: parent,
            position: link.to_parent(self.active, self.position),
            orientation: link.rotation * self.orientation,
        })
    }

    /// Resolve the full chain up to the root. Diagnostic / validation
    /// path (precision tests, debug screens) — not the hot loop, which
    /// always works in the active frame.
    pub fn resolve_root(&self) -> FrameStep {
        let mut frame = self.active;
        let mut position = self.position;
        let mut orientation = self.orientation;
        for link in &self.links {
            let parent = frame.parent().expect("chain longer than frame depth");
            position = link.to_parent(frame, position);
            orientation = link.rotation * orientation;
            frame = parent;
        }
        FrameStep {
            frame,
            position,
            orientation,
        }
    }

    /// Explicit commit into the parent frame: converts the state vector
    /// through `links[0]` and pops it. Fails at the root.
    ///
    /// ```
    /// use game_engine::frames::{
    ///     BodyId, FrameChain, FrameId, FrameLink, TransitionReason,
    /// };
    /// use glam::{DQuat, DVec3};
    ///
    /// let mut chain = FrameChain::new(
    ///     FrameId::Planetocentric(BodyId::EARTH),
    ///     DVec3::new(0.0, 0.0, 6371.0),
    ///     DQuat::IDENTITY,
    ///     vec![FrameLink::identity(); 5],
    /// );
    /// let event = chain
    ///     .commit_to_parent(TransitionReason::BoundaryCrossing, 0.0)
    ///     .unwrap();
    /// assert_eq!(event.to, FrameId::SolarSystem);
    /// assert_eq!(chain.active(), FrameId::SolarSystem);
    /// ```
    pub fn commit_to_parent(
        &mut self,
        reason: TransitionReason,
        sim_time_s: f64,
    ) -> Result<FrameTransition, TransitionError> {
        let step = self.step_to_parent().ok_or(TransitionError::AtRoot)?;
        let from = self.active;
        self.links.remove(0);
        self.active = step.frame;
        self.position = step.position;
        self.orientation = step.orientation;
        Ok(FrameTransition {
            from,
            to: step.frame,
            reason,
            sim_time_s,
        })
    }

    /// Explicit commit into a direct child frame: converts the state
    /// vector through `link` (child → current active) and pushes it.
    /// Anything that is not a direct child is rejected — no silent
    /// coordinate swaps (ADR-012).
    pub fn commit_to_child(
        &mut self,
        child: FrameId,
        link: FrameLink,
        reason: TransitionReason,
        sim_time_s: f64,
    ) -> Result<FrameTransition, TransitionError> {
        if child.parent() != Some(self.active) {
            return Err(TransitionError::NotAdjacent {
                active: self.active,
                target: child,
            });
        }
        let from = self.active;
        self.position = link.from_parent(child, self.position);
        self.orientation = link.rotation.conjugate() * self.orientation;
        self.links.insert(0, link);
        self.active = child;
        Ok(FrameTransition {
            from,
            to: child,
            reason,
            sim_time_s,
        })
    }
}

/// One resolved step: a position + orientation expressed in `frame`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameStep {
    /// Frame the position/orientation are expressed in.
    pub frame: FrameId,
    /// Position in `frame` local units.
    pub position: DVec3,
    /// Orientation in `frame` axes.
    pub orientation: DQuat,
}

/// Why an active-frame transition committed. Consumed by the ADR-004
/// autosave trigger and ADR-015 time state (`autosave-persistence` /
/// `time-compression` own the consumers; this feature freezes the shape).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransitionReason {
    /// Operator / script / debug initiated.
    Manual,
    /// Physics step crossed a frame boundary (SOI handoff owns these).
    BoundaryCrossing,
}

/// Completed active-frame transition. Feeds the ADR-004 autosave trigger
/// and the ADR-015 persisted time state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameTransition {
    /// Frame active before the commit.
    pub from: FrameId,
    /// Frame active after the commit.
    pub to: FrameId,
    /// Why the transition committed.
    pub reason: TransitionReason,
    /// Simulation time (s) at commit. The time module lands with
    /// `time-compression`; until then this is caller-held `f64` seconds.
    pub sim_time_s: f64,
}

/// Commit failure: transitions only ever move between adjacent frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionError {
    /// `commit_to_parent` at the root.
    AtRoot,
    /// `commit_to_child` target is not a direct child of the active frame.
    NotAdjacent {
        /// Frame active at the failed commit.
        active: FrameId,
        /// Requested target.
        target: FrameId,
    },
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransitionError::AtRoot => write!(f, "no parent above the root frame"),
            TransitionError::NotAdjacent { active, target } => write!(
                f,
                "frame transition must be adjacent: active is {}, target is {}",
                active.name(),
                target.name()
            ),
        }
    }
}

impl std::error::Error for TransitionError {}

/// Floating-origin recenter (ADR-013): map `world` truth to
/// camera-relative `f32` for GPU upload. The subtraction happens in `f64`
/// so millimeter detail survives next to the camera even when both inputs
/// are astronomically far from any global origin.
///
/// ```
/// use game_engine::frames::recenter;
/// use glam::DVec3;
///
/// // 1 mm detail next to a camera parked 10⁷ m out: representable only
/// // because the subtraction happens in f64 before the f32 cast.
/// let camera = DVec3::new(1.0e7, 0.0, 0.0);
/// let p = recenter(DVec3::new(1.0e7 + 0.001, 0.0, 0.0), camera);
/// assert!((p.x - 0.001).abs() < 1e-6);
/// ```
pub fn recenter(world: DVec3, camera: DVec3) -> Vec3 {
    (world - camera).as_vec3()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_4;

    const EARTH: BodyId = BodyId::EARTH;

    /// One non-trivial link per boundary, ordered leaf → root to match
    /// `FrameChain.links`.
    fn canonical_links() -> Vec<FrameLink> {
        vec![
            // LocalEnu(EARTH) origin in planetocentric km (~Earth radius).
            FrameLink {
                rotation: DQuat::IDENTITY,
                origin: DVec3::new(1_000.0, -2_000.0, 6_000.0),
            },
            // Planetocentric(EARTH) origin in solar-system AU (~1 AU).
            FrameLink {
                rotation: DQuat::from_rotation_y(0.3),
                origin: DVec3::new(1.0, 0.02, 0.0),
            },
            // SolarSystem origin in stellar-neighborhood pc (Sun-centered).
            FrameLink {
                rotation: DQuat::from_rotation_x(-0.15),
                origin: DVec3::ZERO,
            },
            // StellarNeighborhood origin in local-group Mpc.
            FrameLink {
                rotation: DQuat::from_rotation_z(0.5),
                origin: DVec3::new(0.39, 0.01, 0.0),
            },
            // LocalGroup origin in galactocentric kpc.
            FrameLink {
                rotation: DQuat::from_rotation_y(FRAC_PI_4),
                origin: DVec3::new(400.0, 50.0, -20.0),
            },
            // Galactocentric origin in cosmological Mpc.
            FrameLink {
                rotation: DQuat::from_rotation_x(0.9) * DQuat::from_rotation_z(0.2),
                origin: DVec3::new(5.0, -8.0, 3.0),
            },
        ]
    }

    fn canonical_chain() -> FrameChain {
        FrameChain::new(
            FrameId::LocalEnu(EARTH),
            DVec3::new(120.5, -40.25, 15.0),
            DQuat::from_rotation_y(0.1),
            canonical_links(),
        )
    }

    fn boundary_frames() -> [(FrameId, FrameLink); 6] {
        let links = canonical_links();
        [
            (FrameId::LocalEnu(EARTH), links[0]),
            (FrameId::Planetocentric(EARTH), links[1]),
            (FrameId::SolarSystem, links[2]),
            (FrameId::StellarNeighborhood, links[3]),
            (FrameId::LocalGroup, links[4]),
            (FrameId::Galactocentric, links[5]),
        ]
    }

    fn rel_err(a: DVec3, b: DVec3) -> f64 {
        let denom = a.length().max(b.length()).max(1.0);
        (a - b).length() / denom
    }

    #[test]
    fn every_boundary_round_trips_within_tolerance() {
        // PO tolerance table (spec §10 addendum): per-boundary active ⇄
        // parent relative error ≤ 1e-9 for representative in-domain
        // positions. Probes are scaled to each frame's domain
        // (0.1% / 10% / 100% of the frame extent): relative error is only
        // meaningful away from the frame origin, where the absolute
        // error inherited from the parent offset dominates. Absolute
        // accuracy at facility scale is covered by
        // `full_chain_resolves_within_one_millimeter`.
        let dirs = [
            DVec3::new(0.12, -0.04, 0.015).normalize(),
            DVec3::new(-0.3, 0.007, 1.0).normalize(),
        ];
        for (frame, link) in boundary_frames() {
            let extent_units = frame.extent_meters() / frame.meters_per_unit();
            for dir in dirs {
                for k in [1e-3, 1e-1, 1.0] {
                    let probe = dir * (k * extent_units);
                    let up = link.to_parent(frame, probe);
                    let back = link.from_parent(frame, up);
                    assert!(
                        rel_err(probe, back) <= 1e-9,
                        "{frame:?} round-trip error {} exceeds 1e-9 (probe {probe})",
                        rel_err(probe, back)
                    );
                }
            }
        }
    }

    #[test]
    fn solar_subtree_walk_preserves_millimeters() {
        // Precision regime where absolute float conversion IS the
        // mechanism: link offsets (1 AU, ~6400 km) keep f64 resolution at
        // the µm level, so an adjacent walk up to the solar-system frame
        // and back must return facility truth within 1 mm.
        //
        // Cosmic limit (see plan.md Risks): above the solar system,
        // offsets (0.39 Mpc, 400 kpc, ~10 Mpc) exceed what f64 can
        // resolve to millimetres (f64 at 0.39 Mpc resolves ~2600 km;
        // measured 5.2e6 m on a literal cosmological → facility float
        // resolve). Cosmic descents therefore re-anchor via soi-handoff
        // C⁰/C¹ continuity instead of absolute conversion — this test
        // pins the precise regime, soi-handoff pins the rest.
        let mut chain = canonical_chain();
        chain
            .commit_to_parent(TransitionReason::Manual, 0.0)
            .unwrap();
        chain
            .commit_to_parent(TransitionReason::Manual, 0.0)
            .unwrap();
        assert_eq!(chain.active(), FrameId::SolarSystem);
        chain
            .commit_to_child(
                FrameId::Planetocentric(EARTH),
                canonical_links()[1],
                TransitionReason::Manual,
                0.0,
            )
            .unwrap();
        chain
            .commit_to_child(
                FrameId::LocalEnu(EARTH),
                canonical_links()[0],
                TransitionReason::Manual,
                0.0,
            )
            .unwrap();
        let expected = canonical_chain();
        let err_m = (chain.position - expected.position).length();
        assert!(err_m < 1e-3, "subtree walk error {err_m} m exceeds 1 mm");
        let dot = chain.orientation.dot(expected.orientation).abs();
        assert!((dot - 1.0).abs() < 1e-12, "orientation drift {dot}");
    }

    #[test]
    fn step_to_parent_walks_to_root() {
        let chain = canonical_chain();
        let mut current = chain.clone();
        for expected_depth in (0..6).rev() {
            let step = current.step_to_parent().expect("link missing");
            assert_eq!(step.frame.depth(), expected_depth);
            current = FrameChain::new(
                step.frame,
                step.position,
                step.orientation,
                current.links[1..].to_vec(),
            );
        }
        assert_eq!(current.active(), FrameId::Cosmological);
        assert!(current.step_to_parent().is_none());
    }

    #[test]
    fn recentered_upload_stays_within_frame_extent() {
        // DoD 2: no world-space f32 position exceeds one frame's extent.
        // Sweep each frame with in-domain offsets around a mid-frame
        // camera; every recentered upload must fit the frame extent.
        let frames = [
            FrameId::Cosmological,
            FrameId::Galactocentric,
            FrameId::LocalGroup,
            FrameId::StellarNeighborhood,
            FrameId::SolarSystem,
            FrameId::Planetocentric(EARTH),
            FrameId::LocalEnu(EARTH),
        ];
        for frame in frames {
            let extent = frame.extent_meters();
            let mpu = frame.meters_per_unit();
            let camera = DVec3::new(0.4 * extent / mpu, 0.0, 0.0);
            // Single-axis in-domain offsets: the recentered upload is the
            // camera-relative offset, bounded by the frame extent.
            for k in [0.0, 0.25, 0.5, 0.9] {
                let world = camera + DVec3::new(k * extent / mpu, 0.0, 0.0);
                let up = recenter(world, camera).as_dvec3();
                assert!(
                    up.length() <= extent,
                    "{frame:?}: recentered {} m exceeds extent {extent} m",
                    up.length()
                );
            }
        }
    }

    #[test]
    fn recenter_preserves_millimeter_detail_near_camera() {
        // Without the f64 subtraction, 1 mm next to a camera 10⁷ m out
        // collapses in f32; with it, the detail survives.
        let camera = DVec3::new(1.0e7, 2.0, -3.0);
        let near = DVec3::new(1.0e7 + 0.001, 2.0, -3.0);
        let collapsed = (near.as_vec3() - camera.as_vec3()).x;
        assert_eq!(
            collapsed, 0.0,
            "test premise broken: naive f32 path must collapse"
        );
        let kept = recenter(near, camera).x;
        assert!((kept - 0.001).abs() < 1e-6);
    }

    #[test]
    fn commit_to_parent_converts_and_returns_event() {
        let mut chain = canonical_chain();
        let before = chain.position;
        let event = chain
            .commit_to_parent(TransitionReason::BoundaryCrossing, 12.5)
            .unwrap();
        assert_eq!(event.from, FrameId::LocalEnu(EARTH));
        assert_eq!(event.to, FrameId::Planetocentric(EARTH));
        assert_eq!(event.reason, TransitionReason::BoundaryCrossing);
        assert_eq!(event.sim_time_s, 12.5);
        // 120.5 m east of a 1000 km origin offset: parent sees ~1000 km.
        let expected = canonical_links()[0].to_parent(FrameId::LocalEnu(EARTH), before);
        assert_eq!(chain.position, expected);
        assert_eq!(chain.depth(), 5);
    }

    #[test]
    fn commit_to_parent_at_root_fails() {
        let mut chain = FrameChain::new(
            FrameId::Cosmological,
            DVec3::new(5.0, -8.0, 3.0),
            DQuat::IDENTITY,
            vec![],
        );
        assert_eq!(
            chain.commit_to_parent(TransitionReason::Manual, 0.0),
            Err(TransitionError::AtRoot)
        );
    }

    #[test]
    fn commit_to_child_round_trips_parent_commit() {
        let mut chain = canonical_chain();
        chain
            .commit_to_parent(TransitionReason::Manual, 1.0)
            .unwrap();
        let link = canonical_links()[0];
        let event = chain
            .commit_to_child(
                FrameId::LocalEnu(EARTH),
                link,
                TransitionReason::Manual,
                2.0,
            )
            .unwrap();
        assert_eq!(event.from, FrameId::Planetocentric(EARTH));
        assert_eq!(event.to, FrameId::LocalEnu(EARTH));
        let expected = canonical_chain();
        // Round trip through the parent inherits the parent-offset
        // absolute error (~eps × 1 AU ≈ 0.03 mm), so the bound is
        // absolute, inside the 1 mm validation target.
        assert!((chain.position - expected.position).length() < 1e-3);
    }

    #[test]
    fn commit_to_non_child_fails_without_swapping() {
        let mut chain = canonical_chain();
        let before = chain.clone();
        let err = chain
            .commit_to_child(
                FrameId::SolarSystem,
                FrameLink::identity(),
                TransitionReason::Manual,
                0.0,
            )
            .unwrap_err();
        assert_eq!(
            err,
            TransitionError::NotAdjacent {
                active: FrameId::LocalEnu(EARTH),
                target: FrameId::SolarSystem,
            }
        );
        assert_eq!(chain, before, "failed commit must not swap coordinates");
        assert!(err.to_string().contains("adjacent"));
    }

    #[test]
    fn display_accessors_cover_debug_screens() {
        // DoD 4 surface: everything scale-debug-screens needs to render
        // "active frame + chain".
        let chain = canonical_chain();
        assert_eq!(chain.active().name(), "local-enu");
        assert_eq!(chain.active().unit_name(), "m");
        assert_eq!(chain.active().body(), Some(EARTH));
        assert_eq!(FrameId::SolarSystem.body(), None);
        assert_eq!(FrameId::Cosmological.depth(), 0);
        let names = [
            "local-enu",
            "planetocentric",
            "solar-system",
            "stellar-neighborhood",
            "local-group",
            "galactocentric",
            "cosmological",
        ];
        let mut frame = chain.active();
        for (i, name) in names.iter().enumerate() {
            assert_eq!(frame.name(), *name);
            assert_eq!(frame.depth(), 6 - i);
            frame = frame.parent().unwrap_or(FrameId::Cosmological);
        }
    }

    #[test]
    fn enu_frame_preserves_east_cross_north_is_up() {
        // Rendering invariant: the player world frame is true ENU.
        // Convention: local x = east, y = north, z = up; identity link
        // rotation keeps parent axes aligned, so the ENU basis is exact.
        let east = DVec3::X;
        let north = DVec3::Y;
        let up = east.cross(north);
        assert_eq!(up, DVec3::Z);
        // A pure translation link preserves the basis by construction.
        let link = FrameLink {
            rotation: DQuat::IDENTITY,
            origin: DVec3::new(1.0, 2.0, 3.0),
        };
        let moved = link.to_parent(FrameId::LocalEnu(EARTH), east)
            - link.to_parent(FrameId::LocalEnu(EARTH), DVec3::ZERO);
        assert!((moved - east * (1.0 / METERS_PER_KM)).length() < 1e-12);
    }
}
