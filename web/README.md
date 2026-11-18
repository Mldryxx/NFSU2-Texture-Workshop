# NFSU2 Texture Workshop
Offline-first browser editor for Need for Speed: Underground 2 BIN/TPK archives.

The WebAssembly engine reuses the pinned upstream gizmo-nfs crate from PryHUB at commit `861753d58ef065bc0944ca3241dad8a0dea7bfde`.

## Current capabilities
- Open a BIN/TPK archive locally in the browser and list texture entries.
- Decode and preview individual textures without decoding the whole archive.
- Import DDS files using DXT1, DXT3, DXT5, RGB 24/32-bit, or luminance 8-bit pixel data.
- Replace a selected texture while preserving its original dimensions and the embedded texture metadata.
- Keep the original source file untouched; export a separate `_edited.bin` file.
- Report whether the replacement fit in its original compressed slot or required rebuilding the TPK blob layout and descriptor offsets.

## Important limitations
- DDS mipmaps beyond the top image are not imported; the existing texture mip chain and metadata are retained.
- Base and Mask are currently replaced as individual texture entries. Automatic Base/Mask pairing and batch replacement are not implemented.
- The DDS decoder has 13 automated regression tests covering supported formats, alpha decoding, row pitch, invalid headers, and truncated payloads. The replacement path builds in CI, but both still require validation against real `VINYLS.BIN` files and in-game testing before being considered production-ready.
- Some unusual third-party TPK layouts may be refused by the upstream relocation routine rather than rewritten unsafely.

## Safety limits
- Archive: 60 MiB maximum.
- Texture side: 4096 px maximum.
- Texture area: 4 megapixels maximum.
- A DDS must match the selected texture's dimensions.
- The original BIN is never overwritten.

The app works offline after its web assets have been loaded and cached by the service worker.

## Developer checks
Run the DDS decoder regression suite with `cd web/site && npm test`.
