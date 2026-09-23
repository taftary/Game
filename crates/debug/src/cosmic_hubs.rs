//! Cosmic hub hierarchy: rank-tiered impostors + member scatter.
//!
//! Nodes arrive mass-ranked (densest first), so tiers are pure rank
//! cut-offs: top 1 % are Tier A hubs, the next 10 % Tier B, the rest
//! Tier C beads. Members are a static seeded scatter inside `r_vir`.

use game_engine::core::SeededRng;
use game_engine::universe::WebDescriptor;
use glam::DVec3;

/// Fraction of nodes promoted to Tier A (brightest hubs).
pub const HUB_TIER_A_FRACTION: f64 = 0.01;
/// Fraction of nodes promoted to Tier B (secondary hubs).
pub const HUB_TIER_B_FRACTION: f64 = 0.10;
/// Hard cap on member points (two-pass budget, deterministic).
pub const MAX_MEMBER_POINTS: usize = 40_000;

/// Visual tier of a hub node, by mass rank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubTier {
    /// Top 1 %: pin + core + halo, plus a member scatter.
    A,
    /// Next 10 %: core + halo, plus a small member scatter.
    B,
    /// The rest: a single warm bead, no halo, no members.
    C,
}

impl HubTier {
    /// Tier of `rank` (0 = most massive) out of `n` nodes.
    pub fn of(rank: usize, n: usize) -> HubTier {
        if n == 0 {
            return HubTier::C;
        }
        let a = (HUB_TIER_A_FRACTION * n as f64).ceil() as usize;
        let b_end = ((HUB_TIER_A_FRACTION + HUB_TIER_B_FRACTION) * n as f64).ceil() as usize;
        if rank < a {
            HubTier::A
        } else if rank < b_end {
            HubTier::B
        } else {
            HubTier::C
        }
    }

    /// Within-tier level in (0, 1]: 1 for the tier head, fading by rank.
    pub fn within_level(rank_in_tier: usize, tier_size: usize) -> f32 {
        if tier_size == 0 {
            return 0.0;
        }
        1.0 - rank_in_tier as f32 / tier_size as f32
    }
}

/// Tier sizes `(a_count, b_end)` for `n` nodes; mirrors [`HubTier::of`].
fn tier_bounds(n: usize) -> (usize, usize) {
    let a = (HUB_TIER_A_FRACTION * n as f64).ceil() as usize;
    let b_end = ((HUB_TIER_A_FRACTION + HUB_TIER_B_FRACTION) * n as f64).ceil() as usize;
    (a.min(n), b_end.min(n))
}

/// Within-tier level for an absolute rank.
fn level_for(rank: usize, n: usize) -> f32 {
    let (a, b_end) = tier_bounds(n);
    if rank < a {
        HubTier::within_level(rank, a)
    } else if rank < b_end {
        HubTier::within_level(rank - a, b_end - a)
    } else {
        HubTier::within_level(rank - b_end, n - b_end)
    }
}

/// Origin-relative f32 position of a node (Mpc).
fn rel_pos(position_mpc: [f64; 3], origin: DVec3) -> [f32; 3] {
    [
        (position_mpc[0] - origin.x) as f32,
        (position_mpc[1] - origin.y) as f32,
        (position_mpc[2] - origin.z) as f32,
    ]
}

/// Impostor sprites as `(pos, color, misc=(size, alpha, kind))`: pin and
/// core/bead sizes are px, halo sizes are world-Mpc diameters (kind 1).
pub fn hub_impostors(web: &WebDescriptor, origin: DVec3) -> Vec<([f32; 3], [f32; 3], [f32; 3])> {
    let n = web.nodes.len();
    let mut out = Vec::new();
    for (rank, node) in web.nodes.iter().enumerate() {
        let pos = rel_pos(node.position_mpc, origin);
        let l = level_for(rank, n);
        match HubTier::of(rank, n) {
            HubTier::A => {
                // White-hot pin + pale core + world-sized warm halo.
                out.push((pos, [3.2, 3.0, 2.8], [5.0, 1.0, 2.0]));
                out.push((pos, [3.0, 2.4, 1.6], [12.0 + 8.0 * l, 1.0, 2.0]));
                out.push((
                    pos,
                    [0.9, 0.55, 0.35],
                    [(2.0 * node.virial_radius_mpc) as f32, 0.12, 1.0],
                ));
            }
            HubTier::B => {
                // Warm gold core + small halo.
                out.push((pos, [1.5, 1.15, 0.75], [4.0 + 4.0 * l, 1.0, 2.0]));
                out.push((
                    pos,
                    [0.8, 0.5, 0.3],
                    [node.virial_radius_mpc as f32, 0.10, 1.0],
                ));
            }
            HubTier::C => {
                // Single warm bead, below the bloom threshold.
                out.push((pos, [0.85, 0.72, 0.47], [2.0, 1.0, 0.0]));
            }
        }
    }
    out
}

/// Irwin-Hall-3: sum of 3 uniforms, centered (pure arithmetic).
fn ihalf3(rng: &mut SeededRng) -> f64 {
    rng.unit_f64() + rng.unit_f64() + rng.unit_f64() - 1.5
}

/// Nominal member count for one node (before the budget cap).
fn nominal_members(tier: HubTier, l: f32) -> usize {
    match tier {
        HubTier::A => (40.0 + 200.0 * f64::from(l)).round() as usize,
        HubTier::B => (10.0 + 20.0 * f64::from(l)).round() as usize,
        HubTier::C => 0,
    }
}

/// Member galaxies as `(pos, color, misc)`: NFW-like `r_vir * u^2` scatter
/// with a trig-free direction, class colors from the report table.
pub fn hub_members(
    web: &WebDescriptor,
    seed: u64,
    origin: DVec3,
) -> Vec<([f32; 3], [f32; 3], [f32; 3])> {
    let n = web.nodes.len();
    // Pass 1: nominal counts in canonical node order.
    let mut counts = Vec::with_capacity(n);
    let mut total = 0_usize;
    for (rank, _) in web.nodes.iter().enumerate() {
        let c = nominal_members(HubTier::of(rank, n), level_for(rank, n));
        counts.push(c);
        total += c;
    }
    // Pass 2: scale down deterministically when over budget.
    let scale = if total > MAX_MEMBER_POINTS {
        MAX_MEMBER_POINTS as f64 / total as f64
    } else {
        1.0
    };
    let budgeted: Vec<usize> = counts
        .iter()
        .map(|c| (*c as f64 * scale).floor() as usize)
        .collect();
    let mut rng = SeededRng::stream(seed, "cosmic_web/members");
    let mut out = Vec::new();
    for ((rank, node), count) in web.nodes.iter().enumerate().zip(budgeted.iter()) {
        let l = level_for(rank, n);
        let glow = 1.5 + 1.5 * f64::from(l);
        let size = 1.5 + 1.5 * l;
        let r_vir = node.virial_radius_mpc;
        for _ in 0..*count {
            let u = rng.unit_f64();
            let mut dir = [ihalf3(&mut rng), ihalf3(&mut rng), ihalf3(&mut rng)];
            let mut len2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
            if len2 < 1e-12 {
                // Resample once; fall back to +x if still degenerate.
                dir = [ihalf3(&mut rng), ihalf3(&mut rng), ihalf3(&mut rng)];
                len2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
                if len2 < 1e-12 {
                    dir = [1.0, 0.0, 0.0];
                    len2 = 1.0;
                }
            }
            let len = len2.sqrt();
            let r = r_vir * u * u;
            let pos = [
                (node.position_mpc[0] + r * dir[0] / len - origin.x) as f32,
                (node.position_mpc[1] + r * dir[1] / len - origin.y) as f32,
                (node.position_mpc[2] + r * dir[2] / len - origin.z) as f32,
            ];
            // Class by one uniform: 60 % pink-red, 30 % orange, else white.
            let class = rng.unit_f64();
            let base = if class < 0.6 {
                [1.0, 0.45, 0.50]
            } else if class < 0.9 {
                [1.0, 0.70, 0.40]
            } else {
                [1.0, 0.97, 0.90]
            };
            let e = glow as f32;
            out.push((
                pos,
                [base[0] * e, base[1] * e, base[2] * e],
                [size, 0.9, 0.0],
            ));
        }
    }
    out
}

/// Nearest Tier-A node to home (the fly-to goal candidate).
pub fn nearest_tier_a_to_home(web: &WebDescriptor) -> Option<u32> {
    let n = web.nodes.len();
    if n == 0 {
        return None;
    }
    let home = web.home().position_mpc;
    let mut best: Option<(f64, u32)> = None;
    for (rank, node) in web.nodes.iter().enumerate() {
        if HubTier::of(rank, n) != HubTier::A {
            continue;
        }
        let dx = node.position_mpc[0] - home[0];
        let dy = node.position_mpc[1] - home[1];
        let dz = node.position_mpc[2] - home[2];
        let d2 = dx * dx + dy * dy + dz * dz;
        if best.is_none_or(|(b, _)| d2 < b) {
            best = Some((d2, node.node_index));
        }
    }
    best.map(|(_, index)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_engine::universe::WebNode;

    /// Nodes on the x-axis, 10 Mpc apart, `r_vir` 1 Mpc (A nodes 2 Mpc).
    fn synth_web(n: usize, home: u32) -> WebDescriptor {
        let mut nodes = Vec::with_capacity(n);
        for i in 0..n {
            nodes.push(WebNode {
                node_index: i as u32,
                position_mpc: [i as f64 * 10.0, 0.0, 0.0],
                mass_msun: 1.0e15 - i as f64,
                virial_radius_mpc: if i < 2 { 2.0 } else { 1.0 },
            });
        }
        WebDescriptor::new(42, nodes, Vec::new(), Vec::new(), home, 0.0)
    }

    /// Premultiplied peak channel (bloom input proxy).
    fn peak(color: [f32; 3]) -> f32 {
        color[0].max(color[1]).max(color[2])
    }

    #[test]
    fn tier_cutoffs_at_n6000() {
        assert_eq!(HubTier::of(0, 0), HubTier::C);
        assert_eq!(HubTier::of(0, 6000), HubTier::A);
        assert_eq!(HubTier::of(59, 6000), HubTier::A);
        assert_eq!(HubTier::of(60, 6000), HubTier::B);
        assert_eq!(HubTier::of(659, 6000), HubTier::B);
        assert_eq!(HubTier::of(660, 6000), HubTier::C);
        assert_eq!(HubTier::of(5999, 6000), HubTier::C);
    }

    #[test]
    fn within_level_falls_from_one_to_zero() {
        assert_eq!(HubTier::within_level(0, 0), 0.0);
        assert_eq!(HubTier::within_level(0, 60), 1.0);
        let mut prev = f32::INFINITY;
        for rank in 0..60 {
            let l = HubTier::within_level(rank, 60);
            assert!(l <= prev);
            prev = l;
        }
        assert!(HubTier::within_level(59, 60) > 0.0);
    }

    #[test]
    fn bloom_bands_per_tier() {
        // n=100: rank 0 is A, ranks 1..=10 are B, rest C.
        let web = synth_web(100, 50);
        let sprites = hub_impostors(&web, DVec3::ZERO);
        let (a_pin, a_core, a_halo) = (sprites[0], sprites[1], sprites[2]);
        assert!(peak(a_pin.1) >= 3.0);
        assert!(peak(a_core.1) >= 3.0);
        assert_eq!(a_pin.2[2], 2.0);
        assert_eq!(a_core.2[2], 2.0);
        assert_eq!(a_halo.2[2], 1.0);
        let b_core = sprites[3];
        assert!((peak(b_core.1) - 1.5).abs() <= 0.3);
        assert_eq!(b_core.2[2], 2.0);
        let c_bead = sprites[sprites.len() - 1];
        assert!(peak(c_bead.1) <= 0.9);
        assert_eq!(c_bead.2[2], 0.0);
    }

    #[test]
    fn impostor_sprite_totals() {
        // n=200: A=2, B=20, C=178 -> 6 + 40 + 178 = 224.
        let web = synth_web(200, 100);
        let sprites = hub_impostors(&web, DVec3::ZERO);
        assert_eq!(sprites.len(), 2 * 3 + 20 * 2 + 178);
        // Per-node stride: A nodes own 3 each, B 2, C 1.
        assert_eq!(sprites[0].2[0], 5.0); // A pin size
        let b_start = 2 * 3;
        assert_eq!(sprites[b_start + 1].2[2], 1.0); // B halo kind
        let c_start = b_start + 20 * 2;
        assert_eq!(sprites[c_start].2, [2.0, 1.0, 0.0]); // C bead misc
        assert_eq!(sprites.len() - c_start, 178);
    }

    /// Expected nominal counts for the `synth_web(200)` layout.
    fn expected_counts_200() -> Vec<usize> {
        let (a, b_end) = tier_bounds(200);
        assert_eq!((a, b_end), (2, 22));
        (0..200)
            .map(|rank| nominal_members(HubTier::of(rank, 200), level_for(rank, 200)))
            .collect()
    }

    #[test]
    fn member_counts_match_tiers() {
        let web = synth_web(200, 100);
        let expected = expected_counts_200();
        // A head emits 40+200*1 = 240, second A 40+200*0.5 = 140.
        assert_eq!(expected[0], 240);
        assert_eq!(expected[1], 140);
        // B head emits 10+20*1 = 30; C nodes emit none.
        assert_eq!(expected[2], 30);
        assert!(expected[22..].iter().all(|c| *c == 0));
        let members = hub_members(&web, 7, DVec3::ZERO);
        assert_eq!(members.len(), expected.iter().sum::<usize>());
        assert!(members.len() <= MAX_MEMBER_POINTS);
    }

    #[test]
    fn members_all_inside_virial_radius() {
        let web = synth_web(200, 100);
        let expected = expected_counts_200();
        let members = hub_members(&web, 7, DVec3::ZERO);
        let mut per_node = vec![0_usize; 200];
        for (pos, _, misc) in &members {
            // Nodes sit 10 Mpc apart on x: nearest index is unambiguous.
            let idx = (pos[0] / 10.0).round() as usize;
            assert!(idx < 200);
            let node = &web.nodes[idx];
            let dx = f64::from(pos[0]) - node.position_mpc[0];
            let dy = f64::from(pos[1]) - node.position_mpc[1];
            let dz = f64::from(pos[2]) - node.position_mpc[2];
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            assert!(dist <= node.virial_radius_mpc + 0.01, "dist {dist}");
            per_node[idx] += 1;
            assert_eq!(misc[2], 0.0);
            assert_eq!(misc[1], 0.9);
        }
        assert_eq!(per_node, expected);
    }

    #[test]
    fn member_class_proportions() {
        let web = synth_web(200, 100);
        let members = hub_members(&web, 7, DVec3::ZERO);
        let mut pink = 0_usize;
        let mut orange = 0_usize;
        let mut white = 0_usize;
        for (_, color, _) in &members {
            // Emissive cancels in the g/r ratio: 0.45 / 0.70 / 0.97.
            let ratio = color[1] / color[0];
            if ratio < 0.575 {
                pink += 1;
            } else if ratio < 0.835 {
                orange += 1;
            } else {
                white += 1;
            }
        }
        let total = members.len() as f32;
        assert!((pink as f32 / total - 0.6).abs() <= 0.10);
        assert!((orange as f32 / total - 0.3).abs() <= 0.10);
        assert!((white as f32 / total - 0.1).abs() <= 0.10);
    }

    #[test]
    fn members_replay_identically() {
        let web = synth_web(200, 100);
        let a = hub_members(&web, 7, DVec3::ZERO);
        let b = hub_members(&web, 7, DVec3::ZERO);
        assert_eq!(a, b);
        let c = hub_impostors(&web, DVec3::ZERO);
        let d = hub_impostors(&web, DVec3::ZERO);
        assert_eq!(c, d);
    }

    #[test]
    fn members_are_translation_invariant() {
        let web = synth_web(200, 100);
        let shift = DVec3::new(100.0, -50.0, 25.0);
        let a = hub_members(&web, 7, DVec3::ZERO);
        let b = hub_members(&web, 7, shift);
        assert_eq!(a.len(), b.len());
        for (p, q) in a.iter().zip(b.iter()) {
            for axis in 0..3 {
                let moved = p.0[axis] - q.0[axis];
                assert!((moved - shift.to_array()[axis] as f32).abs() < 0.02);
            }
            assert_eq!(p.1, q.1);
            assert_eq!(p.2, q.2);
        }
        let c = hub_impostors(&web, DVec3::ZERO);
        let d = hub_impostors(&web, shift);
        assert_eq!(c.len(), d.len());
        for (p, q) in c.iter().zip(d.iter()) {
            for axis in 0..3 {
                let moved = p.0[axis] - q.0[axis];
                assert!((moved - shift.to_array()[axis] as f32).abs() < 0.02);
            }
            assert_eq!(p.1, q.1);
            assert_eq!(p.2, q.2);
        }
    }

    #[test]
    fn nearest_tier_a_to_home_picks_nearest_a() {
        let mut web = synth_web(200, 100);
        // Home at x=1000; A nodes at distances 50 and 5; a B node nearer.
        web.nodes[0].position_mpc = [1050.0, 0.0, 0.0];
        web.nodes[1].position_mpc = [1005.0, 0.0, 0.0];
        web.nodes[2].position_mpc = [1001.0, 0.0, 0.0];
        assert_eq!(nearest_tier_a_to_home(&web), Some(1));
        let empty = WebDescriptor::new(1, Vec::new(), Vec::new(), Vec::new(), 0, 0.0);
        assert_eq!(nearest_tier_a_to_home(&empty), None);
    }

    #[test]
    fn nominal_hubs_inside_sphere() {
        // CSC-005 / FR4: impostor sprites sit on nodes (≤ R); members
        // scatter inside `r_vir` of their node, so ≤ R + max r_vir.
        // Slow (~15 s): full 128³ generation.
        use game_engine::universe::{CosmicWebParams, generate_cosmic_web};
        let params = CosmicWebParams::nominal();
        let web = generate_cosmic_web(1234, &params);
        let r = params.descriptor_radius_mpc as f32;
        let max_vir = web
            .nodes
            .iter()
            .map(|n| n.virial_radius_mpc as f32)
            .fold(0.0_f32, f32::max);
        for (pos, _, _) in hub_impostors(&web, DVec3::ZERO) {
            let r2 = pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2];
            assert!(r2 <= r * r, "impostor outside sphere: {pos:?}");
        }
        for (pos, _, _) in hub_members(&web, 1234, DVec3::ZERO) {
            let d = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2]).sqrt();
            assert!(d <= r + max_vir + 0.01, "member outside R + r_vir: {pos:?}");
        }
    }

    /// Spawn-goal tier (FR6): the demo's first fly-to candidate (far
    /// node of the home's strongest link) is Tier C on the nominal
    /// seed — the Local-Group analog's brightest departure leads to
    /// another group, not a cluster. This pins the FR6 *alternative*:
    /// the highlight ring carries the goal (UX-1 fallback), the spawn
    /// faces it down the strongest link, and `cosmic-vista-intro`
    /// frames the nearest Tier A hub explicitly. If generation ever
    /// moves the goal, this fails loudly for re-review.
    /// Slow (~15 s): full 128³ generation.
    #[test]
    fn nominal_spawn_goal_tier_is_c() {
        use game_engine::universe::{CosmicWebParams, generate_cosmic_web};
        let web = generate_cosmic_web(1337, &CosmicWebParams::nominal());
        let goal = web
            .strongest_link_from(web.home_node)
            .map(|link| {
                if link.a == web.home_node {
                    link.b as usize
                } else {
                    link.a as usize
                }
            })
            .expect("home node must have a departure link");
        assert_eq!(HubTier::of(goal, web.nodes.len()), HubTier::C);
    }
}
