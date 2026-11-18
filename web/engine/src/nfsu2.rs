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


struct BitReader<'a> { data: &'a [u8], at: usize, value: u64, count: u32 }
impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self { Self { data, at:0, value:0, count:0 } }
    fn peek(&mut self, n: u32) -> Result<u32,NfsError> {
        while self.count < n && self.at < self.data.len() {
            self.value = (self.value << 8) | self.data[self.at] as u64;
            self.count += 8; self.at += 1;
        }
        if self.count < n { return Ok((self.value << (n-self.count)) as u32); }
        Ok((self.value >> (self.count-n)) as u32)
    }
    fn get(&mut self, n: u32) -> Result<u32,NfsError> {
        let v=self.peek(n)?;
        if self.count < n { return Err(NfsError::Truncated("HUFF bitstream")); }
        self.count -= n;
        self.value &= if self.count == 64 { u64::MAX } else if self.count == 0 { 0 } else { (1u64 << self.count)-1 };
        Ok(v)
    }
    fn number(&mut self) -> Result<usize,NfsError> {
        let mut width=2u32;
        while self.get(1)? == 0 { width+=1; if width>30 { return Err(NfsError::Invalid("HUFF unbounded integer")); } }
        Ok((self.get(width)? as usize) + (1usize << width) - 4)
    }
}
pub fn decompress_huff(data: &[u8], max_output: usize) -> Result<Vec<u8>,NfsError> {
    if !is_huff(data) { return Err(NfsError::Invalid("not a HUFF block")); }
    let expected=u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
    let packed=u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
    if expected==0 || expected>max_output || packed==0 || packed>data.len()-16 { return Err(NfsError::Invalid("HUFF sizes")); }
    let mut bits=BitReader::new(&data[16..16+packed]);
    let mut kind=bits.get(16)? as u16;
    if kind & 0x100 != 0 { bits.get(if kind & 0x8000 != 0 {32} else {24})?; }
    kind &= !0x100;
    if ![0x30fb,0x32fb,0x34fb,0xb0fb,0xb2fb,0xb4fb].contains(&kind) { return Err(NfsError::Unsupported("HUFF coding kind")); }
    let inner=bits.get(if kind & 0x8000 != 0 {32} else {24})? as usize;
    if inner != expected { return Err(NfsError::Invalid("HUFF inner size mismatch")); }
    let clue=bits.get(8)? as u8;
    let mut entries:Vec<(u32,u32,u32)>=Vec::new();
    let (mut base,mut total)=(0u32,0usize);
    let mut complete=false;
    for width in 1..=16u32 {
        base <<= 1;
        let n=bits.number()? as u32;
        if total+n as usize>256 || base+n > (1u32<<width) { return Err(NfsError::Invalid("HUFF bad code table")); }
        entries.push((width,base,n)); base+=n; total+=n as usize;
        if base == (1u32<<width) { complete=true; break; }
    }
    if !complete { return Err(NfsError::Invalid("HUFF incomplete code table")); }
    let mut used=[false;256]; let mut symbols=Vec::with_capacity(total); let mut cur=255usize;
    for _ in 0..total {
        let skip=bits.number()?.checked_add(1).ok_or(NfsError::Invalid("HUFF symbol skip"))?;
        if skip>256-symbols.len() { return Err(NfsError::Invalid("HUFF bad symbol order")); }
        let mut left=skip;
        while left>0 {
            cur=(cur+1)&255;
            if !used[cur] { left-=1; }
        }
        used[cur]=true; symbols.push(cur as u8);
    }
    let mut quick:Vec<Option<(u32,u8)>>=vec![None;256];
    let mut full=std::collections::HashMap::<(u32,u32),u8>::new();
    let mut k=0usize;
    for (width,start,n) in entries {
        for code in start..start+n {
            let sym=symbols[k]; k+=1; full.insert((width,code),sym);
            if width<=8 {
                let step=1usize << (8-width);
                for q in (code as usize*step)..((code as usize+1)*step) { quick[q]=Some((width,sym)); }
            }
        }
    }
    let mut out=Vec::with_capacity(expected);
    loop {
        let first=bits.peek(8)? as usize;
        let sym=if let Some((width,s))=quick[first] { bits.get(width)?; s } else {
            let mut found=None;
            for width in 9..=16u32 {
                let code=bits.peek(width)?;
                if let Some(s)=full.get(&(width,code)) { bits.get(width)?; found=Some(*s); break; }
            }
            found.ok_or(NfsError::Invalid("HUFF unknown code"))?
        };
        if sym != clue { out.push(sym); }
        else {
            let run=bits.number()?;
            if run>0 {
                if out.is_empty() || out.len().checked_add(run).filter(|v|*v<=expected).is_none() { return Err(NfsError::Invalid("HUFF invalid run")); }
                out.extend(std::iter::repeat(*out.last().unwrap()).take(run));
            } else if bits.get(1)? != 0 { break; }
            else { out.push(bits.get(8)? as u8); }
        }
        if out.len()>expected { return Err(NfsError::Invalid("HUFF output overrun")); }
    }
    if out.len()!=expected { return Err(NfsError::Invalid("HUFF decoded length mismatch")); }
    if [0x32fb,0xb2fb,0x34fb,0xb4fb].contains(&kind) {
        let double=[0x34fb,0xb4fb].contains(&kind);
        let (mut v,mut v2)=(0u8,0u8);
        for b in out.iter_mut() { v=v.wrapping_add(*b); v2=v2.wrapping_add(v); *b=if double {v2} else {v}; }
    }
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


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkNode { pub id:u32, pub offset:usize, pub payload_offset:usize, pub size:usize, pub children:Vec<ChunkNode> }
impl ChunkNode {
    pub fn find(&self, id:u32) -> Option<&ChunkNode> { self.children.iter().find(|n|n.id==id) }
    pub fn find_recursive(&self,id:u32)->Option<&ChunkNode> {
        if self.id==id { return Some(self); }
        for n in &self.children { if let Some(v)=n.find_recursive(id) { return Some(v); } }
        None
    }
    pub fn payload<'a>(&self,src:&'a [u8])->Result<&'a [u8],NfsError> {
        src.get(self.payload_offset..self.payload_offset+self.size).ok_or(NfsError::OutOfRange)
    }
}
fn parse_chunk_range(src:&[u8],start:usize,end:usize,depth:usize)->Result<Vec<ChunkNode>,NfsError>{
    if depth>64 { return Err(NfsError::Invalid("chunk nesting too deep")); }
    let mut out=Vec::new(); let mut at=start;
    while at<end {
        if end-at<8 { return Err(NfsError::Truncated("chunk header")); }
        let id=u32::from_le_bytes(src[at..at+4].try_into().unwrap());
        let size=u32::from_le_bytes(src[at+4..at+8].try_into().unwrap()) as usize;
        let payload=at+8; let stop=payload.checked_add(size).ok_or(NfsError::OutOfRange)?;
        if stop>end { return Err(NfsError::Invalid("chunk extends beyond parent")); }
        let children=if id&0x8000_0000!=0 { parse_chunk_range(src,payload,stop,depth+1)? } else { Vec::new() };
        out.push(ChunkNode{id,offset:at,payload_offset:payload,size,children});
        at=stop;
    }
    Ok(out)
}
pub fn parse_chunks(src:&[u8])->Result<Vec<ChunkNode>,NfsError>{ parse_chunk_range(src,0,src.len(),0) }

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum CarPixelFormat { A8R8G8B8, Dxt1, Dxt3, Dxt5 }
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct CarTextureInfo { pub key:u32, pub width:u16, pub height:u16, pub format:CarPixelFormat, pub mip_count:u8, pub slot:TextureSlot, pub pixels:Vec<u8> }
fn mip_size(fmt:CarPixelFormat,w:usize,h:usize)->usize {
    match fmt {
        CarPixelFormat::A8R8G8B8=>w*h*4,
        CarPixelFormat::Dxt1=>((w+3)/4)*((h+3)/4)*8,
        CarPixelFormat::Dxt3|CarPixelFormat::Dxt5=>((w+3)/4)*((h+3)/4)*16,
    }
}
pub fn decode_car_slot(archive:&[u8],slot:TextureSlot,max_output:usize)->Result<CarTextureInfo,NfsError>{
    let raw=decompress_block(slot.block(archive)?,max_output)?;
    if slot.decoded_size!=0 && raw.len()!=slot.decoded_size as usize { return Err(NfsError::Invalid("slot decoded size mismatch")); }
    let p=slot.header_offset(raw.len())?;
    let key=u32::from_le_bytes(raw[p..p+4].try_into().unwrap());
    if key!=slot.key { return Err(NfsError::Invalid("embedded texture key mismatch")); }
    let width=u16::from_le_bytes(raw[p+0x20..p+0x22].try_into().unwrap());
    let height=u16::from_le_bytes(raw[p+0x22..p+0x24].try_into().unwrap());
    let code=raw[p+0x26];
    let format=match code {0x20=>CarPixelFormat::A8R8G8B8,0x22=>CarPixelFormat::Dxt1,0x24=>CarPixelFormat::Dxt3,0x26=>CarPixelFormat::Dxt5,_=>return Err(NfsError::Unsupported("car texture pixel format"))};
    if width==0 || height==0 || width as usize*height as usize>4_194_304 { return Err(NfsError::Invalid("car texture dimensions")); }
    let mut total=0usize; let mut mips=0u8; let (mut w,mut h)=(width as usize,height as usize);
    for _ in 0..16 {
        let n=mip_size(format,w,h);
        if total.checked_add(n).filter(|v|*v<=p).is_none() { break; }
        total+=n; mips+=1; w=(w/2).max(1); h=(h/2).max(1);
    }
    if mips==0 { return Err(NfsError::Invalid("no complete mip levels before embedded header")); }
    Ok(CarTextureInfo{key,width,height,format,mip_count:mips,slot,pixels:raw[..total].to_vec()})
}
pub fn find_car_texture_packs(src:&[u8],max_block:usize)->Result<Vec<(ChunkNode,Vec<CarTextureInfo>)>,NfsError>{
    fn walk(src:&[u8],nodes:&[ChunkNode],max_block:usize,out:&mut Vec<(ChunkNode,Vec<CarTextureInfo>)>)->Result<(),NfsError>{
        for n in nodes {
            if n.id==0xB3300000 {
                if let Some(info)=n.find(0xB3310000) {
                    if let Some(table)=info.find(0x33310003) {
                        let slots=TextureSlot::parse_table(table.payload(src)?)?;
                        let mut textures=Vec::with_capacity(slots.len());
                        for slot in slots { textures.push(decode_car_slot(src,slot,max_block)?); }
                        out.push((n.clone(),textures));
                    }
                }
            }
            walk(src,&n.children,max_block,out)?;
        }
        Ok(())
    }
    let nodes=parse_chunks(src)?; let mut out=Vec::new(); walk(src,&nodes,max_block,&mut out)?; Ok(out)
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
