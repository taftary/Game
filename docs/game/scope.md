# Scope

## v1 in

- Procedural galaxy → systems → planets with deterministic seeds.
- Galaxy/system/planet map + travel.
- Orbit → atmosphere → sky → soil descent and ascent.
- On-foot (or rover-equivalent) surface exploration on one planet at a time.
- Colony placement + robot workers + resource extraction loop.
- Single-player only.
- Touch + mouse/keyboard/gamepad input.
- Custom Vulkan (`vulkano`) renderer, low + high quality tiers.

## v1 out

- Multiplayer / co-op.
- VR rendering and VR input.
- Story campaign, voice acting, complex NPCs.
- Modding API.
- Consoles.
- Traversable universe layer above the galaxy (Level 1 is backdrop only).
- Landable moons (Level 4 companions are visual-only in v1).
- Subterranean geometry of any kind (caves stay surface markers —
  [`journey.md`](journey.md) Level 8).

## VR future-proofing (no implementation in v1)

- Keep world units in meters, camera decoupled from logic.
- No renderer design that assumes a single 2D swapchain (stereo must be addable).
- Input abstracted so a future XR backend can inject poses/actions.
- No UI that only works as flat screen-space overlay; prefer world-space-capable UI primitives.

## Platforms

| Context | OS | Notes |
|---|---|---|
| Dev / debug | Linux, Windows, macOS | All three must build the workspace; CI-less, so contributors run gates locally |
| v1 ship | Android, iOS / iPadOS, Windows desktop, Linux desktop (best-effort), macOS desktop | Phones and tablets are first-class, not ports |
| Future | VR headsets (Quest-class + PCVR-class) | Design only; no v1 code |

Mobile constraints that drive the game (full technical list in [`../techstack/rendering.md`](../techstack/rendering.md)
and [`../techstack/quality.md`](../techstack/quality.md)):

- Touch-first UI; small text and hover-only affordances are bugs.
- Install size and asset streaming matter; procedural > downloaded bulk.
- Backgrounding, suspend/resume, and orientation changes must not corrupt saves.

Desktop gets: higher draw distances, denser terrain, higher shadow/resolution tiers — same saves, same logic.
