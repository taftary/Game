# Cosmic vista reframe — six-reading headline grade (v0.3.4)

**Date:** 2026-09-23
**Scope:** headline match of the reframed `vista-after.png` (seed 1337,
1408×768, High, Intel UHD 620, SHA `450450D9…`, byte-identical ×2) against
the six reference readings in
[`2026-09-20-cosmic-web-visual-description.md`](2026-09-20-cosmic-web-visual-description.md)
§2. The reframed pose is the interior-window `vista_pose` (composition
hub rank 1/4876 Tier A, dive-exact, 28.4°/s; focal NDC `(0.096, 0.000)`;
slab sibling SHA `714E9837…`; limb scans vista `0.181` / slab `0.152`,
both ≤ 0.20).
**Status:** version-close grade — no code change.

Shot: [`../../plans/v0.3.4/cosmic-vista-reframe/shots/vista-after.png`](../../plans/v0.3.4/cosmic-vista-reframe/shots/vista-after.png)
Slab sibling: [`../../plans/v0.3.4/cosmic-vista-reframe/shots/slab-after.png`](../../plans/v0.3.4/cosmic-vista-reframe/shots/slab-after.png)
Timed: `vista-after-hint.png` (t = 1.5 s, Hold — 3D-identical to t = 0,
the hint is shell text), `vista-after-dive.png` (t = 4 s, mid-dive —
framing moved into the web, hub drifted, no snap).

## Six readings (report §2 table)

| # | Reading | Verdict | One-line reason |
|---|---|---|---|
| 1 | Background | met | Deep indigo near-black field; voids hold dark with stray dots, the HDR clear `(0.02, 0.02, 0.06)` reads through (void-contrast floor 1.00× backdrop). |
| 2 | Palette | met | Cool indigo / lavender filament bodies against warm ivory-orange-pink hubs — the warm-vs-cool device is visible in every quadrant. |
| 3 | Luminance tiers | met | Blazing compact cores, medium-bright beaded grain, faint haze plus black voids — three tiers legible; cores ≤ 10 px (compact-cores grade). |
| 4 | Depth cue | partial | Layered grain-over-haze gives near/far read, but the 40 Mpc opening slice compresses the foreground/background separation — no strong crisp-vs-soft split like the reference. |
| 5 | Viewpoint | met | Frame-filling interior window with no centre and no edge: no limb in frame (radial scan `0.181` ≤ `0.20`), field runs to every edge. |
| 6 | Focal hierarchy | met | Tier-A hub at the focal mark `(0.55, 0.5)` by projection pin (NDC `(0.096, 0.000)`); warm secondaries ring the top, right, bottom and left edges. |

**Result: 5 met / 1 partial — the version closes (DoD: ≥ 5 met).**

## Notes

- The depth-cue partial is structural (thin opening slice), not a
  grading miss: the full-depth inspector still shows the volumetric
  overlap, and anisotropic splats (deferred past v0.3.4) are the
  recorded next fidelity step for thread sharpness.
- The slab sibling is deliberately sparse (30 Mpc slice at the hub
  depth through a 20° window): one bright focal hub, faint walls,
  dark voids — the interior-window rule working as specified, not an
  empty frame.
- Captures stay byte-identical per `(build, seed, GPU)`; the trilinear
  displacement fetch keeps cross-vendor identity impossible (as
  recorded in ADR-026).
