# Update notion — sphere-uv-debug / update-2026-09-15-0730 (UTC)

Parent feature: [`../notion.md`](../notion.md) (`done` — cross-linked, untouched)

## Status

`done` (plan: [`plan.md`](plan.md) — all DoD criteria checked)

## Reason for update

Flat UV view after the corner-collapse fix (issue-2026-09-15-0706) is
topologically correct but still unreadable at cuts: dual centers on icosa
base edges/vertices span ≥2 islands, and per-vertex UVs force their fan
triangles to stretch across the net (red bands + wire streaks in the user
screenshot). User selected option C: per-island vertex duplication (proper
unwrap semantics) over hiding or collapsing seams.

## Scope

### In-scope

- Engine `render/uv.rs`: `FlatUnwrap` builder — expanded uv/island/seam
  arrays plus a dedicated flat-view index buffer where every emitted
  triangle has all 3 vertices in one island; duplicate vertices keyed
  `(source, island)`, deterministic; non-seam cells keep original fan
  indices byte-equal.
- Engine `render/uv.rs`: UV wireframe clipper — cross-island segments
  clipped per side to its island triangle (wires break at cuts).
- Debug wiring: `SphereViewerState.flat`, flat upload via `source` map,
  flat draw binds `flat_indices` (no longer shares `fill_indices`),
  headless `uv_flat_tris=` line. Debug `build_wireframe_uv` removed in
  favor of the engine clipper.
- Docs: rendering.md UV paragraph, techstack version bump.

### Out-of-scope

- 3D sphere view (keeps single-UV stretch = honest unwrap visualization).
- Engine `to_indexed_mesh`, `PlanetVertex`, `mesh_hash`, `game_tools`.
- Shader/panel/UI changes (Seams overlay stays default ON; now paints
  island-local cut-cell portions only).

## Requirements delta

- Every flat-view fan triangle references exactly one island (no cross-net
  stretch, proven by test).
- UV wireframe never crosses an island cut (clipped segments, proven by
  test).
- 3D buffers and engine parity tests unchanged and green.

## Definition of Done delta

- [ ] Core invariant test: all flat tris single-island; all uv ∈ [0,1]²;
  duplicates' `source` valid; deterministic rebuild.
- [ ] Wire clip tests: endpoints inside own island (barycentric ≥ −ε);
  segment count == raw edge count; determinism.
- [ ] Non-seam indices byte-equal to original fill fan.
- [ ] Headless prints `uv_flat_tris=`; all quality gates green;
  docs bumped; parent stays `done`.
