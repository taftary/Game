# Plan — waypoint-transitions

Parent notion: [`notion.md`](notion.md)

## Feature breakdown

### Phase 1 — Waypoint model and descriptors

Owning module: `engine::waypoints`, with `FrameChain` queried but not
redefined. `WaypointId` represents the ten player-facing dimensions while a
deterministic mapping combines the active `FrameId` with planetocentric
altitude. `WaypointLeg` identifies the nine directed boundaries.

`TransitionDescriptor` is pure data and interpolation. It supplies exposure
keys, cue weights, zodiacal intensity, atmospheric parameters, and Sun disk
scale for a leg. Manual frame commits and fly-to progress use the same
descriptor path. The 10 -> 9 leg uses explicit synthetic interior keys; it
does not pretend facility geometry exists.

Invariant: the mapping never changes ENU coordinates, frame commits, camera
projection, or picking conventions. Equal inputs produce equal descriptors.

### Phase 2 — Atmospheric rendering and source terms

Owning modules: `engine::render::atmosphere` and existing pure cue helpers.
Add validated atmospheric parameters for altitude sky color, Rayleigh/Mie
scattering, aerial haze, limb glow, and the dual 80 km / 100 km Karman
reference. Add a descriptor-driven shell shader and tier-gated cheap path.
The pass uses the existing un-flipped DirectX projection and CCW/back-face
scene convention; fullscreen resolve exceptions remain local to post.

The source-term API remains headless-testable. GPU plumbing stays in `render`
and the existing tools/debug binaries; `game` receives no Vulkan dependency.

### Phase 3 — Events and debug surface

Owning modules: `engine::waypoints` event queue and `game_debug` transitions
panel. `TransitionEvent` is a small drainable value containing leg, waypoint
ends, progress, simulation time, and lifecycle. It is compatible with the
frozen `FrameTransition`/ADR-004 trigger boundary without implementing save
file I/O. The panel displays current descriptor weights, ETA when supplied,
and a bounded timestamped event log.

### Phase 4 — Evidence and lifecycle

Add engine unit/doc tests for all public pure APIs and a
`game_tools --headless` scripted nine-leg sweep. Each leg prints a stable
`waypoint_leg=... pass=true` result. Add the 6 -> 5 continuity check, the
10 -> 9 parameter-level exposure check, and atmosphere source-term checks.
The windowed debug path supplies the manual shell/panel captures required by
the notion.

## Todo

| ID | Status | Task | Ref notion § |
|----|--------|------|--------------|
| WPT-001 | done | Define `WaypointId`, `WaypointLeg`, altitude bands, and deterministic `FrameChain` mapping in `engine::waypoints` | Functional requirements |
| WPT-002 | done | Implement per-leg descriptors and interpolation for exposure, cue, zodiacal, atmosphere, and Sun-disk inputs | Goals, Functional requirements |
| WPT-003 | done | Add validated atmosphere source terms, Karman dual reference, and GPU shell shader helpers with Low-tier fallback | Goals, Functional requirements, NFR |
| WPT-004 | done | Add bounded `TransitionEvent` queue and lifecycle emission for manual/fly-to leg updates | Functional requirements |
| WPT-005 | done | Add the feature-owned debug transitions panel and event log surface | Definition of Done |
| WPT-006 | done | Add deterministic headless nine-leg harness, exposure handoff, atmosphere, and event assertions | Definition of Done |
| WPT-007 | done | Update rendering/architecture/version/milestone documentation and complete quality gates | NFR, Constraints |

Gate commands: `cargo fmt --check`, `cargo clippy --workspace
--all-targets --all-features -- -D warnings`, `cargo build --workspace`,
`cargo test --workspace --all-targets`, `cargo test --doc --workspace`,
`cargo run --bin game`, `cargo run -p game_debug -- --headless`, and
`cargo run -p game_tools -- --headless --tier low`.

## Role sign-off

Breakdown approved by: ARCHITECT _(signed 2026-09-17 — the waypoint model is
an additive pure engine boundary; render remains the only Vulkan owner; debug
is a consumer; existing projection, ENU, determinism, and save-shape
invariants are preserved; no locked choice is changed)_ · Todos approved by:
TECHLEAD _(signed 2026-09-17 — risk-first order puts the pure model and
descriptor continuity before GPU/debug surfaces; Low-tier fallback and draw
budget are explicit; every todo has a named headless or unit-test outcome)_ ·
UX acceptance rows: _(signed 2026-09-17 — no new player input; manual flight
and select-to-focus share pacing; transitions expose no visible exposure pop;
debug panel gives leg/progress/event feedback; 10 -> 9 is clearly a
  parameter-level developer surface)_ · DoD verified by: ANALYST _(signed
2026-09-17 — all five criteria re-checked against engine tests, headless
evidence, the debug transitions tab, and the workspace gates)_ · Security
reviewed by: SECURITY _(signed 2026-09-17 — no network, save, or untrusted
asset surface; event queue is bounded; numeric inputs are clamped; no new
unsafe code or dependency; debug UI remains non-release; no findings)_.

## DoD verification

| DoD # | Criterion (from notion.md) | Status | Evidence | Verified by |
|-------|----------------------------|--------|----------|-------------|
| 1 | All nine legs demonstrable with per-leg evidence | done | `waypoint_leg=10...2 ... pass=true` plus `waypoint_summary=legs=9 events=18 ... pass=true` from the Low-tier headless harness | ANALYST (signed 2026-09-17) |
| 2 | 6 -> 5 exposure handoff smooth | done | `exposure_handoff=switches=2 final=Starlight max_step=0.0101 pass=true` plus existing `ExposureLoop` continuity tests | ANALYST (signed 2026-09-17) |
| 3 | 10 -> 9 parameter-level exposure cut | done | `waypoint_leg=10(interior->exterior) ... pass=true`; descriptor clamping and finite-key test | ANALYST (signed 2026-09-17) |
| 4 | Atmospheric GPU shell evidence | done | atmosphere sky/haze/limb tests, Karman constants, descriptor-driven `ATMOSPHERE_SHELL_FRAG`, and Low-tier source-term evidence | ANALYST (signed 2026-09-17) |
| 5 | Events visible and drainable | done | bounded queue test, 18 headless lifecycle events, and `Transitions` tools tab in `game_debug` | ANALYST (signed 2026-09-17) |

## Acceptance criteria

- Same `FrameChain` state and progress always produce byte-stable descriptor
  values; no random or wall-clock inputs enter the engine module.
- A leg boundary replaces cue regime state before applying the new descriptor;
  no previous-leg cue weight survives the transition.
- Atmosphere uses the existing DirectX projection contract: NDC `+1` is the
  top row, no Y flip is introduced, and scene winding remains CCW with back
  culling.
- Low tier uses bounded shell work and a cheap approximation rather than
  silently adding a full-resolution volumetric pass.
- Event queues are bounded, drainable, and contain no file/network payloads.
- Debug-only UI remains in `game_debug` and is not added to the release
  binary.

## Risks & Next steps

- The shell is the largest GPU risk. Keep the analytic shell bounded and use
  the Low-tier approximation if the quality budget is exceeded.
- Existing binaries do not run the flight engine tick loop. The headless
  scripted harness is the accepted integration driver until the gameplay
  descent slice owns that loop.
- ADR-004 remains draft. The queue is intentionally an emitter boundary, not
  an autosave implementation.
- Future `scale-debug-screens` can consume the same event queue and absorb the
  panel without changing the event shape.
