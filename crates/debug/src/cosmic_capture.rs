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
}
