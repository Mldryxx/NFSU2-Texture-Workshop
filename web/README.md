# NFSU2 Texture Workshop
Offline-first browser editor for Need for Speed: Underground 2 BIN/TPK archives.

The WebAssembly engine reuses the pinned upstream gizmo-nfs crate from PryHUB at commit 861753d58ef065bc0944ca3241dad8a0dea7bfde, the same Rust library used by this repository's Android integration.

Initial milestone: offline-first UI, TPK directory listing, texture preview, and Rust-based texture replacement API. The visible DDS import control is not enabled yet; DDS decoding must be added and tested before claiming full replacement support. Original BIN is never overwritten. Safety limits: 60 MiB archive, 4096 px per side, 4 megapixels per texture.
