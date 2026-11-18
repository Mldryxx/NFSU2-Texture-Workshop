//! Pure-Rust encoder for the compressed-car texture formats used by NFSU2.
//! Input and output pixels are RGBA8; storage remains in the slot's original format.
use crate::nfsu2::{CarPixelFormat, CarTextureInfo, NfsError};

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
        CarPixelFormat::A8R8G8B8 => unreachable!(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nfsu2::TextureSlot;
    fn texture(fmt:CarPixelFormat,w:u16,h:u16,mips:u8)->CarTextureInfo {
        let mut size=0usize;let(mut x,mut y)=(w as usize,h as usize);
        for _ in 0..mips {
            size+=match fmt {CarPixelFormat::A8R8G8B8=>x*y*4,CarPixelFormat::Dxt1=>((x+3)/4)*((y+3)/4)*8,_=>((x+3)/4)*((y+3)/4)*16};
            x=(x/2).max(1);y=(y/2).max(1);
        }
        CarTextureInfo{key:1,width:w,height:h,format:fmt,mip_count:mips,slot:TextureSlot{key:1,offset:0,compressed_size:0,decoded_size:0,header_from_end:0,unknown:0},pixels:vec![0;size]}
    }
    #[test]
    fn encodes_all_car_formats_and_mips_to_exact_slot_size() {
        for fmt in [CarPixelFormat::A8R8G8B8,CarPixelFormat::Dxt1,CarPixelFormat::Dxt3,CarPixelFormat::Dxt5] {
            let t=texture(fmt,8,8,4);let rgba=vec![128u8;8*8*4];
            let packed=encode_car_pixels(&t,&rgba,8,8).unwrap();
            assert_eq!(packed.len(),t.pixels.len());
        }
    }
    #[test]
    fn rejects_wrong_dimensions_and_buffer_lengths() {
        let t=texture(CarPixelFormat::Dxt1,4,4,1);
        assert!(encode_car_pixels(&t,&[0;64],8,8).is_err());
        assert!(encode_car_pixels(&t,&[0;63],4,4).is_err());
    }
}