import html, os
W="#d8d0e8"
PLUS="#ff4dff"

def esc(s): return html.escape(s)

def mark(wave=PLUS, w=27):
    return f'''<svg viewBox="0 0 64 60" width="{w}" height="{w*60/64:.0f}" fill="none"><path d="M8 20 L20 30 L8 40" stroke="#d8d0e8" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/><rect x="25" y="36" width="15" height="5" rx="1" fill="{wave}"/><path d="M40 29 A7 7 0 0 1 47 36" stroke="{wave}" stroke-width="4" stroke-linecap="round"/><path d="M40 23 A13 13 0 0 1 53 36" stroke="{wave}" stroke-width="4" stroke-linecap="round" opacity="0.7"/><path d="M40 17 A19 19 0 0 1 59 36" stroke="{wave}" stroke-width="4" stroke-linecap="round" opacity="0.4"/></svg>'''

ICONS={
 'chat':'<path d="M4 5h16v11H9l-5 4z"/><path d="M8.5 9.2l2.3 1.8-2.3 1.8M12.5 13h3"/>',
 'mail':'<rect x="3" y="5.5" width="18" height="13" rx="1.5"/><path d="M3.5 7l8.5 6.5L20.5 7"/>',
 'map':'<path d="M9 4.5l-5 2v13l5-2 6 2 5-2v-13l-5 2z"/><path d="M9 4.5v13M15 6.5v13"/>',
 'radio':'<path d="M12 11v10M8.5 21h7"/><circle cx="12" cy="9.5" r="1.6"/><path d="M8.2 6.2a5.4 5.4 0 000 6.6M15.8 6.2a5.4 5.4 0 010 6.6M5.4 3.6a9.4 9.4 0 000 11.8M18.6 3.6a9.4 9.4 0 010 11.8"/>',
 'gear':'<circle cx="12" cy="12" r="3"/><circle cx="12" cy="12" r="7.6" stroke-dasharray="2.7 3.1" stroke-width="3.4" stroke-linecap="butt"/><circle cx="12" cy="12" r="6"/>',
 'back':'<path d="M15 5l-7 7 7 7"/>',
 'down':'<path d="M6 9l6 6 6-6"/>',
 'send':'<path d="M4 12l16-8-6 16-3-7z"/>',
 'bt':'<path d="M7 7l10 10-5 4V3l5 4L7 17"/>',
 'usb':'<path d="M12 3v14M12 17a2 2 0 100 4 2 2 0 000-4zM12 9l-4-2v3M12 12l4-2V7"/><circle cx="8" cy="6" r="1"/><rect x="15" y="5" width="2.2" height="2.2"/>',
 'mic':'<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.500 6.500 0 0013 0M12 17.5V21"/>',
 'pin':'<path d="M12 21s-6.500-6-6.500-11a6.500 6.500 0 0113 0c0 5-6.500 11-6.500 11z"/><circle cx="12" cy="10" r="2.300"/>',
 'bell':'<path d="M6 17V11a6 6 0 0112 0v6l1.500 2h-15zM10 21h4"/>',
 'batt':'<rect x="3" y="8" width="16" height="9" rx="1.500"/><path d="M21 11v3M6 11v3M9 11v3M12 11v3"/>',
 'dots':'<circle cx="12" cy="5.500" r="1.200"/><circle cx="12" cy="12" r="1.200"/><circle cx="12" cy="18.500" r="1.200"/>',
 'x':'<path d="M6 6l12 12M18 6L6 18"/>',
 'clip':'<path d="M19 11l-7.500 7.500a4.500 4.500 0 01-6.400-6.400L13 4.200a3 3 0 014.300 4.300l-7.900 7.900a1.500 1.500 0 01-2.100-2.100L14 7.500"/>',
 'swap':'<path d="M7 4l-4 4 4 4M3 8h14M17 20l4-4-4-4M21 16H7"/>',
 'plus':'<path d="M12 5v14M5 12h14"/>',
 'check':'<path d="M5 12.500l4.500 4.500L19 7.500"/>',
 'loc':'<circle cx="12" cy="12" r="3.500"/><path d="M12 2v4M12 18v4M2 12h4M18 12h4"/>',
 'bolt':'<path d="M13 3L5 13h6l-1 8 8-10h-6z"/>',
 'copy':'<rect x="8" y="8" width="12" height="12" rx="1.500"/><path d="M16 8V5.500A1.500 1.500 0 0014.500 4h-9A1.500 1.500 0 004 5.500v9A1.500 1.500 0 005.500 16H8"/>',
}
def ic(name,cls=''):
    return f'<svg class="ic {cls}" viewBox="0 0 24 24">{ICONS[name]}</svg>'

def statusbar(bt=True, usb=False, time="23:31"):
    extra=''
    if bt: extra+='<svg viewBox="0 0 24 24"><path d="M7 7l10 10-5 4V3l5 4L7 17" fill="none" stroke="currentColor" stroke-width="2.2"/></svg>'
    if usb: extra+='<svg viewBox="0 0 24 24"><path d="M12 2l3 4h-2v6l3 2v-2h2v4h-4l-1-.8V19a2.600 2.600 0 11-2 0v-4.800L8 15.400V13l3 2V6H9z"/></svg>'
    return f'''<div class="statusbar"><span style="display:flex;align-items:center;gap:7px">{time}{mark(PLUS,15)}</span><span class="r">{extra}<svg viewBox="0 0 24 24"><path d="M12 20l-11-12a15 15 0 0122 0z" opacity=".95"/></svg><svg viewBox="0 0 24 24"><path d="M2 22V12l20-10v20z"/></svg><span style="font-size:11px">78%</span><svg viewBox="0 0 24 24" style="width:17px"><rect x="3" y="6" width="17" height="12" rx="2" fill="none" stroke="currentColor" stroke-width="1.8"/><rect x="5" y="8" width="11" height="8" fill="currentColor"/><rect x="21" y="10" width="2" height="4"/></svg></span></div>'''

def appbar(wave=PLUS, utc="UTC 23:31", gear=True):
    g=''
    if gear=='mail': g=f'<div class="iconbtn" style="border:1px solid var(--purple);border-radius:4px;width:40px;height:40px;margin:0 6px 0 2px">{ic("gear")}</div>'
    elif gear: g=f'<div class="iconbtn">{ic("gear")}</div>'
    return f'''<div class="appbar"><div class="brand">{mark(wave)}<b>weechat</b><span>radio</span></div><div class="sp"></div><div class="clock">{utc}<br>LCL 23:31</div>{g}</div>'''

def meter(frac, w=46):
    # green 0-.66 purple .66-.88 orange .88-1
    segs=[(0,.66,'#39ff14'),(.66,.88,'#c4b5fd'),(.88,1,'#ff7a3d')]
    out=''
    for a,b,c in segs:
        e=min(frac,b)
        if e>a: out+=f'<i style="left:{a*100}%;width:{(e-a)*100}%;background:{c}"></i>'
    return f'<span class="meter" style="width:{w}px">{out}</span>'

def strip(kind='usb', tx=False, wait=False):
    if kind=='usb': rig='<span class="g"><span class="dot"></span>Digirig</span>'
    else: rig='<span class="g"><span class="dot"></span>VR-N76</span>'
    ptt='<span class="o">PTT TX</span>' if tx else ('<span class="c-am">PTT WAIT</span>' if wait else '<span class="k">PTT</span> <span class="p">RX</span>')
    pre='vhf-fast' if kind=='usb' else 'vhf-fm'
    cls='strip tx' if tx else 'strip'
    return f'''<div class="{cls}"><span class="v">M7TJF</span>{rig}{ptt}<span><span class="k">IN</span> {meter(.52 if not tx else .2,40)}</span><span class="p">{pre}</span><span class="chev">{ic("down").replace('class="ic "','class="ic" style="width:16px;height:16px"')}</span></div>'''

def nav(active, badge=1, chat_pip=False):
    items=[('chat','chat'),('mail','email'),('map','map'),('radio','radio')]
    out=''
    for k,l in items:
        cls='on' if k==active else ''
        b=''
        if k=='mail' and badge: b=f'<span class="badge">{badge}</span>'
        if k=='chat' and chat_pip: b='<span class="pip"></span>'
        out+=f'<a class="{cls}">{ic(k)}<span>{l}</span>{b}</a>'
    return f'<div class="nav">{out}</div>'

def phone(content, nav_active=None, strip_html='', top=None, overlay='', bt=True, usb=False, show_nav=True, badge=1, chat_pip=True, extra_head='', wave=PLUS, gear=True):
    top = appbar(wave, gear=gear) if top is None else top
    n = nav(nav_active, badge, chat_pip) if show_nav else ''
    return f'''<div class="phone">{statusbar(bt,usb)}{top}{strip_html}<div class="content">{content}{overlay}</div>{n}<div class="gesture"><i></i></div></div>'''

def page(body, extra_css='', extra_js=''):
    return f'''<!doctype html><html><head><meta charset="utf-8"><link rel="stylesheet" href="base.css"><style>{extra_css}</style></head><body>{body}<script>{extra_js}</script></body></html>'''

def mod(left,right=''):
    return f'<div class="mod"><span class="l">{left}</span><span class="r">{right}</span></div>'

# waterfall canvas
WF_JS='''
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
