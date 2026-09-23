//! Vista intro state machine (CVI-002/003): Hold → Dive → Done.
//!
//! Pure function of `(seed, t)`: the opening pose frames an interior
//! window of the web (v0.3.4 `cosmic-vista-reframe` FR1–FR3, folded
//! into `cosmic-sphere-clip` per PO decision 2026-09-23 — the bounded
//! descriptor broke the 180 Mpc outside pose with a 35°/s dive, and
//! the re-pose is required for gates green), then eases to the Chase
//! pose. No window, no GPU — wiring lives in the binary.

use game_engine::handoff::smoothstep;
use game_engine::universe::WebDescriptor;
use glam::DVec3;

/// Hold duration (player reads the composition), seconds.
pub const VISTA_HOLD_S: f64 = 2.0;
/// Dive duration (continuous ease to Chase), seconds. Grade 2026-09-22:
/// 6 s whipped 35°/s against the 9.5 Mpc chase baseline with eased
/// schedules throughout; 8 s lands ≈ 26°/s with margin (the bound is
/// hard, the duration is a tunable — notion Constraints).
pub const VISTA_DIVE_S: f64 = 8.0;
/// Skip fast-ease duration, seconds.
pub const VISTA_SKIP_S: f64 = 0.6;
/// Vista eye distance from the framed hub, Mpc.
pub const VISTA_DISTANCE_MPC: f64 = 180.0;
/// Vista field of view, degrees.
pub const VISTA_FOV_DEG: f32 = 25.0;
/// Vista slab thickness, Mpc (opening shot only; eases off in the dive).
pub const VISTA_SLAB_MPC: f32 = 40.0;
/// Marker minimum dot size during the vista, px.
pub const VISTA_MARKER_MIN_PX: f32 = 6.0;
/// Motion-sickness bound: peak look-direction angular velocity, deg/s.
pub const VISTA_MAX_ANGULAR_DEG_PER_S: f32 = 30.0;
/// Phase-clock epsilon: 480×(1/60 s) accumulations can land ~1 ulp
/// short of exactly 8.0 s; without this the headless FR7 continuity
/// pin flakes at the boundary. Microscopic next to per-tick deltas.
const VISTA_T_EPS: f64 = 1e-9;

/// One camera + depth-window pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VistaPose {
    /// Eye position, Mpc.
    pub eye: DVec3,
    /// Look-at target, Mpc.
    pub target: DVec3,
    /// Vertical field of view, degrees.
    pub fov_y_deg: f32,
    /// Slab half-thickness, Mpc.
    pub slab_half_mpc: f32,
    /// Slab center depth from the eye, Mpc.
    pub slab_center_mpc: f32,
    /// Inverse fog length (`1/fog_l`, `0.0` = fog off).
    pub inv_fog_l: f32,
}

impl VistaPose {
    /// Fog length in Mpc for the depth-window push terms (`0.0` =
    /// fog off when `inv_fog_l` is zero).
    pub fn fog_l_mpc(&self) -> f32 {
        if self.inv_fog_l <= 0.0 {
            0.0
        } else {
            1.0 / self.inv_fog_l
        }
    }
}

/// Vista phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VistaPhase {
    /// Holding the opening composition.
    Hold,
    /// Easing to Chase.
    Dive,
    /// Normal Chase (vista inactive).
    Done,
}

/// Vista state: phase + clock + endpoints.
#[derive(Clone, Copy, Debug)]
pub struct VistaState {
    /// Current phase.
    pub phase: VistaPhase,
    /// Seconds spent in the current phase.
    pub t: f64,
    /// Dive duration: full `VISTA_DIVE_S`, or `VISTA_SKIP_S` after skip.
    pub dive_len: f64,
    /// Dive start pose (opening composition, or skip snapshot).
    pub from: VistaPose,
    /// Dive end pose (Chase).
    pub to: VistaPose,
}

impl VistaState {
    /// Start the vista: Hold at `pose`, diving to `chase`.
    pub fn start(pose: VistaPose, chase: VistaPose) -> Self {
        Self {
            phase: VistaPhase::Hold,
            t: 0.0,
            dive_len: VISTA_DIVE_S,
            from: pose,
            to: chase,
        }
    }

    /// Advance the clock; Hold → Dive → Done. Non-positive `dt` ignored.
    pub fn tick(&mut self, dt: f64) {
        if dt <= 0.0 {
            return;
        }
        self.t += dt;
        match self.phase {
            VistaPhase::Hold => {
                if self.t + VISTA_T_EPS >= VISTA_HOLD_S {
                    self.phase = VistaPhase::Dive;
                    self.t = 0.0;
                }
            }
            VistaPhase::Dive => {
                if self.t + VISTA_T_EPS >= self.dive_len {
                    self.phase = VistaPhase::Done;
                    self.t = 0.0;
                }
            }
            VistaPhase::Done => {}
        }
    }

    /// Skip: snapshot the current pose and dive to Chase in `SKIP_S`.
    /// Continuity holds by construction (new `from` = current pose,
    /// clock restarted on a shortened dive). Idempotent: calling it
    /// again mid-skip (e.g. thrust still held across ticks) neither
    /// restarts the clock nor snaps the pose. Returns whether a skip
    /// started (the shell logs `skipped` only then).
    pub fn skip(&mut self) -> bool {
        match self.phase {
            VistaPhase::Done => false,
            VistaPhase::Hold => {
                self.from = self.pose();
                self.phase = VistaPhase::Dive;
                self.dive_len = VISTA_SKIP_S;
                self.t = 0.0;
                true
            }
            VistaPhase::Dive => {
                if self.dive_len != VISTA_SKIP_S {
                    self.from = self.pose();
                    self.phase = VistaPhase::Dive;
                    self.dive_len = VISTA_SKIP_S;
                    self.t = 0.0;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Replay from the opening pose.
    pub fn replay(&mut self, pose: VistaPose, chase: VistaPose) {
        *self = Self::start(pose, chase);
    }

    /// Current interpolated pose (Hold = `from`, Done = `to`).
    pub fn pose(&self) -> VistaPose {
        match self.phase {
            VistaPhase::Hold => self.from,
            VistaPhase::Done => self.to,
            VistaPhase::Dive => {
                let s = smoothstep01(self.t / self.dive_len);
                vista_interpolate(&self.from, &self.to, s)
            }
        }
    }
}

/// Smoothstep on `[0, 1]`: the `engine::handoff` precedent (zero edge
/// derivatives), re-exported here so the dive shares one easing with
/// the flight code.
pub fn smoothstep01(x: f64) -> f64 {
    smoothstep(x)
}

/// Dive docking distance, Mpc (grade 2026-09-22): the eye Bézier's
/// control point sits this far behind the Chase eye along the Chase
/// look direction, so the path docks ALONG the look axis — a true
/// dolly-in with ~zero end whip. (Shared-schedule and bow variants
/// whipped 103–146°/s at s ≈ 0.9 against the 9.5 Mpc chase baseline.)
pub const VISTA_DOCK_MPC: f64 = 120.0;

/// Eye-path control point for [`vista_interpolate`]: this far behind
/// the endpoint eye on the endpoint look axis (docking), or the
/// midpoint for a degenerate endpoint. Exposed so tests can bound the
/// path analytically instead of statistically.
pub fn vista_eye_control(from: &VistaPose, to: &VistaPose) -> DVec3 {
    let axis = to.target - to.eye;
    if axis.length_squared() < 1e-12 {
        from.eye.lerp(to.eye, 0.5)
    } else {
        to.eye - axis.normalize() * VISTA_DOCK_MPC
    }
}
/// Interpolate two poses (`s` already eased). The eye rides a
/// quadratic Bézier (endpoints exact, so Hold/Done continuity holds)
/// docked along the endpoint look axis; the look target lerps
/// hub→marker on a FRONT-LOADED schedule (done by `s` = 0.625), so the
/// look rotation happens while the eye is far and the late dive is a
/// dolly-in onto a fixed marker. FOV/slab/fog lerp linearly. Pure
/// function of the endpoints.
pub fn vista_interpolate(from: &VistaPose, to: &VistaPose, s: f64) -> VistaPose {
    let s = s.clamp(0.0, 1.0);
    let control = vista_eye_control(from, to);
    // Quadratic Bézier: (1−s)²·A + 2(1−s)s·C + s²·B.
    let eye = from.eye * (1.0 - s) * (1.0 - s) + control * (2.0 * (1.0 - s) * s) + to.eye * (s * s);
    // Target arrives early, eased (zero end-hitch): full hub→marker
    // travel in the first 62.5% while the baseline is huge; the late
    // dive is a dolly-in onto a fixed marker.
    let st = smoothstep01((s / 0.625).min(1.0));
    VistaPose {
        eye,
        target: from.target.lerp(to.target, st),
        fov_y_deg: from.fov_y_deg + (to.fov_y_deg - from.fov_y_deg) * s as f32,
        slab_half_mpc: from.slab_half_mpc + (to.slab_half_mpc - from.slab_half_mpc) * s as f32,
        slab_center_mpc: from.slab_center_mpc
            + (to.slab_center_mpc - from.slab_center_mpc) * s as f32,
        inv_fog_l: from.inv_fog_l + (to.inv_fog_l - from.inv_fog_l) * s as f32,
    }
}

/// Hint-text alpha for the `press any key` affordance (FR5): fades
/// in over the second hold second, gone once the dive starts or on
/// skip. Pure so the shell's UI call and the tests share it.
pub fn vista_hint_alpha(phase: VistaPhase, t_seconds: f64) -> f32 {
    if phase != VistaPhase::Hold {
        return 0.0;
    }
    (t_seconds - 1.0).clamp(0.0, 1.0) as f32
}

/// Tier-A cut: rank < ceil(1% of n). Must match
/// `cosmic_hubs::HubTier::of` (unified in the wiring commit).
fn is_tier_a(rank: usize, n: usize) -> bool {
    rank < n.div_ceil(100).max(1).min(n.max(1))
}

/// Interior-window footprint width, Mpc (FR1 starting value).
pub const VISTA_WINDOW_W_MPC: f64 = 300.0;
/// Interior-window footprint height, Mpc (FR1 starting value, 16:9).
pub const VISTA_WINDOW_H_MPC: f64 = 170.0;
/// Rim safety margin inside the sphere cross-section, Mpc (FR1).
pub const VISTA_WINDOW_MARGIN_MPC: f64 = 15.0;
/// Composition weight: bright vs central (FR2 starting value).
pub const VISTA_HUB_LAMBDA: f64 = 0.5;
/// Slab-centre bound as a fraction of R (FR3).
pub const VISTA_SLAB_CENTER_FRAC: f64 = 0.4;
/// Focal position of the hub in the frame, width fraction (FR3).
pub const VISTA_FOCAL_X: f64 = 0.55;

/// Interior-window fit (FR1): with slab thickness `t` and a `w × h`
/// footprint at distance `|c|` from the sphere centre, the footprint's
/// half-diagonal plus `t/2` must stay inside the sphere cross-section
/// at `c` with `margin` to spare. Returns the eye distance `d` at the
/// 25° vista FOV (`d = (h/2)/tan(12.5°)`), or `None` when the
/// footprint cannot fit. Pure arithmetic, deterministic.
pub fn interior_window(r: f64, c: f64, w: f64, h: f64, t: f64, margin: f64) -> Option<(f64, f32)> {
    if r <= 0.0 || !r.is_finite() || c.abs() >= r {
        return None;
    }
    let cross = (r * r - c * c).sqrt();
    let need = ((w * 0.5).powi(2) + (h * 0.5).powi(2)).sqrt() + t * 0.5;
    if need <= cross - margin {
        let d = (h * 0.5) / (VISTA_FOV_DEG as f64 * 0.5).to_radians().tan();
        Some((d, VISTA_FOV_DEG))
    } else {
        None
    }
}

/// Hub choice by composition (FR2): the Tier-A node maximizing
/// `rank_score − λ·|h|/R` among hubs whose focal footprint fits the
/// interior window (`rank_score = 1 − rank/(n−1)`, 1 for the most
/// massive). Returns the node index, or `None` when no Tier-A hub
/// fits (the caller falls back to the v0.3.3 legacy chain).
/// Deterministic per descriptor (rank order + exact comparisons).
///
/// NOTE: superseded for the opening pose by the dive-exact shortlist
/// in [`vista_pose`] (PO-directed 2026-09-23) — kept as the documented
/// composition-score definition the shortlist ranks by.
pub fn vista_hub_composition_score(rank: usize, n: usize, hub_r_mpc: f64, radius_mpc: f64) -> f64 {
    let rank_score = 1.0 - rank as f64 / (n - 1).max(1) as f64;
    rank_score - VISTA_HUB_LAMBDA * hub_r_mpc / radius_mpc
}

/// Focal footprint centre for a hub (FR3): the look target such that
/// the hub sits at [`VISTA_FOCAL_X`] of the frame width. The look axis
/// is radial (slab plane ⊥ radius); screen-right for a Y-up camera
/// looking along −n̂ is (−n̂)×Y = −(n̂×Y), so the target sits +15 Mpc
/// along `right = n̂×Y` to frame the hub right of centre.
fn focal_footprint_center(hub: DVec3) -> DVec3 {
    let n_hat = hub.try_normalize().unwrap_or(DVec3::Z);
    let mut right = n_hat.cross(DVec3::Y);
    if right.length_squared() < 1e-12 {
        right = n_hat.cross(DVec3::X);
    }
    let right = right.normalize();
    hub + right * ((VISTA_FOCAL_X - 0.5) * VISTA_WINDOW_W_MPC)
}

/// Dive peak for a candidate opening pose against the Chase endpoint:
/// the UX-2 bound measured exactly the way the audit test measures
/// it (60 Hz sampling over [`VISTA_DIVE_S`]). Pure arithmetic over the
/// two endpoint poses — the selection below evaluates it per Tier-A
/// candidate (tens of candidates × 480 steps of unit math: trivial
/// next to a 128³ generation).
fn dive_peak_deg_per_s(from: &VistaPose, to: &VistaPose) -> f32 {
    let dt = 1.0 / 60.0;
    let mut peak = 0.0f32;
    let mut prev_dir = from.target - from.eye;
    for k in 1..=(VISTA_DIVE_S / dt) as usize {
        let p = vista_interpolate(from, to, smoothstep01(k as f64 * dt / VISTA_DIVE_S));
        let dir = p.target - p.eye;
        peak = peak.max(look_angular_velocity_deg_per_s(prev_dir, dir, dt));
        prev_dir = dir;
    }
    peak
}

/// Composition opening pose for one hub (FR3): focal footprint
/// centre as the look target, eye on the radial axis at the
/// interior-window distance, slab plane through the bounded slab
/// centre.
fn composition_pose_for_hub(hub: DVec3, radius_mpc: f64) -> VistaPose {
    let h_len = hub.length();
    let c_vec = if h_len > 1e-6 {
        hub * (VISTA_SLAB_CENTER_FRAC * radius_mpc / h_len).min(1.0)
    } else {
        DVec3::ZERO
    };
    let f = focal_footprint_center(hub);
    let (d, fov) = interior_window(
        radius_mpc,
        f.length(),
        VISTA_WINDOW_W_MPC,
        VISTA_WINDOW_H_MPC,
        f64::from(VISTA_SLAB_MPC),
        VISTA_WINDOW_MARGIN_MPC,
    )
    .expect("hub passed the feasibility filter");
    let n_hat = hub.try_normalize().unwrap_or(DVec3::Z);
    let eye = f + n_hat * d;
    // Slab plane through the slab centre: depth measured from the eye
    // along the look axis.
    let slab_center = (eye - c_vec).dot(n_hat).max(1.0) as f32;
    VistaPose {
        eye,
        target: f,
        fov_y_deg: fov,
        slab_half_mpc: VISTA_SLAB_MPC * 0.5,
        slab_center_mpc: slab_center,
        inv_fog_l: 0.0,
    }
}

/// Opening pose: interior window on the composition hub (FR3), with
/// the v0.3.3 legacy chain as the degenerate fallback.
///
/// Hub choice (FR2, PO-directed 2026-09-23): among Tier-A hubs whose
/// focal footprint fits the window, the dive against `chase` is
/// scored exactly; hubs within the [`VISTA_MAX_ANGULAR_DEG_PER_S`]
/// bound compete on composition (`rank_score − λ·|h|/R`), ties go to
/// the lowest index (mass-ranked, so the brightest). When no fitting
/// hub stays within the bound, the gentlest dive wins outright. The
/// pose is still a pure function of seed-derived inputs (`web` is
/// seed-derived; `chase` derives from the seed-derived spawn).
pub fn vista_pose(web: &WebDescriptor, radius_mpc: f64, chase: &VistaPose) -> VistaPose {
    let n = web.nodes.len();
    if n == 0 {
        return VistaPose {
            eye: DVec3::new(0.0, 0.0, VISTA_DISTANCE_MPC),
            target: DVec3::ZERO,
            fov_y_deg: VISTA_FOV_DEG,
            slab_half_mpc: VISTA_SLAB_MPC * 0.5,
            slab_center_mpc: VISTA_DISTANCE_MPC as f32,
            inv_fog_l: 0.0,
        };
    }
    // Feasible Tier-A hubs with composition scores.
    let mut feasible: Vec<(u32, DVec3, f64)> = Vec::new();
    for (rank, node) in web.nodes.iter().enumerate() {
        if !is_tier_a(rank, n) {
            continue;
        }
        let h = DVec3::new(
            node.position_mpc[0],
            node.position_mpc[1],
            node.position_mpc[2],
        );
        if vista_hub_fits(h, radius_mpc) {
            let comp = vista_hub_composition_score(rank, n, h.length(), radius_mpc);
            feasible.push((node.node_index, h, comp));
        }
    }
    if !feasible.is_empty() {
        // Dive-exact shortlist: within the bound, composition decides;
        // otherwise the gentlest dive wins (best effort, still
        // deterministic). Ties → lowest index (brightest first).
        let picked = pick_dive_safe_hub(&feasible, radius_mpc, chase);
        return composition_pose_for_hub(picked, radius_mpc);
    }
    // (2–4) Legacy chain (v0.3.3, kept for degenerate seeds): nearest
    // Tier A to home → most massive within 150 Mpc of home → most
    // massive overall, viewed from 180 Mpc with home ~6° off-axis.
    legacy_vista_pose(web)
}

/// Picked hub position for an opening pose (test seam): the hub the
/// dive-exact shortlist in [`vista_pose`] selects, or `None` when the
/// legacy fallback owns the pose.
pub fn vista_pose_hub(web: &WebDescriptor, radius_mpc: f64, chase: &VistaPose) -> Option<DVec3> {
    let n = web.nodes.len();
    if n == 0 {
        return None;
    }
    let mut feasible: Vec<(u32, DVec3, f64)> = Vec::new();
    for (rank, node) in web.nodes.iter().enumerate() {
        if !is_tier_a(rank, n) {
            continue;
        }
        let h = DVec3::new(
            node.position_mpc[0],
            node.position_mpc[1],
            node.position_mpc[2],
        );
        if vista_hub_fits(h, radius_mpc) {
            let comp = vista_hub_composition_score(rank, n, h.length(), radius_mpc);
            feasible.push((node.node_index, h, comp));
        }
    }
    if feasible.is_empty() {
        return None;
    }
    Some(pick_dive_safe_hub(&feasible, radius_mpc, chase))
}

/// Dive-exact shortlist over feasible `(index, hub, composition)`
/// candidates: within the angular-velocity bound, composition decides;
/// otherwise the gentlest dive wins. Ties → lowest index.
fn pick_dive_safe_hub(feasible: &[(u32, DVec3, f64)], radius_mpc: f64, chase: &VistaPose) -> DVec3 {
    let mut best_in_bound: Option<(f64, u32, DVec3)> = None;
    let mut best_effort: Option<(f32, u32, DVec3)> = None;
    for (index, h, comp) in feasible {
        let pose = composition_pose_for_hub(*h, radius_mpc);
        let peak = dive_peak_deg_per_s(&pose, chase);
        if peak <= VISTA_MAX_ANGULAR_DEG_PER_S {
            let replace = match best_in_bound {
                None => true,
                Some((b, bi, _)) => *comp > b || (*comp == b && *index < bi),
            };
            if replace {
                best_in_bound = Some((*comp, *index, *h));
            }
        }
        let replace = match best_effort {
            None => true,
            Some((b, bi, _)) => peak < b || (peak == b && *index < bi),
        };
        if replace {
            best_effort = Some((peak, *index, *h));
        }
    }
    best_in_bound
        .map(|(_, _, h)| h)
        .or_else(|| best_effort.map(|(_, _, h)| h))
        .expect("feasible is non-empty")
}

/// Focal-footprint feasibility for one hub: the interior window fits
/// at the hub's focal footprint centre.
fn vista_hub_fits(hub: DVec3, radius_mpc: f64) -> bool {
    let f = focal_footprint_center(hub);
    interior_window(
        radius_mpc,
        f.length(),
        VISTA_WINDOW_W_MPC,
        VISTA_WINDOW_H_MPC,
        f64::from(VISTA_SLAB_MPC),
        VISTA_WINDOW_MARGIN_MPC,
    )
    .is_some()
}

/// Legacy opening pose (v0.3.3 `cosmic-vista-intro`): nearest Tier-A
/// node to home, viewed from 180 Mpc with home ~6° off-axis. Kept as
/// the degenerate-seed fallback for the composition pose above.
fn legacy_vista_pose(web: &WebDescriptor) -> VistaPose {
    let n = web.nodes.len();
    let h = web.home();
    let home_pos = DVec3::new(h.position_mpc[0], h.position_mpc[1], h.position_mpc[2]);
    let dist2 = |p: [f64; 3]| {
        let dx = p[0] - home_pos.x;
        let dy = p[1] - home_pos.y;
        let dz = p[2] - home_pos.z;
        dx * dx + dy * dy + dz * dz
    };
    // (1) Nearest Tier A to home.
    let mut hub: Option<[f64; 3]> = None;
    let mut hub_d2 = f64::INFINITY;
    for (rank, node) in web.nodes.iter().enumerate() {
        if !is_tier_a(rank, n) {
            continue;
        }
        let d2 = dist2(node.position_mpc);
        if d2 < hub_d2 {
            hub_d2 = d2;
            hub = Some(node.position_mpc);
        }
    }
    // (2) Most massive within 150 Mpc of home.
    if hub.is_none() {
        let mut best_mass = 0.0;
        for node in &web.nodes {
            if dist2(node.position_mpc) <= 150.0 * 150.0 && node.mass_msun > best_mass {
                best_mass = node.mass_msun;
                hub = Some(node.position_mpc);
            }
        }
    }
    // (3) Most massive overall (nodes are mass-ranked: index 0).
    let hub = hub.unwrap_or(web.nodes[0].position_mpc);
    let hub_v = DVec3::new(hub[0], hub[1], hub[2]);
    // View direction: far side of the hub from home, with home a
    // controlled ~6° off-axis (horizontal — the wide frame has room).
    // Grade history (2026-09-22): the largest-axis perpendicular put
    // home 19° off-axis on the nominal seed (outside the 25° frame);
    // exact far-side alignment centered home but sent the dive eye
    // straight past the hub, whipping the look direction at 560°/s.
    // The 6° offset keeps home in frame AND the dive gentle (both
    // pinned on the nominal seed below).
    let off = hub_v - home_pos;
    let dist = off.length().max(1e-6);
    let f = off / dist;
    // Stable horizontal perpendicular to the home→hub line.
    let mut u = f.cross(DVec3::Y);
    if u.length_squared() < 1e-12 {
        u = f.cross(DVec3::X);
    }
    let u = u.normalize();
    // Offset gain for exactly ~6°: the home angle from the view axis
    // is ≈ k·180/(D+180) for small k (D = home–hub distance). Grade:
    // exact far-side alignment centered home but whipped the dive
    // (560°/s past the hub); a 10° try traded the edge sibling for
    // corner glare — 6° it is (dive 26.5°/s, home in frame).
    let k = (6.0_f64.to_radians().tan() * (dist + VISTA_DISTANCE_MPC) / 180.0).min(1.0);
    let d = (f + u * k).normalize();
    VistaPose {
        eye: hub_v + d * VISTA_DISTANCE_MPC,
        target: hub_v,
        fov_y_deg: VISTA_FOV_DEG,
        slab_half_mpc: VISTA_SLAB_MPC * 0.5,
        slab_center_mpc: VISTA_DISTANCE_MPC as f32,
        inv_fog_l: 0.0,
    }
}

/// Look-direction angular velocity between two unit dirs, deg/s.
pub fn look_angular_velocity_deg_per_s(prev_dir: DVec3, next_dir: DVec3, dt: f64) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    let a = prev_dir.try_normalize().unwrap_or(DVec3::Z);
    let b = next_dir.try_normalize().unwrap_or(DVec3::Z);
    let cos = a.dot(b).clamp(-1.0, 1.0);
    (cos.acos().to_degrees() / dt) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::universe::{WebLink, WebNode};

    fn node(i: u32, pos: [f64; 3], mass: f64) -> WebNode {
        WebNode {
            node_index: i,
            position_mpc: pos,
            mass_msun: mass,
            virial_radius_mpc: 1.0,
        }
    }

    fn web_of(nodes: Vec<WebNode>, home: u32) -> WebDescriptor {
        WebDescriptor::new(7, nodes, Vec::<WebLink>::new(), Vec::new(), home, 0.5)
    }

    fn pose(eye: [f64; 3], target: [f64; 3]) -> VistaPose {
        VistaPose {
            eye: DVec3::from(eye),
            target: DVec3::from(target),
            fov_y_deg: 25.0,
            slab_half_mpc: 20.0,
            slab_center_mpc: 180.0,
            inv_fog_l: 0.0,
        }
    }

    fn chase() -> VistaPose {
        // Looks −z like the vista opening shot (a real dive never flips
        // hemispheres; an opposing synthetic chase would fake a 180° turn).
        VistaPose {
            eye: DVec3::new(30.0, 8.0, 38.0),
            target: DVec3::new(30.0, 8.0, 30.0),
            fov_y_deg: 60.0,
            slab_half_mpc: 0.0,
            slab_center_mpc: 0.0,
            inv_fog_l: 1.0 / 90.0,
        }
    }

    #[test]
    fn tier_a_cut_matches_hub_tiering() {
        // ceil(1%) semantics: n=6000 → ranks 0..60 are Tier A.
        assert!(is_tier_a(0, 6000));
        assert!(is_tier_a(59, 6000));
        assert!(!is_tier_a(60, 6000));
        assert!(is_tier_a(0, 1));
        assert!(is_tier_a(1, 200)); // ceil(1% of 200) = 2 ranks
        assert!(!is_tier_a(2, 200));
    }

    #[test]
    fn empty_web_falls_back_to_default_axis() {
        let web = web_of(Vec::new(), 0);
        let p = vista_pose(&web, 250.0, &chase());
        assert_eq!(p.target, DVec3::ZERO);
        assert!((p.eye - DVec3::new(0.0, 0.0, VISTA_DISTANCE_MPC)).length() < 1e-9);
    }

    #[test]
    fn interior_window_fits_nominal_and_rejects_rim() {
        // FR1: 300×170 + 40-slab fits at the centre with margin;
        // the rim cross-section rejects it (None).
        let (d, fov) =
            interior_window(250.0, 0.0, 300.0, 170.0, 40.0, 15.0).expect("nominal centre must fit");
        assert_eq!(fov, VISTA_FOV_DEG);
        assert!((d - 383.4).abs() < 0.5, "eye distance {d}");
        assert!(interior_window(250.0, 240.0, 300.0, 170.0, 40.0, 15.0).is_none());
        assert!(interior_window(250.0, 250.0, 300.0, 170.0, 40.0, 15.0).is_none());
        // A smaller 16:9 footprint fits deeper (the FR1 fallback).
        assert!(interior_window(250.0, 100.0, 150.0, 85.0, 40.0, 15.0).is_some());
    }

    #[test]
    fn composition_pose_targets_a_fitting_tier_a_hub() {
        // 200 nodes: ranks 0,1 are Tier A at (0,0,0) and (100,0,0).
        // Home = index 150. The pose must target the focal footprint
        // centre of a fitting Tier-A hub at the window distance —
        // never the legacy 180 Mpc outside framing.
        let mut nodes: Vec<WebNode> = (0..200)
            .map(|i| node(i, [i as f64, 0.0, 0.0], 1.0e13))
            .collect();
        nodes[0].position_mpc = [0.0, 0.0, 0.0];
        nodes[1].position_mpc = [100.0, 0.0, 0.0];
        nodes[150].position_mpc = [90.0, 10.0, 0.0];
        let web = web_of(nodes, 150);
        let to = chase();
        let p = vista_pose(&web, 250.0, &to);
        assert_eq!(p.fov_y_deg, VISTA_FOV_DEG);
        assert_eq!(p.slab_half_mpc, 20.0);
        // Target sits exactly one focal offset (15 Mpc) from a Tier-A
        // node, and the eye rides the window distance for |target|.
        let near_tier_a = web.nodes.iter().enumerate().any(|(rank, nd)| {
            is_tier_a(rank, 200)
                && (DVec3::from(nd.position_mpc) - p.target).length() <= 15.0 + 1e-6
        });
        assert!(near_tier_a, "target must focal-frame a Tier-A hub");
        let (d, _) = interior_window(250.0, p.target.length(), 300.0, 170.0, 40.0, 15.0)
            .expect("pose target must fit the window");
        assert!((p.eye.distance(p.target) - d).abs() < 1e-6);
        // Deterministic pick.
        let q = vista_pose(&web, 250.0, &to);
        assert_eq!(p, q);
        // The hub seam agrees with the pose target.
        let hub = vista_pose_hub(&web, 250.0, &to).expect("a hub must be picked");
        assert!((hub - p.target).length() <= 15.0 + 1e-6);
    }

    #[test]
    fn interpolate_endpoints_and_monotone_eye() {
        let from = pose([0.0, 0.0, 180.0], [0.0, 0.0, 0.0]);
        let to = chase();
        assert_eq!(vista_interpolate(&from, &to, 0.0), from);
        assert_eq!(vista_interpolate(&from, &to, 1.0), to);
        let mut prev = f64::INFINITY;
        for k in 0..=20 {
            let p = vista_interpolate(&from, &to, k as f64 / 20.0);
            let d = p.eye.distance(to.eye);
            assert!(d <= prev + 1e-9, "eye path not monotone at {k}");
            prev = d;
        }
    }

    #[test]
    fn tick_walks_hold_dive_done() {
        let mut s = VistaState::start(pose([0.0, 0.0, 180.0], [0.0, 0.0, 0.0]), chase());
        assert_eq!(s.phase, VistaPhase::Hold);
        s.tick(VISTA_HOLD_S - 0.01);
        assert_eq!(s.phase, VistaPhase::Hold);
        s.tick(0.02);
        assert_eq!(s.phase, VistaPhase::Dive);
        s.tick(VISTA_DIVE_S - 0.01);
        assert_eq!(s.phase, VistaPhase::Dive);
        s.tick(0.02);
        assert_eq!(s.phase, VistaPhase::Done);
        assert_eq!(s.pose(), chase());
        s.tick(-1.0); // ignored
        assert_eq!(s.phase, VistaPhase::Done);
    }

    #[test]
    fn skip_is_continuous_and_fast() {
        let mut s = VistaState::start(pose([0.0, 0.0, 180.0], [0.0, 0.0, 0.0]), chase());
        s.tick(1.0); // mid-hold
        let before = s.pose();
        s.skip();
        assert_eq!(s.phase, VistaPhase::Dive);
        assert_eq!(s.pose(), before, "skip must not jump");
        // Remaining time is the fast ease.
        let mut elapsed = 0.0;
        while s.phase != VistaPhase::Done && elapsed < 5.0 {
            s.tick(1.0 / 60.0);
            elapsed += 1.0 / 60.0;
        }
        assert!(elapsed <= VISTA_SKIP_S + 1.0 / 60.0 + 1e-9);
        assert_eq!(s.phase, VistaPhase::Done);
    }

    #[test]
    fn per_tick_eye_delta_bounded_and_slow_turn() {
        let from = pose([0.0, 0.0, 180.0], [0.0, 0.0, 0.0]);
        let to = chase();
        let dt = 1.0 / 60.0;
        // Analytic bound (NFR1): the Bézier eye velocity is linear in
        // `s`, so its maximum is an endpoint tangent; times the eased
        // schedule's 1.5× peak rate over the dive. Exact, not
        // statistical — measured steps must fit under it.
        let control = vista_eye_control(&from, &to);
        let t0 = (control - from.eye).length() * 2.0;
        let t1 = (to.eye - control).length() * 2.0;
        let bound = t0.max(t1) * (1.5 / VISTA_DIVE_S) * dt * (1.0 + 1e-9);
        let mut prev = from.eye;
        let mut prev_dir = from.target - from.eye;
        let mut peak = 0.0f32;
        for k in 1..=(VISTA_DIVE_S / dt) as usize {
            let p = vista_interpolate(&from, &to, smoothstep01(k as f64 * dt / VISTA_DIVE_S));
            let step = p.eye.distance(prev);
            assert!(
                step <= bound,
                "eye jump {step} over bound {bound} at tick {k}"
            );
            let dir = p.target - p.eye;
            peak = peak.max(look_angular_velocity_deg_per_s(prev_dir, dir, dt));
            prev = p.eye;
            prev_dir = dir;
        }
        assert!(
            peak <= VISTA_MAX_ANGULAR_DEG_PER_S,
            "turn too fast: {peak} deg/s"
        );
    }

    #[test]
    fn nominal_pose_frames_tier_a_with_home_in_frame() {
        // Composition headline contract on the nominal seed: the
        // opening pose frames a Tier-A hub at the focal position
        // ((0.55 ± 0.05, 0.5 ± 0.05) of the frame — DoD 2) with home
        // inside the 25° frame at capture aspect (home must stay
        // findable). Single demo build: the web, chase, and pose all
        // come from one deterministic boot.
        use super::super::cosmic_demo::CosmicDemoState;
        use glam::camera::rh::proj::directx::perspective;
        use glam::camera::rh::view::look_at_mat4;
        use glam::{Mat4, Vec3, Vec4};
        let demo = CosmicDemoState::new(1337);
        let web = &demo.web;
        let chase = demo.chase_pose();
        let pose = vista_pose(web, demo.params.descriptor_radius_mpc, &chase);
        let n = web.nodes.len();
        let hub = vista_pose_hub(web, demo.params.descriptor_radius_mpc, &chase)
            .expect("a hub must be picked on the nominal seed");
        let hub_idx = web
            .nodes
            .iter()
            .position(|nd| {
                DVec3::new(nd.position_mpc[0], nd.position_mpc[1], nd.position_mpc[2]) == hub
            })
            .expect("picked hub must be a web node");
        eprintln!("nominal vista hub: rank {hub_idx}/{n}");
        assert!(
            is_tier_a(hub_idx, n),
            "vista hub rank {hub_idx} is not Tier A"
        );
        // Project hub + home through the real vista camera (25°).
        let eye = Vec3::new(pose.eye.x as f32, pose.eye.y as f32, pose.eye.z as f32);
        let target = Vec3::new(
            pose.target.x as f32,
            pose.target.y as f32,
            pose.target.z as f32,
        );
        let view: Mat4 = look_at_mat4(eye, target, Vec3::Y);
        let proj: Mat4 = perspective(VISTA_FOV_DEG.to_radians(), 1408.0 / 768.0, 1.0, 2000.0);
        let ndc_of = |p: DVec3| {
            let clip = proj * view * Vec4::new(p.x as f32, p.y as f32, p.z as f32, 1.0);
            assert!(clip.w > 0.0, "point behind the vista camera");
            Vec3::new(clip.x, clip.y, clip.z) / clip.w
        };
        let hub_ndc = ndc_of(hub);
        eprintln!("nominal hub NDC: ({:.3}, {:.3})", hub_ndc.x, hub_ndc.y);
        assert!(
            (0.0..=0.2).contains(&hub_ndc.x) && (-0.1..=0.1).contains(&hub_ndc.y),
            "hub off the focal mark: ({:.3}, {:.3})",
            hub_ndc.x,
            hub_ndc.y
        );
        let h = web.home();
        let home_ndc = ndc_of(DVec3::new(
            h.position_mpc[0],
            h.position_mpc[1],
            h.position_mpc[2],
        ));
        eprintln!("nominal home NDC: ({:.3}, {:.3})", home_ndc.x, home_ndc.y);
        assert!(
            home_ndc.x.abs() <= 1.0 && home_ndc.y.abs() <= 1.0,
            "home outside the vista frame: ({:.3}, {:.3})",
            home_ndc.x,
            home_ndc.y
        );
    }

    #[test]
    fn hint_alpha_tracks_the_hold_clock() {
        // FR5: `press any key` is invisible in the first hold second,
        // fades in over the second, and never shows outside Hold.
        assert_eq!(vista_hint_alpha(VistaPhase::Hold, 0.0), 0.0);
        assert_eq!(vista_hint_alpha(VistaPhase::Hold, 0.5), 0.0);
        assert_eq!(vista_hint_alpha(VistaPhase::Hold, 1.0), 0.0);
        assert!((vista_hint_alpha(VistaPhase::Hold, 1.5) - 0.5).abs() < 1e-6);
        assert_eq!(vista_hint_alpha(VistaPhase::Hold, 2.0), 1.0);
        assert_eq!(vista_hint_alpha(VistaPhase::Hold, 99.0), 1.0);
        assert_eq!(vista_hint_alpha(VistaPhase::Dive, 0.0), 0.0);
        assert_eq!(vista_hint_alpha(VistaPhase::Done, 0.0), 0.0);
    }

    #[test]
    fn nominal_marker_projects_through_hold_and_early_dive() {
        // DoD 4 / UX-4: home (the marker anchor) projects inside the
        // frame at t = 0, 1.5 (hint showing), and 4 s (early dive) on
        // the nominal seed — the shared 6 px dot path then draws it.
        // (The UI dot itself is windowed-chrome; the capture harness
        // is 3D-only, so projection + the shared draw path is the
        // evidence, pinned here.)
        use super::super::cosmic_demo::CosmicDemoState;
        use glam::camera::rh::proj::directx::perspective;
        use glam::camera::rh::view::look_at_mat4;
        use glam::{Mat4, Vec3, Vec4};
        let demo = CosmicDemoState::new(1337);
        let web = &demo.web;
        let chase = demo.chase_pose();
        let from = vista_pose(web, demo.params.descriptor_radius_mpc, &chase);
        let h = web.home();
        let home = Vec4::new(
            h.position_mpc[0] as f32,
            h.position_mpc[1] as f32,
            h.position_mpc[2] as f32,
            1.0,
        );
        // Endpoints only set the dive target schedule; the eye/target
        // below come straight from the clock (Hold) or interpolation.
        let to = VistaPose {
            eye: from.eye,
            target: from.target,
            fov_y_deg: 60.0,
            slab_half_mpc: 0.0,
            slab_center_mpc: 0.0,
            inv_fog_l: 1.0 / 90.0,
        };
        for t in [0.0, 1.5, 4.0] {
            let p = if t < VISTA_HOLD_S {
                from
            } else {
                vista_interpolate(&from, &to, smoothstep01((t - VISTA_HOLD_S) / VISTA_DIVE_S))
            };
            let eye = Vec3::new(p.eye.x as f32, p.eye.y as f32, p.eye.z as f32);
            let tgt = Vec3::new(p.target.x as f32, p.target.y as f32, p.target.z as f32);
            let vp: Mat4 = perspective(p.fov_y_deg.to_radians(), 1408.0 / 768.0, 0.1, 5000.0)
                * look_at_mat4(eye, tgt, Vec3::Y);
            let clip = vp * home;
            assert!(clip.w > 0.0, "home behind camera at t={t}");
            let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
            assert!(
                ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0,
                "marker out of frame at t={t}: ({:.3}, {:.3})",
                ndc.x,
                ndc.y
            );
        }
    }
}
