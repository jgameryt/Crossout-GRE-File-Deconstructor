use anyhow::{Result, anyhow};
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct MdlChunk {
    pub header_off: usize,
    pub stride: u8,
    pub fmt_tag: u8,
    pub vcount: u32,
    pub icount: u32,
    pub codes: [u16;3],
    pub offs_bytes: [u16;3], // offsets in bytes (header stores 1/256 byte units)
    pub vertices: Vec<[f32;3]>,
    pub indices: Vec<[u32;3]>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelKey {
    pub fmt_tag: u8,
    pub stride: u8,
    pub codes: [u16;3],
}

#[derive(Clone, Debug)]
pub struct ModelGroup {
    pub key: ModelKey,
    pub lods: Vec<usize>, // indices into chunks
}

fn f16_to_f32(u: u16) -> f32 {
    let s = ((u >> 15) & 1) as i32;
    let e = ((u >> 10) & 0x1f) as i32;
    let f = (u & 0x3ff) as i32;
    if e == 0 {
        if f == 0 { return if s!=0 { -0.0 } else { 0.0 }; }
        let sign = if s!=0 { -1.0 } else { 1.0 };
        return sign * (2f32).powi(-14) * (f as f32 / 1024.0);
    }
    if e == 31 {
        return if f != 0 { f32::NAN } else { if s!=0 { f32::NEG_INFINITY } else { f32::INFINITY } };
    }
    let sign = if s!=0 { -1.0 } else { 1.0 };
    sign * (2f32).powi(e-15) * (1.0 + (f as f32)/1024.0)
}

fn read_u32_le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off+1], b[off+2], b[off+3]])
}
fn read_i16_le(b: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([b[off], b[off+1]])
}
fn read_u16_le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off+1]])
}
fn read_f32_le(b: &[u8], off: usize) -> f32 {
    f32::from_le_bytes([b[off], b[off+1], b[off+2], b[off+3]])
}

pub fn parse_all_chunks(bytes: &[u8]) -> Result<Vec<MdlChunk>> {
    let mut chunks: Vec<MdlChunk> = Vec::new();
    let mut off = 0usize;
    while off + 0x110 <= bytes.len() {
        // Heuristic: header at 'off' if [0x9C]=1, [0x9D]=0 and fmt is 4/5 with reasonable stride
        let sig_ok = bytes[off + 0x9C] == 1 && bytes[off + 0x9D] == 0;
        let fmt_tag = bytes[off + 0x9E];
        let stride = bytes[off + 0x9F];
        let fmt_ok = fmt_tag == 0x04 || fmt_tag == 0x05;
        let stride_ok = (4..=64).contains(&stride);
        if sig_ok && fmt_ok && stride_ok {
            // Read counts
            let vcount = read_u32_le(bytes, off + 0xA4);
            let icount = read_u32_le(bytes, off + 0xA8);
            let vaddr = off + 0x110;
            let iaddr = vaddr.checked_add(stride as usize * vcount as usize).unwrap_or(usize::MAX);
            let iend  = iaddr.checked_add(2 * icount as usize).unwrap_or(usize::MAX);
            if iend <= bytes.len() && vaddr < bytes.len() && iaddr <= bytes.len() {
                // Read descriptor codes and offsets (convert to bytes by >>8)
                let code0 = read_u16_le(bytes, off + 0x64);
                let off0b = (read_u16_le(bytes, off + 0x66) >> 8) as u16;
                let code1 = read_u16_le(bytes, off + 0x68);
                let off1b = (read_u16_le(bytes, off + 0x6A) >> 8) as u16;
                let code2 = read_u16_le(bytes, off + 0x6C);
                let off2b = (read_u16_le(bytes, off + 0x6E) >> 8) as u16;
                let codes = [code0, code1, code2];
                let offs_bytes = [off0b, off1b, off2b];
                // Read vertices (positions only for preview)
                let mut vertices: Vec<[f32;3]> = Vec::with_capacity(vcount as usize);
                for i in 0..(vcount as usize) {
                    let base = vaddr + i*stride as usize;
                    let pos = if fmt_tag == 0x04 {
                        [ read_f32_le(bytes, base + 0),
                          read_f32_le(bytes, base + 4),
                          read_f32_le(bytes, base + 8) ]
                    } else {
                        let x = f16_to_f32(read_u16_le(bytes, base+0));
                        let y = f16_to_f32(read_u16_le(bytes, base+2));
                        let z = f16_to_f32(read_u16_le(bytes, base+4));
                        [x,y,z]
                    };
                    vertices.push(pos);
                }
                // Read indices
                let mut indices: Vec<[u32;3]> = Vec::with_capacity(icount as usize / 3);
                let mut j = 0usize;
                while j + 6 <= (2*icount as usize) {
                    let a = read_u16_le(bytes, iaddr + j) as u32;
                    let b = read_u16_le(bytes, iaddr + j + 2) as u32;
                    let c = read_u16_le(bytes, iaddr + j + 4) as u32;
                    indices.push([a,b,c]);
                    j += 6;
                }
                chunks.push(MdlChunk {
                    header_off: off,
                    stride, fmt_tag, vcount, icount,
                    codes, offs_bytes,
                    vertices, indices
                });
                // Jump near end of this chunk to continue scanning
                off = iend;
                continue;
            }
        }
        off += 0x10; // step by 16 bytes for speed; header seems aligned
    }
    if chunks.is_empty() {
        Err(anyhow!("No MDL chunks found"))
    } else {
        Ok(chunks)
    }
}

pub fn group_models(chunks: &[MdlChunk]) -> Vec<ModelGroup> {
    // Group by (fmt_tag, stride, codes) – this is stable across LODs for the same model family in your assets
    let mut map: BTreeMap<ModelKey, Vec<(usize, u32)>> = BTreeMap::new();
    for (idx, ch) in chunks.iter().enumerate() {
        let key = ModelKey { fmt_tag: ch.fmt_tag, stride: ch.stride, codes: ch.codes };
        map.entry(key).or_default().push((idx, ch.vcount));
    }
    let mut groups: Vec<ModelGroup> = Vec::with_capacity(map.len());
    for (key, mut list) in map {
        // Order by vertex count descending => LOD0, LOD1, ...
        list.sort_by(|a,b| b.1.cmp(&a.1));
        let lods = list.into_iter().map(|(i,_)| i).collect();
        groups.push(ModelGroup { key, lods });
    }
    groups
}

