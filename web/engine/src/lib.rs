use gizmo_nfs::texture::{replace_image, replace_images, Image, Tpk};
use gizmo_nfs::AssetHash;
use serde::Serialize;
use wasm_bindgen::prelude::*;
const MAX_ARCHIVE: usize = 60 * 1024 * 1024;
const MAX_PIXELS: u64 = 4_194_304;
const MAX_SIDE: u32 = 4096;
#[derive(Serialize)]
struct TextureInfo { index: usize, hash: String, name: String, width: Option<u32>, height: Option<u32>, compressed_size: u32, decoded_size: u32 }
fn fail(s: impl ToString) -> JsValue { JsValue::from_str(&s.to_string()) }
fn check(data: &[u8]) -> Result<(), JsValue> {
    if data.is_empty() { return Err(fail("File BIN kosong.")); }
    if data.len() > MAX_ARCHIVE { return Err(fail("File BIN melebihi batas 60 MiB.")); }
    Ok(())
}
#[wasm_bindgen]
pub fn engine_version() -> String { "NFSU2 Texture Workshop engine 0.1.1 (gizmo-nfs)".into() }
#[wasm_bindgen]
pub fn list_textures(data: &[u8]) -> Result<JsValue, JsValue> {
    check(data)?;
    let entries = Tpk::directory(data).map_err(|e| fail(format!("Struktur TPK/BIN tidak valid: {e}")))?;
    let mut out = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        // Listing reads the embedded debug name only; pixel decoding is deferred until selection.
        let name = Tpk::name_of(data, entry).unwrap_or_default();
        out.push(TextureInfo { index, hash: format!("0x{:08X}", entry.hash.0), name, width: None, height: None, compressed_size: entry.size, decoded_size: entry.out_size });
    }
    serde_wasm_bindgen::to_value(&out).map_err(fail)
}
#[wasm_bindgen]
pub fn decode_texture(data: &[u8], index: usize) -> Result<JsValue, JsValue> {
    check(data)?;
    let entries = Tpk::directory(data).map_err(|e| fail(format!("Direktori tekstur tidak valid: {e}")))?;
    let entry = entries.get(index).ok_or_else(|| fail("Indeks tekstur tidak ditemukan."))?;
    let t = Tpk::decode_one(data, entry).map_err(|e| fail(format!("Gagal membaca tekstur: {e}")))?;
    let pixels = t.width as u64 * t.height as u64;
    if pixels == 0 || pixels > MAX_PIXELS || t.width > MAX_SIDE || t.height > MAX_SIDE { return Err(fail("Dimensi tekstur melewati batas aman.")); }
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &"width".into(), &t.width.into()).map_err(|_| fail("Gagal membentuk data preview tekstur."))?;
    js_sys::Reflect::set(&obj, &"height".into(), &t.height.into()).map_err(|_| fail("Gagal membentuk data preview tekstur."))?;
    let rgba = js_sys::Uint8Array::from(t.rgba.as_slice());
    js_sys::Reflect::set(&obj, &"rgba".into(), &rgba.into()).map_err(|_| fail("Gagal membentuk data preview tekstur."))?;
    Ok(obj.into())
}
fn replace_texture_inner(data: &[u8], index: usize, rgba: &[u8], width: u32, height: u32) -> Result<(Vec<u8>, bool), JsValue> {
    check(data)?;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(fail("Dimensi gambar pengganti tidak valid."));
    }
    let expected = (width as usize).checked_mul(height as usize).and_then(|v| v.checked_mul(4))
        .ok_or_else(|| fail("Ukuran buffer overflow."))?;
    if rgba.len() != expected { return Err(fail("Buffer gambar harus RGBA 8-bit.")); }
    let entries = Tpk::directory(data).map_err(|e| fail(format!("Direktori TPK tidak valid: {e}")))?;
    let entry = entries.get(index).ok_or_else(|| fail("Indeks tekstur tidak ditemukan."))?;
    let old = Tpk::decode_one(data, entry).map_err(|e| fail(format!("Tekstur tujuan tidak dapat dibaca: {e}")))?;
    if old.width != width || old.height != height {
        return Err(fail(format!("Ukuran {width}x{height} tidak cocok; target {}x{}.", old.width, old.height)));
    }
    let (edited, relocated) = replace_image(data, AssetHash(entry.hash.0), rgba, width, height)
        .map_err(|e| fail(format!("Penggantian gagal: {e}")))?;
    if edited.len() > MAX_ARCHIVE { return Err(fail("Hasil BIN melebihi batas 60 MiB.")); }
    Ok((edited, relocated))
}

fn replace_texture_pair_inner(data: &[u8], base_index: usize, base_rgba: &[u8], base_width: u32, base_height: u32, mask_index: usize, mask_rgba: &[u8], mask_width: u32, mask_height: u32) -> Result<(Vec<u8>, bool), JsValue> {
    check(data)?;
    let entries = Tpk::directory(data).map_err(|e| fail(format!("Direktori TPK tidak valid: {e}")))?;
    if base_index == mask_index { return Err(fail("Base dan Mask harus merupakan dua tekstur berbeda.")); }
    let base = entries.get(base_index).ok_or_else(|| fail("Indeks Base tidak ditemukan."))?;
    let mask = entries.get(mask_index).ok_or_else(|| fail("Indeks Mask tidak ditemukan."))?;
    for (label, rgba, width, height, entry) in [("Base", base_rgba, base_width, base_height, base), ("Mask", mask_rgba, mask_width, mask_height, mask)] {
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE || (width as u64) * (height as u64) > MAX_PIXELS { return Err(fail(format!("Dimensi {label} melewati batas aman."))); }
        let expected = (width as usize).checked_mul(height as usize).and_then(|v| v.checked_mul(4)).ok_or_else(|| fail("Ukuran buffer overflow."))?;
        if rgba.len() != expected { return Err(fail(format!("Buffer {label} harus RGBA 8-bit."))); }
        let old = Tpk::decode_one(data, entry).map_err(|e| fail(format!("Tekstur {label} tidak dapat dibaca: {e}")))?;
        if old.width != width || old.height != height { return Err(fail(format!("Ukuran {label} tidak cocok; target {}x{}.", old.width, old.height))); }
    }
    let images = [
        Image { hash: AssetHash(base.hash.0), rgba: base_rgba, width: base_width, height: base_height },
        Image { hash: AssetHash(mask.hash.0), rgba: mask_rgba, width: mask_width, height: mask_height },
    ];
    let (edited, relocated) = replace_images(data, &images).map_err(|e| fail(format!("Penggantian pasangan gagal: {e}")))?;
    if edited.len() > MAX_ARCHIVE { return Err(fail("Hasil BIN melebihi batas 60 MiB.")); }
    Ok((edited, relocated))
}
#[wasm_bindgen]
pub fn replace_texture_pair_with_status(data: &[u8], base_index: usize, base_rgba: &[u8], base_width: u32, base_height: u32, mask_index: usize, mask_rgba: &[u8], mask_width: u32, mask_height: u32) -> Result<JsValue, JsValue> {
    let (edited, relocated) = replace_texture_pair_inner(data, base_index, base_rgba, base_width, base_height, mask_index, mask_rgba, mask_width, mask_height)?;
    let obj = js_sys::Object::new();
    let bytes = js_sys::Uint8Array::from(edited.as_slice());
    js_sys::Reflect::set(&obj, &"data".into(), &bytes.into()).map_err(|_| fail("Gagal membentuk hasil BIN."))?;
    js_sys::Reflect::set(&obj, &"relocated".into(), &relocated.into()).map_err(|_| fail("Gagal membentuk status arsip."))?;
    Ok(obj.into())
}

#[wasm_bindgen]
pub fn replace_texture(data: &[u8], index: usize, rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, JsValue> {
    replace_texture_inner(data, index, rgba, width, height).map(|(edited, _)| edited)
}
#[wasm_bindgen]
pub fn replace_texture_with_status(data: &[u8], index: usize, rgba: &[u8], width: u32, height: u32) -> Result<JsValue, JsValue> {
    let (edited, relocated) = replace_texture_inner(data, index, rgba, width, height)?;
    let obj = js_sys::Object::new();
    let bytes = js_sys::Uint8Array::from(edited.as_slice());
    js_sys::Reflect::set(&obj, &"data".into(), &bytes.into())
        .map_err(|_| fail("Gagal membentuk hasil BIN."))?;
    js_sys::Reflect::set(&obj, &"relocated".into(), &relocated.into())
        .map_err(|_| fail("Gagal membentuk status arsip."))?;
    Ok(obj.into())
}
