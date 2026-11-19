// Offline color separation for vinyl artwork. Exports RGBA DDS masks; import one mask per Indexed8 vinyl slot.
const csRoot=document.getElementById("color-separation");
if(csRoot){
 const $c=id=>document.getElementById(id);
 let src=null,clusters=[];
 function say(s){$c("cs-message").textContent=s}
 function hex(c){return "#"+c.map(v=>v.toString(16).padStart(2,"0")).join("")}
 function quantize(data,w,h,k){
  const sample=[],stride=Math.max(1,Math.floor((w*h)/18000));
  for(let i=0;i<w*h;i+=stride) {const p=i*4;if(data[p+3]>32)sample.push([data[p],data[p+1],data[p+2]])}
  if(sample.length<k)throw Error("Artwork terlalu transparan atau terlalu kecil untuk jumlah warna ini.");
  const centers=[sample[0]];
  while(centers.length<k){
   let best=null,bd=-1;
   for(const s of sample){let md=Infinity;for(const c of centers){const d=(s[0]-c[0])**2+(s[1]-c[1])**2+(s[2]-c[2])**2;md=Math.min(md,d)}if(md>bd){bd=md;best=s}}
   centers.push(best);
  }
  const nearest=(s,cs)=>{let bi=0,bd=Infinity;for(let j=0;j<cs.length;j++){const c=cs[j],d=(s[0]-c[0])**2+(s[1]-c[1])**2+(s[2]-c[2])**2;if(d<bd){bd=d;bi=j}}return bi};
  for(let it=0;it<8;it++){
   const sums=Array.from({length:k},()=>[0,0,0,0]);
   for(const s of sample){const j=nearest(s,centers),a=sums[j];a[0]+=s[0];a[1]+=s[1];a[2]+=s[2];a[3]++}
   for(let j=0;j<k;j++)if(sums[j][3])centers[j]=sums[j].slice(0,3).map(v=>Math.round(v/sums[j][3]));
  }
  const assignment=new Uint8Array(w*h),hist=new Uint32Array(k);
  for(let i=0;i<w*h;i++){const p=i*4;if(data[p+3]<32){assignment[i]=255;continue}const j=nearest([data[p],data[p+1],data[p+2]],centers);assignment[i]=j;hist[j]++}
  const order=Array.from({length:k},(_,i)=>i).sort((a,b)=>hist[b]-hist[a]);
  return order.map(old=>({color:centers[old],count:hist[old],mask:assignment.map(v=>v===255?255:v===old?1:0)}));
 }
 function dds(mask,w,h){
  const b=new Uint8Array(128+w*h*4),v=new DataView(b.buffer);
  b.set([68,68,83,32]);v.setUint32(4,124,true);v.setUint32(8,0x0002100F,true);v.setUint32(12,h,true);v.setUint32(16,w,true);v.setUint32(20,w*4,true);v.setUint32(28,1,true);v.setUint32(76,32,true);v.setUint32(80,0x41,true);v.setUint32(84,0,true);v.setUint32(88,32,true);v.setUint32(92,0x00ff0000,true);v.setUint32(96,0x0000ff00,true);v.setUint32(100,0x000000ff,true);v.setUint32(104,0xff000000,true);v.setUint32(108,0x1000,true);
  for(let i=0;i<w*h;i++){const p=128+i*4; if(mask[i]){b[p]=255;b[p+1]=255;b[p+2]=255;b[p+3]=255}else{b[p]=b[p+1]=b[p+2]=b[p+3]=0}}
  return b;
 }
 function downloadBlob(blob,name){const u=URL.createObjectURL(blob),a=document.createElement("a");a.href=u;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(u),1500)}
 function paintLayers(){
  const list=$c("cs-layers");list.replaceChildren();
  clusters.forEach((layer,i)=>{
   const card=document.createElement("div");card.className="cs-card";
   const cv=document.createElement("canvas");cv.width=src.width;cv.height=src.height;const ctx=cv.getContext("2d"),im=ctx.createImageData(src.width,src.height);
   for(let p=0;p<layer.mask.length;p++){const q=p*4;if(layer.mask[p]){im.data[q]=im.data[q+1]=im.data[q+2]=255;im.data[q+3]=255}}
   ctx.putImageData(im,0,0);
   const sw=document.createElement("span");sw.className="cs-swatch";sw.style.background=hex(layer.color);
   const meta=document.createElement("div");meta.className="cs-meta";meta.textContent="Layer "+(i+1)+" · "+hex(layer.color)+" · "+layer.count+" px";
   const preview=document.createElement("div");preview.className="cs-preview";preview.append(cv);
   const btn=document.createElement("button");btn.className="primary";btn.textContent="Unduh DDS Mask";btn.onclick=()=>downloadBlob(new Blob([dds(layer.mask,src.width,src.height)],{type:"application/octet-stream"}),"vinyl_layer_"+String(i+1).padStart(2,"0")+".dds");
   const label=document.createElement("p");label.className="cs-label";label.textContent="Warna target game: "+hex(layer.color)+" (atur warna layer secara manual di game)";
   card.append(sw,meta,preview,label,btn);list.append(card);
  });
  $c("cs-download-all").disabled=!clusters.length;
 }
 $c("cs-input").onchange=async e=>{const file=e.target.files?.[0];if(!file)return;try{say("Memuat artwork…");const bitmap=await createImageBitmap(file);if(bitmap.width>4096||bitmap.height>4096||bitmap.width*bitmap.height>4194304)throw Error("Artwork terlalu besar (maks 4096×4096 / 4,2 MP).");const canvas=$c("cs-source"),ctx=canvas.getContext("2d");canvas.width=bitmap.width;canvas.height=bitmap.height;ctx.clearRect(0,0,canvas.width,canvas.height);ctx.drawImage(bitmap,0,0);src=ctx.getImageData(0,0,canvas.width,canvas.height);$c("cs-source-wrap").hidden=false;$c("cs-size").textContent=canvas.width+" × "+canvas.height;say("Artwork siap dipisah. Pilih jumlah layer lalu tekan Pisahkan warna.");}catch(err){say("Gagal memuat gambar: "+err)}finally{e.target.value=""}};
 $c("cs-run").onclick=()=>{if(!src)return;try{const k=Number($c("cs-count").value);say("Memisahkan warna…");clusters=quantize(src.data,src.width,src.height,k);paintLayers();say("Selesai: "+clusters.length+" layer warna. Unduh mask DDS satu per satu, lalu import ke slot vinyl Indexed8.");}catch(err){say("Color separation gagal: "+err)}};
 $c("cs-download-all").onclick=()=>{clusters.forEach((layer,i)=>downloadBlob(new Blob([dds(layer.mask,src.width,src.height)],{type:"application/octet-stream"}),"vinyl_layer_"+String(i+1).padStart(2,"0")+".dds"))};
}
