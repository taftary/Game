# Assets and content pipeline

- Formats: `glTF` meshes, `KTX2`/Basis-compressed textures, Ogg Opus audio (confirmed by ADR-006 in [`../decisions/`](../decisions/)). Sources (`.blend`, `.png/.wav`) never ship.
- `assets/` holds source + cooked manifests; cooker lives in `crates/tools`.
- Mobile texture ceiling: 2k max, ASTC/ETC2 via Basis; desktop may use 4k.
- Fonts: bundled under `assets/fonts/` (placeholder exists) — must include a license-compatible font with full UI glyph coverage + dynamic fallback. Rasterized at runtime by `fontdue` into UI atlases (see [`stack.md`](stack.md)).
- All assets content-addressed or versioned; missing asset = magenta + error, never crash.
- Hot-reload is dev-only (`cfg(debug_assertions)`), stripped on release/mobile.
