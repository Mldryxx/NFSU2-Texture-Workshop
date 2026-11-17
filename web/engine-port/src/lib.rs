//! Small, platform-neutral decoder layer for the Binarius/Darius port.
//! The decoder validates the 16-byte EA LZ header before allocating output.

use std::convert::TryInto;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    HeaderTooShort,
    UnknownCodec,
    InvalidHeader,
    InvalidSize,
    TruncatedInput,
    InvalidBackReference,
    OutputOverflow,
}

fn read_u32(data: &[u8], offset: usize) -> Result<usize, CodecError> {
    let bytes: [u8; 4] = data.get(offset..offset + 4)
        .ok_or(CodecError::HeaderTooShort)?
        .try_into().map_err(|_| CodecError::HeaderTooShort)?;
    Ok(u32::from_le_bytes(bytes) as usize)
}

/// Decode a Darius/EA 16-byte compression wrapper.
/// Supports RAWW (stored payload) and JDLZ v2. HUFF/COMP/RFPK are explicit errors
/// until their codecs have independent fixtures and tests.
pub fn decompress(input: &[u8], max_output: usize) -> Result<Vec<u8>, CodecError> {
    if input.len() < 16 { return Err(CodecError::HeaderTooShort); }
    let tag = &input[0..4];
    let out_len = read_u32(input, 8)?;
    let comp_len = read_u32(input, 12)?;
    if out_len > max_output || comp_len < 16 || comp_len > input.len() {
        return Err(CodecError::InvalidSize);
    }
    let src = &input[..comp_len];

    match tag {
        b"RAWW" => {
            if src.len() - 16 != out_len { return Err(CodecError::InvalidSize); }
            Ok(src[16..].to_vec())
        }
        b"JDLZ" => decompress_jdlz(src, out_len),
        b"HUFF" | b"COMP" | b"RFPK" => Err(CodecError::UnknownCodec),
        _ => Err(CodecError::UnknownCodec),
    }
}

fn decompress_jdlz(src: &[u8], out_len: usize) -> Result<Vec<u8>, CodecError> {
    if src[4] != 0x02 || src[5] != 0x10 || src[6] != 0 || src[7] != 0 {
        return Err(CodecError::InvalidHeader);
    }
    let mut out = Vec::with_capacity(out_len);
    let mut ip = 16usize;
    let mut flags1 = 1u16;
    let mut flags2 = 1u16;

    while out.len() < out_len {
        if flags1 == 1 {
            let b = *src.get(ip).ok_or(CodecError::TruncatedInput)?;
            ip += 1;
            flags1 = b as u16 | 0x100;
        }
        if flags2 == 1 {
            let b = *src.get(ip).ok_or(CodecError::TruncatedInput)?;
            ip += 1;
            flags2 = b as u16 | 0x100;
        }

        if flags1 & 1 == 1 {
            let b0 = *src.get(ip).ok_or(CodecError::TruncatedInput)?;
            let b1 = *src.get(ip + 1).ok_or(CodecError::TruncatedInput)?;
            ip += 2;

            let (length, distance) = if flags2 & 1 == 1 {
                ((((b0 as usize & 0xF0) << 4) | b1 as usize) + 3,
                 (b0 as usize & 0x0F) + 1)
            } else {
                (((b0 as usize & 0x1F) + 3,
                  ((b0 as usize & 0xE0) << 3 | b1 as usize) + 17)
            };

            if distance == 0 || distance > out.len() {
                return Err(CodecError::InvalidBackReference);
            }
            if out.len().checked_add(length).filter(|&n| n <= out_len).is_none() {
                return Err(CodecError::OutputOverflow);
            }
            for _ in 0..length {
                let value = out[out.len() - distance];
                out.push(value);
            }
            flags2 >>= 1;
        } else {
            let value = *src.get(ip).ok_or(CodecError::TruncatedInput)?;
            ip += 1;
            out.push(value);
        }
        flags1 >>= 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(tag: &[u8; 4], out: u32, comp: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(tag);
        v.extend_from_slice(&[2, 16, 0, 0]);
        v.extend_from_slice(&out.to_le_bytes());
        v.extend_from_slice(&comp.to_le_bytes());
        v
    }

    #[test]
    fn raww_round_trip_payload() {
        let mut v = header(b"RAWW", 3, 19);
        v.extend_from_slice(b"abc");
        assert_eq!(decompress(&v, 1024).unwrap(), b"abc");
    }

    #[test]
    fn jdlz_literal() {
        let mut v = header(b"JDLZ", 1, 19);
        v.extend_from_slice(&[0, 0, b'Z']);
        assert_eq!(decompress(&v, 1024).unwrap(), b"Z");
    }

    #[test]
    fn jdlz_near_back_reference() {
        let mut v = header(b"JDLZ", 4, 20);
        v.extend_from_slice(&[0b0000_0010, 1, b'A', 0, 0]);
        assert_eq!(decompress(&v, 1024).unwrap(), b"AAAA");
    }

    #[test]
    fn rejects_oversized_output() {
        let mut v = header(b"RAWW", 5000, 19);
        v.extend_from_slice(b"abc");
        assert_eq!(decompress(&v, 100).unwrap_err(), CodecError::InvalidSize);
    }
}
