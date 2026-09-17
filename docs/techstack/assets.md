# Assets and content pipeline

- Formats: `glTF` meshes, `KTX2`/Basis-compressed textures, Ogg Opus audio (confirmed by ADR-006 in [`../decisions/`](../decisions/)). Sources (`.blend`, `.png/.wav`) never ship.
- `assets/` holds source + cooked manifests; cooker lives in `crates/tools`.
- Star-catalog tiles (`plans/v0.2.0/star-catalog-streaming`, ADR-017):
  `game_tools catalog` cooks HEALPix binary tiles (`GSCT` v1: header +
  fixed-stride Gaia-schema records) plus `manifest.json` (tile list,
  counts, FNV-1a hashes) from deterministic synthetic generation and/or
  small CSV extracts; same inputs ⇒ byte-identical output. Packaged demo
  extracts live under `assets/catalog/`; generated tiles are never
  committed ad hoc — full-sky real import is later work (spec §10).
- Mobile texture ceiling: 2k max, ASTC/ETC2 via Basis; desktop may use 4k.
- Fonts: bundled under `assets/fonts/` — `DejaVuSans.ttf` 2.37
  (TrueType outlines) + `LICENSE-DejaVu.txt`, from
  <https://github.com/dejavu-fonts/dejavu-fonts/releases/tag/version_2_37>
  (`dejavu-sans-ttf-2.37.zip`); first consumer is the `game_debug`
  sphere viewer (`fontdue` atlas, embedded via `include_bytes!`).
  Any further UI font must be license-compatible with full UI glyph
  coverage + dynamic fallback. Rasterized at runtime by `fontdue` into
  UI atlases (see [`stack.md`](stack.md)).
- All assets content-addressed or versioned; missing asset = magenta + error, never crash.
- Hot-reload is dev-only (`cfg(debug_assertions)`), stripped on release/mobile.
