# Content retirement map

This map records where guidance from deleted standalone documents now lives in
the consolidated Rust handbook. It is intended to prevent lost decisions during
the documentation restructuring.

## Deleted documents and their successors

| Deleted document | New home | Notes |
| --- | --- | --- |
| `docs/ARCHITECTURE.md` | `docs/book/architecture/` (`workspace.md`, `crate-boundaries.md`, `technology.md`, `migration-principles.md`) | The target workspace and crate boundaries are now authoritative. Old baseline-specific module breakdowns were dropped because they described the single-package layout, not the migration target. |
| `docs/technology-definition.md` | `docs/book/architecture/technology.md` | Technology decisions were merged into the target/decision format. |
| `docs/project-structure-and-best-practices/01-basic-project-layout.md` | `docs/book/architecture/workspace.md` and `docs/book/practices/project-structure.md` | Basic layout guidance is now part of the target architecture and practices chapters. |
| `docs/project-structure-and-best-practices/02-modules.md` | `docs/book/practices/project-structure.md` and `docs/book/architecture/crate-boundaries.md` | Module organization guidance moved into practices and crate-boundary pages. |
| `docs/project-structure-and-best-practices/03-library-vs-binary.md` | `docs/book/architecture/crate-boundaries.md` | Library/binary separation is now described in crate boundaries. |
| `docs/project-structure-and-best-practices/04-folder-organization.md` | `docs/book/practices/project-structure.md` | Folder conventions merged into the project-structure practice page. |
| `docs/project-structure-and-best-practices/05-clean-code-practices.md` | `docs/book/practices/` (multiple pages) and `docs/STYLEGUIDE.md` | Clean-code guidance distributed across practices and the style guide. |
| `docs/project-structure-and-best-practices/06-error-handling-and-logging.md` | `docs/book/practices/error-handling.md` | Error-handling content moved here. |
| `docs/project-structure-and-best-practices/07-dependency-management.md` | `docs/book/practices/dependencies.md` | Dependency guidance moved here. |
| `docs/project-structure-and-best-practices/08-example-scalable-project.md` | `docs/book/architecture/workspace.md` | Scalable-project example replaced by the target workspace. |
| `docs/project-structure-and-best-practices/README.md` | `docs/book/index.md` and `docs/CONTRIBUTING.md` | Entry-level guidance moved to the book introduction and contribution guide. |
| `docs/classes-definitions/ARCHITECTURE.md` | Retired | Its generic workspace/crate breakdown was superseded by `docs/book/architecture/`. Its future-extension ideas (networking, scripting, editor GUI, hot-reload, plugins) were intentionally dropped until requirements exist. |

## What was intentionally dropped

- **Old single-package module breakdowns.** Documents that described the
  pre-migration `src/` layout were not preserved because the migration target is
  a workspace with `crates/engine`, `crates/game`, and `crates/tools`.
- **Generic engine subsystem placeholders.** ECS, physics, assets, and audio
  subsystem layouts from the old generic architecture document were dropped
  until requirements and constraints are known. Future decisions will be
  recorded as ADRs under `docs/book/architecture/decisions/` via the ADR
  template.

## Verification

If you cannot find guidance that used to be in one of the deleted documents,
check the corresponding new home above. If it is genuinely missing, open a
follow-up change rather than re-creating the old standalone file.

## 2026-09-14 ADR clearing (fresh get-started)

- `docs/book/architecture/decisions/vulkan-crate.md`,
  `window-lifecycle.md`, `ecs-adoption.md`, `physics-strategy.md`,
  `audio-strategy.md`, `persistence-strategy.md`,
  `telemetry-and-adapters.md`, `networking-strategy.md`,
  `uv-coordinates-on-node.md`, `uv-unfold-for-arbitrary-assemblies.md`,
  `reusable-vertex-buffers.md` - deleted. Old design records from a previous
  project iteration (Vulkan ownership via `render::run`, `winit`
  `ApplicationHandler` lifecycle with `Scenario`/`PlanetConfig`,
  `build_icosphere`/`split_node`/`unsplit_nodes` UV storage,
  `unfold_uvs`/`NodeRef` topology assumptions, `VertexBuffer`/`set_scene`
  reuse, and open ECS/physics/audio/persistence/telemetry/networking
  strategies). They described unimplemented APIs as proven baseline and
  referenced a `plan/RELATED.md` that does not exist.
  Replacement: none yet. `docs/book/architecture/decisions/index.md` is now a
  get-started placeholder ("no decision records exist yet") and
  `docs/book/architecture/decisions/template.md` remains for future proposals.
  `docs/book/SUMMARY.md` and `docs/book/architecture/technology.md` no longer
  link to the deleted records.

## 2026-09-14 module clearing

- `docs/book-output/` (including `specs/*.html`) - deleted local build output.
  Gitignored generated HTML with no `docs/book/specs/` source. Regenerate with
  `docs/scripts/check-book.sh` once the book has a specs chapter.
- `docs/examples/src/*.rs`, `docs/examples/examples/*.rs`,
  `docs/examples/Cargo.toml` - cleared. Dangling imports of
  `planet_crafter_engine` (`node`, `scene`, `text`, `render`, `runtime`) and
  `workspace.*` inheritance with no root `Cargo.toml` and no `crates/engine`
  on disk. Backed up under `Temp/opencode/Game-backup-examples/`.
  Replacement: Planned examples package on re-scaffold.
- Stale implemented-state claims for engine modules (`node`, `scene`, `text`,
  `render`, `runtime`, `lod`, `visibility`), `tests/` targets,
  `crates/engine/src/render/shaders.rs`, `crates/engine/src/render/buffers.rs`,
  `assets/fonts`, and `.cargo/config.toml` - reworded to `Target`/`Planned`
  in `AGENTS.md`, `README.md`, `docs/book/architecture/workspace.md`,
  `docs/book/practices/testing.md`, and `docs/book/examples/index.md`.
  Replacement: Target workspace layout in `AGENTS.md`.
