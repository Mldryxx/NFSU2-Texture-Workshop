# NFSU2 Texture Workshop

Offline-first Android-friendly browser editor for Need for Speed: Underground 2 BIN/TPK archives.

## Engine

The WebAssembly engine is now an independent Rust implementation. It no longer depends on gizmo-nfs or the previous PryHUB engine.

- Parses the NFSU2 0xB3300000 texture-pack tree and the 24-byte compressed car-texture slot table.
- Decodes JDLZ and HUFF blocks.
- Reads the vinyl-pack format codes used by the tested SKYLINEVINYLSORI.BIN archive: DXT1 (0x08), DXT3 (0x80), DXT5 (0x81), and ARGB8 (0x20), plus the standard NFSU2 DXT codes.
- Decodes and re-encodes DXT1, DXT3, DXT5, and ARGB8 textures at their original dimensions.
- Regenerates the target mip chain from the imported top-level DDS image.
- Replaces one texture or a Base/Mask pair atomically. Changed slots are appended on 128-byte boundaries; slot offsets and enclosing chunk sizes are updated. Untouched compressed slots remain byte-for-byte unchanged.
- Keeps the original BIN in memory unchanged and exports a separate edited archive.

## Base and Mask workflow

Some compressed vinyl packs expose texture hashes rather than readable texture names. In that case, select a texture in the list and use Jadikan Base or Jadikan Mask to choose the two target slots. Import the matching DDS files and apply them together.

## Safety and limitations

- Archive and DDS file size: 60 MiB maximum.
- Texture side: 4096 px maximum; texture area: 4 megapixels maximum.
- Replacement dimensions must match the selected target texture.
- The current safe relocation path requires one compressed TPK pack whose pixel-data chunk is the final chunk in the BIN. Unsupported layouts are refused rather than rewritten unsafely.
- Changed slots are written as JDLZ; unchanged HUFF/JDLZ blocks are preserved. Mixed-format acceptance and the edited BIN still need to be tested in the actual game.
- The target SKYLINEVINYLSORI.BIN was structurally checked: 1,802 slot records, no overlapping or out-of-bounds source blocks, and all 1,783 HUFF plus 19 JDLZ source blocks decoded successfully with the reference implementation. This is not yet an in-game verification.

## Developer checks

- cargo test --manifest-path web/engine/Cargo.toml
- cargo check --manifest-path web/engine/Cargo.toml --target wasm32-unknown-unknown
- wasm-pack build --target web --release --out-dir ../site/pkg (run from web/engine)
- npm test --prefix web/site

The app works offline after its web assets have been loaded and cached by the service worker.