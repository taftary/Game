# Notion — web-field-export

## Status

`done` (PO sign-off 2026-09-20; ARCHITECT + TECHLEAD breakdown in
`plan.md`; ADR-025 accepted; ANALYST audit + SECURITY review recorded
in `plan.md` DoD table; single commit on branch `v0.3.3`)

## Context

Stage B of the stage-0 generator
(`crates/engine/src/universe/web/displace.rs:39-69`) moves one
Zel'dovich tracer per lattice cell (`128³ ≈ 2.1M`) by `x = q − D₊·∇Ψ`,
rounds each displaced position to a cell for NGP deposit, and discards
the position. Stage C (`classify.rs:133-245`) labels every cell void /
sheet / filament / node and keeps only the node peaks. The descriptor
that reaches the renderer (`WebDescriptor`) is therefore a **graph**
(~6000 nodes, ~22k straight links, 150k jittered glow points) — and
every v0.3.2 render feature had to re-invent filament bodies by
decorating straight segments (report §11).

Illustris-style renders, including the target
([`target.jpeg`](../../../docs/reports/images/target.jpeg)), are
particle splats of exactly the data stage B throws away: tracer
positions, each with a local density that sets its kernel size and
color. ADR-025 §1 decides to keep that data as a **non-hashed render
sidecar** and leave `WebDescriptor`, `web_hash`, `UNIVERSE_VERSION`,
content IDs, and saves byte-identical.

## Problem & Needs

- The renderer needs **where the matter is** (curved, branching,
  sheet-like structure), not only where the peaks are and which peaks
  are linked.
- The renderer needs **how dense each sample is** to size kernels
  (`h ∝ ρ^-1/3`) and pick colors on a density ramp — the two devices
  that make the reference read as gas + galaxies rather than dots.
- The gas veil (`cosmic-gas-veil-v2`) needs the **cell grid** (density
  + T-web class) to place sheet/filament bodies and to feed a 3D
  texture on Medium/High.
- All of the above must cost nothing on the determinism contract
  (same `(seed, version, params)` → same descriptor, same hash) and
  little on cold start (`quality.md`: < 5 s mid-tier phone; today
  ≈ 0.5 s for the web).

## Goals

1. **Tracer export.** Every displaced tracer whose position lies inside
   the descriptor sphere (`descriptor_radius_mpc`, nominal 250 Mpc) is
   exported as `(position Mpc f32×3, local density f32)`; ~1.0–1.1M
   at nominal (sphere/box volume ≈ 0.49).
2. **Lattice imprint suppressed.** Undisplaced tracers (deep voids)
   would otherwise render the cubic lattice. Each tracer's Lagrangian
   start `q` gets a deterministic sub-cell jitter (Irwin–Hall, σ ≈ 0.3
   cell, own stream) **for the export only** — the NGP deposit and
   everything hashed keep the unjittered path. A test pins that the
   Eulerian density and the descriptor are unchanged by the export.
3. **Smoothed local density.** Per-tracer density = 3³ box-smoothed NGP
   count at its landing cell, divided by the mean (dimensionless
   overdensity `1+δ`), so single-cell speckle does not become per-
   particle color noise.
4. **Grid export.** The `128³` grid as one `u8` per cell packing the
   T-web class (2 bits) and quantized `log2(1+δ)` (6 bits), 2 MB.
5. **One entry point, zero behaviour change.**
   `generate_cosmic_web_with_field(seed, params) -> (WebDescriptor,
   WebField)`; `generate_cosmic_web` becomes a thin wrapper that drops
   the field. Both return the identical descriptor (equality pin).
6. **Optional refinement (High only).** `WebField::refine(factor 2)`
   emits 8 sub-tracers per lattice tracer via trilinear `∇Ψ`
   interpolation — off by default, never on Low/Medium, never hashed.

## Non-goals

- No change to stages A–D behaviour: no new smoothing, no 2LPT, no
  nested grids, no N-body (all still deferred per milestones roadmap).
- No `UNIVERSE_VERSION` bump, no hash change, no save-envelope change,
  no content-ID change. `WebField` is never serialized.
- No gameplay read of `WebField`: selection, fly-to, home node, spawn,
  HUD stay on `WebDescriptor` (ADR-025 §1).
- No GPU code, no debug-crate change beyond compiling (the render
  features consume the export).
- No compression or streaming of the field (fits in memory at ≈ 16 MB).

## Users / Stakeholders

- **Render features** (`cosmic-tracer-splat`, `cosmic-hub-hierarchy`,
  `cosmic-gas-veil-v2`) — the sole consumers.
- **Player** — indirectly: boot cost must stay inside the cold-start gate.
- UX consulted: n-a (no surface). ARCHITECT consulted: yes — first
  `engine::universe::web` touch since ADR-023; ADR-025 written.

## Roles

Author: PO. UX consulted (required if player-facing): n-a.
ARCHITECT consulted (required if cross-module): yes — ADR-025 (new
public struct + entry point in `engine::universe::web`; determinism
contract untouched; sidecar never hashed/saved).

## Functional requirements

- FR1 (API): `pub struct WebField { tracers: Vec<WebTracer>, grid:
  Vec<u8>, grid_cells: u32, cell_size_mpc: f64, mean_density: f64,
  origin_mpc: [f64; 3] }`, `pub struct WebTracer { pos_mpc: [f32; 3],
  overdensity: f32 }`; `pub fn generate_cosmic_web_with_field(seed,
  &CosmicWebParams) -> (WebDescriptor, WebField)`; `generate_cosmic_web`
  unchanged in signature and result. Re-exported from
  `engine::universe`.
- FR2 (coordinates): tracer positions in the same box-centered Mpc
  frame as `WebNode::position_mpc` (`descriptor.rs` `refine()`
  convention), so a tracer landing in a node's cell sits within one
  cell of `position_mpc`. Test: for the 50 densest nodes, ≥ 1 tracer
  within `cell_size_mpc` of the node.
- FR3 (jitter): export-only Lagrangian jitter from stream
  `"cosmic_web/tracer"`, Irwin–Hall-3 scaled to σ ≈ 0.3 cell, applied
  to `q` before displacement in a **second** loop that does not feed
  the NGP deposit. Displacement uses the same central-difference `∇Ψ`
  at the tracer's cell (no interpolation at factor 1).
- FR4 (density): `overdensity = smooth3(count)[landing cell] / mean`,
  with `smooth3` a periodic 3³ box mean over the NGP counts. Values
  finite, ≥ 0; `(1+δ)` mean over all tracers ≈ 1 within 10%.
- FR5 (grid): `grid[i] = (class << 6) | quantize(log2(1+δ_smooth), 6
  bits over [−4, +6])`, class ∈ {VOID, SHEET, FILAMENT, NODE} as in
  `classify.rs`. Helpers `class_at(i)`, `overdensity_at(i)`.
- FR6 (sphere cut): only tracers with `|x − center| ≤
  descriptor_radius_mpc` are kept; count at nominal in `[0.9M, 1.2M]`
  (band test).
- FR7 (refine): `WebField::refine(&self, potential_ref, factor: u32) ->
  WebField` is **not** part of this feature's DoD beyond compiling and a
  unit test on a 32³ box (factor 2 → 8× tracers, all inside the sphere,
  densities finite). Wiring to High tier is `cosmic-tracer-splat`'s
  decision.

## Non-functional requirements

- NFR1 (determinism, hard): `generate_cosmic_web` result and
  `web_hash` vectors byte-identical before/after (existing
  `committed_web_vectors_pin_stage0 = 17_301_221_795_867_311_725`,
  `nominal_descriptor_replays_identically`, `ulp_perturbation_cannot_flip_web_hash`
  all green, untouched). New equality pin: `generate_cosmic_web(s,p) ==
  generate_cosmic_web_with_field(s,p).0` on the nominal seed.
- NFR2 (export replay): `WebField` replays identically on the same
  platform (same-platform contract, like the debug enrichment; it is
  not hashed so cross-platform bit equality is not claimed). Hashed
  paths keep integer decisions + `sqrt`-only; the export may use `log2`
  for quantization because it never feeds a hash.
- NFR3 (boot): `generate_cosmic_web_with_field` at nominal ≤
  `generate_cosmic_web` + 120 ms single-threaded on the reference
  desktop (measured with `std::time::Instant` in the headless check and
  recorded in `plan.md`). If over budget → tier cut: export every 2nd
  tracer on Low (`WebFieldBudget::Low`), never a change to stages A–D.
- NFR4 (memory): ≤ 20 MB resident at nominal (16 B × 1.1M tracers +
  2 MB grid); no clone of the potential or Eulerian arrays survives the
  call.
- NFR5 (boundaries): `engine::universe` stays pure and headless; no
  `render` import; `game` crate untouched; doc tests for the new public
  API (quality.md test policy).

## Definition of Done

1. `generate_cosmic_web_with_field` exists, is re-exported, documented
   with a doc test; `generate_cosmic_web` delegates to it; equality pin
   green.
2. All pre-existing `engine::universe` tests green, unchanged (hash
   vector, replay, ulp, statistical, void bands, mass rank).
3. Tracer count band, sphere cut, density normalisation, node-proximity
   (FR2), grid packing round-trip, and jitter-does-not-touch-density
   tests green.
4. Boot delta measured and ≤ 120 ms (or Low cut applied and recorded);
   memory ≤ 20 MB (measured by allocation size sum, recorded).
5. `refine(2)` compiles + 32³ unit test green (wiring deferred).
6. Docs: `docs/game/universe.md` sidecar sentence, `rendering.md`
   "field sidecar" paragraph, `architecture.md` module note,
   techstack version bump, ADR-025 already in place; links resolve.
7. Full `quality.md` gate list green incl. mobile compile guards;
   ANALYST audit + SECURITY review (no I/O, no serialization, pure
   compute) signed; single `done` commit.

## Constraints & Assumptions

- Second feature on branch `v0.3.3` (after `cosmic-capture-harness`);
  every render feature depends on it.
- Assumes nominal params (128³, 4 Mpc, 250 Mpc sphere) for all bands;
  bands are asserted on `CosmicWebParams::nominal()` only, small-box
  tests assert structure, not counts.
- The second displacement loop re-reads the potential (already in
  memory); no re-generation of the initial field.

## Open questions

- Whether `overdensity` should be stored as `f16`/`u16` to halve tracer
  size (12 → 10 B is not worth the unpack cost on the GPU side; PO
  leans `f32`, revisit if memory gate is threatened on Low).
- Whether to also export per-tracer T-web class (1 byte) so the splat
  shader can tint sheets differently from filaments without a grid
  lookup — deferred to `cosmic-tracer-splat` (it can read the grid).
