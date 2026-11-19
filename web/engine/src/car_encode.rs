//! Pure-Rust encoder for the compressed-car texture formats used by NFSU2.
//! Input and output pixels are RGBA8; storage remains in the slot's original format.
use crate::nfsu2::{CarPixelFormat, CarTextureInfo, NfsError, TextureSlot};

fn to565(c: [u8; 4]) -> u16 {
    (((c[0] as u16 * 31 + 127) / 255) << 11)
        | (((c[1] as u16 * 63 + 127) / 255) << 5)
        | ((c[2] as u16 * 31 + 127) / 255)
}
fn from565(v: u16) -> [u8; 3] {
    let r=((v>>11)&31) as u8; let g=((v>>5)&63) as u8; let b=(v&31) as u8;
    [(r<<3)|(r>>2),(g<<2)|(g>>4),(b<<3)|(b>>2)]
}
fn colors(c0:u16,c1:u16,three:bool)->[[u8;4];4] {
    let a=from565(c0); let b=from565(c1);
    let mut p=[[0u8;4];4];
    p[0]=[a[0],a[1],a[2],255]; p[1]=[b[0],b[1],b[2],255];
    if three {
        for k in 0..3 { p[2][k]=((a[k] as u16+b[k] as u16)/2) as u8; }
        p[2][3]=255; p[3]=[0,0,0,0];
    } else {
        for k in 0..3 {
            p[2][k]=((2*a[k] as u16+b[k] as u16)/3) as u8;
            p[3][k]=((a[k] as u16+2*b[k] as u16)/3) as u8;
        }
        p[2][3]=255; p[3][3]=255;
    }
    p
}
fn encode_color_block(px:&[[u8;4];16], punch:bool, out:&mut Vec<u8>) {
    let transparent=punch && px.iter().any(|p|p[3]<128);
    let mut lo=[255u8;3]; let mut hi=[0u8;3];
    for p in px.iter().filter(|p|!transparent || p[3]>=128) {
        for k in 0..3 { lo[k]=lo[k].min(p[k]); hi[k]=hi[k].max(p[k]); }
    }
    let mut a=to565([hi[0],hi[1],hi[2],255]);
    let mut b=to565([lo[0],lo[1],lo[2],255]);
    if transparent {
        if a>b { std::mem::swap(&mut a,&mut b); }
    } else if a<=b {
        if a==b { if a<65535 {a+=1;} else {b-=1;} }
        if a<=b {std::mem::swap(&mut a,&mut b);}
    }
    out.extend_from_slice(&a.to_le_bytes()); out.extend_from_slice(&b.to_le_bytes());
    let pal=colors(a,b,transparent);
    let mut selectors=0u32;
    for (i,p) in px.iter().enumerate() {
        let sel=if transparent && p[3]<128 {3} else {
            let count=if transparent {3} else {4};
            let mut best=(u32::MAX,0usize);
            for j in 0..count {
                let mut d=0u32;
                for k in 0..3 { let z=p[k] as i32-pal[j][k] as i32; d+=(z*z) as u32; }
                if d<best.0 {best=(d,j);}
            }
            best.1
        };
        selectors|=(sel as u32)<<(2*i);
    }
    out.extend_from_slice(&selectors.to_le_bytes());
}
fn alpha_dxt5_table(a0:u8,a1:u8)->[u8;8] {
    let mut t=[0u8;8]; t[0]=a0;t[1]=a1;
    if a0>a1 { for i in 1..7 {t[i+1]=(((7-i) as u16*a0 as u16+i as u16*a1 as u16)/7) as u8;} }
    else { for i in 1..5 {t[i+1]=(((5-i) as u16*a0 as u16+i as u16*a1 as u16)/5) as u8;} t[6]=0;t[7]=255; }
    t
}
fn encode_block(fmt:CarPixelFormat,px:&[[u8;4];16],out:&mut Vec<u8>) {
    match fmt {
        CarPixelFormat::A8R8G8B8 | CarPixelFormat::Indexed8 => unreachable!(),
        CarPixelFormat::Dxt1 => encode_color_block(px,true,out),
        CarPixelFormat::Dxt3 => {
            let mut alpha=0u64;
            for i in 0..16 { alpha|=(((px[i][3] as u64*15+127)/255)&15)<<(4*i); }
            out.extend_from_slice(&alpha.to_le_bytes());
            encode_color_block(px,false,out);
        }
        CarPixelFormat::Dxt5 => {
            let amin=px.iter().map(|p|p[3]).min().unwrap_or(0);
            let amax=px.iter().map(|p|p[3]).max().unwrap_or(255);
            out.push(amax);out.push(amin);
            let table=alpha_dxt5_table(amax,amin);
            let mut bits=0u64;
            for i in 0..16 {
                let mut best=(u32::MAX,0usize);
                for j in 0..8 {let d=(px[i][3] as i32-table[j] as i32).unsigned_abs();if d<best.0 {best=(d,j);}}
                bits|=(best.1 as u64)<<(3*i);
            }
            for i in 0..6 {out.push((bits>>(8*i)) as u8);}
            encode_color_block(px,false,out);
        }
    }
}
fn downsample(src:&[[u8;4]],w:usize,h:usize)->(Vec<[u8;4]>,usize,usize) {
    let nw=(w/2).max(1);let nh=(h/2).max(1);let mut out=vec![[0u8;4];nw*nh];
    for y in 0..nh {for x in 0..nw {
        for c in 0..4 {
            let mut sum=0u32;let mut n=0u32;
            for dy in 0..2 {for dx in 0..2 {
                let sx=(x*2+dx).min(w-1);let sy=(y*2+dy).min(h-1);
                sum+=src[sy*w+sx][c] as u32;n+=1;
            }}
            out[y*nw+x][c]=((sum+n/2)/n) as u8;
        }
    }}
    (out,nw,nh)
}
fn encode_level(fmt:CarPixelFormat,rgba:&[[u8;4]],w:usize,h:usize)->Vec<u8> {
    if fmt==CarPixelFormat::A8R8G8B8 {
        let mut out=Vec::with_capacity(w*h*4);
        for p in rgba {out.extend_from_slice(&[p[2],p[1],p[0],p[3]]);}
        return out;
    }
    let mut out=Vec::new();
    for by in 0..(h+3)/4 {for bx in 0..(w+3)/4 {
        let mut block=[[0u8;4];16];
        for y in 0..4 {for x in 0..4 {
            let sx=(bx*4+x).min(w-1);let sy=(by*4+y).min(h-1);
            block[y*4+x]=rgba[sy*w+sx];
        }}
        encode_block(fmt,&block,&mut out);
    }}
    out
}
pub fn encode_car_pixels(tex:&CarTextureInfo,rgba:&[u8],width:u32,height:u32)->Result<Vec<u8>,NfsError> {
    if width!=tex.width as u32 || height!=tex.height as u32 {
        return Err(NfsError::Invalid("replacement dimensions differ from target texture"));
    }
    let count=(width as usize).checked_mul(height as usize).ok_or(NfsError::OutOfRange)?;
    if rgba.len()!=count.checked_mul(4).ok_or(NfsError::OutOfRange)? {
        return Err(NfsError::Invalid("replacement buffer must be RGBA8"));
    }
    if tex.format==CarPixelFormat::Indexed8 {
        if tex.pixels.len()!=count {return Err(NfsError::Invalid("indexed slot pixel length mismatch"));}
        // Vinyl slots are game-recolored P8 indices. Convert DDS luminance into
        // the 0..15 intensity ramp used by the preview; alpha-zero becomes index 0.
        // No RGB palette is stored in the vinyl record, so color itself cannot
        // be preserved in this format.
        let mut out=Vec::with_capacity(count);
        for p in rgba.chunks_exact(4) {
            if p[3]<128 {out.push(0);}
            else {
                let lum=(77u32*p[0] as u32+150u32*p[1] as u32+29u32*p[2] as u32+128)/256;
                out.push(1+((lum*14+127)/255) as u8);
            }
        }
        return Ok(out);
    }
    let mut img:Vec<[u8;4]>=rgba.chunks_exact(4).map(|p|[p[0],p[1],p[2],p[3]]).collect();
    let mut w=width as usize;let mut h=height as usize;
    let mut out=Vec::with_capacity(tex.pixels.len());
    for level in 0..tex.mip_count {
        out.extend_from_slice(&encode_level(tex.format,&img,w,h));
        if level+1<tex.mip_count {
            let next=downsample(&img,w,h);img=next.0;w=next.1;h=next.2;
        }
    }
    if out.len()!=tex.pixels.len() {
        return Err(NfsError::Invalid("encoded mip chain size differs from original slot"));
    }
    Ok(out)
}


/// Apply several slot replacements atomically without changing any archive offsets.
/// Existing slot extents are treated as fixed-capacity regions; a replacement that
/// does not fit is rejected rather than moving opaque BIN data or corrupting offsets.
pub fn replace_car_texture_slots(
    archive:&[u8],
    textures:&[CarTextureInfo],
    replacements:&[(usize,&[u8],u32,u32)],
)->Result<Vec<u8>,NfsError> {
    use crate::nfsu2::{compress_jdlz,decompress_block,find_car_slot_tables,parse_chunks};
    use std::collections::HashSet;
    if replacements.is_empty() {return Err(NfsError::Invalid("no texture replacements supplied"));}
    if archive.len()<8 {return Err(NfsError::Truncated("BIN root chunk"));}
    let packs=find_car_slot_tables(archive)?;
    if packs.len()!=1 {return Err(NfsError::Unsupported("replacement currently requires exactly one compressed TPK pack"));}
    let (root_ref,slots)=&packs[0];
    let tree=parse_chunks(archive)?;
    let root=tree.iter().find(|n|n.offset==root_ref.offset&&n.id==0xB3300000).ok_or(NfsError::Invalid("TPK root node not found"))?;
    let info=root.find(0xB3310000).ok_or(NfsError::Invalid("TPK info chunk missing"))?;
    let table=info.find(0x33310003).ok_or(NfsError::Invalid("TPK slot table missing"))?;
    let data=root.find(0xB3320000).ok_or(NfsError::Invalid("TPK data chunk missing"))?;
    let pixels=data.find(0x33320002).ok_or(NfsError::Invalid("TPK pixel data chunk missing"))?;
    let root_end=root.offset.checked_add(8).and_then(|v|v.checked_add(root.size)).ok_or(NfsError::OutOfRange)?;
    let data_end=data.offset.checked_add(8).and_then(|v|v.checked_add(data.size)).ok_or(NfsError::OutOfRange)?;
    let pixels_end=pixels.offset.checked_add(8).and_then(|v|v.checked_add(pixels.size)).ok_or(NfsError::OutOfRange)?;
    if root.offset!=0 || root_end!=archive.len() || data_end!=archive.len() || pixels_end!=archive.len() {
        return Err(NfsError::Unsupported("TPK pixel block is not the final BIN chunk; safe tail relocation is unavailable"));
    }
    if table.size!=slots.len()*TextureSlot::SIZE {return Err(NfsError::Invalid("slot table size mismatch"));}
    let mut seen=HashSet::new();
    let mut patches:Vec<(usize,Vec<u8>,usize)>=Vec::with_capacity(replacements.len());
    for (index,rgba,w,h) in replacements {
        if !seen.insert(*index) {return Err(NfsError::Invalid("duplicate texture index in replacement set"));}
        let tex=textures.get(*index).ok_or(NfsError::Invalid("texture index not found"))?;
        let slot_index=slots.iter().position(|s|s==&tex.slot).ok_or(NfsError::Invalid("texture slot not present in BIN table"))?;
        let mut raw=decompress_block(tex.slot.block(archive)?,64*1024*1024)?;
        if raw.len()!=tex.slot.decoded_size as usize {return Err(NfsError::Invalid("decoded slot size changed"));}
        let encoded=encode_car_pixels(tex,rgba,*w,*h)?;
        if encoded.len()!=tex.pixels.len() || encoded.len()>raw.len() {return Err(NfsError::Invalid("replacement pixel extent is invalid"));}
        raw[..encoded.len()].copy_from_slice(&encoded);
        let packed=compress_jdlz(&raw)?;
        let mut check=decompress_block(&packed,64*1024*1024)?;
        if check!=raw {return Err(NfsError::Invalid("JDLZ round-trip validation failed"));}
        check.clear();
        patches.push((slot_index,packed,tex.slot.offset as usize));
    }
    let mut output=archive.to_vec();
    let mut added=0usize;
    for (slot_index,packed,_old_offset) in patches {
        let padding=(128-(output.len()%128))%128;
        output.resize(output.len()+padding,0);
        let new_offset=output.len();
        let new_end=new_offset.checked_add(packed.len()).ok_or(NfsError::OutOfRange)?;
        if new_end>60*1024*1024 {return Err(NfsError::Invalid("edited BIN exceeds 60 MiB limit"));}
        output.extend_from_slice(&packed);
        let record=table.payload_offset+slot_index*TextureSlot::SIZE;
        let off=u32::try_from(new_offset).map_err(|_|NfsError::OutOfRange)?;
        let enc=u32::try_from(packed.len()).map_err(|_|NfsError::OutOfRange)?;
        output[record+4..record+8].copy_from_slice(&off.to_le_bytes());
        output[record+8..record+12].copy_from_slice(&enc.to_le_bytes());
        added=output.len()-archive.len();
    }
    let new_pixel_size=pixels.size.checked_add(added).ok_or(NfsError::OutOfRange)?;
    let new_data_size=data.size.checked_add(added).ok_or(NfsError::OutOfRange)?;
    let new_root_size=root.size.checked_add(added).ok_or(NfsError::OutOfRange)?;
    output[pixels.offset+4..pixels.offset+8].copy_from_slice(&(new_pixel_size as u32).to_le_bytes());
    output[data.offset+4..data.offset+8].copy_from_slice(&(new_data_size as u32).to_le_bytes());
    output[root.offset+4..root.offset+8].copy_from_slice(&(new_root_size as u32).to_le_bytes());
    let reparsed=parse_chunks(&output)?;
    if reparsed.len()!=1 || reparsed[0].size!=new_root_size {return Err(NfsError::Invalid("rebuilt BIN chunk validation failed"));}
    let verified=find_car_slot_tables(&output)?;
    if verified.len()!=1 {return Err(NfsError::Invalid("rebuilt BIN slot table validation failed"));}
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::nfsu2::TextureSlot;
    fn texture(fmt:CarPixelFormat,w:u16,h:u16,mips:u8)->CarTextureInfo {
        let mut size=0usize;let(mut x,mut y)=(w as usize,h as usize);
        for _ in 0..mips {
            size+=match fmt {CarPixelFormat::A8R8G8B8=>x*y*4,CarPixelFormat::Dxt1=>((x+3)/4)*((y+3)/4)*8,CarPixelFormat::Dxt3|CarPixelFormat::Dxt5=>((x+3)/4)*((y+3)/4)*16,CarPixelFormat::Indexed8=>x*y};
            x=(x/2).max(1);y=(y/2).max(1);
        }
        CarTextureInfo{key:1,name:None,width:w,height:h,format:fmt,mip_count:mips,slot:TextureSlot{key:1,offset:0,compressed_size:0,decoded_size:0,header_from_end:0,unknown:0},pixels:vec![0;size]}
    }
    #[test]
    fn converts_rgba_to_vinyl_palette_indices_and_alpha_background() {
        let t=texture(CarPixelFormat::Indexed8,4,4,1);
        let mut rgba=vec![255u8,255,255,255].repeat(16);
        rgba[0..4].copy_from_slice(&[0,0,0,0]);
        rgba[4..8].copy_from_slice(&[0,0,0,255]);
        rgba[8..12].copy_from_slice(&[128,128,128,255]);
        rgba[12..16].copy_from_slice(&[255,255,255,255]);
        let indices=encode_car_pixels(&t,&rgba,4,4).unwrap();
        assert_eq!(indices.len(),16);
        assert_eq!(indices[0],0);
        assert_eq!(indices[1],1);
        assert_eq!(indices[2],8);
        assert_eq!(indices[3],15);
    }
    #[test]
    fn encodes_all_car_formats_and_mips_to_exact_slot_size() {
        for fmt in [CarPixelFormat::A8R8G8B8,CarPixelFormat::Dxt1,CarPixelFormat::Dxt3,CarPixelFormat::Dxt5] {
            let t=texture(fmt,8,8,4);let rgba=vec![128u8;8*8*4];
            let packed=encode_car_pixels(&t,&rgba,8,8).unwrap();
            assert_eq!(packed.len(),t.pixels.len());
        }
    }
    fn chunk(id:u32,payload:&[u8])->Vec<u8>{
        let mut b=Vec::new();b.extend_from_slice(&id.to_le_bytes());b.extend_from_slice(&(payload.len() as u32).to_le_bytes());b.extend_from_slice(payload);b
    }
    fn synthetic_archive()->Vec<u8>{
        let key=0x10203040u32;
        let mut raw=vec![0u8;128];
        raw[8..12].copy_from_slice(&key.to_le_bytes());
        raw[8+0x20..8+0x22].copy_from_slice(&4u16.to_le_bytes());
        raw[8+0x22..8+0x24].copy_from_slice(&4u16.to_le_bytes());
        raw[8+0x26]=0x22;
        let packed=crate::nfsu2::compress_jdlz(&raw).unwrap();
        let mut record=Vec::new();
        for v in [key,0,packed.len() as u32,raw.len() as u32,256,0] {record.extend_from_slice(&v.to_le_bytes());}
        let info=chunk(0xB3310000,&chunk(0x33310003,&record));
        let data=chunk(0xB3320000,&chunk(0x33320002,&packed));
        let mut root_payload=Vec::new();root_payload.extend_from_slice(&info);root_payload.extend_from_slice(&data);
        let mut archive=chunk(0xB3300000,&root_payload);
        let offset=(8+info.len()+8+8) as u32;
        archive[8+8+8+4..8+8+8+8].copy_from_slice(&offset.to_le_bytes());
        archive
    }
    #[test]
    fn replaces_texture_and_relocates_slot_without_touching_original() {
        use crate::nfsu2::{decode_car_slot,find_car_slot_tables};
        let original=synthetic_archive();
        let tables=find_car_slot_tables(&original).unwrap();
        let slot=tables[0].1[0];
        let tex=decode_car_slot(&original,slot,1024).unwrap();
        let before=original.clone();
        let rgba=vec![255u8,0,0,255].repeat(16);
        let output=replace_car_texture_slots(&original,&[tex],&[(0,&rgba,4,4)]).unwrap();
        assert_eq!(original,before);
        assert!(output.len()>original.len());
        let slots=find_car_slot_tables(&output).unwrap();
        assert_ne!(slots[0].1[0].offset,slot.offset);
        let edited=decode_car_slot(&output,slots[0].1[0],1024).unwrap();
        assert_eq!((edited.width,edited.height),(4,4));
        assert_eq!(edited.pixels.len(),8);
        let tree=crate::nfsu2::parse_chunks(&output).unwrap();
        assert_eq!(tree[0].offset+8+tree[0].size,output.len());
    }

    #[test]
    fn rejects_wrong_dimensions_and_buffer_lengths() {
        let t=texture(CarPixelFormat::Dxt1,4,4,1);
        assert!(encode_car_pixels(&t,&[0;64],8,8).is_err());
        assert!(encode_car_pixels(&t,&[0;63],4,4).is_err());
    }
}