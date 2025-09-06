use anyhow::{anyhow, Result};
use std::{borrow::Cow, io::Cursor};

/// Decoded texture data from a TFD/TFH pair.
pub struct TfdImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// Decode a TFD data stream with help from the accompanying TFH header.
///
/// Supports raw BC1/BC3 textures and container-compressed streams (zstd
/// compressed) which typically hold BC5 normal maps. Only the top mip level is
/// expanded to RGBA pixels.
pub fn decode(tfd: &[u8], tfh: &[u8]) -> Result<TfdImage> {
    // Tile-compressed TFDs aren't multiples of 8 bytes. Those streams are
    // zstd-compressed; decode them into a temporary buffer first.
    let (raw, is_compressed) = if tfd.len() % 8 == 0 {
        (Cow::Borrowed(tfd), false)
    } else {
        let data = zstd::stream::decode_all(Cursor::new(tfd))?;
        (Cow::Owned(data), true)
    };

    // Try to infer the top-level dimension, mip count and block footprint from
    // the raw size, optionally using the TFH's dimension hint as a tie breaker.
    let (top, _mips, fp) = guess_from_tfd_len(raw.len())
        .or_else(|| {
            let hint = tfh_dim_hint(tfh)?;
            [BcFootprint::Bc1_4, BcFootprint::Bc3_5_7]
                .into_iter()
                .find_map(|bpb| {
                    for mips in 1..10 {
                        if sum_bc_bytes(hint, bpb as usize, mips) == raw.len() {
                            return Some((hint, mips, bpb));
                        }
                    }
                    None
                })
        })
        .ok_or_else(|| anyhow!("Cannot infer BC footprint/mips from TFD length"))?;

    let width = top;
    let height = top;
    let rgba = if is_compressed {
        // Compressed streams in the samples are BC5 normal maps.
        decode_bc5_top_mip_to_rgba(&raw, width, height)?
    } else {
        match fp {
            BcFootprint::Bc1_4 => decode_bc1_top_mip_to_rgba(&raw, width, height)?,
            BcFootprint::Bc3_5_7 => decode_bc3_top_mip_to_rgba(&raw, width, height)?,
        }
    };

    Ok(TfdImage { width, height, rgba })
}

#[derive(Clone, Copy, Debug)]
enum BcFootprint {
    Bc1_4 = 8,
    Bc3_5_7 = 16,
}

fn sum_bc_bytes(top: usize, bpb: usize, mips: usize) -> usize {
    let mut total = 0usize;
    for m in 0..mips {
        let w = (top >> m).max(1);
        let h = (top >> m).max(1);
        let bw = (w + 3) / 4;
        let bh = (h + 3) / 4;
        total += bw * bh * bpb;
    }
    total
}

fn guess_from_tfd_len(tfd_len: usize) -> Option<(usize, usize, BcFootprint)> {
    for &bpb in [BcFootprint::Bc1_4, BcFootprint::Bc3_5_7].iter() {
        let bpbv = bpb as usize;
        for top in [4096, 2048, 1024, 512, 256, 128, 64] {
            for mips in 1..10 {
                if sum_bc_bytes(top, bpbv, mips) == tfd_len {
                    return Some((top, mips, bpb));
                }
            }
        }
    }
    None
}

fn tfh_dim_hint(tfh: &[u8]) -> Option<usize> {
    if tfh.len() >= 0xA4 {
        let w = u32::from_le_bytes(tfh[0xA0..0xA4].try_into().ok()?) as usize;
        if w.is_power_of_two() && (64..=8192).contains(&w) {
            return Some(w);
        }
    }
    None
}

fn decode_bc3_top_mip_to_rgba(src: &[u8], w: usize, h: usize) -> Result<Vec<u8>> {
    let bw = (w + 3) / 4;
    let bh = (h + 3) / 4;
    let mut rgba = vec![0u8; w * h * 4];
    let pitch = w * 4;
    let bpb = 16usize;
    for y in 0..bh {
        for x in 0..bw {
            let off = (y * bw + x) * bpb;
            let block = &src[off..off + bpb];
            let mut tmp = [0u8; 4 * 4 * 4];
            bcdec_rs::bc3(block, &mut tmp, 4 * 4);
            for row in 0..4 {
                let dst = (y * 4 + row) * pitch + x * 4 * 4;
                let src_row = row * 4 * 4;
                rgba[dst..dst + 4 * 4]
                    .copy_from_slice(&tmp[src_row..src_row + 4 * 4]);
            }
        }
    }
    Ok(rgba)
}

fn decode_bc5_top_mip_to_rgba(src: &[u8], w: usize, h: usize) -> Result<Vec<u8>> {
    let bw = (w + 3) / 4;
    let bh = (h + 3) / 4;
    let mut rgba = vec![0u8; w * h * 4];
    let pitch = w * 4;
    let bpb = 16usize;
    for y in 0..bh {
        for x in 0..bw {
            let off = (y * bw + x) * bpb;
            let block = &src[off..off + bpb];
            let mut tmp = [0u8; 4 * 4 * 4];
            bcdec_rs::bc5(block, &mut tmp, 4 * 4, true);
            for row in 0..4 {
                let dst = (y * 4 + row) * pitch + x * 4 * 4;
                let src_row = row * 4 * 4;
                rgba[dst..dst + 4 * 4]
                    .copy_from_slice(&tmp[src_row..src_row + 4 * 4]);
            }
        }
    }
    Ok(rgba)
}

fn rgb565_to_888(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 0x1F) as u32;
    let g = ((c >> 5) & 0x3F) as u32;
    let b = (c & 0x1F) as u32;
    [
        ((r * 255 + 15) / 31) as u8,
        ((g * 255 + 31) / 63) as u8,
        ((b * 255 + 15) / 31) as u8,
    ]
}

fn decode_bc1_top_mip_to_rgba(src: &[u8], w: usize, h: usize) -> Result<Vec<u8>> {
    let bw = (w + 3) / 4;
    let bh = (h + 3) / 4;
    let mut out = vec![0u8; w * h * 4];
    let mut off = 0usize;
    for by in 0..bh {
        for bx in 0..bw {
            let c0 = u16::from_le_bytes([src[off + 0], src[off + 1]]);
            let c1 = u16::from_le_bytes([src[off + 2], src[off + 3]]);
            let mut idx = u32::from_le_bytes([
                src[off + 4],
                src[off + 5],
                src[off + 6],
                src[off + 7],
            ]);
            off += 8;

            let p0 = rgb565_to_888(c0);
            let p1 = rgb565_to_888(c1);
            let (p2, p3, use_transparent) = if c0 > c1 {
                (
                    [
                        ((2 * p0[0] as u16 + p1[0] as u16) / 3) as u8,
                        ((2 * p0[1] as u16 + p1[1] as u16) / 3) as u8,
                        ((2 * p0[2] as u16 + p1[2] as u16) / 3) as u8,
                    ],
                    [
                        ((p0[0] as u16 + 2 * p1[0] as u16) / 3) as u8,
                        ((p0[1] as u16 + 2 * p1[1] as u16) / 3) as u8,
                        ((p0[2] as u16 + 2 * p1[2] as u16) / 3) as u8,
                    ],
                    false,
                )
            } else {
                (
                    [
                        ((p0[0] as u16 + p1[0] as u16) / 2) as u8,
                        ((p0[1] as u16 + p1[1] as u16) / 2) as u8,
                        ((p0[2] as u16 + p1[2] as u16) / 2) as u8,
                    ],
                    [0, 0, 0],
                    true,
                )
            };
            let pal = [p0, p1, p2, p3];

            for py in 0..4 {
                for px in 0..4 {
                    let code = (idx & 0x3) as usize;
                    idx >>= 2;
                    let x = bx * 4 + px;
                    let y = by * 4 + py;
                    if x < w && y < h {
                        let o = (y * w + x) * 4;
                        let rgb = pal[code];
                        out[o + 0] = rgb[0];
                        out[o + 1] = rgb[1];
                        out[o + 2] = rgb[2];
                        out[o + 3] = if use_transparent && code == 3 { 0 } else { 255 };
                    }
                }
            }
        }
    }
    Ok(out)
}
