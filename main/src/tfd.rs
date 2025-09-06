use anyhow::{Result, bail};

/// Decoded texture data from a TFD/TFH pair.
pub struct TfdImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// Decode a TFD data stream with help from the accompanying TFH header.
///
/// Only the top mip level is decoded and only uncompressed 16 byte block
/// formats (BC3/DXT5 etc.) are supported for now.
pub fn decode(tfd: &[u8], tfh: &[u8]) -> Result<TfdImage> {
    let (width, height) = parse_tfh(tfh)?;
    // For now assume 16 bytes per block (BC3/DXT5/BC7).
    let bpb = 16usize;
    let bw = (width + 3) / 4;
    let bh = (height + 3) / 4;
    let expected = bw * bh * bpb;
    if tfd.len() < expected {
        bail!("TFD too small: expected at least {expected} bytes, got {}", tfd.len());
    }
    let mut rgba = vec![0u8; width * height * 4];
    let pitch = width * 4;
    for y in 0..bh {
        for x in 0..bw {
            let off = (y * bw + x) * bpb;
            let dest = (y * 4 * pitch) + (x * 4 * 4);
            let block = &tfd[off..off + bpb];
            let dst_slice = &mut rgba[dest .. dest + pitch * 4];
            // decode as BC3/DXT5
            bcdec_rs::bc3(block, dst_slice, pitch);
        }
    }
    Ok(TfdImage { width, height, rgba })
}

fn parse_tfh(tfh: &[u8]) -> Result<(usize, usize)> {
    if tfh.len() < 0x7C { bail!("TFH header too small"); }
    let mut largest: u32 = 0;
    for i in 0..5 {
        let size = u32::from_le_bytes(tfh[0x40 + i * 8 + 4..0x40 + i * 8 + 8].try_into().unwrap());
        if size > largest { largest = size; }
    }
    if largest == 0 { bail!("TFH contains no tile info"); }
    let tiles_per_side = (largest as f32).sqrt() as usize;
    let dim = tiles_per_side * 16;
    Ok((dim, dim))
}
