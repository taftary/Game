# Update plan — cosmic-scale-player / update-2026-09-19-1933 (UTC)

Parent update notion: [`notion.md`](notion.md)

## Feature delta breakdown

### Phase 1 — Palette knobs (code)

**Module:** `crates/debug/src/cosmic_web.rs`,
`crates/debug/src/main.rs`.

- Braid ramps (`braid_segments`): alpha `0.18 + 0.40d` →
  `0.05 + 0.95d`; rgb `[0.40+0.32d, 0.42+0.36d, 0.85+0.20d]` →
  `[0.18+0.30d, 0.22+0.35d, 0.60+1.35d]` (dense blue premult ≈ 1.95,
  well past bloom threshold 1.0 — the blur chain keeps only ~1/4 of a
  1-px line's margin, so "just above threshold" is invisible; faint
  link ≈ backdrop). All channels monotonic in density and ≤ 2.0
  (test bands).
- Shared `mass_level(mass)` ramp added (1e12 → 0, ~3e14 → 1,
  re-centered from 1e12.7–1e15.4) and used by `node_color`,
  `node_size_px`, `node_impostors` — ordinary cluster hubs read
  golden, not just the rarest giants.
- `node_color`: gold end `[1.00, 0.93, 0.72]` → `[1.00, 0.88, 0.62]`.
- `node_impostors`: core emissive `1.2 + 1.3l` → `1.5 + 3.5l` (max
  channel 5.0 = test band max); core size `2.5 + 3.5l` → `3.0 + 9.0l`
  px (the half-res chain dilutes point sources ~1/(2πσ²) — only large
  bright cores survive as visible golden blooms); halo cyan →
  warm-graded by mass, diameter `2 + 6l` → `3 + 5l` (pinned 2–8 Mpc
  band kept), alpha `0.10` → `0.14 + 0.10l`.
- `glow_point_cloud`: alpha 0.12 → 0.09 (void darkness headroom).
- `COSMIC_REDSHIFT_PER_MPC` 0.004 → 0.002 (saturation 125 → 250 Mpc).
- `GLOW_VERT` / `WEBLINE_VERT` tint softening:
  `1.0 + 0.9z` → `1.0 + 0.55z`; `1/(1 + 1.2z)` → `1/(1 + 0.7z)`;
  dim `1/(1 + 0.8z)` → `1/(1 + 0.45z)`. Safety clamps
  (`max(clip.w, 0.0)`, z-cap 0.5) unchanged (string-pinned tests).
- `COSMIC_BACKDROP` `[0.012, 0.008, 0.030]` → `[0.008, 0.005, 0.024]`.
- **Per-surface grade** (added during iteration — the inspector's
  zoomed-out view stacks ~50 strands/px vs. the demo's few; one grade
  whited-out the inspector): new `WebLinePush.exposure` field scales
  braid alpha in-shader; `GlowPush.exposure` (pre-existing) and the
  resolve push take per-surface values from bin-local consts —
  `COSMIC_DEMO_*` (line 1.0 / glow 1.0 / exposure 1.15 / intensity
  2.2) vs `COSMIC_MAP_*` (0.14 / 0.3 / 0.85 / 1.2). Engine
  `BloomParams::spec_defaults()` (threshold 1.0, blur σ) untouched.

### Phase 1b — Second pass: gas, red areas, halo blob (user feedback)

User screenshot review ("check the lines, the gaz part and the red
area") vs. target drove three more knob moves, same constraints:

- **Gas veil**: `glow_point_cloud` reworked from fixed-pixel specks
  (1.5 px, α 0.09, kind 0) to world-sized soft sprites (2.8 Mpc, α
  0.045, kind 1) hugging the links — filaments sit in faint blue mist
  like the target; voids untouched (glow emits along links only).
  Overdraw measured fine: 60 fps / 16.7 ms avg on UHD 620.
- **Red/warm areas**: braid strands warm toward amber `[1.05, 0.72,
  0.42]` near hub endpoints (convex mix by `1 − √taper`; monotonic
  density grading and the ≤ 2.0 band preserved); giant halos go
  cyan→amber (`[1.0, 0.63, 0.50]` at l = 1); giant cores deepened to
  `[1.00, 0.82, 0.50]`; redshift red boost 0.55z → 0.75z (blue kill
  unchanged) so distant structures warm like the target's pink-tinged
  far filaments.
- **Halo clamp blob** (grey smudge near the player visible in the user
  screenshot = the known 256 px `gl_PointSize` clamp): halo diameter
  band narrowed 3–8 → 2.5–6 Mpc, alpha 0.14+0.10l → 0.12+0.08l —
  smaller, dimmer smudge. Real fix stays in the deferred ribbon
  update (quad impostors).

### Phase 2 — Visual iteration

Build, run the viewer, capture both surfaces (Game Demo default tab,
then Cosmic Web tab via `1`), compare against the target reference,
retune the consts above (2–3 rounds expected).

### Phase 3 — Gates + docs

Enrichment test check, full `quality.md` gates, then docs:
`rendering.md` cosmic palette values, architecture report refresh,
`docs/techstack/README.md` 0.33.4 → 0.33.5, and the
`update-2026-09-19-1245` draft (palette phase landed here; ribbon
width = world-space Mpc + min-px clamp recorded; premature `done`
todo statuses reset).

## Role sign-off

Breakdown approved by: ARCHITECT _(done 2026-09-19 — no pipeline,
topology, image, or invariant changes; additive blend, projection,
write-once bloom targets all untouched)_ · Todos approved by: TECHLEAD
_(done 2026-09-19 — zero budget delta: same vertex counts, draws,
passes; knobs only)_ · Verified by ANALYST: _(pending)_ · Security
reviewed by SECURITY: _(pending)_

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| UPD-20260919-1933-001 | done | Create update folder (notion + plan); update the 1245 draft (palette landed here, ribbon width decision, status reset) | §Scope |
| UPD-20260919-1933-002 | done | Regrade braid palette ramps (alpha floor 0.05, blue premult ≈1.95) in `cosmic_web.rs` | §In-scope |
| UPD-20260919-1933-003 | done | Regrade `node_color` + `node_impostors` (shared `mass_level` re-centered, golden cores to 5.0, warm halos) + `glow_point_cloud` alpha | §In-scope |
| UPD-20260919-1933-004 | done | Soften redshift (`COSMIC_REDSHIFT_PER_MPC` 0.002, tint `0.55/0.7/0.45`) keeping safety clamps | §In-scope |
| UPD-20260919-1933-005 | done | Deepen `COSMIC_BACKDROP`; per-surface grade consts + `WebLinePush.exposure` field | §In-scope |
| UPD-20260919-1933-006 | in-review | Screenshot both surfaces vs. target; 8 tuning rounds (r1–r8 captures); **user visual sign-off pending** | DoD 1 |
| UPD-20260919-1933-007 | done | Enrichment + engine determinism tests green (13/13 cosmic_web, 181/181 game_debug, 296/296 engine); no pins needed repair | DoD 2, 3 |
| UPD-20260919-1933-008 | done | Full quality gates green (fmt, clippy `-D warnings`, build, test, doc test, `game`, both `--headless`) — re-run after Phase 1b | DoD 4 |
| UPD-20260919-1933-009 | done | Docs: `rendering.md` palette-pass §, architecture report values, techstack README 0.33.5 | DoD 5 |
| UPD-20260919-1933-010 | done | Phase 1b (user feedback): gas veil (world-sized glow sprites), red/warm areas (endpoint amber, amber halos, deeper gold, 0.75z red), halo-clamp blob mitigation | §In-scope |

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence |
|-------|----------------------------|--------|----------|
| 1 | Before/after screenshots, both surfaces, user sign-off | partial | r1–r8 captures (before = user-provided build screenshot). r6: golden hubs + blue-violet glowing filaments + dark voids ("a lot better" — user). r7/r8 (feedback pass): gas-veil mist on filaments, amber/red hub accents, halo-blob smudge reduced; inspector balanced. **Sign-off pending** |
| 2 | Enrichment tests green (bands respected) | ✅ | `cargo test -p game_debug --lib cosmic_web`: 13/13 ok; full lib 181/181 |
| 3 | Descriptor hash unchanged; determinism green | ✅ | No engine file touched (`git diff --stat`: cosmic_web.rs + main.rs only); engine suite 296/296 |
| 4 | All quality.md gates green | ✅ | fmt / clippy -D warnings / build / test --workspace --all-targets / doc tests / `cargo run --bin game` / `game_debug --headless` / `game_tools --headless --tier low` all ok 2026-09-19 |
| 5 | Docs synced | ✅ | `rendering.md` palette-pass paragraph, architecture report §3/§4/§5/§6/§8b values, techstack README 0.33.5; 1245 draft cross-links resolve |

## Acceptance criteria

- Visual: dark voids, blue-violet filaments that bloom at
  crossings/dense links, golden hubs that survive depth — on both
  cosmic surfaces, per user sign-off.
- Technical: same geometry/draws/passes as baseline (zero perf delta);
  bloom write-once rule trivially preserved (no image changes);
  descriptor hash untouched.

## Risks & Next steps

- **Over-bloom on dense tangles**: blue premult 1.07/strand sums fast
  at crossings; firewalls (bright cap 64, resolve clamp 65000) hold;
  retune ramp ceiling if screenshots blow out.
- **1-px spines remain**: palette pass cannot add physical thickness —
  expected; ribbon update (`update-2026-09-19-1245`) is the follow-up
  with width = world-space Mpc + min-px clamp (PO decision
  2026-09-19).
- **Golden-core vs. redshift balance**: softening redshift too far
  kills the depth cue; 0.002 keeps saturation at 250 Mpc (half the
  visible box) — verify in screenshots.

**Next steps after this update:** ribbon update
(`update-2026-09-19-1245`, P1+P2 remaining), then P3 sparkle / P5 LOD.

**Evening follow-up:** user rejected the graded wireframe ("lines
dominant, one flat gold, lines not good at all") → P1 ribbons
executed against the 1245 draft (instanced `TriangleList` ribbon
pipeline, 3-layer node light, `WebLinePush` deleted; rb1–rb2
screenshots). This update's palette work (ramps, gas veil, per-surface
grades, redshift) carries over unchanged under the ribbons.
