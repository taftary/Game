//! Bloom mip-chain pass description + write-once structural pin.
//!
//! Pure `Vec<PassDesc>` built by the same shape of code that records the
//! Vulkan commands, so the Intel UHD 620 write-once rule (every bloom
//! target written exactly once, never read by its writing pass) is an
//! executable pin instead of a hope (`plans/v0.3.3/bloom-mip-chain`).

use std::collections::HashSet;

/// Scene (full-res HDR) image id: read-only chain input.
pub const BLOOM_IMG_SCENE: u32 = 0;

/// Down-pyramid image id: `1 + k` for level `k` (½ … ½^levels).
pub fn bloom_img_down(k: u8) -> u32 {
    1 + u32::from(k)
}

/// Up-pyramid image id: `1 + levels + k` for level `k`.
pub fn bloom_img_up(levels: u8, k: u8) -> u32 {
    1 + u32::from(levels) + u32::from(k)
}

/// What a bloom pass does (shader + target pairing, Vulkan-agnostic).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BloomPassKind {
    /// Soft-knee prefilter: scene → `down[0]`.
    Prefilter,
    /// Jimenez 13-tap downsample: `down[k]` → `down[k+1]`.
    Down,
    /// Alias-free copy: `down[last]` → `up[last]`.
    PassThrough,
    /// Tent upsample: `tent(up[k+1])·w + down[k]` → `up[k]`.
    Up,
    /// Composite resolve into the swapchain sink (`u32::MAX`).
    Resolve,
}

/// One recorded pass: the image it writes plus the images it samples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BloomPassDesc {
    /// Image id written (`u32::MAX` = swapchain sink, write-only).
    pub writes: u32,
    /// Image ids sampled by this pass.
    pub reads: Vec<u32>,
    /// Shader + target pairing.
    pub kind: BloomPassKind,
}

/// Full bloom-on chain for `levels` (clamped to [2, 5]): prefilter →
/// downs → pass-through → ups → resolve reading `{scene, up[0]}`.
pub fn describe_bloom_chain(levels: u8) -> Vec<BloomPassDesc> {
    let levels = levels.clamp(2, 5);
    let mut passes = Vec::with_capacity(2 * usize::from(levels) + 1);
    passes.push(BloomPassDesc {
        writes: bloom_img_down(0),
        reads: vec![BLOOM_IMG_SCENE],
        kind: BloomPassKind::Prefilter,
    });
    for k in 0..levels - 1 {
        passes.push(BloomPassDesc {
            writes: bloom_img_down(k + 1),
            reads: vec![bloom_img_down(k)],
            kind: BloomPassKind::Down,
        });
    }
    let last = levels - 1;
    passes.push(BloomPassDesc {
        writes: bloom_img_up(levels, last),
        reads: vec![bloom_img_down(last)],
        kind: BloomPassKind::PassThrough,
    });
    for k in (0..last).rev() {
        passes.push(BloomPassDesc {
            writes: bloom_img_up(levels, k),
            reads: vec![bloom_img_up(levels, k + 1), bloom_img_down(k)],
            kind: BloomPassKind::Up,
        });
    }
    passes.push(BloomPassDesc {
        writes: u32::MAX,
        reads: vec![BLOOM_IMG_SCENE, bloom_img_up(levels, 0)],
        kind: BloomPassKind::Resolve,
    });
    passes
}

/// Bloom-off chain (`GAME_DEBUG_COSMIC_BLOOM=0`): prefilter only, then
/// resolve reading `{scene, down[0]}`.
pub fn describe_bloom_chain_bloom_off(levels: u8) -> Vec<BloomPassDesc> {
    // Accepted for call-site symmetry; the off chain is always two
    // passes regardless of tier.
    let _ = levels.clamp(2, 5);
    vec![
        BloomPassDesc {
            writes: bloom_img_down(0),
            reads: vec![BLOOM_IMG_SCENE],
            kind: BloomPassKind::Prefilter,
        },
        BloomPassDesc {
            writes: u32::MAX,
            reads: vec![BLOOM_IMG_SCENE, bloom_img_down(0)],
            kind: BloomPassKind::Resolve,
        },
    ]
}

/// Write-once pin: every written image except the swapchain sink is
/// written exactly once, and no pass samples the image it writes.
pub fn assert_write_once(passes: &[BloomPassDesc]) -> Result<(), String> {
    let mut written = HashSet::new();
    for (index, pass) in passes.iter().enumerate() {
        if pass.reads.contains(&pass.writes) {
            return Err(format!(
                "pass {index} ({:?}) reads the image it writes ({})",
                pass.kind, pass.writes
            ));
        }
        if pass.writes != u32::MAX && !written.insert(pass.writes) {
            return Err(format!(
                "image {} written more than once (second write at pass {index})",
                pass.writes
            ));
        }
    }
    Ok(())
}

/// Image count for `levels` (clamped): scene + down[] + up[].
pub fn bloom_image_count(levels: u8) -> usize {
    1 + 2 * usize::from(levels.clamp(2, 5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_once_green_for_all_tier_levels() {
        for levels in [2, 3, 4, 5] {
            let chain = describe_bloom_chain(levels);
            assert_eq!(chain.len(), 2 * usize::from(levels) + 1, "{levels}");
            assert_write_once(&chain).expect("bloom-on chain must be write-once");
            assert_write_once(&describe_bloom_chain_bloom_off(levels))
                .expect("bloom-off chain must be write-once");
        }
    }

    #[test]
    fn resolve_reads_the_top_up_level() {
        for levels in [2, 3, 4, 5] {
            let clamped = levels.clamp(2, 5);
            let chain = describe_bloom_chain(levels);
            let resolve = chain.last().expect("chain ends with resolve");
            assert_eq!(resolve.kind, BloomPassKind::Resolve);
            assert_eq!(resolve.writes, u32::MAX);
            assert_eq!(
                resolve.reads,
                vec![BLOOM_IMG_SCENE, bloom_img_up(clamped, 0)]
            );
        }
        let off = describe_bloom_chain_bloom_off(4);
        let resolve = off.last().expect("off chain ends with resolve");
        assert_eq!(resolve.reads, vec![BLOOM_IMG_SCENE, bloom_img_down(0)]);
    }

    #[test]
    fn image_count_matches_scene_plus_pyramids() {
        for levels in [2, 3, 4, 5] {
            assert_eq!(
                bloom_image_count(levels),
                1 + 2 * usize::from(levels),
                "{levels}"
            );
        }
    }

    #[test]
    fn levels_clamp_to_pyramid_range() {
        assert_eq!(describe_bloom_chain(0), describe_bloom_chain(2));
        assert_eq!(describe_bloom_chain(99), describe_bloom_chain(5));
        assert_eq!(bloom_image_count(0), bloom_image_count(2));
        assert_eq!(bloom_image_count(99), bloom_image_count(5));
    }

    #[test]
    fn double_write_and_self_read_fail_the_pin() {
        // Hand-built double write: down[0] written twice.
        let double_write = vec![
            BloomPassDesc {
                writes: bloom_img_down(0),
                reads: vec![BLOOM_IMG_SCENE],
                kind: BloomPassKind::Prefilter,
            },
            BloomPassDesc {
                writes: bloom_img_down(0),
                reads: vec![BLOOM_IMG_SCENE],
                kind: BloomPassKind::Down,
            },
        ];
        assert!(assert_write_once(&double_write).is_err());
        // Hand-built self read: a pass sampling its own target.
        let self_read = vec![BloomPassDesc {
            writes: bloom_img_down(1),
            reads: vec![bloom_img_down(1)],
            kind: BloomPassKind::Down,
        }];
        assert!(assert_write_once(&self_read).is_err());
    }
}
