const MAX_PIXELS=4194304,MAX_SIDE=4096;
const u32=(v,o)=>v.getUint32(o,true);
const rgb565=c=>[((c>>11)&31)*255/31,((c>>5)&63)*255/63,(c&31)*255/31];
export function decodeDDS(buf){
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
