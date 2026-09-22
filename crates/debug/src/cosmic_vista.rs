//! Vista intro state machine (CVI-002/003): Hold → Dive → Done.
//!
//! Pure function of `(seed, t)`: the opening pose frames the nearest
//! Tier-A hub from outside, then eases to the Chase pose. No window,
//! no GPU — wiring lives in the binary.

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

/// Opening pose: nearest Tier-A node to home, viewed from 180 Mpc with
/// home in frame. Fallback chain: nearest Tier A → most massive within
/// 150 Mpc of home → most massive overall → default axis.
pub fn vista_pose(web: &WebDescriptor) -> VistaPose {
    let n = web.nodes.len();
    let home_pos = if n == 0 {
        return VistaPose {
            eye: DVec3::new(0.0, 0.0, VISTA_DISTANCE_MPC),
            target: DVec3::ZERO,
            fov_y_deg: VISTA_FOV_DEG,
            slab_half_mpc: VISTA_SLAB_MPC * 0.5,
            slab_center_mpc: VISTA_DISTANCE_MPC as f32,
            inv_fog_l: 0.0,
        };
    } else {
        let h = web.home();
        DVec3::new(h.position_mpc[0], h.position_mpc[1], h.position_mpc[2])
    };
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
    use game_engine::universe::{WebLink, WebNode, generate_cosmic_web};

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
        let p = vista_pose(&web);
        assert_eq!(p.target, DVec3::ZERO);
        assert!((p.eye - DVec3::new(0.0, 0.0, 180.0)).length() < 1e-9);
    }

    #[test]
    fn pose_frames_nearest_tier_a_with_home_in_frame() {
        // 200 nodes: ranks 0,1 are Tier A. Home = index 150.
        let mut nodes: Vec<WebNode> = (0..200)
            .map(|i| node(i, [i as f64, 0.0, 0.0], 1.0e13))
            .collect();
        nodes[0].position_mpc = [0.0, 0.0, 0.0];
        nodes[1].position_mpc = [100.0, 0.0, 0.0];
        nodes[150].position_mpc = [90.0, 10.0, 0.0];
        let web = web_of(nodes, 150);
        let p = vista_pose(&web);
        // Nearest Tier A to home (90,10,0) is rank 1 at (100,0,0).
        assert!((p.target - DVec3::new(100.0, 0.0, 0.0)).length() < 1e-9);
        assert!((p.eye.distance(p.target) - VISTA_DISTANCE_MPC).abs() < 1e-6);
        assert_eq!(p.fov_y_deg, VISTA_FOV_DEG);
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
        // CVI-002/DoD 1: on the nominal seed the opening pose targets a
        // Tier-A hub with home inside the 25° frame at capture aspect
        // (the headline-shot contract — home must be findable, the hub
        // dominant). "In frame" = inside NDC (wide aspect buys
        // horizontal room; a raw off-axis angle would be too strict).
        use game_engine::universe::CosmicWebParams;
        use glam::camera::rh::proj::directx::perspective;
        use glam::camera::rh::view::look_at_mat4;
        use glam::{Mat4, Vec3, Vec4};
        let web = generate_cosmic_web(1337, &CosmicWebParams::nominal());
        let pose = vista_pose(&web);
        let n = web.nodes.len();
        let hub_idx = web
            .nodes
            .iter()
            .position(|nd| {
                DVec3::new(nd.position_mpc[0], nd.position_mpc[1], nd.position_mpc[2])
                    == pose.target
            })
            .expect("pose must target a web node");
        eprintln!("nominal vista hub: rank {hub_idx}/{n}");
        assert!(
            is_tier_a(hub_idx, n),
            "vista hub rank {hub_idx} is not Tier A"
        );
        // Project home through the real vista camera (25°, 1408×768).
        let eye = Vec3::new(pose.eye.x as f32, pose.eye.y as f32, pose.eye.z as f32);
        let target = Vec3::new(
            pose.target.x as f32,
            pose.target.y as f32,
            pose.target.z as f32,
        );
        let view: Mat4 = look_at_mat4(eye, target, Vec3::Y);
        let proj: Mat4 = perspective(VISTA_FOV_DEG.to_radians(), 1408.0 / 768.0, 1.0, 2000.0);
        let h = web.home();
        let home = Vec4::new(
            h.position_mpc[0] as f32,
            h.position_mpc[1] as f32,
            h.position_mpc[2] as f32,
            1.0,
        );
        let clip = proj * view * home;
        assert!(clip.w > 0.0, "home behind the vista camera");
        let ndc = Vec3::new(clip.x, clip.y, clip.z) / clip.w;
        eprintln!("nominal home NDC: ({:.3}, {:.3})", ndc.x, ndc.y);
        assert!(
            ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0,
            "home outside the vista frame: ({:.3}, {:.3})",
            ndc.x,
            ndc.y
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
        use game_engine::universe::CosmicWebParams;
        use glam::camera::rh::proj::directx::perspective;
        use glam::camera::rh::view::look_at_mat4;
        use glam::{Mat4, Vec3, Vec4};
        let web = generate_cosmic_web(1337, &CosmicWebParams::nominal());
        let from = vista_pose(&web);
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
