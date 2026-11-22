from m3 import *
from s2 import tfield, mono
import json

def wfblock(id,seed,h,tx=True,w=364,compact=False):
    bursts=[{'y0':5,'y1':14,'x0':.15,'x1':.85,'lvl':1.0,'tx':False},
     ({'y0':26,'y1':44,'x0':.14,'x1':.86,'lvl':.98,'tx':True} if tx else {'y0':0,'y1':0,'x0':0,'x1':0,'lvl':0,'tx':False}),
     {'y0':58,'y1':76,'x0':.16,'x1':.84,'lvl':1.0,'tx':False},{'y0':84,'y1':90,'x0':.18,'x1':.82,'lvl':.8,'tx':False},
     {'y0':104,'y1':124,'x0':.14,'x1':.82,'lvl':1.0,'tx':False},{'y0':140,'y1':148,'x0':.2,'x1':.8,'lvl':.7,'tx':False},{'y0':166,'y1':184,'x0':.16,'x1':.84,'lvl':1.0,'tx':False}]
    js=f"drawWF('{id}',{seed},0,{json.dumps({'bursts':bursts})});"
    leg=''.join(f'<span style="display:flex;align-items:center;gap:5px"><i class="dotc" style="background:{c};border-radius:2px"></i>{t}</span>' for c,t in (('#0b1230','quiet'),('#8c3cc8','noise'),('#ffe14d','signal'),('#ff7a3d','your TX')))
    html=f'''<div class="card" style="padding:12px 12px 10px"><div style="display:flex;justify-content:space-between;align-items:baseline;margin-bottom:8px"><span style="font-size:14px;font-weight:500">Waterfall</span><span class="osv" style="font-size:12px">modem73 audio · 30 s</span></div>
    <div style="border-radius:12px;overflow:hidden;background:#0b1230"><canvas id="{id}" width="{w}" height="{h}" style="display:block;width:{w}px;height:{h}px"></canvas><div style="height:4px;position:relative;background:#0b1230"><i style="position:absolute;left:10%;width:80%;top:0;height:3px;background:var(--tx);opacity:.7"></i></div></div>
    <div style="display:flex;justify-content:space-between;font-size:10.5px;color:var(--on-surface-variant);margin-top:3px" class="mono"><span>0.3k</span><span>1.2k</span><span>2.1k</span><span>3.0 kHz</span></div>
    {"" if compact else '<div style="display:flex;gap:14px;font-size:11px;color:var(--on-surface-variant);margin-top:6px;justify-content:center">'+leg+'</div>'}</div>'''
    return html,js

def mrow(k,frac,db):
    return f'<div style="display:flex;align-items:center;gap:12px"><span class="lbl" style="width:30px">{k}</span><div style="flex:1">{meterbar(frac,10)}</div><span class="mono" style="width:56px;text-align:right;font-size:13px;color:var(--ok)">{db} dB</span></div>'

def devcard(icon,name,sub,btns,extra=''):
    return f'''<div class="card" style="padding:14px 16px 12px"><div style="display:flex;align-items:center;gap:12px"><div style="width:40px;height:40px;border-radius:20px;background:var(--primary-container);color:var(--on-primary-container);display:flex;align-items:center;justify-content:center">{ic(icon)}</div><div style="flex:1"><div style="font-size:16px;line-height:22px;font-weight:500">{name}</div><div style="display:flex;align-items:center;gap:6px;font-size:13px" class="ok"><i class="dotc" style="background:var(--ok)"></i>linked</div></div></div>
    <div class="osv" style="font-size:13px;line-height:19px;margin:10px 0 12px">{sub}</div>{extra}<div style="display:flex;gap:8px">{btns}</div></div>'''
def btns(a,b,c):
    return f'<div class="btn b-tonal" style="flex:1;padding:0 8px">{a}</div><div class="btn b-out" style="flex:1;padding:0 8px">{b}</div><div class="btn b-text" style="padding:0 10px;color:var(--on-surface-variant)">{c}</div>'
def qsy(f,sub): 
    return f'<div style="display:flex;align-items:flex-end;justify-content:space-between"><div><div class="mono txc" style="font-size:36px;line-height:40px;font-weight:500;letter-spacing:-1px">{f}<span style="font-size:15px;letter-spacing:0;margin-left:5px;font-weight:400">MHz</span></div><div class="txc" style="font-size:13px">{sub}</div></div><div class="btn b-tonal" style="padding:0 12px 0 20px;gap:4px">QSY{ic("drop",18)}</div></div>'
def radiobar(sub):
    return topbar('Radio',sub,logo=True,actions=gear())

def radio_usb(theme='dark'):
    wf,js=wfblock('wf1',7,176)
    c=f'''<div style="flex:1;overflow:hidden;padding:4px 12px 0;display:flex;flex-direction:column;gap:12px">
    {devcard('usb','Digirig Mini','USB audio 48 kHz · CAT serial 9600 · PTT by CAT. Radio stays plugged in; the phone can sleep. <span class="ok">Background listening: on.</span>',btns('Reconnect','Test PTT','Disconnect'))}
    {qsy('433.625','NFM · NARROW · 5 W')}
    <div style="display:flex;gap:6px;flex-wrap:nowrap"><span class="chip sel mono" style="font-size:13px;padding:0 10px 0 8px">{ic("check",16)}vhf-fast</span><span class="chip" style="color:#ff4dff;border-color:#ff4dff55">radio-plus</span><span class="chip ok" style="gap:5px"><i class="dotc" style="background:var(--ok)"></i>PTT RX</span><span class="chip">OCC 18%</span></div>
    {wf}
    <div style="display:flex;flex-direction:column;gap:8px">{mrow('IN',.52,'-18')}{mrow('OUT',.42,'-14')}</div>
    <div style="display:flex;gap:8px">{''.join(f'<div class="card low" style="flex:1;padding:8px 12px"><div class="lbl">{k}</div><div class="mono" style="font-size:16px;margin-top:1px">{v}</div></div>' for k,v in (('SNR','11 dB'),('TX','—'),('RETRY','0')))}</div></div>'''
    return phone(c,'radio',theme,radiobar('Digirig Mini · linked'),chatdot=True),js

def radio_bt():
    wf,js=wfblock('wf2',21,48,tx=False,compact=True)
    pbar=lambda k,v: f'<div style="display:flex;align-items:center;gap:12px"><span class="lbl" style="width:52px;color:var(--on-warn-c)">{k}</span><div style="flex:1;height:8px;border-radius:4px;background:rgba(255,255,255,.14);position:relative;overflow:hidden"><i style="position:absolute;left:0;top:0;bottom:0;width:{v}%;background:var(--ok);border-radius:4px"></i></div><span class="mono" style="font-size:13px;width:40px;text-align:right">{v}%</span></div>'
    batt=f'''<div style="border-radius:16px;background:var(--warn-c);color:var(--on-warn-c);padding:12px 16px"><div style="display:flex;gap:10px;align-items:center;font-size:14px;font-weight:500">{ic("batt",20)}Battery · Bluetooth link</div><div style="display:flex;flex-direction:column;gap:7px;margin-top:10px">{pbar('PHONE',78)}{pbar('RADIO',62)}</div><div style="font-size:12.5px;line-height:17px;margin-top:9px;opacity:.9">Background listening is on (foreground service). Rest between TX blocks: handhelds drain fast when keyed repeatedly.</div></div>'''
    ladder=f'''<div><div style="display:flex;gap:6px;flex-wrap:wrap"><span class="chip sel mono" style="font-size:13px;padding:0 10px 0 8px">{ic("check",16)}vhf-fm</span><span class="chip mono" style="font-size:13px">hf-good</span><span class="chip mono" style="font-size:13px">hf-poor</span><span class="chip mono" style="font-size:13px">hf-weak</span><span class="chip mono" style="font-size:13px">hf-deep</span><span class="chip mono" style="font-size:13px;border-color:var(--warn)">vhf-fast<span class="wn" style="font-family:Roboto;font-size:11px;display:flex;align-items:center;gap:2px">{ic("warn",13)}untested</span></span></div><div class="osv" style="font-size:12px;line-height:17px;margin-top:6px"><span class="wn">vhf-fast is untested on Bluetooth.</span> For mail files and fast links, USB (Digirig) is the safe choice.</div></div>'''
    c=f'''<div style="flex:1;overflow:hidden;padding:4px 12px 0;display:flex;flex-direction:column;gap:10px">
    {devcard('bt','VR-N76','Bluetooth audio (modem73) · PTT by Bluetooth · <span class="mono" style="font-size:12px">38:D2:00:01:03:49</span> · squelch 0 · HT app closed',btns('Reconnect','Test PTT','Unpair'))}
    {batt}
    {qsy('433.625','NFM · NARROW · low power')}
    {ladder}
    {wf}
    <div style="display:flex;flex-direction:column;gap:6px">{mrow('IN',.5,'-19')}</div></div>'''
    return phone(c,'radio','dark',radiobar('VR-N76 · linked'),sb=statusbar(bt=True,usb=False),chatdot=True),js

def kvrow(k,v,vc='',mono_=False):
    return f'<div class="li" style="min-height:40px;padding:4px 16px"><div class="mid" style="color:var(--on-surface-variant);font-size:14px">{k}</div><div class="{"mono" if mono_ else ""} {vc}" style="font-size:{13.5 if mono_ else 14}px;text-align:right">{v}</div></div>'
def frame(t,kind,who,snr,via,text='',sent=False):
    icn={'bulletin':('tag','var(--pu)'),'receipt':('done_all','var(--ok)'),'mail':('mail','var(--warn)'),'dm':('person','var(--primary)')}[kind]
    s=f' · <span class="mono" style="font-size:12px">{snr} dB</span>' if snr is not None else ''
    tx=f'<div class="sp one" style="font-size:13px;color:var(--on-surface)">{text}</div>' if text else ''
    return f'<div class="li" style="min-height:56px;padding:6px 16px;gap:14px"><div class="lead pl">{ic(icn[0],22,icn[1])}</div><div class="mid"><div style="font-size:14px;line-height:20px"><span class="mono" style="font-size:13px;color:{"var(--tx)" if sent else "var(--cy)"}">{who}</span><span class="osv" style="font-size:12px"> · {kind}{s} · {via}</span></div>{tx}</div><span class="ts mono">{t}</span></div>'

def radio_stats():
    c=f'''<div style="flex:1;overflow:hidden">
    <div class="sec" style="padding-top:4px">Station</div>
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:0 8px;padding:0 4px">{kvrow('Call','M7TJF','pri',True)}{kvrow('Grid','IO81UF','',True)}{kvrow('Mode','radio-plus','',True)}{kvrow('PTT','RX','',True)}{kvrow('Queue','1','',True)}{kvrow('Air','0','',True)}{kvrow('Retry','0','',True)}{kvrow('Beacon','in 4 min','',False)}</div>
    {kvrow('GPS','51.2308, -2.3207 · fix · 9 sats','',True)}{kvrow('Radio','Digirig Mini','ok')}{kvrow('Hold','retry in 9 s')}
    <div style="display:flex;align-items:center;gap:12px;padding:2px 16px 6px"><span style="color:var(--on-surface-variant);font-size:14px;flex:1">Hub</span><span class="chip ok sm">UP</span></div>
    <div style="display:flex;align-items:center;gap:12px;padding:2px 16px 8px"><span style="color:var(--on-surface-variant);font-size:14px;width:70px">Mail GW</span><div style="display:flex;gap:4px;flex:1"><i style="flex:1;height:6px;border-radius:3px;background:var(--ok)"></i><i style="flex:1;height:6px;border-radius:3px;background:var(--ok)"></i><i style="flex:1;height:6px;border-radius:3px;background:var(--ok)"></i></div></div>
    <div class="div"></div>
    <div style="display:flex;align-items:baseline;padding:14px 16px 2px"><div style="font-size:16px;font-weight:500;flex:1">On the air</div><span class="osv" style="font-size:12px">38 frames</span></div>
    <div class="osv" style="padding:0 16px 6px;font-size:12px;line-height:16px">Every frame this station heard or sent, newest first. Read only: to answer a station, open a direct chat.</div>
    {frame('23:31:08','bulletin','M7TJF → #bulletin',None,'sent','Also testing the phone mail screen tonight 📧',True)}
    {frame('23:30:41','receipt','XX0AAA → M7TJF',10,'rf')}
    {frame('23:24:55','mail','M7TJF → M7TJF',8,'rf','chunk 3/3 · wcr-qa@mail.grokbot.com')}
    {frame('23:22:03','dm','XX0BBB → M7TJF',11,'rf','Heard the 3 pieces fine 👍')}
    {frame('23:19:47','bulletin','XX0CCC → #bulletin',2,'internet','See you all Sunday')}
    </div>'''
    return phone(c,'radio','dark',topbar('Station','Digirig Mini · linked',logo=True,actions=gear()),chatdot=True)

# ---------------- onboarding ----------------
def onb(step,body,primary,back=True,title='Set up'):
    top=f'''<div class="tbar"><div class="ib">{ic("back" if step>1 else "close")}</div><div class="tt" style="padding-left:4px"><h1 style="font-size:20px;line-height:26px">{title}</h1><div class="sub">Step {step} of 4</div></div><div style="padding-right:16px">{mark('#ff4dff',24,'var(--on-surface)')}</div></div><div style="display:flex;gap:4px;padding:0 16px 4px;flex:none">{''.join(f'<i style="flex:1;height:4px;border-radius:2px;background:{"var(--primary)" if i<step else "var(--secondary-container)"}"></i>' for i in range(4))}</div>'''
    foot=f'''<div style="flex:none;background:var(--surface-container);padding:12px 16px 14px;display:flex;gap:12px;border-radius:16px 16px 0 0">{'<div class="btn b-text" style="flex:1;font-size:15px">Back</div>' if back else ''}<div class="btn b-fill" style="flex:2;font-size:15px">{primary}</div></div>'''
    c=f'<div style="flex:1;overflow:hidden;padding:12px 16px 0;display:flex;flex-direction:column;gap:12px">{body}</div>{foot}'
    return phone(c,None,'dark',top,sb=statusbar(usb=True),navbar=False)
def h1(t,sub=''): return f'<div style="font-size:28px;line-height:34px;font-weight:400;margin-top:6px">{t}</div>'+(f'<div class="osv" style="font-size:14px;margin-top:-6px">{sub}</div>' if sub else '')
def opt(sel,icon,title,sub,extra=''):
    b='2px solid var(--primary)' if sel else '1px solid var(--outline-variant)'
    bg='background:color-mix(in srgb,var(--primary) 12%,transparent);' if sel else ''
    return f'''<div style="border:{b};{bg}border-radius:16px;padding:{14 if sel else 15}px 16px;display:flex;gap:14px;align-items:flex-start"><div class="rad {'on' if sel else ''}" style="margin-top:2px"></div><div style="flex:1;min-width:0"><div style="display:flex;align-items:center;gap:10px"><span style="font-size:16px;font-weight:500">{title}</span><span style="margin-left:auto;color:var(--primary)">{ic(icon)}</span></div><div class="osv" style="font-size:13.5px;line-height:19px;margin-top:3px">{sub}</div>{extra}</div></div>'''
def onb1():
    found=f'<div style="margin-top:8px;display:flex;align-items:center;gap:6px;font-size:13px" class="ok"><i class="dotc" style="background:var(--ok)"></i>Digirig Mini found · audio + serial</div>'
    diagram=''.join(f'<div style="display:flex;align-items:center;gap:6px"><div style="width:44px;height:44px;border-radius:22px;background:var(--secondary-container);color:var(--on-secondary-container);display:flex;align-items:center;justify-content:center">{ic(i)}</div>{"<span class=osv>—</span>" if k<3 else ""}</div>' for k,i in enumerate(('phone','usb','mic','ant')))
    body=h1('How is your radio connected?')+opt(False,'bt','Bluetooth','VR-N76, UV-PRO, GA-5WB, VR-N7600. No cables: the radio does audio and PTT.')+opt(True,'usb','USB cable','Digirig or a USB sound card, with CAT or VOX for PTT. Best for vhf-fast, mail and files.',found)+opt(False,'cloud','No radio, internet only','Chat and email over the hub. You can add a radio later.')+f'<div style="display:flex;justify-content:center;gap:2px;margin-top:4px;align-items:center">{diagram}</div><div class="osv" style="font-size:12px;line-height:17px;text-align:center">You are the control operator. Stay within your licence and your band plan.</div>'
    return onb(1,body,'Next: you',back=False)
def devrow(icn,name,sub,right):
    return f'<div class="li" style="padding:8px 4px"><div class="lead">{ic(icn,22)}</div><div class="mid"><div class="hl">{name}</div><div class="sp mono" style="font-size:12px">{sub}</div></div>{right}</div>'
def chk(ok,t):
    return f'<div style="display:flex;gap:12px;align-items:flex-start;font-size:14px;line-height:19px;padding:4px 0">{ic("check",20,"var(--ok)") if ok else ic("warn",20,"var(--warn)")}<span>{t}</span></div>'
def onb1b():
    body=h1('Find your radio')+f'<div class="seg"><div class="on">{ic("check")}Audio · modem73</div><div>TNC · 1200 bd</div></div>'+devrow('bt','VR-N76','38:D2:00:01:03:49 · −52 dBm','<span class="chip ok sm">linked</span>')+devrow('bt','UV-PRO','11:3A:5C:07:E2:90 · not paired','<div class="btn b-tonal" style="height:34px;padding:0 16px">Pair</div>')+f'<div class="btn b-out ic" style="width:100%;margin-top:-2px">{ic("refresh")}Find radio again</div><div><div class="lbl" style="margin:4px 0 4px">ON THE RADIO, ONCE</div>{chk(1,"Squelch 0 (short bursts vanish at 1)")}{chk(1,"KISS TNC off · Digital Mode off")}{chk(1,"Menu → Pairing, PIN 0000 if asked")}{chk(0,"Close the HT phone app: the radio takes one Bluetooth client at a time")}</div>'+f'''<div class="card low" style="padding:12px 14px"><div class="lbl" style="margin-bottom:8px">PRESETS ON BLUETOOTH AUDIO</div><div style="display:flex;gap:6px;flex-wrap:wrap"><span class="chip sel mono" style="height:28px;font-size:12.5px">vhf-fm · default</span><span class="chip mono" style="height:28px;font-size:12.5px">hf-good</span><span class="chip mono" style="height:28px;font-size:12.5px">hf-poor</span><span class="chip mono" style="height:28px;font-size:12.5px">hf-weak</span><span class="chip mono" style="height:28px;font-size:12.5px">hf-deep</span><span class="chip mono" style="height:28px;font-size:12.5px;border-color:var(--warn)">vhf-fast <span class="wn" style="font-family:Roboto;font-size:11px">untested on Bluetooth</span></span></div></div>'''
    return onb(1,body,'Next: you')
def onb2():
    pv=f'''<div class="card low" style="padding:0;overflow:hidden;display:flex;align-items:center"><div style="width:120px;height:72px;background:url(img/terrain.png) 40px -150px/412px 500px;position:relative"><i style="position:absolute;left:46px;top:26px;width:20px;height:20px;border-radius:10px;background:var(--tx);box-shadow:0 0 0 4px rgba(255,138,80,.3)"></i></div><div style="padding:0 14px"><div class="mono" style="font-size:14px">IO81UF</div><div class="osv" style="font-size:12.5px">Frome · grid square preview</div></div></div>'''
    modes=''.join(f'<div class="li" style="min-height:52px;padding:6px 4px;gap:14px"><div class="rad {"on" if s else ""}"></div><div class="mid"><div class="hl" style="font-size:15px;color:{col}">{m}</div><div class="sp" style="font-size:12.5px;line-height:17px">{d}</div></div></div>' for m,d,col,s in (('internet','chat over the hub, no radio','#00e5ff',0),('internet-radio','radio + hub gateway','#39ff14',0),('radio','radio only, nothing forwarded','#ffbf00',0),('radio-plus','radio, a gateway may carry mail','#ff4dff',1)))
    body=h1('Who is on the air?')+f'<div style="display:flex;flex-direction:column;gap:20px;margin-top:6px">{tfield("Callsign","M7TJF",mono_=True,tl=ic("check",24,"var(--ok)"))}</div><div class="osv" style="font-size:12px;margin:-8px 0 0 16px">Every frame carries it. Add /M, /P or /A if you operate that way.</div>'+f'<div style="display:flex;gap:10px;align-items:center;margin-top:6px"><div style="flex:1">{tfield("Locator (grid)","IO81UF",mono_=True)}</div><div class="btn b-tonal ic" style="padding:0 18px 0 12px">{ic("loc")}Use GPS</div></div><div class="osv" style="font-size:12px;line-height:16px;margin:-6px 0 0 16px">GPS fix 51.2308, −2.3207 matches IO81UF. Used for the map; never message text.</div>{pv}<div class="lbl" style="margin-top:2px">MODE</div><div style="margin-top:-8px">{modes}</div>'
    return onb(2,body,'Next: levels')
def slider(frac,label,center=False):
    return f'<div class="slider" style="margin-top:22px"><div class="vl" style="left:{frac*100}%">{label}</div><div class="tr1"><i style="width:{frac*100}%"></i></div><div class="th" style="left:calc({frac*100}% - 2px)"></div></div>'
def onb3():
    body=h1('Preset and audio levels')+f'''<div style="display:flex;gap:6px;flex-wrap:wrap"><span class="chip sel mono" style="font-size:13px;padding:0 10px 0 8px">{ic("check",16)}vhf-fast</span><span class="chip mono" style="font-size:13px">vhf-fm</span><span class="chip mono" style="font-size:13px">hf-good</span><span class="chip mono" style="font-size:13px">hf-poor</span><span class="chip mono" style="font-size:13px">afsk-1200</span></div>
    <div class="osv" style="font-size:13px;line-height:19px;margin-top:-2px">VHF/UHF FM, strong local links (OFDM QAM16 1/2). Cabled PTT preferred. Frequency <span class="mono txc" style="font-size:12.5px">433.625 MHz</span> NFM, NARROW, low power indoors.</div>
    <div class="card" style="padding:14px 16px"><div style="display:flex;align-items:baseline"><div style="font-size:16px;font-weight:500;flex:1">Level check</div><span class="osv" style="font-size:12px">live from the radio</span></div>
    <div style="margin-top:12px">{mrow('IN',.55,'-17')}</div><div class="osv" style="font-size:12px;margin:4px 0 0 42px">Open squelch or key a friend. Green is healthy; orange is clipping.</div>
    <div style="display:flex;align-items:center;gap:12px;margin-top:14px"><span class="lbl" style="width:30px">OUT</span><div style="flex:1">{slider(.44,'-6.0 dB')}</div></div>
    <div style="display:flex;gap:8px;margin-top:10px"><div class="btn b-tonal ic" style="flex:1;padding:0 12px">{ic("play")}Test tone</div><div class="btn b-out" style="flex:1;padding:0 12px">Auto-set</div></div></div>
    <div class="card" style="padding:14px 16px"><div style="font-size:14px;font-weight:500;margin-bottom:10px">PTT method</div><div class="seg"><div class="on">{ic("check")}CAT</div><div>VOX</div><div>RTS</div></div><div class="osv" style="font-size:12.5px;line-height:18px;margin-top:10px">VOX trips when IN passes <span class="mono txc" style="font-size:12px">-32 dB</span>. Works on a strong link; CAT is steadier.</div></div>
    <div class="osv" style="font-size:12.5px;line-height:18px">Set the radio to NFM narrow, low power indoors. Handhelds: short rests between TX blocks.</div>'''
    return onb(3,body,'Next: permissions')
def perm(icn,t,sub,right):
    return f'<div class="li" style="align-items:flex-start;padding:10px 4px;gap:14px"><div class="lead pl" style="margin-top:2px;color:var(--primary)">{ic(icn)}</div><div class="mid"><div class="hl" style="font-size:15px">{t}</div><div class="sp" style="font-size:13px;line-height:18px">{sub}</div></div><div style="padding-top:2px">{right}</div></div>'
def onb4():
    body=h1('Let the phone run a station')+f'''<div style="margin-top:-6px">{perm('mic','Microphone','Hear the radio through the USB or Bluetooth audio, and measure levels. Never records to disk.','<span class="chip ok sm">allowed</span>')}
    {perm('usb','USB device','Digirig: audio, CAT and PTT. Asked when you plug it in.','<span class="chip warn sm">ask on plug</span>')}
    {perm('bt','Nearby devices','Find and link the VR-N76 over Bluetooth.','<span class="chip sm">not needed</span>')}
    {perm('pin','Location','Locator and map pins, while the app is open.','<span class="chip ok sm">while using</span>')}
    {perm('bell','Notifications','New mail, direct messages and TX alerts, even with the screen off.','<span class="chip tx sm">allow</span>')}
    {perm('batt','Listen in background','On by default. Keeps listening with the screen off and shows a notification: weechat-radio listening on 433.625. Uses battery; turn off in Settings.','<div class="sw on"><i>'+ic("check")+'</i></div>')}</div>'''
    return onb(4,body,'Start station')

# ---------------- settings ----------------
def sli(t,v='',sub='',sw=None,chev=True,vc=''):
    r=''
    if sw is not None: r=f'<div class="sw {"on" if sw else ""}"><i>{ic("check")}</i></div>'
    else:
        r=(f'<span class="{vc}" style="font-size:14px;color:var(--on-surface-variant)">{v}</span>' if v else '')+(ic("chev",24,'var(--on-surface-variant)') if chev else '')
    s=f'<div class="sp one" style="font-size:13px">{sub}</div>' if sub else ''
    return f'<div class="li" style="min-height:{64 if sub else 52}px;padding:6px 16px"><div class="mid"><div class="hl" style="font-size:16px">{t}</div>{s}</div><div style="display:flex;align-items:center;gap:4px">{r}</div></div>'
def settings():
    top=f'<div class="tbar"><div class="ib">{ic("back")}</div><div class="tt" style="padding-left:4px"><h1>Settings</h1></div><span class="ts" style="padding-right:16px">v1.0.21</span></div>'
    c=f'''<div style="flex:1;overflow:hidden">
    <div class="sec" style="padding-top:4px">Station</div>{sli('Callsign','<span class="mono" style="font-size:13px">M7TJF</span>')}{sli('Locator','<span class="mono" style="font-size:13px">IO81UF</span> · from GPS')}{sli('Mode','radio-plus','a gateway may carry my mail')}
    <div class="div"></div><div class="sec">Radio</div>{sli('Connection','USB · Digirig Mini')}{sli('Frequency','<span class="mono" style="font-size:13px">433.625</span> NFM')}{sli('Preset','<span class="mono" style="font-size:13px">vhf-fast</span>')}{sli('TX duty','handheld','rests 6 s between blocks')}
    <div class="div"></div><div class="sec">Audio</div>{sli('PTT method','CAT')}{sli('IN / OUT level','-17 dB / -6.0 dB')}{sli('Show SNR on lines',sw=True)}
    <div class="div"></div><div class="sec">Email</div>{sli('Copy-to','me@example.com','confirmed')}{sli('Mail gateway','Auto','M7TJF · Pi, Frome')}
    <div class="div"></div><div class="sec">Phone</div>{sli('Listen in background',sw=True,sub='persistent notification · uses battery')}{sli('Notifications','','mail · direct · TX')}{sli('Time','UTC + local')}
    <div class="div"></div><div class="sec">About</div>{sli('Update','up to date',vc='ok',chev=False)}{sli('Identity key','back up')}{sli('Re-run setup','')}</div>'''
    return phone(c,None,'dark',top,sb=statusbar(usb=True),navbar=False)

def shade():
    def n(app,when,title,body,acts='',big=False):
        return f'''<div style="background:var(--surface-container-high);border-radius:24px;padding:14px 16px 12px;margin:0 12px 6px"><div style="display:flex;align-items:center;gap:8px;font-size:12px;color:var(--on-surface-variant)"><span style="width:18px;height:18px;border-radius:9px;background:var(--primary-container);display:flex;align-items:center;justify-content:center">{mark('#ff4dff',12,'var(--on-primary-container)')}</span><span>{app}</span><span>·</span><span>{when}</span><span style="margin-left:auto">{ic("drop",18)}</span></div><div style="font-size:15px;line-height:21px;font-weight:500;margin-top:8px">{title}</div><div style="font-size:14px;line-height:20px;color:var(--on-surface-variant);margin-top:2px">{body}</div>{acts}</div>'''
    act='<div style="display:flex;gap:4px;margin:8px 0 0 -12px">'+''.join(f'<div class="btn b-text" style="height:36px;{extra}">{t}</div>' for t,extra in (('Open',''),('Mute',''),('Stop','color:var(--tx)')))+'</div>'
    tiles=''.join(f'<div style="flex:1;height:64px;border-radius:32px;background:{"var(--primary)" if on else "var(--surface-container-high)"};color:{"var(--on-primary)" if on else "var(--on-surface)"};display:flex;align-items:center;gap:10px;padding:0 16px;font-size:14px;font-weight:500">{ic(i,24)}{t}</div>' for i,t,on in (('wifi','Wi-Fi',1),('bt','Bluetooth',0)))
    c=f'''<div style="flex:1;overflow:hidden;background:var(--surface)"><div style="padding:6px 18px 12px;display:flex;justify-content:space-between;align-items:baseline"><span style="font-size:28px;line-height:34px">23:31</span><span class="osv" style="font-size:14px">Sat, 21 Nov</span></div>
    <div style="display:flex;gap:8px;padding:0 12px 14px">{tiles}</div>
    <div style="padding:0 20px 8px;display:flex;justify-content:space-between;font-size:13px" class="osv"><span>Notifications</span><span class="pri" style="font-weight:500">Clear all</span></div>
    {n('weechat radio','now','weechat-radio listening on 433.625','M7TJF · Digirig · vhf-fast · RX. Keeps listening with the screen off and uses battery.',act)}
    {n('weechat radio','23:12','New mail · wcr-qa@mail.grokbot.com','Re: phone attachment test · 📎 1 file')}
    {n('weechat radio','22:52','XX0BBB','Net notes for Sunday: we meet on 433.625 NFM at 10:00, low power only…')}
    <div class="osv" style="margin:12px 24px 0;font-size:12px;line-height:17px">The first notification is the foreground service: Android requires it to keep the radio audio alive with the screen off. Background listening is on by default; turn it off in Settings.</div></div>'''
    return phone(c,None,'dark','',sb=statusbar(usb=True),navbar=False)

def register(add):
    h,js=radio_usb(); add('13-radio-usb',h,js)
    h,js=radio_usb('light'); add('13-radio-usb-light',h,js)
    h,js=radio_bt(); add('14-radio-bluetooth',h,js)
    add('15-radio-stats-on-air',radio_stats())
    add('16-onboarding-1-radio',onb1()); add('17-onboarding-1b-bluetooth',onb1b())
    add('18-onboarding-2-you',onb2()); add('19-onboarding-3-levels',onb3()); add('20-onboarding-4-permissions',onb4())
    add('21-settings',settings()); add('22-notification-listening',shade())
