use crate::nfsu2::{decompress_block,decompress_huff};

#[test]
fn decodes_synthetic_canonical_huff_stream() {
    fn put(bits:&mut Vec<u8>,value:u32,width:usize) {
        for shift in (0..width).rev() { bits.push(((value>>shift)&1) as u8); }
    }
    fn number(bits:&mut Vec<u8>,value:usize) {
        let mut width=2usize;
        while value>((1usize<<(width+1))-5) { bits.push(0); width+=1; }
        bits.push(1);
        put(bits,(value-(1usize<<width)+4) as u32,width);
    }
    let mut bits=Vec::new();
    put(&mut bits,0x30fb,16);
    put(&mut bits,2,24);
    put(&mut bits,255,8);
    for value in [1,2,0,0,253] { number(&mut bits,value); }
    put(&mut bits,0,1);
    put(&mut bits,2,2);
    put(&mut bits,3,2);
    number(&mut bits,0);
    put(&mut bits,1,1);
    while bits.len()%8!=0 { bits.push(0); }
    let mut packed=Vec::new();
    for byte in bits.chunks_exact(8) {
        packed.push(byte.iter().fold(0u8,|a,b|(a<<1)|*b));
    }
    let mut block=b"HUFF\x01\x10\0\0".to_vec();
    block.extend_from_slice(&2u32.to_le_bytes());
    block.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    block.extend_from_slice(&packed);
    assert_eq!(decompress_huff(&block,1024).unwrap(),vec![0,1]);
    assert_eq!(decompress_block(&block,1024).unwrap(),vec![0,1]);
}