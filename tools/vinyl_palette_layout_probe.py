#!/usr/bin/env python3
"""Byte-layout probe for NFSU2 vinyl BIN files (no Rust toolchain required).

This deliberately does not assume palette placement semantics. It records structural
facts needed to validate the PalettePlacement/PaletteSize hypothesis against real BINs.
"""
from __future__ import annotations
import argparse, json, struct
from pathlib import Path

ROOT_ID = 0xB3300000
INFO_ID = 0xB3310000
TEXTURE_INFO_ID = 0x33310002
SLOT_TABLE_ID = 0x33310003
DATA_ID = 0xB3320000
PIXELS_ID = 0x33320002
SLOT_BYTES = 24

def chunk(data, pos, end):
    if pos + 8 > end:
        raise ValueError("truncated chunk header")
    cid, size = struct.unpack_from("<II", data, pos)
    payload = pos + 8
    stop = payload + size
    if stop > end:
        raise ValueError("chunk extends outside parent")
    return {"id": cid, "offset": pos, "payload": payload, "size": size, "end": stop}

def children(data, parent):
    out = []
    p = parent["payload"]
    while p < parent["end"]:
        c = chunk(data, p, parent["end"])
        out.append(c)
        p = c["end"]
    return out

def find_one(items, cid):
    found = [x for x in items if x["id"] == cid]
    if len(found) != 1:
        raise ValueError(f"expected exactly one chunk 0x{cid:08X}; got {len(found)}")
    return found[0]

def probe(path):
    b = Path(path).read_bytes()
    root = chunk(b, 0, len(b))
    if root["id"] != ROOT_ID or root["end"] != len(b):
        raise ValueError("unexpected root layout")
    root_children = children(b, root)
    info = find_one(root_children, INFO_ID)
    info_children = children(b, info)
    texture_info = find_one(info_children, TEXTURE_INFO_ID)
    table = find_one(info_children, SLOT_TABLE_ID)
    data_chunk = find_one(root_children, DATA_ID)
    pixel_chunk = find_one(children(b, data_chunk), PIXELS_ID)
    if table["size"] % SLOT_BYTES:
        raise ValueError("slot table size not divisible by 24")
    slots = []
    for idx in range(table["size"] // SLOT_BYTES):
        p = table["payload"] + idx * SLOT_BYTES
        key, off, compressed, decoded, hfe, unknown = struct.unpack_from("<6I", b, p)
        block = b[off:off + compressed]
        kind = ("HUFF" if block.startswith(b"HUFF\x01\x10")
                else "JDLZ" if block.startswith(b"JDLZ") else "RAW")
        slots.append({"index": idx, "key": key, "offset": off,
            "compressed_size": compressed, "decoded_size": decoded,
            "header_from_end": hfe, "unknown": unknown, "codec": kind})
    codec_counts = {}
    for s in slots:
        codec_counts[s["codec"]] = codec_counts.get(s["codec"], 0) + 1
    return {
        "input_file": Path(path).name, "file_size": len(b), "root_chunk": root,
        "texture_info_chunk": texture_info, "texture_info_bytes": texture_info["size"],
        "texture_info_record_bytes_candidate": 8,
        "texture_info_record_count_candidate": texture_info["size"] // 8,
        "slot_table_chunk": table, "slot_count": len(slots), "codec_counts": codec_counts,
        "data_chunk": data_chunk, "pixel_data_chunk": pixel_chunk,
        "sample_slots": slots[:5],
        "validation_notes": ["Structural-only probe: does not decode HUFF/JDLZ blocks or confirm palette field offsets/swizzling."]
    }

if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("bin_file")
    ap.add_argument("-o", "--output")
    args = ap.parse_args()
    result = probe(args.bin_file)
    output = json.dumps(result, indent=2)
    if args.output:
        Path(args.output).write_text(output + "\n", encoding="utf-8")
    print(output)
