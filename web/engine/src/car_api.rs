//! WebAssembly-facing API for the independent NFSU2 car TPK engine.
use crate::car_encode::replace_car_texture_slots;
use crate::nfsu2::{decode_car_pixels,decode_car_slot,find_car_slot_tables,CarTextureInfo,NfsError,TextureSlot};
use serde::Serialize;
use wasm_bindgen::prelude::*;

const MAX_ARCHIVE:usize=60*1024*1024;
const MAX_BLOCK:usize=8*1024*1024;

#[derive(Serialize)]
struct TextureInfo {
    index:usize,
    hash:String,
    name:String,
    width:u32,
    height:u32,
    compressed_size:u32,
    decoded_size:u32,
}
fn fail(e:impl std::fmt::Display)->JsValue {JsValue::from_str(&e.to_string())}
fn check(data:&[u8])->Result<(),JsValue>{
    if data.is_empty(){return Err(JsValue::from_str("File BIN kosong."));}
    if data.len()>MAX_ARCHIVE{return Err(JsValue::from_str("File BIN melebihi batas 60 MiB."));}
    Ok(())
}
fn flatten_slots(data:&[u8])->Result<Vec<(usize,TextureSlot)>,NfsError>{
    let packs=find_car_slot_tables(data)?;
    let mut out=Vec::new();
    for (pack_index,(_,slots)) in packs.into_iter().enumerate(){
        for (local,slot) in slots.into_iter().enumerate(){
            out.push((pack_index*1_000_000+local,slot));
        }
    }
    Ok(out)
}
fn get_slot(data:&[u8],index:usize)->Result<(usize,TextureSlot),NfsError>{
    let packs=find_car_slot_tables(data)?;
    let mut global=0usize;
    for (pack_index,(_,slots)) in packs.into_iter().enumerate(){
        if index<global+slots.len(){return Ok((pack_index,slots[index-global]));}
        global+=slots.len();
    }
    Err(NfsError::Invalid("texture index not found"))
}
fn get_slot_in_pack(data:&[u8],index:usize)->Result<(usize,usize,TextureSlot),NfsError>{
    let packs=find_car_slot_tables(data)?;
    let mut global=0usize;
    for (pack_index,(_,slots)) in packs.into_iter().enumerate(){
        if index<global+slots.len(){let local=index-global;return Ok((pack_index,local,slots[local]));}
        global+=slots.len();
    }
    Err(NfsError::Invalid("texture index not found"))
}
fn decode_info(data:&[u8],slot:TextureSlot)->Result<CarTextureInfo,NfsError>{decode_car_slot(data,slot,MAX_BLOCK)}

#[wasm_bindgen]
pub fn nfsu2_engine_version()->String {
    "NFSU2 Texture Workshop engine 0.2.0 (independent Rust JDLZ/HUFF + car TPK)".into()
}
#[wasm_bindgen]
pub fn list_car_textures(data:&[u8])->Result<JsValue,JsValue>{
    check(data)?;
    let slots=flatten_slots(data).map_err(fail)?;
    let mut out=Vec::with_capacity(slots.len());
    for (index,slot) in slots {
        let t=decode_info(data,slot).map_err(|e|fail(format!("Tekstur {index} tidak dapat dibaca: {e}")))?;
        let hash=format!("0x{:08X}",slot.key);
        out.push(TextureInfo{index,hash:hash.clone(),name:hash,width:t.width as u32,height:t.height as u32,compressed_size:slot.compressed_size,decoded_size:slot.decoded_size});
    }
    serde_wasm_bindgen::to_value(&out).map_err(fail)
}
#[wasm_bindgen]
pub fn decode_car_texture(data:&[u8],index:usize)->Result<JsValue,JsValue>{
    check(data)?;
    let (_,slot)=get_slot(data,index).map_err(fail)?;
    let t=decode_info(data,slot).map_err(fail)?;
    let rgba=decode_car_pixels(&t).map_err(fail)?;
    let obj=js_sys::Object::new();
    js_sys::Reflect::set(&obj,&"width".into(),&t.width.into()).map_err(|_|fail("Gagal membentuk preview."))?;
    js_sys::Reflect::set(&obj,&"height".into(),&t.height.into()).map_err(|_|fail("Gagal membentuk preview."))?;
    let bytes=js_sys::Uint8Array::from(rgba.as_slice());
    js_sys::Reflect::set(&obj,&"rgba".into(),&bytes.into()).map_err(|_|fail("Gagal membentuk preview."))?;
    Ok(obj.into())
}
#[wasm_bindgen]
pub fn replace_car_texture_with_status(data:&[u8],index:usize,rgba:&[u8],width:u32,height:u32)->Result<JsValue,JsValue>{
    check(data)?;
    let (_,slot)=get_slot(data,index).map_err(fail)?;
    let t=decode_info(data,slot).map_err(fail)?;
    let edited=replace_car_texture_slots(data,&[t],&[(0,rgba,width,height)]).map_err(fail)?;
    let obj=js_sys::Object::new();
    let bytes=js_sys::Uint8Array::from(edited.as_slice());
    js_sys::Reflect::set(&obj,&"data".into(),&bytes.into()).map_err(|_|fail("Gagal membentuk hasil BIN."))?;
    js_sys::Reflect::set(&obj,&"relocated".into(),&false.into()).map_err(|_|fail("Gagal membentuk status arsip."))?;
    Ok(obj.into())
}
#[wasm_bindgen]
pub fn replace_car_texture_pair_with_status(data:&[u8],base_index:usize,base_rgba:&[u8],base_width:u32,base_height:u32,mask_index:usize,mask_rgba:&[u8],mask_width:u32,mask_height:u32)->Result<JsValue,JsValue>{
    check(data)?;
    if base_index==mask_index{return Err(fail("Base dan Mask harus merupakan dua tekstur berbeda."));}
    let (base_pack,base_local,base_slot)=get_slot_in_pack(data,base_index).map_err(fail)?;
    let (mask_pack,mask_local,mask_slot)=get_slot_in_pack(data,mask_index).map_err(fail)?;
    if base_pack!=mask_pack{return Err(fail("Base dan Mask harus berada dalam TPK yang sama."));}
    let base=decode_info(data,base_slot).map_err(|e|fail(format!("Tekstur Base tidak dapat dibaca: {e}")))?;
    let mask=decode_info(data,mask_slot).map_err(|e|fail(format!("Tekstur Mask tidak dapat dibaca: {e}")))?;
    let textures=[base,mask];
    let edited=replace_car_texture_slots(data,&textures,&[(0,base_rgba,base_width,base_height),(1,mask_rgba,mask_width,mask_height)]).map_err(fail)?;
    let obj=js_sys::Object::new();
    let bytes=js_sys::Uint8Array::from(edited.as_slice());
    js_sys::Reflect::set(&obj,&"data".into(),&bytes.into()).map_err(|_|fail("Gagal membentuk hasil BIN."))?;
    js_sys::Reflect::set(&obj,&"relocated".into(),&false.into()).map_err(|_|fail("Gagal membentuk status arsip."))?;
    let _=(base_local,mask_local);
    Ok(obj.into())
}