JS='''
function rng(s){return function(){s|=0;s=s+0x6D2B79F5|0;let t=Math.imul(s^s>>>15,1|s);t=t+Math.imul(t^t>>>7,61|t)^t;return((t^t>>>14)>>>0)/4294967296}}
function drawWF(id,seed,rows,opts){
  const c=document.getElementById(id);const W=c.width,H=c.height;const x=c.getContext('2d');const r=rng(seed);
  const img=x.createImageData(W,H);
  const ramp=(v)=>{ // 0..1 -> quiet navy -> purple noise -> yellow signal
    const stops=[[0,[11,18,48]],[.3,[22,30,96]],[.52,[140,60,200]],[.68,[236,170,90]],[.82,[255,214,70]],[1,[255,235,110]]];
    for(let i=1;i<stops.length;i++){if(v<=stops[i][0]){const a=stops[i-1],b=stops[i];const t=(v-a[0])/(b[0]-a[0]);return [0,1,2].map(k=>a[1][k]+(b[1][k]-a[1][k])*t)}}
    return stops[stops.length-1][1]}
  for(let y=0;y<H;y++){
    // y=0 newest
    const bursts=opts.bursts.filter(b=>y>=b.y0&&y<b.y1);
    for(let xx=0;xx<W;xx++){
      let v=.10+.07*r()+0.04*Math.sin(xx/21)+ (y%23<2?.03:0);
      // gentle noise skirt
      v+=0.05*Math.sin(xx/9+y/5);
      for(const b of bursts){
        if(xx>=b.x0*W&&xx<b.x1*W){
          const u=(xx-b.x0*W)/((b.x1-b.x0)*W);
          const edge=Math.min(u,1-u)/0.06;
          const sub=0.70+0.20*Math.sin(u*38+b.y0)+0.12*Math.sin(u*97+b.y0*3);
          const rowj=0.74+0.16*Math.sin((y-b.y0)*1.7)+0.16*(r()-.5);
          let s=b.lvl*Math.max(0.25,sub)*rowj*Math.min(1,Math.max(0,edge));
          if(!b.tx) v=Math.max(v,s*0.92+0.06*r());
        }
      }
      const col=ramp(Math.min(1,Math.max(0,v)));
      let rgb=col;
      for(const b of bursts){ if(b.tx&&xx>=b.x0*W&&xx<b.x1*W){const s=(0.70+0.3*r());rgb=[255*s,122*s+20,61*s]} }
      const p=(y*W+xx)*4;img.data[p]=rgb[0];img.data[p+1]=rgb[1];img.data[p+2]=rgb[2];img.data[p+3]=255;
    }
  }
  x.putImageData(img,0,0);
}
'''