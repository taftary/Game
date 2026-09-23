//! GPU-free capture harness data (CAP-002).
//!
//! Camera presets, `WxH` parsing, windowed-capture filenames, and 8-bit
//! RGBA PNG encoding shared by the offscreen `--capture` path and the
//! `F12` windowed shortcut. No window, no GPU: poses are Mpc offsets
//! relative to the home node so presets survive reseeds.
//!
//! Row convention: `pixels_top_first` row 0 is the top of the picture
//! (NDC `+1` = top), matching `docs/techstack/rendering.md`.

/// Capture camera preset selector (`--view`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureView {
    /// Current inspector default framing.
    Inspector,
    /// Target framing; reserved until cosmic-depth-window lands.
    Slab,
    /// Player spawn (Chase boot framing).
    Demo,
    /// Opening shot; the t = 0 vista pose (filled by cosmic-vista-intro).
    Vista,
}

/// Seed-independent camera pose + output size for one capture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapturePreset {
    /// Which preset this is.
    pub view: CaptureView,
    /// True for the demo surface, false for the inspector surface.
    pub surface_is_demo: bool,
    /// Eye position in Mpc, relative to the home node.
    pub eye_offset_mpc: [f64; 3],
    /// Look-at target in Mpc, relative to the home node.
    pub target_offset_mpc: [f64; 3],
    /// Vertical field of view, degrees.
    pub fov_y_deg: f32,
    /// Slab thickness in Mpc; `None` until cosmic-depth-window lands.
    pub slab_thickness_mpc: Option<f32>,
    /// Output width in px.
    pub width: u32,
    /// Output height in px.
    pub height: u32,
}

/// Default capture size in px (target's aspect).
pub const CAPTURE_DEFAULT_SIZE: (u32, u32) = (1408, 768);

/// Inspector framing: default 430 Mpc-class view, no slab.
pub const INSPECTOR_PRESET: CapturePreset = CapturePreset {
    view: CaptureView::Inspector,
    surface_is_demo: false,
    eye_offset_mpc: [0.0, 150.0, 400.0],
    target_offset_mpc: [0.0, 0.0, 0.0],
    fov_y_deg: 60.0,
    slab_thickness_mpc: None,
    width: CAPTURE_DEFAULT_SIZE.0,
    height: CAPTURE_DEFAULT_SIZE.1,
};

/// Target framing (inspector basis + 30 Mpc slab); the binary narrows
/// to the 20° slab camera at capture time.
pub const SLAB_PRESET: CapturePreset = CapturePreset {
    view: CaptureView::Slab,
    surface_is_demo: false,
    eye_offset_mpc: [0.0, 150.0, 400.0],
    target_offset_mpc: [0.0, 0.0, 0.0],
    fov_y_deg: 60.0,
    slab_thickness_mpc: Some(30.0),
    width: CAPTURE_DEFAULT_SIZE.0,
    height: CAPTURE_DEFAULT_SIZE.1,
};

/// Player spawn framing (Chase boot view on the demo surface).
pub const DEMO_PRESET: CapturePreset = CapturePreset {
    view: CaptureView::Demo,
    surface_is_demo: true,
    eye_offset_mpc: [38.0, 14.0, -46.0],
    target_offset_mpc: [30.0, 8.0, -38.0],
    fov_y_deg: 60.0,
    slab_thickness_mpc: None,
    width: CAPTURE_DEFAULT_SIZE.0,
    height: CAPTURE_DEFAULT_SIZE.1,
};

/// Opening shot (`cosmic-vista-intro` CVI-008): the t = 0 vista pose —
/// 25° FOV, 40 Mpc slab, fog off — computed per seed by
/// `cosmic_vista::vista_pose` (the binary poses the demo camera from
/// it; the static offsets below mirror the demo basis framing like
/// the slab mirrors the inspector).
pub const VISTA_PRESET: CapturePreset = CapturePreset {
    view: CaptureView::Vista,
    surface_is_demo: true,
    eye_offset_mpc: [38.0, 14.0, -46.0],
    target_offset_mpc: [30.0, 8.0, -38.0],
    fov_y_deg: 25.0,
    slab_thickness_mpc: Some(40.0),
    width: CAPTURE_DEFAULT_SIZE.0,
    height: CAPTURE_DEFAULT_SIZE.1,
};

/// Look up the preset for a view.
pub fn preset_for(view: CaptureView) -> CapturePreset {
    match view {
        CaptureView::Inspector => INSPECTOR_PRESET,
        CaptureView::Slab => SLAB_PRESET,
        CaptureView::Demo => DEMO_PRESET,
        CaptureView::Vista => VISTA_PRESET,
    }
}

/// Parse a `--view` name; exactly `inspector|slab|demo|vista`.
pub fn parse_view(s: &str) -> Result<CaptureView, String> {
    match s {
        "inspector" => Ok(CaptureView::Inspector),
        "slab" => Ok(CaptureView::Slab),
        "demo" => Ok(CaptureView::Demo),
        "vista" => Ok(CaptureView::Vista),
        _ => Err(format!(
            "unknown capture view {s:?}: expected inspector|slab|demo|vista"
        )),
    }
}

/// Parse a `--size` value (`WxH`), each side in `1..=4096`.
pub fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let invalid = || format!("invalid capture size {s:?}: expected WxH with each side in 1..=4096");
    let (w, h) = s.split_once('x').ok_or_else(invalid)?;
    let w: u32 = w.parse().map_err(|_| invalid())?;
    let h: u32 = h.parse().map_err(|_| invalid())?;
    if !(1..=4096).contains(&w) || !(1..=4096).contains(&h) {
        return Err(invalid());
    }
    Ok((w, h))
}

/// Filename for an `F12` windowed capture: `captures/<surface>-<seed>-<ts>.png`.
/// The surface name is sanitized (non `alphanumeric`/`-`/`_` become `_`).
pub fn windowed_capture_filename(surface: &str, seed: u64, timestamp: &str) -> String {
    let safe: String = surface
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("captures/{safe}-{seed}-{timestamp}.png")
}

/// Encode 8-bit RGBA to PNG; input row 0 is the picture's top row.
pub fn encode_png_rgba8(
    width: u32,
    height: u32,
    pixels_top_first: &[u8],
) -> Result<Vec<u8>, String> {
    let expected = width as usize * height as usize * 4;
    if pixels_top_first.len() != expected {
        return Err(format!(
            "pixel buffer length {} != {width}x{height}x4 ({expected})",
            pixels_top_first.len()
        ));
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("png header: {e}"))?;
        writer
            .write_image_data(pixels_top_first)
            .map_err(|e| format!("png encode: {e}"))?;
    }
    Ok(out)
}

/// Fraction of pixels with all three channels at or above
/// `threshold` (DoD wash checks, e.g. `cosmic-tracer-splat` near-eye:
/// no frame with > 50 % of pixels above the bloom threshold).
/// Pure CPU over RGBA8 top-first bytes.
pub fn bright_fraction_rgba8(pixels: &[u8], threshold: u8) -> f64 {
    if pixels.is_empty() {
        return 0.0;
    }
    let mut bright = 0usize;
    let mut total = 0usize;
    for px in pixels.chunks_exact(4) {
        total += 1;
        if px[0] >= threshold && px[1] >= threshold && px[2] >= threshold {
            bright += 1;
        }
    }
    bright as f64 / total.max(1) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_VIEWS: [CaptureView; 4] = [
        CaptureView::Inspector,
        CaptureView::Slab,
        CaptureView::Demo,
        CaptureView::Vista,
    ];

    #[test]
    fn parse_view_accepts_all_four_names() {
        assert_eq!(parse_view("inspector"), Ok(CaptureView::Inspector));
        assert_eq!(parse_view("slab"), Ok(CaptureView::Slab));
        assert_eq!(parse_view("demo"), Ok(CaptureView::Demo));
        assert_eq!(parse_view("vista"), Ok(CaptureView::Vista));
    }

    #[test]
    fn parse_view_rejects_unknown_names() {
        for bad in [
            "",
            "Inspector",
            "INSPECTOR",
            "slab ",
            " demo",
            "all",
            "demo2",
        ] {
            assert!(parse_view(bad).is_err(), "expected error for {bad:?}");
        }
    }

    #[test]
    fn parse_size_accepts_valid_sizes() {
        assert_eq!(parse_size("1408x768"), Ok((1408, 768)));
        assert_eq!(parse_size("1408x768"), Ok(CAPTURE_DEFAULT_SIZE));
        assert_eq!(parse_size("1x1"), Ok((1, 1)));
        assert_eq!(parse_size("4096x4096"), Ok((4096, 4096)));
    }

    #[test]
    fn parse_size_rejects_bad_sizes() {
        for bad in [
            "abc",
            "1408",
            "1408x",
            "x768",
            "",
            "0x10",
            "1408x0",
            "0x0",
            "5000x10",
            "10x5000",
            "1408x768x2",
        ] {
            assert!(parse_size(bad).is_err(), "expected error for {bad:?}");
        }
    }

    #[test]
    fn presets_have_finite_poses_and_default_size() {
        for view in ALL_VIEWS {
            let p = preset_for(view);
            assert_eq!(p.view, view);
            for v in p.eye_offset_mpc.iter().chain(p.target_offset_mpc.iter()) {
                assert!(v.is_finite(), "{view:?} offset not finite");
            }
            assert!(p.fov_y_deg.is_finite() && p.fov_y_deg > 0.0);
            assert_eq!((p.width, p.height), CAPTURE_DEFAULT_SIZE);
            if let Some(t) = p.slab_thickness_mpc {
                assert!(t.is_finite() && t > 0.0);
            }
        }
    }

    #[test]
    fn slab_thickness_only_on_slab_and_vista_presets() {
        assert_eq!(preset_for(CaptureView::Slab).slab_thickness_mpc, Some(30.0));
        assert_eq!(
            preset_for(CaptureView::Vista).slab_thickness_mpc,
            Some(40.0)
        );
        for view in [CaptureView::Inspector, CaptureView::Demo] {
            assert_eq!(preset_for(view).slab_thickness_mpc, None);
        }
    }

    #[test]
    fn demo_flag_only_on_demo_and_vista() {
        assert!(!preset_for(CaptureView::Inspector).surface_is_demo);
        assert!(!preset_for(CaptureView::Slab).surface_is_demo);
        assert!(preset_for(CaptureView::Demo).surface_is_demo);
        assert!(preset_for(CaptureView::Vista).surface_is_demo);
    }

    #[test]
    fn reserved_presets_match_their_basis_framing() {
        let inspector = preset_for(CaptureView::Inspector);
        let slab = preset_for(CaptureView::Slab);
        assert_eq!(slab.eye_offset_mpc, inspector.eye_offset_mpc);
        assert_eq!(slab.target_offset_mpc, inspector.target_offset_mpc);
        assert_eq!(slab.fov_y_deg, inspector.fov_y_deg);
        // Vista filled (CVI-008): demo-basis offsets, own 25° FOV +
        // 40 Mpc slab (the t = 0 pose itself is per-seed — the binary
        // poses from `vista_pose`, pinned by the capture test).
        let demo = preset_for(CaptureView::Demo);
        let vista = preset_for(CaptureView::Vista);
        assert_eq!(vista.eye_offset_mpc, demo.eye_offset_mpc);
        assert_eq!(vista.target_offset_mpc, demo.target_offset_mpc);
        assert_eq!(vista.fov_y_deg, 25.0);
        assert_eq!(vista.slab_thickness_mpc, Some(40.0));
    }

    #[test]
    fn windowed_capture_filename_format() {
        assert_eq!(
            windowed_capture_filename("inspector", 1337, "20260920-120000"),
            "captures/inspector-1337-20260920-120000.png"
        );
        assert_eq!(
            windowed_capture_filename("demo surface!", 7, "20260920-120001"),
            "captures/demo_surface_-7-20260920-120001.png"
        );
    }

    #[test]
    fn encode_png_rejects_length_mismatch() {
        assert!(encode_png_rgba8(2, 2, &[0u8; 15]).is_err());
        assert!(encode_png_rgba8(2, 2, &[0u8; 17]).is_err());
        assert!(encode_png_rgba8(2, 2, &[0u8; 16]).is_ok());
    }

    #[test]
    fn bright_fraction_counts_white_pixels() {
        assert_eq!(bright_fraction_rgba8(&[], 200), 0.0);
        // 2x2: one white, one gray, two black.
        let pixels = [
            255, 255, 255, 255, 200, 200, 200, 255, 0, 0, 0, 255, 199, 199, 199, 255,
        ];
        assert!((bright_fraction_rgba8(&pixels, 200) - 0.5).abs() < 1e-9);
        assert!((bright_fraction_rgba8(&pixels, 255) - 0.25).abs() < 1e-9);
        // Single-channel brights don't count (bloom needs all three).
        let red = [255, 0, 0, 255];
        assert_eq!(bright_fraction_rgba8(&red, 200), 0.0);
    }

    #[test]
    fn encode_png_round_trip_top_row_first() {
        let (w, h) = (5u32, 3u32);
        // Distinct rows so a flip would fail the comparison below.
        let mut pixels = Vec::new();
        for y in 0..h {
            for x in 0..w {
                pixels.extend_from_slice(&[(y * 80) as u8, (x * 40) as u8, 0xAB, 0xFF]);
            }
        }
        let encoded = encode_png_rgba8(w, h, &pixels).expect("encode");
        let decoder = png::Decoder::new(encoded.as_slice());
        let mut reader = decoder.read_info().expect("png header");
        let (dw, dh) = {
            let info = reader.info();
            (info.width, info.height)
        };
        assert_eq!((dw, dh), (w, h));
        let mut decoded = vec![0u8; reader.output_buffer_size()];
        reader.next_frame(&mut decoded).expect("png frame");
        let row_len = w as usize * 4;
        assert_eq!(&decoded[..row_len], &pixels[..row_len]);
    }

    /// Near-eye wash pin (`cosmic-tracer-splat` DoD 5): the committed
    /// `demo-after.png` (boot framing inside a filament — spawn sits in
    /// one by construction) must show no white flash — fewer than half
    /// its pixels above the bright bar. CPU-only over the committed
    /// shot: a standing invariant, not a grade lock (future features
    /// replace the shot and re-record the number in their plan).
    #[test]
    fn demo_after_shot_has_no_white_flash() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../plans/v0.3.3/cosmic-tracer-splat/shots/demo-after.png"
        );
        let bytes = std::fs::read(path).expect("demo-after.png must exist");
        let decoder = png::Decoder::new(bytes.as_slice());
        let mut reader = decoder.read_info().expect("png header");
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);
        let mut pixels = vec![0u8; reader.output_buffer_size()];
        reader.next_frame(&mut pixels).expect("png frame");
        let frac = bright_fraction_rgba8(&pixels, 200);
        eprintln!("demo-after bright fraction (>=200): {frac:.4}");
        assert!(
            frac < 0.5,
            "white flash: {frac:.3} of demo-after.png pixels are bright"
        );
    }

    /// Load a committed RGBA8 shot (top-first) for CPU scans.
    #[cfg(test)]
    fn load_shot_rgba8(name: &str) -> (u32, u32, Vec<u8>) {
        let path = format!(
            "{}/../../plans/v0.3.4/cosmic-void-contrast/shots/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let bytes = std::fs::read(&path).expect("committed shot must exist");
        let decoder = png::Decoder::new(bytes.as_slice());
        let mut reader = decoder.read_info().expect("png header");
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);
        let (w, h) = (reader.info().width, reader.info().height);
        let mut pixels = vec![0u8; reader.output_buffer_size()];
        reader.next_frame(&mut pixels).expect("png frame");
        assert_eq!(pixels.len(), w as usize * h as usize * 4);
        (w, h, pixels)
    }

    /// Rec.709 luminance per pixel (0–255).
    #[cfg(test)]
    fn shot_luminance(pixels: &[u8]) -> Vec<f64> {
        pixels
            .chunks_exact(4)
            .map(|px| {
                0.2126 * f64::from(px[0]) + 0.7152 * f64::from(px[1]) + 0.0722 * f64::from(px[2])
            })
            .collect()
    }

    /// Void-contrast grade (`cosmic-void-contrast` DoD 1): the
    /// committed `slab-after.png` (seed 1337, 1408×768, High) holds
    /// dark voids against bright threads — void floor ≤ 1.15× the
    /// clear backdrop (corner blocks, outside the slice), knot-peak
    /// ridge ≥ 6× the floor, faint-wall sheet ≥ 1.5×. Bands, not
    /// hand-picked pixels: floor = mean of the darkest 5 % (pure
    /// backdrop where the slice is empty), ridge = mean above p99.9
    /// (~1080 px over dozens of beads — the beaded crests, not
    /// inter-knot thread), sheet = p90 (voids dominate to the median;
    /// walls emerge past p75). CPU-only over the committed shot; the
    /// numbers print for the plan record.
    #[test]
    fn slab_after_void_contrast_targets() {
        let (w, h, pixels) = load_shot_rgba8("slab-after.png");
        assert_eq!((w, h), (1408, 768));
        let lum = shot_luminance(&pixels);
        // Backdrop: four 8×8 corner blocks (outside the slab slice —
        // pure clear colour through the resolve).
        let mut backdrop = Vec::new();
        for (cx, cy) in [(0, 0), (w - 8, 0), (0, h - 8), (w - 8, h - 8)] {
            for dy in 0..8 {
                for dx in 0..8 {
                    backdrop.push(lum[((cy + dy) * w + (cx + dx)) as usize]);
                }
            }
        }
        let backdrop_mean = backdrop.iter().sum::<f64>() / backdrop.len() as f64;
        let mut sorted = lum.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let n = sorted.len();
        let floor: f64 = sorted[..n / 20].iter().sum::<f64>() / (n / 20) as f64;
        // Ridge = knot peaks: mean above p99.9 (~1080 px over dozens of
        // beads — the beaded crests the reference grades, not
        // inter-knot thread). Sheet = p90 (voids dominate to the
        // median; walls emerge past p75).
        let peaks: Vec<f64> = sorted[n - n / 1000..].to_vec();
        let ridge: f64 = peaks.iter().sum::<f64>() / peaks.len() as f64;
        let sheet = sorted[n * 90 / 100];
        let context: f64 = sorted[n - n / 200..].iter().sum::<f64>() / (n / 200) as f64;
        eprintln!(
            "slab backdrop={backdrop_mean:.3} floor={floor:.3} sheet(p90)={sheet:.3} ridge(peaks)={ridge:.3} thread(top-0.5%)={context:.3}"
        );
        eprintln!(
            "slab ratios floor/backdrop={:.3} ridge/floor={:.3} sheet/floor={:.3}",
            floor / backdrop_mean,
            ridge / floor.max(1e-6),
            sheet / floor.max(1e-6)
        );
        assert!(
            floor <= 1.15 * backdrop_mean,
            "void floor {floor:.3} exceeds 1.15x backdrop {backdrop_mean:.3}"
        );
        assert!(
            ridge / floor.max(1e-6) >= 6.0,
            "ridge/floor contrast too low: {ridge:.3}/{floor:.3}"
        );
        assert!(
            sheet / floor.max(1e-6) >= 1.5,
            "sheet/floor contrast too low: {sheet:.3}/{floor:.3}"
        );
    }

    /// Rim-fade pin (`cosmic-void-contrast` DoD 2): radial luminance
    /// profile of the committed `inspector-after.png` outside the
    /// central hub complex (r ≥ 60 px — the centre holds the goal hub
    /// and its point-source steps are navigation content, not a limb)
    /// — no radial step over 20 % within any 10 px span (the transfer
    /// rim fades the last 30 Mpc, so the former sphere limb must not
    /// read as an edge).
    #[test]
    fn inspector_after_shows_no_limb() {
        let (w, h, pixels) = load_shot_rgba8("inspector-after.png");
        assert_eq!((w, h), (1408, 768));
        let lum = shot_luminance(&pixels);
        let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
        let max_r = cx.min(cy) as usize;
        // Ring means every 2 px.
        let mut rings = Vec::new();
        let mut r = 0usize;
        while r < max_r {
            let (mut sum, mut count) = (0.0, 0usize);
            for y in 0..h as usize {
                for x in (0..w as usize).step_by(2) {
                    let d = ((x as f64 - cx).powi(2) + (y as f64 - cy).powi(2)).sqrt();
                    if (d - r as f64).abs() < 1.0 {
                        sum += lum[y * w as usize + x];
                        count += 1;
                    }
                }
            }
            assert!(count > 0, "empty ring at r={r}");
            rings.push(sum / count as f64);
            r += 2;
        }
        // Max relative step over any 10 px span (5 ring steps), past
        // the central hub complex (first 30 rings).
        let mut worst: f64 = 0.0;
        for i in 30..rings.len().saturating_sub(5) {
            let step = (rings[i + 5] - rings[i]).abs() / rings[i].max(1.0);
            worst = worst.max(step);
        }
        eprintln!(
            "inspector radial rings={} worst 10px step r>=60px={worst:.3}",
            rings.len()
        );
        assert!(worst <= 0.20, "limb step detected: {worst:.3} over 10 px");
    }

    /// Filament-interior read (`cosmic-void-contrast` DoD 3): the
    /// committed `demo-after.png` (boot framing inside a filament)
    /// stays below the hot bar (≤ 50 % bright — the CGV-015 wash
    /// measure) while keeping dark cells around the ship (≥ 5 % of
    /// pixels within 1.5× of the slab void floor: voids visible,
    /// never a wash).
    #[test]
    fn demo_after_hot_fraction_and_dark_cells() {
        let (w, h, pixels) = load_shot_rgba8("demo-after.png");
        assert_eq!((w, h), (1408, 768));
        let hot = bright_fraction_rgba8(&pixels, 200);
        // Dark = within 1.5× of the slab void floor (24.744 — the
        // sibling scan's measured backdrop; the boot backdrop is out
        // of frame in the demo view).
        let dark = shot_luminance(&pixels)
            .iter()
            .filter(|l| **l < 1.5 * 24.744)
            .count() as f64
            / (w as usize * h as usize) as f64;
        eprintln!("demo hot(>=200)={hot:.4} dark(<1.5x void)={dark:.4}");
        assert!(
            hot <= 0.5,
            "filament interior washed: hot fraction {hot:.3}"
        );
        assert!(
            dark >= 0.05,
            "no dark cells around the ship: dark fraction {dark:.4}"
        );
    }

    /// Load a committed RGBA8 shot from the compact-cores feature folder.
    #[cfg(test)]
    fn load_compact_shot_rgba8(name: &str) -> (u32, u32, Vec<u8>) {
        let path = format!(
            "{}/../../plans/v0.3.4/cosmic-hub-compact-cores/shots/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let bytes = std::fs::read(&path).expect("compact-cores shot must exist");
        let decoder = png::Decoder::new(bytes.as_slice());
        let mut reader = decoder.read_info().expect("png header");
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);
        let (w, h) = (reader.info().width, reader.info().height);
        let mut pixels = vec![0u8; reader.output_buffer_size()];
        reader.next_frame(&mut pixels).expect("png frame");
        assert_eq!(pixels.len(), w as usize * h as usize * 4);
        (w, h, pixels)
    }

    /// Connected bright components (4-connectivity) above `threshold`
    /// (all three channels ≥ threshold). Returns `(count, largest_diam,
    /// largest_area)` where `largest_diam` is the max of the bounding-box
    /// width/height (px, ≈ diameter for round sprites). Pure CPU over
    /// RGBA8 top-first bytes.
    #[cfg(test)]
    fn bright_components(pixels: &[u8], w: u32, h: u32, threshold: u8) -> (usize, u32, usize) {
        let (w, h) = (w as usize, h as usize);
        let bright = |x: usize, y: usize| {
            let i = (y * w + x) * 4;
            pixels[i] >= threshold && pixels[i + 1] >= threshold && pixels[i + 2] >= threshold
        };
        let mut seen = vec![false; w * h];
        let mut count = 0_usize;
        let mut largest_diam = 0_u32;
        let mut largest_area = 0_usize;
        for y in 0..h {
            for x in 0..w {
                if !bright(x, y) || seen[y * w + x] {
                    continue;
                }
                // BFS one component.
                count += 1;
                let mut stack = vec![(x, y)];
                seen[y * w + x] = true;
                let (mut xmin, mut xmax, mut ymin, mut ymax) = (x, x, y, y);
                let mut area = 0_usize;
                while let Some((cx, cy)) = stack.pop() {
                    area += 1;
                    xmin = xmin.min(cx);
                    xmax = xmax.max(cx);
                    ymin = ymin.min(cy);
                    ymax = ymax.max(cy);
                    for (nx, ny) in [
                        (cx.wrapping_sub(1), cy),
                        (cx + 1, cy),
                        (cx, cy.wrapping_sub(1)),
                        (cx, cy + 1),
                    ] {
                        if nx < w && ny < h && bright(nx, ny) && !seen[ny * w + nx] {
                            seen[ny * w + nx] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
                let diam = ((xmax - xmin + 1).max(ymax - ymin + 1)) as u32;
                if area > largest_area {
                    largest_area = area;
                    largest_diam = diam;
                }
            }
        }
        (count, largest_diam, largest_area)
    }

    /// Compact-hub grade (`cosmic-hub-compact-cores` DoD 1): the
    /// committed `slab-after.png` (seed 1337, 1408×768) shows point
    /// cores, never discs — largest bright core (≥200, all channels)
    /// ≤ 12 px diameter (the ≤ 10 blazing-hub count and the halo ≤ 3×
    /// ratio are measured per-hub on crops in
    /// `slab_after_tier_a_crops_hold_member_dots`, where splat knots
    /// cannot contaminate the count). CPU-only over the committed shot;
    /// numbers print for the plan record.
    #[test]
    fn slab_after_compact_hub_cores() {
        let (w, h, pixels) = load_compact_shot_rgba8("slab-after.png");
        assert_eq!((w, h), (1408, 768));
        // Full component census at the bright bar (all channels ≥200).
        let (wusz, husz) = (w as usize, h as usize);
        let bright = |x: usize, y: usize| {
            let i = (y * wusz + x) * 4;
            pixels[i] >= 200 && pixels[i + 1] >= 200 && pixels[i + 2] >= 200
        };
        let mut seen = vec![false; wusz * husz];
        let mut total = 0_usize;
        let mut large = 0_usize;
        let mut largest_diam = 0_u32;
        let mut largest_area = 0_usize;
        for y in 0..husz {
            for x in 0..wusz {
                if !bright(x, y) || seen[y * wusz + x] {
                    continue;
                }
                total += 1;
                let mut stack = vec![(x, y)];
                seen[y * wusz + x] = true;
                let (mut xmin, mut xmax, mut ymin, mut ymax) = (x, x, y, y);
                let mut area = 0_usize;
                while let Some((cx, cy)) = stack.pop() {
                    area += 1;
                    xmin = xmin.min(cx);
                    xmax = xmax.max(cx);
                    ymin = ymin.min(cy);
                    ymax = ymax.max(cy);
                    for (nx, ny) in [
                        (cx.wrapping_sub(1), cy),
                        (cx + 1, cy),
                        (cx, cy.wrapping_sub(1)),
                        (cx, cy + 1),
                    ] {
                        if nx < wusz && ny < husz && bright(nx, ny) && !seen[ny * wusz + nx] {
                            seen[ny * wusz + nx] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
                let diam = ((xmax - xmin + 1).max(ymax - ymin + 1)) as u32;
                if diam >= 4 {
                    large += 1;
                }
                if area > largest_area {
                    largest_area = area;
                    largest_diam = diam;
                }
            }
        }
        eprintln!(
            "slab cores: total_bright={total} large(diam>=4)={large} core_diam={largest_diam} core_area={largest_area} (blazing hubs counted per-hub in the crop test)"
        );
        assert!(
            largest_diam <= 12,
            "largest core disc too big: {largest_diam}px (want ≤ 12)"
        );
    }

    /// Member-dot read (`cosmic-hub-compact-cores` DoD 1): the committed
    /// `slab-after.png` resolves hub members as discrete dots — count
    /// small isolated bright specks (1–9 px components at ≥150, all
    /// channels) across the frame. CPU-only; the ≥3 Tier A hub crops
    /// are ANALYST-counted on 64 px crops for the plan record (see
    /// `slab_after_tier_a_crops_hold_member_dots`).
    #[test]
    fn slab_after_member_dots_resolve() {
        let (w, h, pixels) = load_compact_shot_rgba8("slab-after.png");
        assert_eq!((w, h), (1408, 768));
        let (wusz, husz) = (w as usize, h as usize);
        let bright = |x: usize, y: usize| {
            let i = (y * wusz + x) * 4;
            pixels[i] >= 150 && pixels[i + 1] >= 150 && pixels[i + 2] >= 150
        };
        let mut seen = vec![false; wusz * husz];
        let mut small_dots = 0_usize;
        for y in 0..husz {
            for x in 0..wusz {
                if !bright(x, y) || seen[y * wusz + x] {
                    continue;
                }
                let mut stack = vec![(x, y)];
                seen[y * wusz + x] = true;
                let mut area = 0_usize;
                while let Some((cx, cy)) = stack.pop() {
                    area += 1;
                    for (nx, ny) in [
                        (cx.wrapping_sub(1), cy),
                        (cx + 1, cy),
                        (cx, cy.wrapping_sub(1)),
                        (cx, cy + 1),
                    ] {
                        if nx < wusz && ny < husz && bright(nx, ny) && !seen[ny * wusz + nx] {
                            seen[ny * wusz + nx] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
                if (1..=9).contains(&area) {
                    small_dots += 1;
                }
            }
        }
        eprintln!("slab small dots (1-9px @150): {small_dots}");
        assert!(
            small_dots >= 50,
            "members not resolved as dots: {small_dots} (want ≥ 50)"
        );
    }

    /// Tier A crop check (`cosmic-hub-compact-cores` DoD 1): project the
    /// nominal Tier A hubs into the `slab-after.png` framing (inspector
    /// 20° slab at the home depth, 1408×768) and count discrete member
    /// dots in 64 px crops around each — ≥ 3 crops must hold dots —
    /// plus the per-hub halo ≤ 3× core ratio (bloom widens the capped
    /// core, never a disc). Slow (~15 s): full 128³ generation +
    /// projection.
    #[test]
    fn slab_after_tier_a_crops_hold_member_dots() {
        use crate::cosmic_web::CosmicWebInspector;
        use crate::map_camera::MapOrbitCamera;
        use crate::picking::project_to_screen;
        use crate::ui::Rect;
        use game_engine::universe::{CosmicWebParams, generate_cosmic_web};
        use glam::Vec3;
        let params = CosmicWebParams::nominal();
        let web = generate_cosmic_web(1337, &params);
        // Slab framing: default inspector + 20° keep-framing (the
        // capture path), 1408×768.
        let mut inspector = CosmicWebInspector::new();
        inspector.camera.set_fov_keep_framing(20.0);
        let aspect = 1408.0 / 768.0;
        let vp_mat = inspector.camera.view_proj(aspect);
        let vp = Rect {
            x: 0.0,
            y: 0.0,
            w: 1408.0,
            h: 768.0,
        };
        // Tier A hubs by mass rank (top 1 %).
        let n = web.nodes.len();
        let a_count = (0.01 * n as f64).ceil() as usize;
        let (_, _, pixels) = load_compact_shot_rgba8("slab-after.png");
        let (wusz, _husz) = (1408_usize, 768_usize);
        let bright = |x: usize, y: usize| {
            let i = (y * wusz + x) * 4;
            pixels[i] >= 150 && pixels[i + 1] >= 150 && pixels[i + 2] >= 150
        };
        let mut crops_with_dots = 0_usize;
        let mut crops_checked = 0_usize;
        for rank in 0..a_count.min(20) {
            let node = &web.nodes[rank];
            let world = Vec3::new(
                node.position_mpc[0] as f32,
                node.position_mpc[1] as f32,
                node.position_mpc[2] as f32,
            );
            let Some((sx, sy)) = project_to_screen(world, vp_mat, vp) else {
                continue;
            };
            if sx < 32.0 || sy < 32.0 || sx >= 1408.0 - 32.0 || sy >= 768.0 - 32.0 {
                continue;
            }
            crops_checked += 1;
            // 64 px crop: count 1–9 px dots (members), excluding the
            // central 13×13 core (the pin + core itself).
            let (cx, cy) = (sx as usize, sy as usize);
            let (x0, y0) = (cx - 32, cy - 32);
            let mut seen = vec![false; 64 * 64];
            let mut dots = 0_usize;
            for y in 0..64 {
                for x in 0..64 {
                    let gx = x0 + x;
                    let gy = y0 + y;
                    // Skip the central core.
                    if (x as isize - 32).abs() <= 6 && (y as isize - 32).abs() <= 6 {
                        continue;
                    }
                    if !bright(gx, gy) || seen[y * 64 + x] {
                        continue;
                    }
                    // Flood within the crop.
                    let mut stack = vec![(x, y)];
                    seen[y * 64 + x] = true;
                    let mut area = 0_usize;
                    while let Some((qx, qy)) = stack.pop() {
                        area += 1;
                        for (nx, ny) in [
                            (qx.wrapping_sub(1), qy),
                            (qx + 1, qy),
                            (qx, qy.wrapping_sub(1)),
                            (qx, qy + 1),
                        ] {
                            if nx < 64 && ny < 64 {
                                let (ggx, ggy) = (x0 + nx, y0 + ny);
                                if (nx as isize - 32).abs() <= 6 && (ny as isize - 32).abs() <= 6 {
                                    continue;
                                }
                                if bright(ggx, ggy) && !seen[ny * 64 + nx] {
                                    seen[ny * 64 + nx] = true;
                                    stack.push((nx, ny));
                                }
                            }
                        }
                    }
                    if (1..=9).contains(&area) {
                        dots += 1;
                    }
                }
            }
            eprintln!("Tier A rank {rank} @({sx:.0},{sy:.0}) crop dots={dots}");
            if dots >= 3 {
                crops_with_dots += 1;
            }
            // Per-hub halo ≤ 3× core (DoD 1): flood from the hub
            // center (32,32 in the crop) at the core bar (≥200, all
            // channels) vs the halo bar (≥100, any channel, warm bloom)
            // — the hub's own component, so filaments elsewhere in the
            // crop never contaminate the ratio.
            let mut core_diam = 0_u32;
            let mut halo_diam = 0_u32;
            for pass in 0..2 {
                let is_core = pass == 0;
                let center_hit = if is_core {
                    let i = (cy * wusz + cx) * 4;
                    pixels[i] >= 200 && pixels[i + 1] >= 200 && pixels[i + 2] >= 200
                } else {
                    let i = (cy * wusz + cx) * 4;
                    pixels[i] >= 100 || pixels[i + 1] >= 100 || pixels[i + 2] >= 100
                };
                if !center_hit {
                    // Hub center below the bar (out-of-slice dimmed hub
                    // or background) — no core/halo to measure here.
                    continue;
                }
                let mut seen_c = vec![false; 64 * 64];
                let mut stack = vec![(32_usize, 32_usize)];
                seen_c[32 * 64 + 32] = true;
                let (mut xmin, mut xmax, mut ymin, mut ymax) =
                    (32_usize, 32_usize, 32_usize, 32_usize);
                while let Some((qx, qy)) = stack.pop() {
                    xmin = xmin.min(qx);
                    xmax = xmax.max(qx);
                    ymin = ymin.min(qy);
                    ymax = ymax.max(qy);
                    for (nx, ny) in [
                        (qx.wrapping_sub(1), qy),
                        (qx + 1, qy),
                        (qx, qy.wrapping_sub(1)),
                        (qx, qy + 1),
                    ] {
                        if nx < 64 && ny < 64 && !seen_c[ny * 64 + nx] {
                            let (ggx, ggy) = (x0 + nx, y0 + ny);
                            let hit = if is_core {
                                let i = (ggy * wusz + ggx) * 4;
                                pixels[i] >= 200 && pixels[i + 1] >= 200 && pixels[i + 2] >= 200
                            } else {
                                let i = (ggy * wusz + ggx) * 4;
                                pixels[i] >= 100 || pixels[i + 1] >= 100 || pixels[i + 2] >= 100
                            };
                            if hit {
                                seen_c[ny * 64 + nx] = true;
                                stack.push((nx, ny));
                            }
                        }
                    }
                }
                let diam = ((xmax - xmin + 1).max(ymax - ymin + 1)) as u32;
                if pass == 0 {
                    core_diam = diam;
                } else {
                    halo_diam = diam;
                }
            }
            eprintln!(
                "Tier A rank {rank} core={core_diam}px halo={halo_diam}px ratio={:.2}",
                halo_diam as f64 / core_diam.max(1) as f64
            );
            if core_diam > 0 {
                assert!(
                    core_diam <= 12,
                    "hub core too big at rank {rank}: {core_diam}px (want ≤ 12)"
                );
                assert!(
                    halo_diam as f64 <= 3.0 * core_diam as f64,
                    "halo too wide at rank {rank}: {halo_diam}px vs core {core_diam}px (want ≤ 3×)"
                );
            }
        }
        // Silence unused-import lint for the camera type re-export.
        let _ = MapOrbitCamera::new(Vec3::ZERO, 430.0, 0.5, 0.9, 10.0, 1600.0, 250.0);
        eprintln!("Tier A crops checked={crops_checked} with_dots={crops_with_dots}");
        assert!(
            crops_checked >= 3,
            "too few Tier A hubs in frame: {crops_checked}"
        );
        assert!(
            crops_with_dots >= 3,
            "member dots around < 3 Tier A hubs: {crops_with_dots}"
        );
        // Blazing-hub count (DoD 1, FR5): Tier A pins in the slab slice
        // whose premultiplied luminance clears 3.0 (white-hot, above
        // the 1.0 bloom threshold) — ≤ 10 (was ≤ 15). CPU over the
        // descriptor (linear, deterministic): `3.2 × window_vis` with
        // the slab framing (inspector 20°, 30 Mpc at the home depth,
        // fog off, hub floor 0.25) — out-of-slice hubs floor to 0.8
        // (< 3.0, no blaze); Tier B (1.5) and C (≤0.9) never clear 3.0
        // by construction. The PNG white-center census (12×255) is
        // recorded for context (it folds ACES + bloom + white members).
        use crate::cosmic_window::{SlabState, window_vis};
        let slab = SlabState::default_on();
        let slab_half = slab.thickness_mpc * 0.5;
        let eye = inspector.camera.eye();
        let fwd = (inspector.camera.target() - eye).normalize_or_zero();
        let home = web.home().position_mpc;
        let home_depth = ((home[0] as f32 - eye.x) * fwd.x
            + (home[1] as f32 - eye.y) * fwd.y
            + (home[2] as f32 - eye.z) * fwd.z)
            .max(0.0);
        let mut blazing_hubs = 0_usize;
        let mut hubs_in_frame = 0_usize;
        for rank in 0..a_count.min(n) {
            let node = &web.nodes[rank];
            let world = Vec3::new(
                node.position_mpc[0] as f32,
                node.position_mpc[1] as f32,
                node.position_mpc[2] as f32,
            );
            let Some((sx, sy)) = project_to_screen(world, vp_mat, vp) else {
                continue;
            };
            if sx < 0.0 || sy < 0.0 || sx >= 1408.0 || sy >= 768.0 {
                continue;
            }
            hubs_in_frame += 1;
            let depth =
                ((world.x - eye.x) * fwd.x + (world.y - eye.y) * fwd.y + (world.z - eye.z) * fwd.z)
                    .max(0.0);
            // View depth for the window term is the perspective divide
            // (`clip.w`), approximated here by the view-directional
            // depth (the slab-relief precedent in the binary); fog off
            // (inspector full depth), hub floor on.
            let dist = (world - eye).length();
            let vis = window_vis(dist, 0.0, depth, home_depth, slab_half, true);
            if 3.2 * vis >= 3.0 {
                blazing_hubs += 1;
            }
        }
        // PNG white-center census for the record (folds ACES + bloom +
        // white members — context only, not the gate).
        let b_end = ((0.01 + 0.10) * n as f64).ceil() as usize;
        let mut white_centers = 0_usize;
        for rank in 0..b_end.min(n) {
            let node = &web.nodes[rank];
            let world = Vec3::new(
                node.position_mpc[0] as f32,
                node.position_mpc[1] as f32,
                node.position_mpc[2] as f32,
            );
            let Some((sx, sy)) = project_to_screen(world, vp_mat, vp) else {
                continue;
            };
            if sx < 0.0 || sy < 0.0 || sx >= 1408.0 || sy >= 768.0 {
                continue;
            }
            let (cx, cy) = (sx as usize, sy as usize);
            let i = (cy * wusz + cx) * 4;
            if pixels[i] == 255 && pixels[i + 1] == 255 && pixels[i + 2] == 255 {
                white_centers += 1;
            }
        }
        eprintln!(
            "blazing hubs CPU (A in-slice, 3.2*vis>=3.0): {blazing_hubs}/{hubs_in_frame} (PNG white centers A+B: {white_centers})"
        );
        assert!(
            blazing_hubs <= 10,
            "too many blazing hub nodes: {blazing_hubs} (want ≤ 10)"
        );
    }

    /// Demo spawn read (`cosmic-hub-compact-cores` DoD 2): the committed
    /// `demo-after.png` (spawn Chase) shows the goal as a point (bright
    /// core ≤ 12 px) with a discrete-dot swarm around it, and the timed
    /// `demo-after-10mpc.png` (10 Mpc approach, `GAME_DEBUG_DEMO_-
    /// APPROACH_MPC=10`) keeps the swarm discrete (small dots, no wash)
    /// with a byte-different frame (the ship moved). The ≤ 25 % swarm-
    /// height line is ANALYST-judged off the crops (limitation L-5:
    /// whole-frame hot rows span the filament thread when inside it,
    /// so an automated height needs the demo camera in the harness —
    /// re-verify hands-on at `cosmic-vista-reframe`).
    #[test]
    fn demo_after_point_and_approach() {
        let (w, h, spawn) = load_compact_shot_rgba8("demo-after.png");
        assert_eq!((w, h), (1408, 768));
        let (_, core_diam, _) = bright_components(&spawn, w, h, 200);
        eprintln!("demo spawn core_diam={core_diam}");
        assert!(
            core_diam <= 12,
            "spawn goal core too big: {core_diam}px (want ≤ 12)"
        );
        // Spawn swarm: small discrete dots (1–9 px @150), like the slab.
        let (wusz, husz) = (w as usize, h as usize);
        let bright150 = |x: usize, y: usize| {
            let i = (y * wusz + x) * 4;
            spawn[i] >= 150 && spawn[i + 1] >= 150 && spawn[i + 2] >= 150
        };
        let mut seen = vec![false; wusz * husz];
        let mut dots = 0_usize;
        for y in 0..husz {
            for x in 0..wusz {
                if !bright150(x, y) || seen[y * wusz + x] {
                    continue;
                }
                let mut stack = vec![(x, y)];
                seen[y * wusz + x] = true;
                let mut area = 0_usize;
                while let Some((cx, cy)) = stack.pop() {
                    area += 1;
                    for (nx, ny) in [
                        (cx.wrapping_sub(1), cy),
                        (cx + 1, cy),
                        (cx, cy.wrapping_sub(1)),
                        (cx, cy + 1),
                    ] {
                        if nx < wusz && ny < husz && bright150(nx, ny) && !seen[ny * wusz + nx] {
                            seen[ny * wusz + nx] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
                if (1..=9).contains(&area) {
                    dots += 1;
                }
            }
        }
        let hot = bright_fraction_rgba8(&spawn, 200);
        eprintln!("demo spawn dots(1-9px @150)={dots} hot(>=200)={hot:.4}");
        assert!(dots >= 50, "spawn swarm not resolved as dots: {dots}");
        assert!(hot <= 0.5, "spawn washed: hot {hot:.3}");
        let (w2, h2, approach) = load_compact_shot_rgba8("demo-after-10mpc.png");
        assert_eq!((w2, h2), (1408, 768));
        let (_, approach_core, _) = bright_components(&approach, w2, h2, 200);
        let approach_hot = bright_fraction_rgba8(&approach, 200);
        eprintln!("demo approach core={approach_core}px hot={approach_hot:.4}");
        assert!(
            approach_core <= 12,
            "approach core too big: {approach_core}px (want ≤ 12, caps hold close)"
        );
        assert!(
            approach_hot <= 0.5,
            "approach washed: hot {approach_hot:.3}"
        );
        // Byte-inequality: the approach shot differs from spawn (the
        // ship moved 10 Mpc).
        assert_ne!(spawn, approach, "approach shot must differ from spawn");
    }
}
