# Bundled fonts

`DejaVuSans.ttf` 2.37 (TrueType outlines) + `LICENSE-DejaVu.txt`
(Bitstream Vera license + public-domain DejaVu changes). Source:
<https://github.com/dejavu-fonts/dejavu-fonts/releases/tag/version_2_37>
(`dejavu-sans-ttf-2.37.zip`).

First consumer: the `game_debug` sphere viewer embeds it via
`include_bytes!` for `fontdue` rasterization (see
`plans/debug-sphere-viewer`). Do not add another UI font without
updating `docs/techstack/assets.md`.
