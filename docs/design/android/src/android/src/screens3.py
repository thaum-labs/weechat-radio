from lib import *

# ---------- map geometry (lon -2.62..-2.12, 51.4335 top) ----------
def XY(lat,lon,h=None):
    return ((lon+2.62)*824, (51.4335-lat)*1323)
FROME=XY(51.2308,-2.3207); BATH=XY(51.3811,-2.3590)
PLACES={'Bradford on Avon':XY(51.3466,-2.2516),'Trowbridge':XY(51.3197,-2.2084),'Radstock':XY(51.2915,-2.4475),'Warminster':XY(51.2049,-2.1795),'Frome':FROME,'Bath':BATH,'Midsomer Norton':XY(51.2866,-2.4818)}

def lerp(a,b,t): return (a[0]+(b[0]-a[0])*t, a[1]+(b[1]-a[1])*t)

def mapsvg(w=412,h=340,ring=True,zoomctl=True,legend=True,stations=True,dy=0):
    F=FROME; B=BATH
    seg=[(0,.24,'#7d9bff'),(.24,.62,'#ffd60a'),(.62,1,'#7d9bff')]
    path=''
    for a,b,c in seg:
        p1=lerp(F,B,a);p2=lerp(F,B,b)
        path+=f'<line x1="{p1[0]:.1f}" y1="{p1[1]:.1f}" x2="{p2[0]:.1f}" y2="{p2[1]:.1f}" stroke="{c}" stroke-width="3.2" stroke-linecap="round" opacity=".95"/>'
    mid=lerp(F,B,.43)
    rr=47
    rings=''
    if ring:
        for P,lab in ((F,'sure 4 km'),(B,'sure 4 km')):
            rings+=f'<circle cx="{P[0]:.1f}" cy="{P[1]:.1f}" r="{rr}" fill="rgba(125,155,255,.10)" stroke="#7d9bff" stroke-opacity=".7" stroke-dasharray="4 4"/>'
        rings+=f'<text x="{F[0]+rr+4:.0f}" y="{F[1]+3:.0f}" fill="#9fb4ff" font-size="9.5">sure 4 km</text>'
        rings+=f'<text x="{B[0]+rr+4:.0f}" y="{B[1]+3:.0f}" fill="#9fb4ff" font-size="9.5">sure 4 km</text>'
    # roads
    roads=f'''<path d="M-10 322 C80 300 160 270 {F[0]:.0f} {F[1]:.0f} S330 215 430 196" stroke="#3a4468" stroke-width="1.6" fill="none" opacity=".8"/>
    <path d="M{B[0]:.0f} {B[1]:.0f} C250 120 320 140 {PLACES['Trowbridge'][0]:.0f} {PLACES['Trowbridge'][1]:.0f} S380 260 {PLACES['Warminster'][0]:.0f} {PLACES['Warminster'][1]:.0f}" stroke="#3a4468" stroke-width="1.4" fill="none" opacity=".7"/>
    <path d="M-10 150 C60 160 100 190 {PLACES['Radstock'][0]:.0f} {PLACES['Radstock'][1]:.0f} S190 120 {B[0]:.0f} {B[1]:.0f}" stroke="#3a4468" stroke-width="1.4" fill="none" opacity=".7"/>
    <path d="M60 -10 C90 40 120 70 {B[0]:.0f} {B[1]:.0f}" stroke="#2f5d86" stroke-width="2.4" fill="none" opacity=".55"/>'''
    labels=''
    for n,(x,y) in PLACES.items():
        if n in ('Frome','Bath','Midsomer Norton','Bradford on Avon'): continue
        anc='end' if x>300 else 'start'; dx=-5 if x>300 else 5
        labels+=f'<circle cx="{x:.0f}" cy="{y:.0f}" r="2" fill="#6f6880"/><text x="{x+dx:.0f}" y="{y+14 if x>300 else y+3:.0f}" text-anchor="{anc}" fill="#8a829b" font-size="9.5">{n}</text>'
    others=''
    if stations:
        for (lat,lon,call,col) in [(51.3197,-2.2084,'XX0DDD','#00e5ff'),(51.2915,-2.4475,'XX0EEE','#ffbf00'),(51.2049,-2.1795,'XX0FFF','#ff4dff'),(51.2530,-2.2650,'XX0AAA','#39ff14')]:
            x,y=XY(lat,lon)
            anc='end' if x>280 else 'start'; dx=-8 if x>280 else 8
            others+=f'<circle cx="{x:.0f}" cy="{y:.0f}" r="9" fill="{col}" opacity=".12"/><circle cx="{x:.0f}" cy="{y:.0f}" r="4.5" fill="{col}"/><text x="{x+dx:.0f}" y="{y-6:.0f}" text-anchor="{anc}" fill="#d8d0e8" font-size="9.5" style="paint-order:stroke" stroke="#07070a" stroke-width="3">{call}</text>'
    def pin(P,lab,col,sub):
        return f'''<g transform="translate({P[0]:.1f},{P[1]:.1f})"><circle r="14" fill="{col}" opacity=".18"/><circle r="10" fill="#07070a" stroke="{col}" stroke-width="2"/>
        <path d="M-5 -3 L-1 0 L-5 3" stroke="#d8d0e8" stroke-width="1.8" fill="none" stroke-linecap="round" stroke-linejoin="round"/><rect x="0" y="2" width="5" height="1.8" fill="{col}"/>
        <text y="26" text-anchor="middle" fill="#fff" font-size="10" style="paint-order:stroke" stroke="#07070a" stroke-width="3.5">{lab}</text><text y="37" text-anchor="middle" fill="#9a93aa" font-size="9" style="paint-order:stroke" stroke="#07070a" stroke-width="3.5">{sub}</text></g>'''
    pins=pin(B,'RX','#7d9bff','XX0BBB · Bath')+pin(F,'HOME','#ff7a3d','M7TJF · IO81UF')
    mid_tag=f'<g transform="translate({mid[0]:.0f},{mid[1]:.0f})"><circle r="4" fill="#ffd60a"/></g>'
    leg='''<div style="position:absolute;left:10px;top:10px;background:rgba(7,7,10,.82);border:1px solid var(--line);padding:5px 9px;font-size:10px;color:var(--dim);display:flex;gap:9px;align-items:center;border-radius:3px"><span><i class="sw" style="background:#3b4f9a;border-color:#7d9bff"></i>clear</span><span><i class="sw" style="background:#ffd60a;border-color:#ffd60a"></i>marginal</span><span><i class="sw" style="background:#ff3b6b;border-color:#ff3b6b"></i>blocked</span></div>''' if legend else ''
    zc=f'''<div style="position:absolute;right:10px;top:10px;display:flex;flex-direction:column;gap:6px"><div class="iconbtn" style="width:40px;height:40px;background:rgba(7,7,10,.85);border:1px solid var(--line);border-radius:4px;color:var(--fg)">{ic("plus")}</div><div class="iconbtn" style="width:40px;height:40px;background:rgba(7,7,10,.85);border:1px solid var(--line);border-radius:4px;color:var(--fg)"><svg class="ic" viewBox="0 0 24 24"><path d="M5 12h14"/></svg></div><div class="iconbtn" style="width:40px;height:40px;background:rgba(7,7,10,.85);border:1px solid var(--accent);border-radius:4px;color:var(--accent);margin-top:6px">{ic("loc")}</div></div>''' if zoomctl else ''
    scale=f'<div style="position:absolute;left:10px;bottom:8px;font-size:9.5px;color:#b7aec8;display:flex;align-items:center;gap:6px"><span style="display:block;width:{11.9*5:.0f}px;height:5px;border:1px solid #b7aec8;border-top:0"></span>5 km</div>'
    attr='<div style="position:absolute;right:8px;bottom:6px;font-size:8.5px;color:#6f6880">sample basemap</div>'
    return f'''<div class="mapbox" style="height:{h}px;background:url(img/terrain.png) 0 {-dy}px/412px 500px"><svg width="{w}" height="{h}" viewBox="0 0 {w} {h}" style="position:absolute;inset:0;font-family:inherit">{roads}{labels}{rings}{path}{mid_tag}{others}{pins}</svg>{leg}{zc}{scale}{attr}</div>'''

def search_row(txt='other station: callsign, city or grid'):
    return f'''<div style="display:flex;gap:8px;padding:8px 10px;background:var(--shell);border-bottom:1px solid var(--line)"><div class="input" style="flex:1;color:#5d576b;min-height:42px;font-size:12.5px">{txt}</div><div class="btn s c-di" style="min-height:42px;border-color:var(--line)">on air</div></div>'''

def led_tag(txt,col='c-am'):
    return f'<div class="chip {col}" style="background:none;border-radius:0;padding:5px 10px;font-size:12.5px;line-height:1.4;white-space:normal;align-items:flex-start"><span class="led" style="margin-top:5px;flex:none"></span><span>{txt}</span></div>'

def bandchip(label,w,col,sel=False,sub=''):
    dot={'g':'#39ff14','y':'#ffd60a','n':'#8a8a99'}[col]
    if sel:
        return f'<div style="flex:1;border:1px solid var(--accent);background:var(--accent);color:#07070a;padding:6px 0;text-align:center;line-height:1.2;font-size:13px"><span style="display:inline-block;width:7px;height:7px;border-radius:50%;background:{dot};margin-right:4px;box-shadow:0 0 0 1px #07070a"></span>{label}<br><span style="font-size:10px;opacity:.75">{w}</span></div>'
    c={'g':'#39ff14','y':'#ffd60a','n':'#8a8a99'}[col]
    return f'<div style="flex:1;border:1px solid var(--line);color:{c};padding:6px 0;text-align:center;line-height:1.2;font-size:13px"><span style="display:inline-block;width:7px;height:7px;border-radius:50%;background:{dot};margin-right:4px"></span>{label}<br><span style="font-size:10px;color:var(--dim)">{w}</span></div>'

def bandrow(items):
    return '<div style="display:flex;gap:0">'+''.join(bandchip(*i) for i in items)+'</div>'

def legend3():
    return '<div class="legend"><span><i style="background:#39ff14"></i>works</span><span><i style="background:#ffd60a"></i>skips, may or may not work</span><span><i style="background:#8a8a99"></i>won\'t work</span></div>'

def map_screen():
    sheet=f'''<div style="flex:1;background:var(--shell);border-top:1px solid var(--accent);border-radius:14px 14px 0 0;margin-top:-10px;position:relative;z-index:3;padding:8px 14px 0;display:flex;flex-direction:column;gap:9px">
      <div class="grab" style="width:40px;height:4px;border-radius:2px;background:#3b3f5a;margin:0 auto"></div>
      {led_tag('Marginal: a ridge 6 km from Frome clips the line. Raise the antennas or add power for a solid link.')}
      <div style="display:flex;align-items:flex-end;justify-content:space-between"><div><div style="font-size:34px;line-height:1;color:var(--orange);letter-spacing:-.04em">433.625 <span style="font-size:15px;letter-spacing:0">MHz</span></div><div style="margin-top:4px;color:var(--orange);font-size:13px">70cm · NFM · vhf-fast</div></div><div class="sub" style="text-align:right;line-height:1.5">17 km<br>5 W each end</div></div>
      {bandrow([('70cm','5 W','y',True),('2m','5 W','g'),('80m','5 W','y'),('40m','5 W','y'),('20m','5 W','n'),('10m','5 W','n')])}
      {legend3()}
      <div style="display:flex;gap:8px;margin-top:2px"><div class="btn c-acc" style="flex:1">Plan details ▴</div><div class="btn fillo" style="flex:1">Tune radio</div></div>
    </div>'''
    c=search_row()+mapsvg(h=348)+sheet
    return phone(c,'map',strip('usb'),chat_pip=True)

# ---------- profile (terrain) svg ----------
def profile_svg():
    return '''<svg viewBox="0 0 384 112" width="384" height="112" style="font-family:inherit">
    <rect x="0" y="0" width="384" height="112" fill="#0a0a10"/>
    <rect x="0" y="0" width="64" height="112" fill="rgba(125,155,255,.08)"/><rect x="320" y="0" width="64" height="112" fill="rgba(125,155,255,.08)"/>
    <path d="M0 98 Q192 88 384 98" stroke="#4b557f" stroke-dasharray="3 4" fill="none"/>
    <path d="M0 92 L26 88 L50 84 L80 80 L104 70 L122 56 L136 48 L150 54 L172 66 L200 74 L232 70 L262 62 L300 66 L340 72 L384 80 L384 112 L0 112Z" fill="#1c2540" stroke="#2e3a66"/>
    <path d="M114 63 L136 48 L158 59" fill="#ffd60a" opacity=".85"/>
    <line x1="6" y1="40" x2="378" y2="34" stroke="#7d9bff" stroke-width="2"/>
    <line x1="98" y1="38.8" x2="236" y2="36.2" stroke="#ffd60a" stroke-width="3"/>
    <circle cx="6" cy="40" r="3" fill="#ff7a3d"/><circle cx="378" cy="34" r="3" fill="#7d9bff"/>
    <text x="8" y="14" fill="#ff7a3d" font-size="10">HOME · duck 1.5 m</text><text x="376" y="14" fill="#7d9bff" font-size="10" text-anchor="end">RX · mobile whip 1.8 m</text>
    <text x="136" y="30" fill="#ffd60a" font-size="9.5" text-anchor="middle">ridge clips here</text>
    <text x="32" y="106" fill="#9fb4ff" font-size="9" text-anchor="middle">sure 4 km</text><text x="352" y="106" fill="#9fb4ff" font-size="9" text-anchor="middle">sure 4 km</text>
    <text x="192" y="106" fill="#6f6880" font-size="9" text-anchor="middle">17 km</text>
    </svg>'''

def stationcards():
    def card(lab,col,ant,h,p,extra=''):
        return f'''<div class="card" style="flex:1;padding:9px 10px;border-radius:4px"><div style="color:{col};font-size:12px">{lab}</div><div style="font-size:12.5px;margin-top:3px;line-height:1.3">{ant}</div><div class="sub" style="margin-top:3px;line-height:1.35">{h} · {p}</div>{extra}</div>'''
    return f'''<div style="display:flex;gap:8px">{card("HOME","var(--orange)","handheld rubber duck","1.5 m","5 W")}{card("RX","var(--accent)","mobile whip","1.8 m car roof","5 W",'<div class="chip c-pu" style="margin-top:4px;font-size:10px;padding:0 6px">Mobile antenna</div>')}</div>'''

def plan_screen():
    head=f'''<div style="display:flex;align-items:center;gap:8px;padding:6px 10px 6px 4px;border-bottom:1px solid var(--line);background:var(--shell)"><div class="iconbtn" style="color:var(--purple)">{ic("back")}</div><div style="flex:1;min-width:0"><div style="font-size:13px;color:var(--fg)">Frome <span class="c-pu">IO81UF</span> → Bath <span class="c-pu">IO81TJ</span></div><div class="sub">17 km · XX0BBB</div></div><div class="btn s c-di" style="border-color:var(--line)">swap</div></div>'''
    body=f'''<div style="padding:10px 14px;display:flex;flex-direction:column;gap:10px;overflow:hidden;flex:1">
      {stationcards()}
      <div style="border:1px solid var(--line)">{profile_svg()}</div>
      {bandrow([('70cm','5 W','y',True),('2m','5 W','g'),('80m','5 W','y'),('40m','5 W','y'),('20m','5 W','n'),('10m','5 W','n')])}
      {legend3()}
      <div style="display:flex;gap:16px;align-items:flex-start"><div><div style="font-size:34px;line-height:1;color:var(--orange);letter-spacing:-.04em">433.625<span style="font-size:15px;letter-spacing:0"> MHz</span></div><div style="margin-top:4px;color:var(--orange);font-size:13px">70cm · NFM</div></div>
      <div class="hdr" style="grid-template-columns:auto 1fr;font-size:12px;margin-left:auto;gap:2px 10px"><span class="k">power</span><span>5 W each end</span><span class="k">preset</span><span class="c-pu">vhf-fast</span><span class="k">sure</span><span>4 km each end</span></div></div>
      {led_tag('Marginal: a ridge 6 km from Frome clips the line. Raise the antennas or add power for a solid link.')}
      <div class="sub" style="line-height:1.5;color:var(--purple)">NARROW FM on 433.625, low power indoors. Handhelds: rest between TX blocks.</div>
      <div class="sub" style="font-size:11px;color:#ffd60a">Check your band plan before you transmit.</div>
    </div>
    <div style="flex:none;background:var(--shell);border-top:1px solid var(--line);padding:10px 14px 12px"><div style="display:flex;gap:8px;align-items:center;margin-bottom:8px"><div class="input" style="flex:1;min-height:36px;font-size:12px;color:var(--fg)"><span class="c-di" style="margin-right:6px">$</span>/radio qsy 433.625</div><div class="btn s c-di" style="min-height:36px;border-color:var(--line)">{ic("copy").replace('class="ic "','class="ic" style="width:15px;height:15px"')} copy</div></div><div class="btn fillo" style="width:100%">Tune radio: 433.625 NFM + vhf-fast</div></div>'''
    return phone(head+body,'map',strip('usb'))

def sky_svg():
    return '''<svg viewBox="0 0 384 130" width="384" height="130" style="font-family:inherit">
    <rect width="384" height="130" fill="#0a0a10"/>
    <path d="M0 26 Q192 6 384 26 L384 44 Q192 24 0 44Z" fill="rgba(140,100,220,.30)" stroke="rgba(160,120,240,.5)"/>
    <text x="6" y="16" fill="#a98ae8" font-size="9">F2 layer · side view · not to scale</text>
    <path d="M0 112 Q192 100 384 112 L384 130 L0 130Z" fill="#141c38" stroke="#3b4a86"/>
    <path d="M168 104 L190 38 L212 104" stroke="#ffd60a" stroke-width="2.4" fill="none" stroke-linejoin="round"/>
    <path d="M168 104 L190 38" stroke="#ffd60a" stroke-dasharray="1 0"/>
    <path d="M190 38 L206 -2" stroke="#ff4d6d" stroke-width="1.5" stroke-dasharray="3 3" fill="none"/>
    <text x="250" y="12" fill="#ff7a8f" font-size="9">40m goes through</text>
    <circle cx="168" cy="104" r="3" fill="#fff"/><circle cx="212" cy="104" r="3" fill="#fff"/>
    <text x="150" y="124" fill="#d8d0e8" font-size="9" text-anchor="end">TX · take-off 89°</text><text x="228" y="124" fill="#d8d0e8" font-size="9">RX</text>
    </svg>'''

def sky_screen():
    head=f'''<div style="display:flex;align-items:center;gap:8px;padding:6px 10px 6px 4px;border-bottom:1px solid var(--line);background:var(--shell)"><div class="iconbtn" style="color:var(--purple)">{ic("back")}</div><div style="flex:1;min-width:0"><div style="font-size:13px;color:var(--fg)">Frome <span class="c-pu">IO81UF</span> → Bath <span class="c-pu">IO81TJ</span></div><div class="sub">17 km · HF sky path · 23:00 UTC</div></div><div class="btn s c-di" style="border-color:var(--line)">swap</div></div>'''
    cells=''.join(f'<i style="flex:1;height:14px;background:{"#39ff14" if k==19 else "#ffd60a"};{"outline:2px solid #fff;outline-offset:1px;z-index:2;" if k==0 else ""}"></i>' for k in range(24))
    body=f'''<div style="padding:10px 14px;display:flex;flex-direction:column;gap:10px;overflow:hidden;flex:1">
      <div style="border:1px solid var(--line)">{sky_svg()}</div>
      {bandrow([('80m','1 W','y',True),('60m','1 W','y'),('40m','1 W','y'),('30m','1 W','y'),('20m','1 W','n'),('10m','1 W','n')])}
      {legend3()}
      <div style="display:flex;gap:14px;align-items:flex-start"><div><div style="font-size:34px;line-height:1;color:var(--orange);letter-spacing:-.04em">3.580<span style="font-size:15px;letter-spacing:0"> MHz</span></div><div style="margin-top:4px;color:var(--orange);font-size:13px">80m · USB</div></div>
      <div class="hdr" style="grid-template-columns:auto 1fr;font-size:12px;margin-left:auto;gap:2px 10px"><span class="k">SSB</span><span>on</span><span class="k">power</span><span>1 W each end</span><span class="k">preset</span><span class="c-pu">hf-poor</span><span class="k">path</span><span>17 km</span></div></div>
      {led_tag('Likely this hour. 80m is open on this path and 1 W is enough. It stays open all day.')}
      <div class="sub" style="line-height:1.45;color:var(--accent)">1 hop · ~20 km each · take-off 89° · MUF 3.5 MHz</div>
      <div><div class="sub" style="font-size:10.5px;margin-bottom:4px">NEXT 24 H · 80m</div><div style="display:flex;gap:2px">{cells}</div></div>
      <div class="sub" style="color:#ffd60a;font-size:11px">⚠ 80m needs an amateur licence.</div>
    </div>
    <div style="flex:none;background:var(--shell);border-top:1px solid var(--line);padding:10px 14px 12px"><div style="display:flex;gap:8px"><div class="input" style="flex:1;min-height:36px;font-size:11.5px;color:var(--fg)"><span class="c-di" style="margin-right:6px">$</span>/radio qsy 3.580 · preset hf-poor</div><div class="btn s c-di" style="min-height:36px;border-color:var(--line)">copy</div></div><div class="btn fillo" style="width:100%;margin-top:8px">Tune radio: 3.580 USB + hf-poor</div></div>'''
    return phone(head+body,'map',strip('usb'))

def register(add):
    add('10-map', map_screen())
    add('11-plan-vhf', plan_screen())
    add('12-plan-hf-sky', sky_screen())
    try:
        import screens4; screens4.register(add)
    except ImportError as e: print('no screens4',e)
