import os, sys, importlib.util
from pathlib import Path
from m3 import *
from s1 import composer
_DESKTOP = Path(os.environ.get('WCR_DESKTOP_SRC', Path(__file__).resolve().parents[2] / 'android' / 'src'))
sys.path.insert(0, str(_DESKTOP))
_spec=importlib.util.spec_from_file_location('d3', str(_DESKTOP / 'screens3.py'))
import lib as _dl  # desktop lib (not m3)
d3=importlib.util.module_from_spec(_spec); _spec.loader.exec_module(d3)

DOT={'g':'var(--ok)','y':'var(--warn)','n':'var(--outline)'}
def bchip(label,col,sel=False):
    return f'<span class="chip {"sel" if sel else ""}" style="padding:0 8px 0 7px;gap:5px;font-size:13px"><i class="dotc" style="background:{DOT[col]};width:10px;height:10px;border-radius:5px"></i>{label}</span>'
def bands(items):
    return '<div style="display:flex;gap:4px;flex-wrap:nowrap">'+''.join(bchip(*i) for i in items)+'</div>'
def legend():
    return f'<div style="display:flex;gap:14px;font-size:12px;color:var(--on-surface-variant);align-items:center;white-space:nowrap">'+''.join(f'<span style="display:flex;align-items:center;gap:5px"><i class="dotc" style="background:{c}"></i>{t}</span>' for c,t in (('var(--ok)','works'),('var(--warn)','skips, may or may not work'),('var(--outline)',"won't work")))+'</div>'
def verdict(t,col='warn'):
    ico='warn'
    return f'<div style="border-radius:16px;background:var(--{col}-c);color:var(--on-{col}-c);padding:12px 16px;display:flex;gap:12px;align-items:flex-start;font-size:14px;line-height:20px">{ic(ico,22)}<div><b style="font-weight:500">{t[0]}</b> {t[1]}</div></div>'
VERD=('Marginal.','A ridge 6 km from Frome clips the line. Raise the antennas or add power for a solid link.')
def freq(f,sub,size=40):
    return f'<div><div class="mono txc" style="font-size:{size}px;line-height:{size+4}px;font-weight:500;letter-spacing:-1px">{f}<span style="font-size:16px;letter-spacing:0;margin-left:6px;font-weight:400">MHz</span></div><div class="txc" style="font-size:14px;margin-top:2px">{sub}</div></div>'

def map_screen():
    m=d3.mapsvg(h=420,legend=False,zoomctl=False)
    search=f'''<div style="position:absolute;left:12px;right:12px;top:8px;height:56px;border-radius:28px;background:var(--surface-container-high);display:flex;align-items:center;padding:0 8px 0 16px;gap:12px;box-shadow:0 2px 8px rgba(0,0,0,.4);z-index:6;color:var(--on-surface-variant)">{ic("search")}<span style="flex:1;font-size:15px">Callsign, city or grid</span><span class="chip" style="height:32px;border-radius:8px">on air</span>{ic("gear")}</div>'''
    leg=f'<div style="position:absolute;left:12px;top:74px;display:flex;gap:6px;z-index:6"><span class="chip sm" style="background:var(--surface-container-high);border:0;gap:5px"><i style="width:12px;height:4px;border-radius:2px;background:#7d9bff;display:block"></i>clear</span><span class="chip sm" style="background:var(--surface-container-high);border:0;gap:5px"><i style="width:12px;height:4px;border-radius:2px;background:#ffd60a;display:block"></i>marginal</span><span class="chip sm" style="background:var(--surface-container-high);border:0;gap:5px"><i style="width:12px;height:4px;border-radius:2px;background:#ff3b6b;display:block"></i>blocked</span></div>'
    fabs=f'<div style="position:absolute;right:12px;top:78px;display:flex;flex-direction:column;gap:8px;z-index:6"><div class="fab sm" style="position:static;box-shadow:0 2px 6px rgba(0,0,0,.4)">{ic("add")}</div><div class="fab sm" style="position:static;box-shadow:0 2px 6px rgba(0,0,0,.4)">{ic("minus")}</div><div class="fab sm" style="position:static;margin-top:6px;background:var(--primary-container);color:var(--on-primary-container);box-shadow:0 2px 6px rgba(0,0,0,.4)">{ic("loc")}</div></div>'
    sheet=f'''<div style="position:absolute;left:0;right:0;top:424px;bottom:0;background:var(--surface-container-low);border-radius:28px 28px 0 0;z-index:7;padding:0 16px;display:flex;flex-direction:column;gap:10px;box-shadow:0 -2px 10px rgba(0,0,0,.4)"><div style="width:32px;height:4px;border-radius:2px;background:var(--on-surface-variant);opacity:.4;margin:10px auto 0"></div>
    {verdict(VERD)}
    <div style="display:flex;justify-content:space-between;align-items:flex-end">{freq('433.625','70cm · NFM · vhf-fast',38)}<div class="osv" style="text-align:right;font-size:13px;line-height:19px">17 km<br>5 W each end</div></div>
    {bands([('70cm','y',True),('2m','g'),('80m','y'),('40m','y'),('20m','n'),('10m','n')])}
    {legend()}
    <div style="display:flex;gap:10px;margin-top:2px"><div class="btn b-out ic" style="flex:1;padding:0 16px 0 14px">{ic("up")}Plan details</div><div class="btn b-fill ic" style="flex:1.2;background:var(--tx);color:#2a0d00">{ic("tune")}Tune radio</div></div></div>'''
    c=f'<div style="position:relative;flex:1;overflow:hidden">{rig_strip()}{search}{leg}{fabs}<div style="position:absolute;left:0;top:0;width:412px;overflow:hidden;height:440px;background:#131b27"><div style="position:absolute;left:0;top:46px;width:412px">{m}</div></div>{sheet}</div>'
    return phone(c,'map',chatdot=True,mail=1)
def rig_strip(): return ''

def planbar(sub):
    return f'<div class="tbar"><div class="ib">{ic("back")}</div><div class="tt" style="padding-left:4px"><h1 style="font-size:20px;line-height:26px">Frome → Bath</h1><div class="sub"><span class="mono" style="font-size:11px">IO81UF → IO81TJ</span> · {sub}</div></div><div class="ib">{ic("swap")}</div></div>'

def stcard(lab,col,ant,h,p,extra=''):
    return f'<div class="card o" style="flex:1;padding:12px"><div class="mono" style="font-size:12px;font-weight:500;color:{col}">{lab}</div><div style="font-size:14px;line-height:19px;margin-top:3px">{ant}</div><div class="osv" style="font-size:12px;margin-top:2px">{h} · {p}</div>{extra}</div>'

def terr(svg):
    return svg.replace('fill="#0a0a10"','fill="#00000000"').replace('width="384" height="112"','width="100%" height="112" preserveAspectRatio="none"',1).replace('width="384" height="130"','width="100%" height="130" preserveAspectRatio="none"',1)

def bottom_cmd(cmd,label):
    return f'''<div style="flex:none;background:var(--surface-container);border-radius:16px 16px 0 0;padding:12px 16px 14px"><div style="display:flex;gap:8px;align-items:center;margin-bottom:10px"><div style="flex:1;height:44px;border-radius:12px;background:var(--surface-container-highest);display:flex;align-items:center;padding:0 14px;gap:8px"><span class="osv mono" style="font-size:13px">$</span><span class="mono" style="font-size:13px">{cmd}</span></div><div class="ib" style="background:var(--secondary-container);color:var(--on-secondary-container)">{ic("copy")}</div></div><div class="btn b-tx ic" style="width:100%;height:48px;border-radius:24px;font-size:15px">{ic("tune")}{label}</div></div>'''

def plan_screen():
    kv=f'<div style="display:grid;grid-template-columns:auto auto;gap:2px 12px;font-size:13px;margin-left:auto;align-content:end"><span class="osv">power</span><span>5 W each end</span><span class="osv">preset</span><span class="mono" style="font-size:12.5px;color:var(--pu)">vhf-fast</span><span class="osv">sure</span><span>4 km each end</span></div>'
    body=f'''<div style="padding:4px 16px 8px;display:flex;flex-direction:column;gap:12px;flex:1;overflow:hidden">
    <div style="display:flex;gap:10px">{stcard('HOME','var(--tx)','Handheld rubber duck','1.5 m','5 W')}{stcard('RX · XX0BBB','var(--primary)','Mobile whip','1.8 m car roof','5 W','<span class="chip sm pri" style="margin-top:6px">Mobile antenna</span>')}</div>
    <div class="card low" style="padding:8px 0 0;overflow:hidden">{terr(d3.profile_svg())}</div>
    {bands([('70cm','y',True),('2m','g'),('80m','y'),('40m','y'),('20m','n'),('10m','n')])}
    {legend()}
    <div style="display:flex">{freq('433.625','70cm · NFM',36)}{kv}</div>
    {verdict(VERD)}
    <div style="font-size:13px;line-height:19px;color:var(--on-surface-variant)">Narrow FM on 433.625, low power indoors. Handhelds: rest between TX blocks. <span class="wn">Check your band plan before you transmit.</span></div></div>
    {bottom_cmd('/radio qsy 433.625','Tune radio: 433.625 NFM + vhf-fast')}'''
    return phone(f'<div style="flex:1;display:flex;flex-direction:column;overflow:hidden">{body}</div>','map','dark',planbar('17 km · XX0BBB'))

def sky_screen():
    cells=''.join(f'<i style="flex:1;height:18px;border-radius:3px;background:{"var(--ok)" if k==19 else "var(--warn)"};{"outline:2px solid var(--on-surface);outline-offset:1px;" if k==0 else ""}"></i>' for k in range(24))
    kv='<div style="display:grid;grid-template-columns:auto auto;gap:2px 12px;font-size:13px;margin-left:auto;align-content:end"><span class="osv">SSB</span><span>on</span><span class="osv">power</span><span>1 W each end</span><span class="osv">preset</span><span class="mono" style="font-size:12.5px;color:var(--pu)">hf-poor</span><span class="osv">path</span><span>17 km</span></div>'
    body=f'''<div style="padding:4px 16px 8px;display:flex;flex-direction:column;gap:12px;flex:1;overflow:hidden">
    <div class="card low" style="padding:0;overflow:hidden">{terr(d3.sky_svg())}</div>
    {bands([('80m','y',True),('60m','y'),('40m','y'),('30m','y'),('20m','n'),('10m','n')])}
    {legend()}
    <div style="display:flex">{freq('3.580','80m · USB',36)}{kv}</div>
    {verdict(('Likely this hour.','80m is open on this path and 1 W is enough. It stays open all day.'))}
    <div style="font-size:13px;color:var(--primary)">1 hop · ~20 km each · take-off 89° · MUF 3.5 MHz</div>
    <div><div class="lbl" style="margin-bottom:6px">NEXT 24 H · 80M</div><div style="display:flex;gap:2px">{cells}</div></div>
    <div style="display:flex;gap:8px;align-items:center;font-size:13px" class="wn">{ic("warn",18)}80m needs an amateur licence.</div></div>
    {bottom_cmd('/radio qsy 3.580 · preset hf-poor','Tune radio: 3.580 USB + hf-poor')}'''
    return phone(f'<div style="flex:1;display:flex;flex-direction:column;overflow:hidden">{body}</div>','map','dark',planbar('HF sky path · 23:00 UTC'))

def register(add):
    add('10-map',map_screen()); add('11-plan-vhf',plan_screen()); add('12-plan-hf-sky',sky_screen())
