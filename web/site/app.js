import init,{engine_version,list_textures,decode_texture,replace_texture}from"./pkg/nfsu2_web_engine.js";
const $=id=>document.getElementById(id);
let wasm=false,archive=null,textures=[],selected=-1,sourceName="archive.bin",modified=false;
const MAX_PIXELS=4194304,MAX_SIDE=4096;
function message(s){$("message").textContent=s}
function u32(v,o){return v.getUint32(o,true)}
function rgb565(c){return [((c>>11)&31)*255/31,((c>>5)&63)*255/63,(c&31)*255/31]}
function decodeDDS(buf){
 const b=new Uint8Array(buf),v=new DataView(buf);
 if(b.length<128||String.fromCharCode(...b.subarray(0,4))!=="DDS "||u32(v,4)!==124||u32(v,76)!==32)throw Error("Header DDS tidak valid.");
 const h=u32(v,12),w=u32(v,16),pf=u32(v,80),four=String.fromCharCode(...b.subarray(84,88)),bits=u32(v,88);
 if(!w||!h||w>MAX_SIDE||h>MAX_SIDE||w*h>MAX_PIXELS)throw Error("Dimensi DDS melewati batas aman.");
 const out=new Uint8ClampedArray(w*h*4),data=128;
 const put=(x,y,c)=>{if(x<w&&y<h){const p=(y*w+x)*4;out[p]=c[0];out[p+1]=c[1];out[p+2]=c[2];out[p+3]=c[3]??255}};
 const color=(off,forceFour)=>{
  const c0=v.getUint16(off,true),c1=v.getUint16(off+2,true),a=rgb565(c0),z=rgb565(c1),pal=[a,z];
  if(c0>c1||forceFour)pal.push(a.map((n,i)=>(2*n+z[i])/3),a.map((n,i)=>(n+2*z[i])/3));
  else pal.push(a.map((n,i)=>(n+z[i])/2),[0,0,0]);
  return {pal,idx:v.getUint32(off+4,true),transparent:c0<=c1&&!forceFour};
 };
 if((pf&4)!==0){
  if(!["DXT1","DXT3","DXT5"].includes(four))throw Error("FourCC DDS tidak didukung: "+four);
  const blockBytes=four==="DXT1"?8:16,need=Math.ceil(w/4)*Math.ceil(h/4)*blockBytes;
  if(b.length-data<need)throw Error("Data blok DDS terpotong.");
  let off=data;
  for(let by=0;by<Math.ceil(h/4);by++)for(let bx=0;bx<Math.ceil(w/4);bx++){
   let alpha=null,co=off;
   if(four==="DXT3"){
    alpha=[];for(let j=0;j<8;j++){const q=b[off+j];alpha.push((q&15)*17,(q>>4)*17)}co+=8;
   }else if(four==="DXT5"){
    const a0=b[off],a1=b[off+1],p=[a0,a1];
    if(a0>a1){for(let i=1;i<=6;i++)p.push(((7-i)*a0+i*a1)/7)}
    else{for(let i=1;i<=4;i++)p.push(((5-i)*a0+i*a1)/5);p.push(0,255)}
    let bits48=0n;for(let j=0;j<6;j++)bits48|=BigInt(b[off+2+j])<<BigInt(8*j);
    alpha=Array.from({length:16},(_,i)=>p[Number((bits48>>BigInt(3*i))&7n)]);co+=8;
   }
   const c=color(co,four!=="DXT1");
   for(let py=0;py<4;py++)for(let px=0;px<4;px++){
    const i=py*4+px,ci=(c.idx>>>(2*i))&3,cc=c.pal[ci];
    let al=alpha?alpha[i]:((four==="DXT1"&&c.transparent&&ci===3)?0:255);
    put(bx*4+px,by*4+py,[...cc,al]);
   }
   off+=blockBytes;
  }
 }else if((pf&0x40)!==0){
  if(bits!==24&&bits!==32)throw Error("DDS RGB harus 24-bit atau 32-bit.");
  const bytes=bits/8,need=w*h*bytes;if(b.length-data<need)throw Error("Data piksel DDS terpotong.");
  const masks=[u32(v,92),u32(v,96),u32(v,100),u32(v,104)];
  if(!masks[0]||!masks[1]||!masks[2])throw Error("Mask RGB DDS tidak valid.");
  const channel=(value,mask)=>{if(!mask)return 255;let shift=0;while(((mask>>>shift)&1)===0)shift++;const m=mask>>>shift;return Math.round(((value&mask)>>>shift)*255/m)};
  for(let i=0;i<w*h;i++){const p=data+i*bytes,val=bytes===4?u32(v,p):(b[p]|b[p+1]<<8|b[p+2]<<16);put(i%w,Math.floor(i/w),[channel(val,masks[0]),channel(val,masks[1]),channel(val,masks[2]),masks[3]?channel(val,masks[3]):255])}
 }else if((pf&0x20000)!==0&&bits===8){
  if(b.length-data<w*h)throw Error("Data luminance DDS terpotong.");
  for(let i=0;i<w*h;i++)put(i%w,Math.floor(i/w),[b[data+i],b[data+i],b[data+i],255]);
 }else throw Error("Format DDS tidak didukung. Gunakan DXT1/DXT3/DXT5, RGB/RGBA 24/32-bit, atau luminance 8-bit.");
 return {width:w,height:h,rgba:out};
}
async function boot(){try{await init();wasm=true;$("engine-status").textContent=engine_version();$("engine-status").style.color="var(--good)";message("Mesin siap. Pilih file BIN.")}catch(e){$("engine-status").textContent="Mesin belum tersedia";message("Build WebAssembly belum selesai: "+e)}}
function render(){const q=$("search").value.toLowerCase().trim(),root=$("texture-list");root.replaceChildren();const rows=textures.filter(t=>t.hash.toLowerCase().includes(q)||String(t.index).includes(q));if(!rows.length){root.textContent="Tidak ada tekstur yang cocok.";return}for(const t of rows){const b=document.createElement("button");b.className="texture-item"+(selected===t.index?" active":"");const a=document.createElement("span"),z=document.createElement("small");a.textContent=String(t.index).padStart(4,"0")+" · "+t.hash;z.textContent=t.width&&t.height?t.width+"×"+t.height:"Preview —";b.append(a,z);b.onclick=()=>select(t.index);root.append(b)}}
function select(i){selected=i;render();const t=textures[i];$("selected-title").textContent="Texture "+String(i).padStart(4,"0")+" · "+t.hash;$("dimensions").textContent=t.width&&t.height?t.width+" × "+t.height:"—";$("dds-input").disabled=!t.width||!t.height;$("import-label").classList.toggle("disabled",$("dds-input").disabled);try{const d=decode_texture(archive,i),c=$("preview-canvas"),ctx=c.getContext("2d");c.width=d.width;c.height=d.height;ctx.putImageData(new ImageData(new Uint8ClampedArray(d.rgba),d.width,d.height),0,0);c.style.display="block";$("preview-placeholder").style.display="none";message("Preview berhasil dibaca.")}catch(e){$("preview-canvas").style.display="none";$("preview-placeholder").style.display="block";$("preview-placeholder").textContent=String(e);message("Preview tekstur gagal dibaca: "+e)}}
$("bin-input").onchange=async e=>{const f=e.target.files&&e.target.files[0];if(!f)return;if(!wasm){message("Mesin belum siap.");return}if(f.size>60*1024*1024){message("File melebihi batas 60 MiB.");return}message("Membaca struktur BIN…");try{const bytes=new Uint8Array(await f.arrayBuffer()),list=list_textures(bytes);archive=bytes;textures=list;sourceName=f.name;modified=false;selected=-1;$("file-name").textContent=f.name;$("texture-count").textContent=list.length;$("archive-status").textContent="Terbaca";$("search").disabled=false;$("export-bin").disabled=false;$("dds-input").disabled=true;$("texture-list").classList.remove("empty");render();message("Berhasil membaca "+list.length+" entri. File asli tetap aman.")}catch(err){archive=null;textures=[];$("archive-status").textContent="Gagal";message(String(err))}};
$("search").oninput=render;
$("dds-input").onchange=async e=>{const f=e.target.files&&e.target.files[0];if(!f||!archive||selected<0)return;try{message("Membaca dan memvalidasi DDS…");const d=decodeDDS(await f.arrayBuffer()),t=textures[selected];if(!t.width||!t.height)throw Error("Dimensi tekstur target tidak dapat dibaca.");if(d.width!==t.width||d.height!==t.height)throw Error("Ukuran DDS "+d.width+"×"+d.height+" tidak cocok dengan target "+t.width+"×"+t.height+".");message("Mengganti tekstur dalam salinan arsip…");const updated=replace_texture(archive,selected,d.rgba,d.width,d.height);archive=new Uint8Array(updated);modified=true;$("archive-status").textContent="Diubah · belum diekspor";select(selected);message("Tekstur berhasil diganti di memori. Arsip asli tidak ditimpa; ekspor BIN untuk menyimpan.")}catch(err){message("Import DDS gagal: "+err)}finally{e.target.value=""}};
$("export-bin").onclick=()=>{if(!archive)return;const url=URL.createObjectURL(new Blob([archive],{type:"application/octet-stream"})),a=document.createElement("a");a.href=url;a.download=sourceName.replace(/\.bin$/i,"")+(modified?"_edited":"_copy")+".bin";a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);message(modified?"BIN hasil edit berhasil diekspor sebagai file baru.":"Salinan BIN diunduh; sumber tidak ditimpa.")};
if("serviceWorker"in navigator)addEventListener("load",()=>navigator.serviceWorker.register("./sw.js").catch(()=>{}));boot();
