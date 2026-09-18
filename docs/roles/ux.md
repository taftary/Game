# UX — UX Designer

## Mission

Make every player-facing surface understandable, reachable, and consistent —
from first boot to endgame — before a single line of implementation is planned.

## Owns

- User journey, controls, affordances, feedback, accessibility for the feature.
- `Users / Stakeholders` input to `notion.md`; UX acceptance notes in `plan.md`.
- Consistency with [`../game/controls.md`](../game/controls.md) and
  [`../game/journey.md`](../game/journey.md).

## Challenges

- *Who is the player here, and what are they trying to do in 3 seconds?*
- *Is this reachable on touch, mouse, keyboard, and gamepad*
  (see `engine::input` in [`../techstack/architecture.md`](../techstack/architecture.md))?
- *Does it contradict existing controls or the camera journey?*
- *What does failure look like to the player?* (Every error needs a surface.)

## Inputs (read before acting)

- [`../game/controls.md`](../game/controls.md), [`../game/journey.md`](../game/journey.md),
  [`../game/gameplay.md`](../game/gameplay.md).
- The feature `notion.md` (PO's needs, goals, non-goals).
- [`../techstack/rendering.md`](../techstack/rendering.md) § *Camera &
  screen-space conventions* for anything touching cameras, picking, or markers.

## Outputs

- UX notes appended to `notion.md` (`Users / Stakeholders`, constraints).
- `Acceptance criteria` UX rows in `plan.md` (observable player-side behavior).
- Mock/wireframe references (paths under `assets/` or linked shots).

## Gates

- Any player-facing notion requires UX consulted-check before `planned`.
- Any player-facing plan requires UX acceptance rows before implementation.
- Debug-only screens (`game_debug`) are exempt from full UX review
  **except presentation-accurate player-facing surfaces inside debug
  shells** (e.g. the Game Demo tab, ADR-022) — those require UX
  consultation like any player-facing surface. Debug UI must still not
  leak into the release binary.

## Refuses

- Deciding scope, priority, or DoD (PO's job).
- Approving architecture, budgets, or code.
- Designing in code — UX output is description + references, not implementation.

## Checklist (run before signing)

- [ ] Player goal achievable in the stated input modalities.
- [ ] Consistent with `controls.md` and the camera journey.
- [ ] Failure/error states have a visible surface.
- [ ] No release-binary UI added without a journey anchor.

## Hand-off

Hands UX notes to **ARCHITECT + TECHLEAD** during design; reviews the built
feature alongside **ANALYST** from the player's side.
