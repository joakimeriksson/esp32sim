import fs from 'node:fs';
import {createHash} from 'node:crypto';
const sha=b=>createHash('sha256').update(b).digest('hex');
// web/run.html drawFrame: byte1/u16le width, byte3/u16le height, then RGB565le.
// Binary type byte 1 is LCD; other binary outputs (audio/camera) are not LCD frames.
export function decodeLcdFrame(bytes){
 if(bytes[0]!==1)return null;
 if(bytes.length<5)throw Error('Truncated LCD header');
 const v=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength),width=v.getUint16(1,true),height=v.getUint16(3,true);
 if(!width||!height||width>4096||height>4096||bytes.length!==5+width*height*2)throw Error('Malformed full LCD RGB565 frame');
 const rgba=new Uint8ClampedArray(width*height*4),rgb565=bytes.slice(5),colors=new Set();let min=255,max=0;
 for(let i=0;i<width*height;i++){const p=v.getUint16(5+2*i,true);colors.add(p);rgba[4*i]=(p>>11)*255/31;rgba[4*i+1]=((p>>5)&63)*255/63;rgba[4*i+2]=(p&31)*255/31;rgba[4*i+3]=255;for(let c=0;c<3;c++){min=Math.min(min,rgba[4*i+c]);max=Math.max(max,rgba[4*i+c]);}}
 return {width,height,rgba,rgb565,uniqueColors:colors.size,nonuniform:colors.size>1,minChannel:min,maxChannel:max};
}
export function createFinalFrameCapture(){let last=null,count=0;return{
 accept(event){const decoded=decodeLcdFrame(event.bytes);if(decoded){count++;last={...decoded,outputIndex:event.index};}},
 write(output){if(!last)return {present:false,lcdFrames:0};const {rgba,rgb565,...metadata}=last;fs.writeFileSync(output+'/final-frame.rgba',rgba);fs.writeFileSync(output+'/final-frame.rgb565le',rgb565);const receipt={present:true,lcdFrames:count,...metadata,rgbaFile:'final-frame.rgba',rgbaSha256:sha(rgba),rgb565File:'final-frame.rgb565le',rgb565Sha256:sha(rgb565),layout:'Full unrotated row-major panel; RGB565 little-endian decoded using Uint8ClampedArray exactly as web/run.html drawFrame; alpha 255',limits:'Last published LCD output, not an independent framebuffer read. Different publication cadence or guest stopping point can change the final image. Nonuniform pixels are a sanity check, not proof of correct rendering.'};fs.writeFileSync(output+'/final-frame.json',JSON.stringify(receipt,null,2)+'\n');return receipt;}
};}
