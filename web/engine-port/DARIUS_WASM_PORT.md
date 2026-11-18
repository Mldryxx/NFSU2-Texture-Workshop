# Binarius/Darius WebAssembly Port Notes

This branch isolates the engine migration from the existing experimental gizmo-nfs implementation. Do not remove the current engine until the Darius-backed tests pass.

## Verified Darius contract

Source: nlgxzef/Darius, Nikki/Utils/Interop.cs and Nikki/Reflection/Enum/LZCompressionType.cs.

The repository tree contains a prebuilt `Nikki/LZCompressLib.dll` binary (25,088 bytes), but no corresponding native source implementation was found in the audited tree. It is imported through Cdecl:
- `BlockDecompress(input, input_length, output)`
- `BlockCompress(input, input_length, output, compression_type)`

Compression identifiers (little-endian four-character tags):
- RAWW = 0x57574152
- JDLZ = 0x5A4C444A
- HUFF = 0x46465548
- COMP = 0x504D4F43
- RFPK = 0x4B504652
- BEST = 0x54534542 (encoder selection request, not a stored payload codec)

Darius reads the decompressed output length from offset 0x08 when the first 4 bytes match a known codec tag. Inputs shorter than 16 bytes are rejected by its wrapper. Unknown tags are returned unchanged.

## NFSU2 TPK serialization facts

From Nikki/Support.Underground2/Class/Texture.cs:
- Texture metadata is serialized as RAWW.
- Texture payload is split into chunks of at most 0x8000 bytes.
- Each texture chunk is passed to BEST during serialization.
- Texture metadata retains dimensions, palette size, compression enum, mipmap count, UV/scroll/render flags, five unknown fields, and three compression values.
- Deserialization expects the metadata block and each data chunk to be decoded before reconstructing the texture's data array.

From Nikki/Support.Underground2/Class/TPKBlock.cs:
- TPK metadata uses RAWW.
- Texture objects are serialized individually.
- TPK assembly supports compressed and decompressed layouts; offsets and padding are rebuilt by Darius.

## Porting strategy

1. Implement and test RAWW decode/encode in a small, platform-neutral codec layer.
2. Implement JDLZ decode/encode and verify byte-level round trips against known fixtures. Treat the current JDLZ v2 decoder as provisional: public references disagree on v2 flag semantics, so do not enable it for real TPK data until checked against NFSU2/Darius-produced fixtures.
3. Audit HUFF, COMP, and RFPK separately; do not map BEST directly to a decoder. BEST must select a supported encoder based on actual output size/behavior.
4. Add a Darius adapter around the NFSU2 TPK classes only after the compression layer is tested.
5. Preserve all original texture metadata on replacement. Do not use Darius Texture.Initialize/Reload unchanged for replacement: it resets multiple texture attributes to defaults and takes dimensions/compression from the imported DDS.
6. Add fixture-based tests: decode original VINYLS.BIN, export a texture, replace a test texture, rebuild, reopen rebuilt BIN, and compare metadata and decoded pixels.
7. Keep original BIN immutable; every write produces a new output buffer/file.

## Current status

- Source contract audited: yes.
- Native compression dependency: Darius repository includes a prebuilt `LZCompressLib.dll`, but this is a native binary, not a portable WebAssembly implementation. It cannot simply be called from a browser/WASM module; the compression algorithms must be ported/reimplemented or replaced with a WASM-compatible library, and verified against Darius output.
- Isolated Rust codec crate: started; RAWW and provisional JDLZ v2 decoder are present.
- GitHub Actions validation: passed on commit `789b71b739fa3adfa7851b9aa43b0cc5a623e40d`. All four unit tests passed, and the `wasm32-unknown-unknown` compile check passed. This validates the current small codec crate only; it does not validate real game archives.
- JDLZ semantics: provisional; the near-reference test now passes, but the decoder must still be validated against a real NFSU2/Darius-produced fixture.
- HUFF/COMP/RFPK and BEST encoder: not implemented.
- Darius TPK parser integrated: not yet.
- Android browser test: not yet.
- Safe BIN rebuild test: not yet.

## Licensing

Binarius is GPL-3.0. Darius and the referenced helper projects have their own license notices. Any distributed derivative must retain applicable notices and comply with the GPL source-distribution requirements. Re-check the exact license files and versions before release.
