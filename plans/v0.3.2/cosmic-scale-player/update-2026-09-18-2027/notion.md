# Update notion — cosmic-scale-player / update-2026-09-18-2027 (UTC)

Parent feature: [`../notion.md`](../notion.md)

## Status

`in-review` (DEV complete 2026-09-18 — gates green, evidence in
`plan.md` DoD table; ANALYST + SECURITY sign-off pending before the
single `v0.3.2` commit)

## Reason for update

Playtest of the shipped Game Demo: click-to-target + `E` fly-to moves the
player, but WASD free flight does nothing visible — the ship is effectively
uncontrollable in space. Investigation (report below) shows input is fully
wired (keys → `held` → `ThrustInput` → `step_free_flight` every tick); the
movement *model* is unusable at cosmic scale:

- Position is f64 **Mpc** (~30 Mpc magnitude at spawn) → one ulp ≈
  7×10⁻¹⁵ Mpc ≈ 200,000 km.
- Thrust is the spec §10 physical **30 m/s²**; near the spawn node the
  occupancy-driven compression ratio sits at ~1–30.
- Per-frame displacement from rest ≈ ½·a·dt² ≈ **10⁻²² Mpc** — thousands of
  times below the ulp, so `pos + δ == pos` bit-exactly. Velocity must climb
  to ~10⁸ m/s (days of holding W) before the position register can change.
- Even at the web's maximum depth, a minute of thrust displaces ~10⁻¹³ Mpc
  — invisible on a tens-of-Mpc view.
- Fly-to "works" only because it writes absolute eased positions per frame.
- Secondary gap: the demo HUD shows frame/time/target but **no speed
  readout**, so the player gets zero feedback even while velocity silently
  accumulates.
- Latent bug exposed: `FlyToExec` hand-back velocity is Mpc/**sim-s**, but
  the free-flight path treats it as Mpc/**Gyr** (×3.156×10¹⁶ mismatch,
  currently masked by the ulp wall).

The spec §2 "free flight with real physical momentum" + §10 30 m/s²
envelope cannot be felt at Mpc scale. This update replaces raw physical
thrust at `FrameId::Cosmological` with a **scale-relative cruise law**
(real-time screen-speed authority, depth-scaled, momentum feel via easing)
and pins the velocity-unit contract.

## Roles

Author: PO. Consulted: UX (player-facing control change — cruise feel +
HUD feedback), ARCHITECT (movement-model change, velocity-unit pin),
TECHLEAD (todos). ANALYST + SECURITY sign off at `in-review` per the
lifecycle gates.

## Scope

### In-scope

- Cruise controller at Cosmological: depth-scaled target velocity, real-time
  easing (τ ≈ 0.75 s), coast on release, exact position integration well
  above the ulp wall (`CosmicPlayerState::step_free` cruise branch,
  `debug` crate only).
- Velocity-unit pin: `ship.vel` at Cosmological := Mpc per **sim-second**,
  consistent with `FlyToExec` hand-back, HUD, persistence.
- `Shift`+wheel cruise-speed adjust (`t_cross`), hint-line + Controls
  registry update, live speed HUD row in the demo.
- Tests: cruise reach/coast/replay, 5-second visible-motion regression,
  depth-scaling monotonicity, cancel hand-back unit continuity; delete the
  two tests that pin the removed physical-thrust behavior.
- Docs: `controls.md` demo paragraph, ADR-023 amendment note,
  `techstack/README.md` version bump, stale `engine::flight` time-unit
  doc comments (docs-only).

### Out-of-scope

- `engine::flight` behavior (fly-to easing, `step_free_flight` for SI-time
  frames), compression-clock law, mouse-steer law, camera modes, marker
  contract, sphere-walker (`game::player`), inspector tab, gravity, ship
  mesh, collision, propellant.

## Requirements delta

- Boot: unchanged (spawn, camera framing, reseed, determinism).
- Flight: WASD/arrows command body-axis cruise direction; dropout coast
  preserves momentum (damping OFF per spec §10, `S` is the brake);
  `Shift`+wheel adjusts `t_cross` (notch ×1.5, clamped [2, 600] s).
- Motion is real-time-anchored and depth-scaled: near-node ≈ precision
  speeds (r_vir-scaled floor), mid-web ≈ ~Mpc/s class; visibly moving
  within seconds at any depth.
- Fly-to: engage/cancel/arrival semantics unchanged; cancel hands back a
  unit-consistent velocity (eased Mpc/sim-s continues seamlessly under
  cruise).
- HUD: demo shows a live speed row (`Mpc/s · cruise <t_cross>s`); frame /
  time / target / SOI lines unchanged.
- Settings → Controls lists the cruise-speed action; registry grows by
  exactly one action and is re-pinned (the v0.3.2 notion rule).

## Definition of Done delta

- [ ] WASD displaces the ship screen-visibly (≫ ulp) within 5 s real time
      at spawn — pinned by a headless regression test.
- [ ] Cruise speed is depth-scaled (nearest-node monotone, r_vir-scaled
      floor near nodes) and `Shift`+wheel-adjustable — pinned by tests.
- [ ] Release coasts with preserved momentum; fixed-step replay is
      bit-identical.
- [ ] Fly-to engage/cancel/arrival unchanged (existing tests green);
      cancel hand-back velocity is unit-consistent (pinned test).
- [ ] Demo HUD shows a live speed row; hint line + Controls docs updated.
- [ ] Removed-behavior tests deleted (`thrust_matches_the_frame_unit_convention`,
      `single_step_matches_fine_substeps`); no other test regressions.
- [ ] Quality gates green (`fmt --check`; `clippy --workspace --all-targets
      --all-features -- -D warnings`; `build --workspace`;
      `test --workspace --all-targets`; `test --doc --workspace`;
      `game_debug --headless`; `game_tools --headless --tier low`).
- [ ] Docs sweep landed (`controls.md`, ADR-023 amendment, techstack
      version bump); parent files untouched, cross-linked; one commit on
      branch `v0.3.2` when done.
