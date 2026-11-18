import test from "node:test";
import assert from "node:assert/strict";
import { decodeDDS } from "../dds.js";

function dds({width=4,height=4,flags=4,fourCC="DXT1",bits=0,masks=[0,0,0,0],payload=Buffer.alloc(8)}={}) {
  const b=Buffer.alloc(128+payload.length);
  b.write("DDS ",0,"ascii"); b.writeUInt32LE(124,4);
  b.writeUInt32LE(height,12); b.writeUInt32LE(width,16);
  b.writeUInt32LE(32,76); b.writeUInt32LE(flags,80);
  b.write(fourCC,84,"ascii"); b.writeUInt32LE(bits,88);
  masks.forEach((m,i)=>b.writeUInt32LE(m,92+i*4));
  payload.copy(b,128);
  return b.buffer.slice(b.byteOffset,b.byteOffset+b.byteLength);
}
function dxtBlock(alphaBytes=0) {
  const p=Buffer.alloc(16); p.fill(alphaBytes,0,8);
  p.writeUInt16LE(0xf800,8); p.writeUInt16LE(0,10); p.writeUInt32LE(0,12);
  return p;
}

test("decodes DXT1 red block",()=>{
 const p=Buffer.alloc(8);p.writeUInt16LE(0xf800,0);
 const d=decodeDDS(dds({payload:p}));
 assert.deepEqual([...d.rgba.slice(0,4)],[255,0,0,255]);
});
test("decodes DXT1 transparent index",()=>{
 const p=Buffer.alloc(8);p.writeUInt16LE(0,0);p.writeUInt16LE(0xf800,2);p.writeUInt32LE(0xffffffff,4);
 const d=decodeDDS(dds({payload:p}));
 assert.equal(d.rgba[3],0);
});
test("decodes DXT3 alpha",()=>{
 const d=decodeDDS(dds({fourCC:"DXT3",payload:dxtBlock(0xff)}));
 assert.equal(d.rgba[3],255);
});
test("decodes DXT5 alpha",()=>{
 const p=Buffer.alloc(16);p[0]=255;p[1]=0;p.writeUInt16LE(0xf800,8);
 const d=decodeDDS(dds({fourCC:"DXT5",payload:p}));
 assert.equal(d.rgba[3],255);
});
test("decodes RGB24 using channel masks",()=>{
 const d=decodeDDS(dds({flags:0x40,bits:24,masks:[0x00ff0000,0x0000ff00,0x000000ff,0],payload:Buffer.from([0,0,255,...Array(45).fill(0)])}));
 assert.deepEqual([...d.rgba.slice(0,4)],[255,0,0,255]);
});
test("decodes RGBA32 alpha channel",()=>{
 const d=decodeDDS(dds({flags:0x41,bits:32,masks:[0x00ff0000,0x0000ff00,0x000000ff,0xff000000],payload:Buffer.from([0,0,255,128,...Array(60).fill(0)])}));
 assert.deepEqual([...d.rgba.slice(0,4)],[255,0,0,128]);
});
test("decodes luminance 8-bit",()=>{
 const d=decodeDDS(dds({flags:0x20000,bits:8,masks:[255,0,0,0],payload:Buffer.from([127,...Array(15).fill(0)])}));
 assert.deepEqual([...d.rgba.slice(0,4)],[127,127,127,255]);
});
test("rejects unsupported FourCC and truncated payload",()=>{
 assert.throws(()=>decodeDDS(dds({fourCC:"ATI2"})),/tidak didukung/);
 assert.throws(()=>decodeDDS(dds({payload:Buffer.alloc(2)})),/terpotong/);
});
test("rejects invalid dimensions",()=>{
 assert.throws(()=>decodeDDS(dds({width:0})),/Dimensi/);
});
