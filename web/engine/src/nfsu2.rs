//! Independent NFSU2 compressed-car TPK primitives.
//! Based on the documented 24-byte slot records and JDLZ block format.
//! This module deliberately does not depend on gizmo-nfs.
use std::convert::TryInto;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NfsError {
    Truncated(&'static str),
    Invalid(&'static str),
    OutOfRange,
    Unsupported(&'static str),
}
impl std::fmt::Display for NfsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl std::error::Error for NfsError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureSlot {
    pub key: u32,
    pub offset: u32,
    pub compressed_size: u32,
    pub decoded_size: u32,
    pub header_from_end: u32,
    pub unknown: u32,
}
impl TextureSlot {
    pub const SIZE: usize = 24;
    pub fn parse_table(bytes: &[u8]) -> Result<Vec<Self>, NfsError> {
        if bytes.len() % Self::SIZE != 0 { return Err(NfsError::Invalid("slot table length is not a multiple of 24")); }
        let mut slots = Vec::with_capacity(bytes.len() / Self::SIZE);
        for r in bytes.chunks_exact(Self::SIZE) {
            let v = |n: usize| u32::from_le_bytes(r[n..n+4].try_into().unwrap());
            slots.push(Self { key:v(0), offset:v(4), compressed_size:v(8), decoded_size:v(12), header_from_end:v(16), unknown:v(20) });
        }
        Ok(slots)
    }
    pub fn block<'a>(&self, archive: &'a [u8]) -> Result<&'a [u8], NfsError> {
        let start = self.offset as usize;
        let end = start.checked_add(self.compressed_size as usize).ok_or(NfsError::OutOfRange)?;
        archive.get(start..end).ok_or(NfsError::OutOfRange)
    }
    pub fn header_offset(&self, decoded_len: usize) -> Result<usize, NfsError> {
        decoded_len.checked_sub(self.header_from_end as usize)
            .and_then(|v| v.checked_add(0x88))
            .filter(|v| *v <= decoded_len && decoded_len.saturating_sub(*v) >= 0x28)
            .ok_or(NfsError::Invalid("embedded texture header is out of range"))
    }
}

pub fn is_jdlz(data: &[u8]) -> bool { data.len() >= 16 && &data[..4] == b"JDLZ" }
pub fn is_huff(data: &[u8]) -> bool { data.len() >= 16 && &data[..6] == b"HUFF\x01\x10" }

pub fn decompress_jdlz(data: &[u8]) -> Result<Vec<u8>, NfsError> {
    if !is_jdlz(data) { return Err(NfsError::Invalid("not a JDLZ block")); }
    let raw_size = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
    let comp_size = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
    if raw_size == 0 || comp_size < 16 || comp_size > data.len() { return Err(NfsError::Invalid("bad JDLZ sizes")); }
    let src = &data[..comp_size];
    let mut out = vec![0u8; raw_size];
    let (mut i, mut o, mut f1, mut f2) = (16usize, 0usize, 1u16, 1u16);
    while i < src.len() && o < raw_size {
        if f1 == 1 { let b=*src.get(i).ok_or(NfsError::Truncated("JDLZ flag1"))?; i+=1; f1=b as u16|0x100; }
        if f2 == 1 { let b=*src.get(i).ok_or(NfsError::Truncated("JDLZ flag2"))?; i+=1; f2=b as u16|0x100; }
        if f1 & 1 != 0 {
            let b0=*src.get(i).ok_or(NfsError::Truncated("JDLZ reference"))?;
            let b1=*src.get(i+1).ok_or(NfsError::Truncated("JDLZ reference"))?;
            i+=2;
            let (length, dist) = if f2 & 1 != 0 {
                ((((b0 as usize & 0xf0) << 4) | b1 as usize) + 3, (b0 as usize & 0x0f) + 1)
            } else {
                ((((b0 as usize & 0x1f)) + 3), (((b0 as usize & 0xe0) << 3) | b1 as usize) + 17)
            };
            if dist > o || o.checked_add(length).filter(|v| *v <= raw_size).is_none() { return Err(NfsError::Invalid("JDLZ back-reference out of range")); }
            for k in 0..length { out[o+k] = out[o+k-dist]; }
            o += length;
            f2 >>= 1;
        } else {
            out[o] = *src.get(i).ok_or(NfsError::Truncated("JDLZ literal"))?;
            o += 1; i += 1;
        }
        f1 >>= 1;
    }
    if o != raw_size { return Err(NfsError::Invalid("JDLZ decoded length mismatch")); }
    Ok(out)
}

pub fn decompress_block(data: &[u8], max_output: usize) -> Result<Vec<u8>, NfsError> {
    if is_jdlz(data) {
        let raw = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        if raw > max_output { return Err(NfsError::Invalid("decompressed block exceeds limit")); }
        decompress_jdlz(data)
    } else if is_huff(data) {
        Err(NfsError::Unsupported("HUFF decoder is the next engine module"))
    } else {
        if data.len() > max_output { return Err(NfsError::Invalid("raw block exceeds limit")); }
        Ok(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_slot_records_without_changing_values() {
        let mut b=Vec::new();
        for v in [0x12345678u32, 64, 100, 80, 256, 7] { b.extend_from_slice(&v.to_le_bytes()); }
        let s=TextureSlot::parse_table(&b).unwrap();
        assert_eq!(s, vec![TextureSlot{key:0x12345678,offset:64,compressed_size:100,decoded_size:80,header_from_end:256,unknown:7}]);
        assert!(TextureSlot::parse_table(&b[..23]).is_err());
    }
    #[test]
    fn decodes_literal_only_jdlz() {
        // 3 literals; f1=000, f2=000, followed by ABC
        let mut b=b"JDLZ".to_vec();
        b.extend_from_slice(&0x1002u32.to_le_bytes());
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&21u32.to_le_bytes());
        b.extend_from_slice(&[0,0,b'A',b'B',b'C']);
        assert_eq!(decompress_jdlz(&b).unwrap(), b"ABC");
    }
    #[test]
    fn rejects_bad_backreference_and_truncation() {
        let mut b=b"JDLZ".to_vec();
        b.extend_from_slice(&0x1002u32.to_le_bytes());
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&20u32.to_le_bytes());
        b.extend_from_slice(&[1,1,0,0]);
        assert!(decompress_jdlz(&b).is_err());
    }
    #[test]
    fn rejects_slot_outside_archive() {
        let s=TextureSlot{key:1,offset:8,compressed_size:9,decoded_size:0,header_from_end:0,unknown:0};
        assert!(s.block(&[0;16]).is_err());
    }
}
