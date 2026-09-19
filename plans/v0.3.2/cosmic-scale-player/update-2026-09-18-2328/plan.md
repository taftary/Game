# Update plan — cosmic-scale-player / update-2026-09-18-2328 (UTC)

Parent update notion: [`notion.md`](notion.md)

## Feature delta breakdown

ARCHITECT notes (module + invariant annotations; `engine::universe`
**untouched** — no descriptor, hash, version, save, or content-ID
change; `Game`/`gameplay` untouched; picking/fly-to/cruise/HUD/marker/
ring/camera laws byte-identical):

### UPD-A — Visual enrichment layout (`debug::cosmic_web`)

New pure functions over `(&WebDescriptor, seed, origin)` consumed by
both cosmic surfaces: `braid_segments()` (per link: 10 subdivisions,
1–3 strands by `WebLink::density`, deterministic twist phase from
`SeededRng::stream(seed, "cosmic_web/braid")` in canonical link order,
amplitude ~1–2 Mpc tapering to 0 at both endpoints), `grain_cloud()`
(extra particulate sprites along strands, tier-scaled counts, same
stream discipline), `node_impostors()` (2 `MapVertex` sprites per
node: emissive core `color > 1.0` mass-graded + soft halo).
Transcendentals allowed here (non-hashed visual layer) but streams
and ordering follow the glow precedent; tests use tolerance bands.
Invariant rows: "same (seed, web, origin) → identical layouts",
"descriptor bytes unchanged (existing hash tests pin it)",
"picking iterates `web.nodes` only — visuals never feed selection".

### UPD-B — Engine bloom GLSL (`engine::render::post`)

`BloomParams` (threshold, intensity, tight/wide radii; validated
constructor + `spec_defaults`), `BRIGHT_FRAG` (threshold extract at
½ res), `BLUR_FRAG` (separable 9-tap Gaussian, push-constant
direction + texel), `resolve_frag_bloom()` (string-composed from
`RESOLVE_FRAG_FIXED`: `aces_fit(hdr·exposure + bloom·intensity)`,
same no-hand-duplication pattern as `resolve_frag_aces`). Reuses
`RESOLVE_VERT` (fullscreen triangle, empty vertex input). Headless
tests: naga compile of all three, composition anchors, bright/blur
CPU mirrors, param validation. Invariant row: "existing resolve
sources and `HdrSelection` semantics unchanged — additive only".

### UPD-C — Cosmic pipelines + draws (`debug::main`, shaders)

`ShaderSet` gains `glow_vert/glow_frag` (soft Gaussian sprite,
additive `AttachmentBlend::(One, One)`, redshift push) and
`webline_vert/webline_frag` (per-vertex rgba in a new
`GlowLineVertex`, additive, redshift push); shared
`map`/`line`/`fill`/`ui` pipelines and shaders stay byte-identical.
`Pipelines` + `build_pipelines` gain `map_glow`, `web_line`
(against the main pass for LDR bypass) plus scene-pass variants
below. Upload fns replace `upload_cosmic_points/lines` for cosmic
views (demo + tab): lines → braid, points → grain + descriptor glow
+ impostors, marker path unchanged and drawn last. Indigo clear per
cosmic screen. Invariant rows: rendering invariants (AGENTS.md) —
un-flipped projection, CCW/back, NDC-top fullscreen UVs per the
`post.rs` orientation contract; "old pipelines build identically".

### UPD-D — Post chain GPU half (debug shell, cosmic views only)

`select_hdr_format` device query at window creation (tools
precedent); `WindowContext` gains an HDR scene pass (HDR color +
fresh depth via `create_depth_view`), ½- and ¼-res bloom images
under one reusable post pass, bright/blur/resolve pipelines, and
descriptor sets from the existing allocator. HDR mode: scene →
bright → blur×4 → resolve draw into the main pass, then UI as today.
LDR bypass: identical cosmic draws direct-to-swapchain (no post).
Recreate rebuilds HDR/bloom images + sets with the swapchain.
`--headless` unchanged (no GPU); new CPU buffer assertions in the
boot self-test. Invariant row: "non-cosmic views record exactly the
old command sequence — new passes exist only on the cosmic path".

### UPD-E — Docs sweep

`docs/techstack/rendering.md` cosmic extension (bloom chain,
additive paths, redshift push, orientation notes); post-chain budget
line in `docs/techstack/quality.md` (+bright/blur passes at ≤½ res,
debug shell, HDR-or-bypass); version bump in
`docs/techstack/README.md`; parent files untouched, cross-linked.

## Role sign-off

Breakdown approved by: ARCHITECT _(done 2026-09-18 — module/invariant
annotations in the delta breakdown above)_ · Todos approved by:
TECHLEAD _(done 2026-09-18 — risk-first order below; todo IDs carry
the HHMM (`UPD-20260918-2328-NNN`) because the plain YYYYMMDD
namespace already belongs to `update-2026-09-18-2027` in this
feature)_ · Implemented by DEV: _(done 2026-09-19 — gates green,
evidence in DoD table)_ · Verified by ANALYST: _(pending)_ ·
Security reviewed by SECURITY: _(pending)_

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UPD-20260918-2328-001 | done | Write update notion + plan (this folder), roles filled | Requirements delta |
| UPD-20260918-2328-002 | done | UPD-A: `braid_segments` + tests (replay, strand counts, endpoint taper, amplitude bounds) | DoD 1 |
| UPD-20260918-2328-003 | done | UPD-A: `grain_cloud` + `node_impostors` + tests (counts, bounds, emissive ordering, replay) | DoD 1 |
| UPD-20260918-2328-004 | done | UPD-B: `BloomParams` + bright/blur/bloom-resolve GLSL + engine tests | DoD 3 |
| UPD-20260918-2328-005 | done | UPD-C: glow + webline shaders/pipelines/uploads/draws; indigo clear; naga/debug-shader tests; marker last | DoD 2, 5 |
| UPD-20260918-2328-006 | done | UPD-D: HDR scene pass + bloom images/passes + resolve ordering + LDR bypass + recreate + headless assertions | DoD 4, 5 |
| UPD-20260918-2328-007 | done | UPD-E: rendering.md, quality.md, techstack README bump, parent cross-links | DoD 7 |
| UPD-20260918-2328-008 | done | Quality gates green (fmt/clippy/build/test/doc/headless ×2); DoD evidence in table | DoD 6 |
| UPD-20260918-2328-009 | pending | User visual sign-off on rebuilt viewer (both cosmic tabs) | DoD 8 |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Braid/grain/impostor layouts replay-identical, count/taper/bound bands (named tests) | done | 7 new `cosmic_web` tests: `braid_strand_counts_follow_density`, `braid_replays_identically_and_tapers_at_endpoints`, `braid_colors_grade_with_density`, `grain_replays_identically_and_respects_budget`, `grain_stays_near_its_strand`, `impostors_emit_core_and_halo_per_node`, `enrichment_layouts_are_origin_relative`; `cargo test -p game_debug --lib cosmic_web` 12/12 |
| 2 | New pipelines compile (naga), no validation errors; shared paths unchanged | done | `viewer_shaders_compile` (+4 new sources) + `cosmic_push_constants_fit_vulkan_floor` + `cosmic_vertex_inputs_match_vertex_fields` (regression pin for the `line_pos`/`position` startup panic — vulkano maps inputs by name, naga cannot catch it) green; fill/line/ui/map builders+shaders byte-untouched; full suite green |
| 3 | Bloom GLSL compiles; bright/blur mirrors + composition test pass | done | 4 new `post` tests (`bloom_params_validate_and_default`, `bloom_shaders_compile_under_naga`, `bloom_weights_match_cpu_mirror`, `bloom_resolve_composition_shares_the_fixed_source`) + 4 new doc-tests; `render::post` suite 9/9 |
| 4 | HDR path + LDR bypass + recreate reach screen; headless asserts new buffers | done | `game_debug --headless`: `cosmic_layout=braid1220440 grain800000 impostors12000 ok`; windowed smoke on Intel UHD 620: HDR chain active (`R16G16B16A16_SFLOAT`), demo tab frames clean for 150 s; two startup validation failures found windowed and fixed — resolve pipeline needs explicit depth state on the depth-owning main pass (VUID-06043; bright/blur keep `None`), inspector marker must bind `map` explicitly (stale `resolve`/`map_glow` bind trips VUID-06425); bypass keeps WS3 direct draws, recreate rebuilds transients (code path mirrors tools); windowed pixels await DoD 8 |
| 5 | Gameplay unchanged (picking/fly-to/cruise/HUD/marker/ring tests green, unmodified) | done | `select_engage_cancel_and_cruise_cancel_flow`, `tick_cruises_visibly_and_tracks_camera`, `demo_target_ring_overlays_selected_node`, `inspector_selected_ring_overlays_node`, cruise/HUD tests all green, unmodified; workspace suite 547/0 (40 game + 180 debug-lib + 26 debug-bin + 293 engine + 3 integration + 5 tools) |
| 6 | Quality gates green | done | `cargo fmt --check` clean; `clippy --workspace --all-targets --all-features -- -D warnings` clean; `build --workspace` ok; `test --workspace --all-targets` 547/0; `test --doc --workspace` 87/0 (6 game + 81 engine); `cargo run --bin game` traces ok; `game_debug --headless` ok; `game_tools --headless --tier low` ok; mobile guard `cargo check --workspace` × (android + ios) clean |
| 7 | Docs sweep; parents untouched/cross-linked; one `feat:` commit on `v0.3.2` | partial | `rendering.md` cosmic refresh (tab description + new extension §), `quality.md` bloom-budget row, techstack `0.33.4`; parents untouched (update links up only); `git status` shows exactly the 7 touched files + new folder; commit pending ANALYST + SECURITY sign-off |
| 8 | User visual sign-off (both cosmic tabs, default + reseeded) | pending | — |

## Acceptance criteria

- In the running viewer, the Cosmic Web tab shows braided glowing
  filaments with bright cluster hubs and visible bloom; the cube
  boundary is still present (out of scope) but no longer the subject.
- The Game Demo tab shows the same treatment around the ship;
  flying + `R` reseed keep working; marker and amber ring stay
  legible over the glow.
- Click + `E` fly-to, cruise, HUD, and all other tabs behave exactly
  as v0.3.2 shipped.

## Risks & Next steps

- Tuning (open): `BloomParams` defaults + braid amplitude/grain
  density + redshift strength land as constants; expect 1–2 visual
  iterations with the user before sign-off (DoD 8 is the gate).
- Rebase rebuild: braid/grain buffers rebuild on 250 Mpc rebases
  (~10–30 ms one-frame, rare); tier-scaled counts are the mitigation.
- Buffer memory (landed values, 2026-09-19, nominal seed): braid
  1.22M verts (~34 MB) + glow ~0.96M verts (~35 MB) per surface set,
  ~140 MB for demo + inspector sets combined. Inside the app-memory
  budgets (debug shell, desktop-first) but recorded here — a
  low-tier grain cap is the follow-up if device profiles complain.
- Follow-ups (out of scope): cubic-boundary treatment (clip vs
  fade — deferred per PO), DoF, raymarched haze via
  `cue::sample_web_density`, non-cosmic bloom, release-binary post.
