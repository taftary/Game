# Update notion — cosmic-scale-player / update-2026-09-18-2328 (UTC)

Parent feature: [`../notion.md`](../notion.md)

## Status

`in-review` (DEV complete 2026-09-19 — gates green, evidence in
`plan.md` DoD table; ANALYST + SECURITY sign-off pending before the
single `feat:` commit; user visual sign-off pending as DoD 8)

## Reason for update

The shipped cosmic web reads as a flat cube of hard-edged sprites:
filaments are straight constant-color segments, nodes are identical
pixel dots, there is no bloom, no depth cue, and the whole volume has
the crisp silhouette of the 512 Mpc generation box. The v0.4 spec
(§9.1, §9.3) mandates Hubble-redshift depth cueing plus volumetric
glow at this scale; the v0.3.2 notion (§3-D) already listed redshift
tint + cue haze as part of the descriptor's render intent. None of it
landed in the two cosmic surfaces.

This update gives both cosmic surfaces (Game Demo flight + Cosmic Web
inspector) a cinematic treatment: seeded braided filaments with
particulate grain, emissive cluster hubs, additive glow, in-shader
redshift depth tint, and a real HDR bloom post chain — while the
stage-0 descriptor, gameplay (picking/fly-to/HUD/cruise), and every
other viewer surface stay byte-identical.

## Roles

Author: PO. Consulted: UX (player-facing Game Demo surface — look,
legibility of marker/HUD/ring over glow, no controls change),
ARCHITECT (new pipelines + render passes in the debug shell;
`engine::render::post` GLSL additions), TECHLEAD (todos + budgets).
ANALYST + SECURITY sign off at `in-review` per the lifecycle gates.

## Scope

### In-scope

- Visual enrichment layout in `debug::cosmic_web` (shared by both
  cosmic surfaces): seeded **braided filament strands** (polyline
  subdivision, 1–3 strands twisting around each link trunk, scaled by
  the existing `WebLink::density`), **particulate grain** sprites along
  the strands, **two-sprite node impostors** (emissive hot core +
  soft pale-cyan halo, mass-graded). All derived deterministically
  from the descriptor + seed; the descriptor itself is untouched (no
  `UNIVERSE_VERSION` bump, no save/content-ID impact).
- New cosmic-only GPU paths in the debug binary: **additive-blend**
  point pipeline (soft Gaussian sprite mask, in-shader redshift depth
  tint) and **additive colored-line pipeline** (per-vertex rgba,
  density-graded, node-tapered). The shared `map`/`line` pipelines and
  every other surface (galaxy/system/planet/sky) are untouched.
- **Real HDR bloom post chain, cosmic views only**: HDR scene target
  (`post::select_hdr_format`, 16F → packed-float → LDR bypass),
  fullscreen bright pass, 2-scale separable Gaussian blur, ACES
  resolve composite. New GLSL + `BloomParams` live in
  `engine::render::post` (headless-tested, naga-compile-tested);
  the GPU half follows the `game_tools` `ResolvePass` precedent.
- LDR-bypass parity: incapable devices draw the same new layouts
  through the additive pipelines direct-to-swapchain (bloom/tonemap
  omitted, content identical).
- Per-screen deep-indigo clear color for both cosmic views.
- Docs: `docs/techstack/rendering.md` cosmic extension, post-chain
  budget line in `docs/techstack/quality.md`, techstack `README.md`
  version bump; parent files untouched, cross-linked.

### Out-of-scope

- Generation/descriptor changes (no sphere clip, no edge fade — the
  cubic boundary stays per PO decision; recorded as a follow-up).
- Gameplay: picking, fly-to, cruise, HUD, markers, target ring, seed
  loader, camera laws, reseed semantics.
- Depth of field; raymarched volumetric haze (`cue::sample_web_density`
  stays the future source term); galaxy/system/planet/sky visuals;
  the release `game` binary.
- Real bloom for non-cosmic views.

## Requirements delta

- Boot/reseed: same web, same spawn, same cameras. Uploads grow:
  braid line verts (~1–3 strands × ~10 subdivisions per link),
  grain sprites (tier-scaled, ~150k low → ~800k high), impostors
  (2 sprites per node).
- Frame draw (cosmic views): indigo clear → additive braid lines →
  additive grain/glow → additive node impostors → player marker
  (existing alpha path, on top) → [HDR mode] bright + blur chain →
  ACES resolve + UI; [LDR bypass] direct-to-swapchain, same draws,
  no post.
- Resize/recreate: HDR + bloom images/framebuffers/sets rebuild with
  the swapchain (the tools recreate path precedent).
- Headless: `game_debug --headless` stays GPU-free; new layout
  assertions run in-process.
- Redshift tint: observer-relative (view depth `clip.w`), strength
  from a single cosmic push constant (artist-tunable, exaggerated per
  the spec's perceptual-exaggeration rule).
- Bloom tuning (`BloomParams`: threshold, intensity, radii) isolated
  in one struct; defaults ship usable, 1–2 visual iterations expected.

## Definition of Done delta

- [ ] Braid/grain/impostor layouts replay-identically per
      (seed, web) and satisfy count/taper/bound bands — pinned by
      named unit tests (WS1).
- [ ] New pipelines + shaders compile (naga tests) and draw with no
      validation errors; shared pipelines and non-cosmic surfaces
      unchanged — existing tests green (WS3).
- [ ] Bloom GLSL compiles; bright/blur math pinned by CPU-mirror
      tests; resolve-composite string test passes (WS2).
- [ ] HDR cosmic path + LDR bypass + resize/recreate all reach the
      screen; headless boot asserts the new buffers non-empty (WS4).
- [ ] No gameplay change: picking/fly-to/cruise/HUD/marker/ring
      tests green, unmodified (WS3/WS4).
- [ ] Quality gates green (`fmt --check`; `clippy --workspace
      --all-targets --all-features -- -D warnings`;
      `build --workspace`; `test --workspace --all-targets`;
      `test --doc --workspace`; `game_debug --headless`;
      `game_tools --headless --tier low`).
- [ ] Docs sweep landed (rendering.md cosmic extension, quality.md
      post-chain line, techstack version bump); parent files
      untouched, cross-linked; one `feat:` commit on `v0.3.2`.
- [ ] User visual sign-off on the rebuilt viewer (both cosmic tabs,
      default + reseeded) — the "visual pending user" precedent.
