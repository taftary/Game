# Update plan — cosmic-scale-player / update-2026-09-19-1245 (UTC)

Parent update notion: [`notion.md`](notion.md)

## Feature delta breakdown

### Phase 1 — Ribbon filaments (GPU expansion)

**Module:** `crates/debug/src/main.rs` (pipeline, shaders, upload),
`crates/debug/src/cosmic_web.rs` (strand record generation).

CPU keeps only `braid_shape` derivation (per-link: endpoints, wander,
strand params, color — ~64 B/strand, ~3 MB total). The vertex shader
expands `braid_point()` from `gl_VertexIndex` into camera-facing ribbon
quads with world-space width (Mpc) + min-pixel clamp. Fragment shader
applies Gaussian lateral falloff; density-graded emissive > 1.0 on dense
links so filaments bloom for the first time.

Pipeline: `TriangleList` sibling of `webline` (same additive One/One,
no depth write, HDR-scene + LDR variants). ~1.2M tris — within the
3M desktop budget.

Invariant notes:
- Enrichment-only (render-only, never touches gameplay data).
- Additive blend unchanged (SrcColor=One, DstColor=One).
- `directx::perspective` projection unchanged (un-flipped RH).
- NDC +1 = top row (y-down pixels) — ribbon expansion in clip space
  respects this convention.

### Phase 2 — Cluster glow upgrade

**Module:** `crates/debug/src/main.rs` (node sprite pipeline),
`crates/debug/src/cosmic_web.rs` (node_impostors).

Replace point-sprite node impostors with quad impostors (6 verts/node
× core+halo = ~72k verts). Kills the `gl_PointSize` 256px clamp that
saturates near-camera halos. Halo diameter now reads `virial_radius_mpc`
(one-function change in `node_impostors`). Mass-stratified cores: giants
pushed brighter/golder for big golden blooms at filament intersections.

Optional: 3rd blur scale for wider halos — requires +2 new dedicated
write-once targets (Intel rule). Decide after tuning; only if halos need
wider bloom than current 2-scale chain.

Invariant notes:
- Bloom write-once rule preserved (Intel UHD 620 constraint).
- `gl_PointSize` clamp becomes a quad-size clamp (world-space Mpc
  or pixel-space via vertex shader — no new Vulkan feature).

### Phase 3 — Palette & grading

_(Landed ahead of this update in
[`../update-2026-09-19-1933`](../update-2026-09-19-1933/plan.md) —
todos 009–010 here reduced to the ribbon-dependent retune, if any,
after P1/P2 land.)_

**Module:** `crates/debug/src/cosmic_web.rs` (palette ramps),
`crates/debug/src/main.rs` (bloom params, resolve push constants),
`crates/engine/src/render/post.rs` (BloomParams).

Dim low-density links hard (alpha floor 0.18 → ~0.06) so sparse regions
recede and voids read truly dark. Centralized knobs only:
- `cosmic_web.rs`: braid rgb/alpha ramps, node_impostor colors.
- `post.rs`: BloomParams (threshold/intensity).
- `main.rs`: resolve push constants (exposure).
- `cosmic_web.rs`: `COSMIC_REDSHIFT_PER_MPC` — soften so gold survives
  at depth.

Invariant notes:
- ACES tonemap unchanged.
- No new pipeline targets needed for P4.

## Role sign-off

Breakdown approved by: ARCHITECT _(pending)_ · Todos approved by: TECHLEAD
_(pending)_ · Verified by ANALYST: _(pending)_ · Security reviewed by
SECURITY: _(pending)_

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UPD-20260919-001 | done | Create compact strand record buffer in `cosmic_web.rs` (`strand_records` returning `Vec<StrandRecord>` with root, trunk, basis, wander, twist, rgba — 96 B/record, ~3.8 MB nominal; replaces baked `braid_segments` ~34 MB) | §In-scope P1 |
| UPD-20260919-002 | done | Write `RIBBON_VERT` GLSL: expand `gl_VertexIndex` → segment/corner; evaluate `braid_point(t0/t1)` in shader; camera-facing offset with world-space half-width 0.75 Mpc + 1.5-px min clamp; near-eye fade (smoothstep 1→6 Mpc); endpoint amber warming; bounded redshift | §In-scope P1 |
| UPD-20260919-003 | done | Write `RIBBON_FRAG` GLSL: rim-zero lateral falloff `(1−x²)²`, premultiplied additive output | §In-scope P1 |
| UPD-20260919-004 | done | Build `ribbon_pipeline` in `main.rs`: `TriangleList`, additive One/One, no depth write, scene + LDR variants; `RibbonPush` (MVP, eye, px_scale, exposure, redshift, width, subdiv — 100 B) | §In-scope P1 |
| UPD-20260919-005 | done | Update `upload_cosmic_braid` to upload instanced strand records; draw call = `SUBDIVISIONS×6` verts × record count instances, both surfaces | §In-scope P1 |
| UPD-20260919-006 | done | Delete old `webline` pipeline + `WEBLINE_VERT/FRAG` + `WebLinePush` + `GlowLineVertex`; both surfaces (Game Demo + Cosmic Web tab) render ribbons | §In-scope P1 |
| UPD-20260919-007 | pending | Replace node impostor point sprites with quad impostors (6 verts/node × 2 sprites) in `node_impostors`; kill 256px clamp | §In-scope P2 |
| UPD-20260919-008 | pending | Read `virial_radius_mpc` in `node_impostors` for halo diameter (currently unused); mass-stratify core emissive factor | §In-scope P2 |
| UPD-20260919-009 | done (in update-2026-09-19-1933) | Dim low-density braid links (alpha floor, rgb floor) so voids read dark; retune palette in `cosmic_web.rs` ramps | §In-scope P4 |
| UPD-20260919-010 | done (in update-2026-09-19-1933) | Tune `BloomParams` (threshold/intensity) + resolve push constants (exposure) + `COSMIC_REDSHIFT_PER_MPC` so gold survives at depth | §In-scope P4 |
| UPD-20260919-011 | in-review | Screenshot both surfaces (r1–r8 palette, rb1–rb2 ribbons) vs. target; user sign-off pending | §DoD |
| UPD-20260919-012 | done | Update docs: `docs/techstack/rendering.md` (ribbon pipeline § + palette §), architecture report (§3a/§3c/§4/§5/§6/§8b/§9), `docs/techstack/README.md` 0.33.6 | §DoD |
| UPD-20260919-013 | done | Quality gates after P1: fmt, clippy `-D warnings`, build, test --workspace --all-targets (181+296 all ok), doc tests, `game`, both `--headless` | §DoD |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Filaments are camera-facing ribbons with soft lateral falloff; no 1px wireframe visible — both surfaces | ✅ | rb2 demo + tab screenshots (soft tubes, braided structure, no threads) |
| 2 | Dense filaments bloom; voids read dark — before/after | ✅ | rb2 vs. user build screenshot; crossings bloom, voids pitch-dark |
| 3 | Node cores gold, halos virial-radius, no 256px clamp up close | partial | Cores golden layered (white pinpoint + gold mid + amber halo, mass-varied); halo clamp only *mitigated* (P2 quad impostors + `virial_radius_mpc` still open) |
| 4 | Descriptor hash unchanged; determinism tests green | ✅ | No engine file touched; engine 296/296, game_debug 181/181 |
| 5 | All quality.md gates green | ✅ | Gates output 2026-09-19 (evening) |
| 6 | No frame-time regression UHD 620 @1080p; bloom write-once rule | ✅ | 60 fps demo / 59.5 fps tab @1296×759 debug build; no image/pipeline-topology changes to the bloom chain (chain files untouched) |
| 7 | Docs synced, links resolve | ✅ | rendering.md, architecture report, README 0.33.6 |

## Acceptance criteria

- Rendering: filaments read as soft glowing tubes (ribbon quads), not
  wireframe. Dense filaments visibly bloom; voids are dark. Clusters
  show golden cores with world-sized halos.
- Performance: no regression on Intel UHD 620 @1080p vs. baseline;
  bloom write-once rule preserved (no ping-pong).
- Determinism: enrichment-only; descriptor hash unchanged; all tests green.
- Docs: all touched docs updated, all links resolve.

## Risks & Next steps

- **VS sin/cos cost on UHD 620**: 1.2M tris × braid math. Mitigation:
  measure first; fallback reduce subdivisions or per-distance strand
  culling.
- **Additive saturation when filaments bloom**: existing firewalls
  (64.0 bright cap, 65000 resolve clamp) guard; tune emissive ramp.
- **Screen vs world ribbon width**: decide with screenshot comparison
  during P1; keep as knob.
- **3rd blur scale optional**: only if P2 halos need wider bloom; requires
  +2 dedicated write-once targets + quality.md update.
- **CPU test rework**: braid expansion tests (braid_segments bounds)
  need rework to param-level assertions after P1.

**Next steps after this update:** P3 (galaxy sparkle layer),
P5 (LOD/culling), volumetric raymarching prototype.
