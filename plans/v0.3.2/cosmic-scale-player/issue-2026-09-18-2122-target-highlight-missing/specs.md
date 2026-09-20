# Issue specs — cosmic-scale-player / issue-2026-09-18-2122-target-highlight-missing (UTC)

Parent feature: [`../notion.md`](../notion.md) · Plan: [`../plan.md`](../plan.md)

## Status

`planned`

## Roles

Reported by (role: player/playtest): clicking a target in the Game Demo
gives no in-world highlight. Priority set by PO: **fix on `v0.3.2`** —
the shipped behavior falls short of the spec the feature was accepted
against. UX consulted (player-facing): amber ring, both cosmic views.

## Observed vs Expected

- Observed: in the Game Demo tab (`F1`), left-clicking a web node
  selects it (`Target node {i} · [E] fly-to` notice pill + HUD `target:`
  line while a fly-to leg is live) but draws **no marker on the node**
  in the 3D viewport. On the Cosmic Web dimension tab, clicking a node
  updates only the right-dock SELECTION readout — again no in-viewport
  marker.
- Expected: [`../notion.md`](../notion.md) functional requirements
  ("Fly-to: left-click selects the nearest node within 8 px (**highlight
  + target line**)") and [`../plan.md`](../plan.md) UX-3 ("Clicking a
  node shows a **target affordance**").

## Reproduction steps

1. `cargo run -p game_debug`, land on GAME DEMO (`F1`).
2. Left-click a bright node. Notice pill appears; HUD stays `target: —`
   until `E`.
3. No ring, outline, or marker appears on the clicked node at any point
   (before, during, or after `E` fly-to).
4. `F2` → Cosmic Web tab → click a node: SELECTION readout updates, no
   viewport marker.

## Scope & Impact

- Scope: overlay-only (UI-pass rects); picking, flight, and camera code
  untouched. Two draw sites: Game Demo overlay block and Cosmic Web
  tab UI builder, both in `crates/debug/src/main.rs`.
- Impact: closes the UX-3 spec gap; zero behavior change to selection,
  fly-to, or rendering pipelines. No new draw calls (UI pass only),
  no budget risk (TECHLEAD: +8 quads worst case per frame).

## Logs / Evidence

- `CosmicDemoState::select_node_at` sets `player.target_node`
  (`crates/debug/src/cosmic_demo.rs:231`) and the demo's `draw_main`
  branch draws links + point cloud only (`main.rs:5952–6046`) — nothing
  keyed to `target_node`.
- `build_demo_ui` (`main.rs:1215`) draws dock text only; the Game Demo
  overlay block (`main.rs:2678–2691`) draws the player YOU marker only.
- `CosmicWebInspector::select_at` sets `selected`
  (`crates/debug/src/cosmic_web.rs:63`); `build_cosmic_web_ui`
  (`main.rs:1272–1411`) draws docks only — no viewport ring (galaxy /
  system tabs both have one, `main.rs:1934–1978`, `2251–2306`).

## Suspected area

Missing overlay draws, not broken picking: the selection state
(`target_node` / `selected`) is correct and tested; the UI pass never
reads it for a marker. Fix = project the stored node through the
owning camera and draw the map-tab-style ring.
