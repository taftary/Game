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
}
