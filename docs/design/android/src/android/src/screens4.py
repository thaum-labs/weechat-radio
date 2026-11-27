from lib import *

def kv(k,v,vc='c-pu'):
    return f'<div class="kv"><span class="k">{k}</span><span class="{vc}">{v}</span></div>'

def meterrow(k,frac,db,col='c-gr',live=True):
    segs=[(0,.66,'#39ff14'),(.66,.88,'#c4b5fd'),(.88,1,'#ff7a3d')]
    out=''
    for a,b,c in segs:
        e=min(frac,b)
        if e>a: out+=f'<i style="left:{a*100}%;width:{(e-a)*100}%;background:{c}"></i>'
    return f'<div class="kv"><span class="k">{k}</span><div class="meterbar">{out}</div><span class="{col}" style="width:34px;text-align:right">{db}</span></div>'

def ring(frac,col,live=True):
    import math
    r=6.5; c=2*math.pi*r
    arc=f'<circle cx="9" cy="9" r="{r}" fill="none" stroke="{col}" stroke-width="2" stroke-dasharray="{c*frac:.1f} {c:.1f}" transform="rotate(-90 9 9)" stroke-linecap="butt"/>' if live else ''
    return f'<svg width="18" height="18" viewBox="0 0 18 18"><circle cx="9" cy="9" r="{r}" fill="none" stroke="#212840" stroke-width="2"/>{arc}</svg>'

def schedule(k,frac,col,label,live=True):
    return f'<div class="kv"><span class="k">{k}</span>{ring(frac,"#"+col if not col.startswith("#") else col,live)}<span style="color:{col if col.startswith("#") else "#"+col}">{label}</span></div>'

def wf_block(id,seed,h=150,tx=True):
    # newest at top
    bursts=[
      {'y0':5,'y1':14,'x0':.15,'x1':.85,'lvl':1.0,'tx':False},
      {'y0':26,'y1':44,'x0':.14,'x1':.86,'lvl':.98,'tx':True} if tx else {'y0':0,'y1':0,'x0':0,'x1':0,'lvl':0,'tx':False},
      {'y0':58,'y1':76,'x0':.16,'x1':.84,'lvl':1.0,'tx':False},
      {'y0':84,'y1':90,'x0':.18,'x1':.82,'lvl':.8,'tx':False},
      {'y0':104,'y1':124,'x0':.14,'x1':.82,'lvl':1.0,'tx':False},
      {'y0':140,'y1':148,'x0':.2,'x1':.8,'lvl':.7,'tx':False},
      {'y0':166,'y1':184,'x0':.16,'x1':.84,'lvl':1.0,'tx':False},
    ]
    import json
    js=f"drawWF('{id}',{seed},0,{json.dumps({'bursts':bursts})});"
    cv=f'<canvas id="{id}" width="380" height="{h}" style="height:{h}px"></canvas>'
    ax='<div class="wfaxis"><span style="left:0;transform:none">0.3k</span><span style="left:33.3%">1.2k</span><span style="left:66.6%">2.1k</span><span style="left:100%;transform:translateX(-100%)">3.0 kHz</span></div>'
    pas='<div style="margin:2px 0 0 10%;width:80%;height:3px;background:var(--orange);opacity:.6"></div>'
    key='<div class="legend" style="margin-top:2px;font-size:10.5px"><span><i class="sw" style="background:#0b1230"></i>quiet</span><span><i class="sw" style="background:#8c3cc8"></i>noise</span><span><i class="sw" style="background:#ffe14d"></i>signal</span><span><i class="sw" style="background:#ff7a3d"></i>your TX</span></div>'
    return f'<div class="wfwrap">{cv}{pas}{ax}{key}</div>', js

def radio_usb():
    wf,js=wf_block('wf1',7,196)
    c=f'''{mod("STATION","v1.0.21 · android")}
    <div style="padding:0 14px;display:flex;flex-direction:column;gap:10px;overflow:hidden;flex:1">
    <div class="card" style="padding:11px 12px"><div style="display:flex;align-items:center;gap:8px"><span class="c-di" style="font-size:12px;width:52px">RADIO</span><span class="c-gr"><span class="dot"></span>Digirig Mini · linked</span><span style="margin-left:auto;color:var(--accent)">{ic("usb").replace('class="ic "','class="ic" style="width:20px;height:20px"')}</span></div>
      <div class="sub" style="margin:6px 0 8px;line-height:1.5">USB audio 48 kHz · CAT serial 9600 · PTT by CAT<br>Radio stays plugged in; the phone can sleep. Background listening: ON.</div>
      <div style="display:flex;gap:8px"><div class="btn s c-acc" style="flex:1">Reconnect</div><div class="btn s c-pu" style="flex:1">Test PTT</div><div class="btn s c-di" style="flex:1;border-color:var(--line)">Disconnect</div></div></div>
    <div style="display:flex;align-items:flex-end;justify-content:space-between"><div><div style="font-size:32px;line-height:1;color:var(--orange);letter-spacing:-.04em">433.625<span style="font-size:14px;letter-spacing:0"> MHz</span></div><div style="margin-top:3px;font-size:12.5px;color:var(--orange)">NFM · NARROW <span class="c-di">· 5 W</span></div></div>
      <div style="display:flex;gap:6px"><div class="btn s c-acc" style="min-height:40px">QSY ▾</div></div></div>
    <div style="display:flex;gap:6px;flex-wrap:wrap;align-items:center"><span class="chip c-pu">vhf-fast</span><span class="chip" style="color:#ff4dff">radio-plus</span><span class="chip c-gr">PTT RX</span><span class="chip c-di">OCC 18%</span></div>
    <div><div style="display:flex;justify-content:space-between;font-size:12px;margin-bottom:4px"><span class="c-acc">WATERFALL</span><span class="c-di">modem73 audio · 30 s</span></div>{wf}</div>
    <div>{meterrow("IN",.52,"-18")}{meterrow("OUT",.42,"-14")}<div class="kv"><span class="k">SNR</span><span class="c-pu">11 dB</span><span class="k" style="margin-left:20px;width:50px">TX</span><span class="c-pu">—</span><span class="k" style="margin-left:20px;width:56px">RETRY</span><span class="c-pu">0</span></div></div>
    </div>'''
    return phone(c,'radio',strip_html='',bt=False,usb=True,chat_pip=True), js

def radio_bt():
    wf,js=wf_block('wf2',21,84,tx=False)
    c=f'''{mod("STATION","v1.0.21 · android")}
    <div style="padding:0 14px;display:flex;flex-direction:column;gap:9px;overflow:hidden;flex:1">
    <div class="card" style="padding:11px 12px"><div style="display:flex;align-items:center;gap:8px"><span class="c-di" style="font-size:12px;width:52px">RADIO</span><span class="c-gr"><span class="dot"></span>VR-N76 linked</span><span style="margin-left:auto;color:var(--accent)">{ic("bt").replace('class="ic "','class="ic" style="width:20px;height:20px"')}</span></div>
      <div class="sub" style="margin:6px 0 8px;line-height:1.5">Bluetooth audio (modem73) · PTT by Bluetooth<br>38:D2:00:01:03:49 · squelch 0 · HT app closed</div>
      <div style="display:flex;gap:8px"><div class="btn s c-acc" style="flex:1">Reconnect</div><div class="btn s c-pu" style="flex:1">Test PTT</div><div class="btn s c-di" style="flex:1;border-color:var(--line)">Unpair</div></div></div>
    <div style="border:1px solid var(--amber);border-radius:6px;padding:10px 12px;background:rgba(255,196,61,.07)"><div style="display:flex;align-items:center;gap:8px;color:var(--amber);font-size:12.5px">{ic("batt").replace('class="ic "','class="ic" style="width:20px;height:20px"')}<span>Battery · Bluetooth link</span></div>
      <div class="kv" style="margin-top:4px"><span class="k" style="width:52px">PHONE</span><div class="meterbar" style="height:8px"><i style="width:78%;background:var(--green)"></i></div><span class="c-gr" style="width:40px;text-align:right">78%</span></div>
      <div class="kv"><span class="k" style="width:52px">RADIO</span><div class="meterbar" style="height:8px"><i style="width:62%;background:var(--green)"></i></div><span class="c-gr" style="width:40px;text-align:right">62%</span></div>
      <div class="sub" style="line-height:1.45;margin-top:4px">Listening over Bluetooth keeps a foreground service running. Rest between TX blocks: handhelds heat up and drain fast on repeated keying.</div></div>
    <div style="display:flex;align-items:flex-end;justify-content:space-between"><div><div style="font-size:30px;line-height:1;color:var(--orange);letter-spacing:-.04em">433.625<span style="font-size:14px;letter-spacing:0"> MHz</span></div><div style="margin-top:3px;font-size:12.5px;color:var(--orange)">NFM · NARROW <span class="c-di">· low power</span></div></div><div class="btn s c-acc" style="min-height:40px">QSY ▾</div></div>
    <div><div style="display:flex;align-items:center;gap:6px;flex-wrap:wrap"><span class="chip c-pu" style="background:rgba(196,181,253,.2)">vhf-fm</span><span class="chip c-di">hf-good</span><span class="chip c-di">hf-poor</span><span class="chip c-di">hf-weak</span><span class="chip c-di">hf-deep</span><span class="chip c-di">vhf-fast <span class="c-am" style="font-size:10px">⚠ untested</span></span></div><div class="sub" style="margin-top:5px;line-height:1.4;font-size:11px"><span class="c-am">vhf-fast is untested on Bluetooth.</span> For mail files and fast links, USB (Digirig) is the safe choice.</div></div>
    <div><div style="display:flex;justify-content:space-between;font-size:12px;margin-bottom:4px"><span class="c-acc">WATERFALL</span><span class="c-di">modem73 audio · 30 s</span></div>{wf}</div>
    <div>{meterrow("IN",.5,"-19")}{meterrow("SNR",0,"")}</div>
    </div>'''.replace(meterrow("SNR",0,""),'<div class="kv"><span class="k">SNR</span><span class="c-pu">9 dB</span><span class="k" style="margin-left:20px;width:50px">OCC</span><span class="c-pu">12%</span></div>')
    return phone(c,'radio',strip_html='',bt=True,usb=False,chat_pip=True), js

def frame(t,kind,who,snr,via,text='',col='c-di',sent=False,snrc=None):
    kc={'dm':'c-acc','bulletin':'c-pu','group':'c-pu','receipt':'c-gr','mail':'c-am'}[kind]
    s=f'<span class="{snrc}">{snr} dB</span>' if snr is not None else ''
    tx=f'<div style="font-size:12.5px;margin-top:1px;color:#d8d0e8">{text}</div>' if text else ''
    return f'''<div style="padding:7px 14px;border-bottom:1px solid var(--line)"><div style="display:flex;gap:8px;align-items:baseline;font-size:12px;flex-wrap:wrap"><span class="c-di">{t}</span><span class="{kc}">{kind}</span><span class="{'c-or' if sent else 'c-fg'}" style="font-size:13px">{who}</span>{s}<span class="c-di">{via}</span></div>{tx}</div>'''

def radio_stats():
    c=f'''{mod("STATION","")}
    <div style="padding:0 14px 4px">
      {kv("CALL","M7TJF","c-acc")}{kv("GRID","IO81UF")}{kv("GPS","51.2308, -2.3207 · fix · 9 sats","c-di")}<div class="kv"><span class="k">MODE</span><span style="color:#ff4dff">radio-plus</span></div>
      {kv("PTT","RX","c-di")}{kv("RADIO","Digirig Mini","c-gr")}
      <div class="kv"><span class="k">QUEUE</span><span class="c-pu">1</span><span class="k" style="margin-left:24px;width:40px">AIR</span><span class="c-pu">0</span><span class="k" style="margin-left:24px;width:56px">RETRY</span><span class="c-pu">0</span></div>
      {schedule("BEACN",.62,"7d9bff","off on VOX" if False else "in 4 min",True)}
      {schedule("HOLD",.18,"ff7a3d","retry in 9 s",True)}
      <div class="kv"><span class="k">HUB</span><span class="c-gr">UP</span><span class="k" style="margin-left:24px;width:68px">MAIL GW</span><div class="seg3" style="flex:1"><i style="background:var(--green)"></i><i style="background:var(--green)"></i><i style="background:var(--green)"></i></div></div>
    </div>
    <div class="mod" style="border-top:1px solid var(--line);padding-top:10px"><span class="l">ON THE AIR</span><span class="r">38 frames</span></div>
    <div class="sub" style="padding:0 14px 6px;line-height:1.4;font-size:11px">Every frame this station heard or sent, newest first. Read only: to answer a station, open a direct chat.</div>
    <div style="flex:1;overflow:hidden;border-top:1px solid var(--line)">
      {frame("23:31:08","bulletin","M7TJF → #bulletin",None,"sent","Also testing the phone mail screen tonight 📧",sent=True)}
      {frame("23:30:41","receipt","XX0AAA → M7TJF",10,"rf",snrc="c-gr")}
      {frame("23:30:12","bulletin","M7TJF → #bulletin",None,"relayed","Net control is fine. I will set up in Frome…",sent=True)}
      {frame("23:24:55","mail","M7TJF → M7TJF",8,"rf","chunk 3/3 · wcr-qa@mail.grokbot.com",snrc="c-am")}
      {frame("23:22:03","dm","XX0BBB → M7TJF",11,"rf","Heard the 3 pieces fine 👍",snrc="c-gr")}
      {frame("23:19:47","bulletin","XX0CCC → #bulletin",2,"internet","See you all Sunday",snrc="c-re")}
    </div>'''
    return phone(c,'radio',strip_html='',bt=False,usb=True,chat_pip=True)

# ---------------- onboarding ----------------
def onb(step,total,body,primary,back=True,skip=False,title_prompt=''):
    bars=''.join(f'<i class="{"on" if i<step-1 else ("cur" if i==step-1 else "")}"></i>' for i in range(total))
    top=f'''<div class="appbar" style="padding-left:14px"><div class="brand">{mark(PLUS)}<b>weechat</b><span>radio</span></div><div class="sp"></div><div class="c-di" style="font-size:11.5px;padding-right:12px">step {step} of {total}</div></div><div style="padding:10px 0 2px;background:var(--bg)"><div class="steps">{bars}</div></div>'''
    foot=f'''<div style="flex:none;padding:10px 14px 12px;background:var(--shell);border-top:1px solid var(--line);display:flex;gap:10px">{'<div class="btn c-pu" style="flex:1">Back</div>' if back else ''}<div class="btn fill" style="flex:2">{primary}</div></div>'''
    c=f'<div style="flex:1;overflow:hidden;padding:12px 14px 0;display:flex;flex-direction:column;gap:12px">{body}</div>{foot}'
    return phone(c,None,top=top,show_nav=False,bt=False,usb=True)

def opt(sel,icon,title,sub,extra=''):
    return f'''<div class="opt {'sel' if sel else ''}"><div class="radio"></div><div style="flex:1;min-width:0"><div style="display:flex;align-items:center;gap:8px"><span style="color:{'var(--accent)' if sel else 'var(--fg)'};font-size:14.5px">{title}</span><span style="margin-left:auto;color:var(--purple)">{ic(icon).replace('class="ic "','class="ic" style="width:20px;height:20px"')}</span></div><div class="sub" style="margin-top:4px;line-height:1.45">{sub}</div>{extra}</div></div>'''

def onb1():
    body=f'''<div class="prompt"><b>weechat-radio:</b><i>$</i> ./setup</div><div class="h1">How is your radio connected?</div>
    {opt(False,'bt','Bluetooth','VR-N76, UV-PRO, GA-5WB, VR-N7600. No cables: the radio does audio and PTT.')}
    {opt(True,'usb','USB cable','Digirig or a USB sound card, with CAT or VOX for PTT. Best for vhf-fast, mail and files.','<div style="margin-top:8px;display:flex;align-items:center;gap:8px;color:var(--green);font-size:12.5px"><span class="dot"></span>Digirig Mini found · audio + serial</div>')}
    {opt(False,'chat','No radio, internet only','Chat and email over the hub. You can add a radio later.')}
    <div class="card" style="padding:10px 12px"><svg viewBox="0 0 360 74" width="100%" height="74" style="font-family:inherit"><rect x="6" y="8" width="52" height="58" rx="8" fill="none" stroke="#c4b5fd" stroke-width="1.5"/><text x="32" y="42" fill="#d8d0e8" font-size="10" text-anchor="middle">phone</text>
<line x1="58" y1="37" x2="128" y2="37" stroke="#7d9bff" stroke-width="2"/><text x="93" y="30" fill="#7d9bff" font-size="9" text-anchor="middle">USB-C</text>
<rect x="128" y="22" width="76" height="30" rx="4" fill="none" stroke="#7d9bff" stroke-width="1.5"/><text x="166" y="41" fill="#d8d0e8" font-size="10" text-anchor="middle">Digirig</text>
<line x1="204" y1="37" x2="274" y2="37" stroke="#ff7a3d" stroke-width="2"/><text x="239" y="30" fill="#ff7a3d" font-size="9" text-anchor="middle">audio + PTT</text>
<rect x="274" y="10" width="40" height="54" rx="5" fill="none" stroke="#39ff14" stroke-width="1.5"/><text x="294" y="40" fill="#d8d0e8" font-size="10" text-anchor="middle">radio</text><path d="M326 20 Q334 37 326 54 M334 14 Q346 37 334 60" stroke="#39ff14" fill="none" stroke-width="1.5" opacity=".7"/></svg></div>
    <div class="sub" style="line-height:1.5">You are the control operator. Stay within your licence and your band plan.</div>'''
    return onb(1,4,body,'Next: you',back=False)

def onb1b():
    body=f'''<div class="prompt"><b>weechat-radio:</b><i>$</i> ./pair vr-n76</div><div class="h1">Find your radio</div>
    <div class="seg" style="display:flex;border:1px solid var(--line)"><div style="flex:1;text-align:center;padding:9px 0;background:var(--accent);color:#07070a;font-size:12.5px">Audio · modem73</div><div style="flex:1;text-align:center;padding:9px 0;color:var(--dim);font-size:12.5px">TNC · 1200 bd</div></div>
    <div class="card" style="padding:0"><div style="display:flex;align-items:center;gap:10px;padding:12px;border-bottom:1px solid var(--line)"><span style="color:var(--accent)">{ic('bt')}</span><div style="flex:1"><div style="color:var(--fg)">VR-N76</div><div class="sub">38:D2:00:01:03:49 · paired · −52 dBm</div></div><span class="chip c-gr">linked</span></div>
      <div style="display:flex;align-items:center;gap:10px;padding:12px"><span style="color:var(--dim)">{ic('bt')}</span><div style="flex:1"><div style="color:var(--dim)">UV-PRO</div><div class="sub">11:3A:5C:07:E2:90 · not paired</div></div><span class="chip c-acc">pair</span></div></div>
    <div class="btn c-acc" style="width:100%">Find radio again</div>
    <div class="sub" style="color:var(--purple)">On the radio, once</div>
    <div style="display:flex;flex-direction:column;gap:7px;font-size:12.5px;line-height:1.35">
      <div style="display:flex;gap:8px"><span class="c-gr">✓</span><span>Squelch 0 (short bursts vanish at 1)</span></div>
      <div style="display:flex;gap:8px"><span class="c-gr">✓</span><span>KISS TNC off · Digital Mode off</span></div>
      <div style="display:flex;gap:8px"><span class="c-gr">✓</span><span>Menu → Pairing, PIN 0000 if asked</span></div>
      <div style="display:flex;gap:8px"><span class="c-am">!</span><span>Close the HT phone app: the radio takes one Bluetooth client at a time</span></div></div>
    <div class="card" style="padding:10px 12px;font-size:11.5px;line-height:1.5"><span class="c-pu">Presets on Bluetooth audio</span><br><span class="c-fg">vhf-fm</span> <span class="c-di">(default)</span> · hf-good · hf-poor · hf-weak · hf-deep<br><span class="c-fg">vhf-fast</span> <span class="chip c-am" style="font-size:10px;padding:0 6px;margin-left:2px">untested on Bluetooth</span></div>'''
    p=onb(1,4,body,'Next: you',back=True)
    return p

def input_(label,val,sub='',extra=''):
    return f'<div><div class="c-di" style="font-size:12px;margin-bottom:4px">{label}</div><div class="input" style="min-height:48px;font-size:15px">{val}{extra}</div>{f"<div class=sub style=margin-top:4px;line-height:1.4>{sub}</div>" if sub else ""}</div>'

def modeopt(col,name,sub,sel=False):
    b=f'border:1px solid {col};background:color-mix(in srgb,{col} 12%,transparent)' if sel else 'border:1px solid var(--line)'
    return f'<div style="{b};border-radius:5px;padding:9px 10px;flex:1"><div style="color:{col};font-size:12.5px">{name}</div><div class="sub" style="font-size:10.5px;line-height:1.3;margin-top:2px">{sub}</div></div>'

def onb2():
    body=f'''<div class="prompt"><b>weechat-radio:</b><i>$</i> ./whoami</div><div class="h1">Who is on the air?</div>
    {input_("Callsign","<span class=c-or>M7TJF</span><span style=margin-left:auto class=c-gr>✓</span>","Every frame carries it. Add /M, /P or /A if you operate that way.")}
    {input_("Locator (grid)","<span class=c-pu>IO81UF</span><span style=margin-left:auto class=c-di>Frome</span>")}
    <div style="display:flex;gap:8px;align-items:center"><div class="btn s c-acc" style="min-height:40px">{ic("loc").replace('class="ic "','class="ic" style="width:16px;height:16px"')} Use GPS</div><span class="sub" style="line-height:1.35">GPS fix 51.2308, −2.3207 matches IO81UF. Used for the map; never message text.</span></div>
    <div class="card" style="padding:0;overflow:hidden"><svg viewBox="0 0 384 118" width="100%" height="118"><image href="img/terrain.png" x="-120" y="-250" width="824" height="1000" opacity=".9" preserveAspectRatio="none"/><rect x="150" y="30" width="84" height="56" fill="rgba(125,155,255,.12)" stroke="#7d9bff" stroke-dasharray="4 3"/><circle cx="192" cy="58" r="9" fill="#07070a" stroke="#ff7a3d" stroke-width="2"/><path d="M188 55 L192 58 L188 61" stroke="#d8d0e8" fill="none" stroke-width="1.6"/><text x="192" y="104" fill="#fff" font-size="10.5" text-anchor="middle" style="paint-order:stroke" stroke="#07070a" stroke-width="3.5">IO81UF · Frome</text><text x="8" y="16" fill="#9a93aa" font-size="9.5" style="paint-order:stroke" stroke="#07070a" stroke-width="3">grid square preview</text></svg></div>
    <div><div class="c-di" style="font-size:12px;margin-bottom:6px">Mode</div>
      <div style="display:flex;gap:8px;margin-bottom:8px">{modeopt("#00e5ff","internet","chat over the hub, no radio")}{modeopt("#39ff14","internet-radio","radio + hub gateway")}</div>
      <div style="display:flex;gap:8px">{modeopt("#ffbf00","radio","radio only, nothing forwarded")}{modeopt("#ff4dff","radio-plus","radio, a gateway may carry mail",True)}</div></div>'''
    return onb(2,4,body,'Next: levels')

def onb3():
    body=f'''<div class="prompt"><b>weechat-radio:</b><i>$</i> ./levels</div><div class="h1">Preset and audio levels</div>
    <div style="display:flex;gap:8px;flex-wrap:wrap"><span class="chip c-pu" style="padding:5px 12px;font-size:12.5px;background:rgba(196,181,253,.2)">vhf-fast ✓</span><span class="chip c-di" style="padding:5px 12px;font-size:12.5px">vhf-fm</span><span class="chip c-di" style="padding:5px 12px;font-size:12.5px">hf-good</span><span class="chip c-di" style="padding:5px 12px;font-size:12.5px">hf-poor</span><span class="chip c-di" style="padding:5px 12px;font-size:12.5px">afsk-1200</span></div>
    <div class="sub" style="line-height:1.45;margin-top:-4px">VHF/UHF FM, strong local links (OFDM QAM16 1/2). Cabled PTT preferred. Frequency <span class="c-or">433.625 MHz NFM</span>, NARROW, low power indoors.</div>
    <div class="card" style="padding:12px">
      <div style="display:flex;justify-content:space-between;font-size:12px;margin-bottom:6px"><span class="c-acc">LEVEL CHECK</span><span class="c-di">live from the radio</span></div>
      {meterrow("IN",.55,"-17")}
      <div class="sub" style="margin:2px 0 8px;font-size:11px">Open squelch or key a friend. Green is healthy; orange is clipping.</div>
      <div class="kv"><span class="k">OUT</span><div class="meterbar"><i style="width:44%;background:var(--green)"></i><i style="left:44%;width:2px;background:#fff"></i></div><span class="c-pu" style="width:56px;text-align:right">-6.0 dB</span></div>
      <div style="display:flex;gap:8px;margin-top:8px"><div class="btn s c-or" style="flex:1">Play test tone</div><div class="btn s c-acc" style="flex:1">Auto-set</div></div></div>
    <div class="card" style="padding:12px"><div class="c-di" style="font-size:12px;margin-bottom:8px">PTT method</div><div style="display:flex;border:1px solid var(--line)"><div style="flex:1;text-align:center;padding:9px 0;background:var(--accent);color:#07070a;font-size:12.5px">CAT</div><div style="flex:1;text-align:center;padding:9px 0;color:var(--dim);font-size:12.5px">VOX</div><div style="flex:1;text-align:center;padding:9px 0;color:var(--dim);font-size:12.5px">RTS</div></div>
    <div class="sub" style="margin-top:8px;line-height:1.4">VOX trips when IN passes <span class="c-or">-32 dB</span>. Works on a strong link; CAT is steadier.</div></div>
    <div class="sub" style="line-height:1.5;color:var(--purple)">Set the radio to NFM narrow, low power indoors. Handhelds: short rests between TX blocks.</div>'''
    return onb(3,4,body,'Next: permissions')

def perm(icon,t,sub,state,col):
    return f'<div class="perm"><div class="pi">{ic(icon)}</div><div style="flex:1;min-width:0"><div style="font-size:13.5px">{t}</div><div class="sub" style="line-height:1.35">{sub}</div></div>{('<span class="toggle on"></span>' if state=='toggle' else f'<span class="chip {col}">{state}</span>')}</div>'

def onb4():
    body=f'''<div class="prompt"><b>weechat-radio:</b><i>$</i> ./permissions</div><div class="h1">Let the phone run a station</div>
    <div style="margin-top:-4px">
    {perm('mic','Microphone','Hear the radio through the USB or Bluetooth audio, and measure levels. Never records to disk.','allowed','c-gr')}
    {perm('usb','USB device','Digirig: audio, CAT and PTT. Asked when you plug it in.','ask on plug','c-am')}
    {perm('bt','Nearby devices','Find and link the VR-N76 over Bluetooth.','not needed','c-di')}
    {perm('pin','Location','Locator and map pins, while the app is open.','while using','c-gr')}
    {perm('bell','Notifications','New mail, direct messages and TX alerts, even with the screen off.','allow','c-or')}
    {perm('batt','Listen in background','ON by default. Keeps listening with the screen off and shows a notification: weechat-radio listening on 433.625. Uses battery; turn off in Settings.','toggle','')}
    </div>'''
    return onb(4,4,body,'Start station')

# ---------------- settings ----------------
def srow(t,v='',sub='',chev=True,toggle=None,vc='c-pu'):
    right=''
    if toggle is not None: right=f'<span class="toggle {"on" if toggle else ""}"></span>'
    else:
        right=f'<span class="v {vc}">{v}</span>'+('<span class="chev">›</span>' if chev else '')
    s=f'<span class="sub">{sub}</span>' if sub else ''
    return f'<div class="setrow"><div class="grow"><span style="font-size:13.5px">{t}</span>{s}</div>{right}</div>'

def settings():
    top=f'''<div class="appbar" style="padding-left:4px"><div class="iconbtn" style="color:var(--purple)">{ic("back")}</div><div style="font-size:16px;color:var(--ink)">Settings</div><div class="sp"></div><div class="c-di" style="font-size:11px;padding-right:14px">v1.0.21</div></div>'''
    c=f'''<div style="flex:1;overflow:hidden">
    <div class="sethead" style="padding-top:10px"><span>STATION</span><em>M7TJF · IO81UF</em></div>
    {srow("Callsign","M7TJF")}{srow("Locator","IO81UF · from GPS")}{srow("Mode","radio-plus","a gateway may carry my mail",vc="")}
    <div class="sethead"><span>RADIO</span><em>Digirig · linked</em></div>
    {srow("Connection","USB · Digirig Mini")}{srow("Frequency","433.625 NFM")}{srow("Preset","vhf-fast")}{srow("TX duty","handheld","rests 6 s between blocks")}
    <div class="sethead"><span>AUDIO</span><em></em></div>
    {srow("PTT method","CAT")}{srow("IN / OUT level","-17 dB / -6.0 dB")}{srow("Show SNR on lines",toggle=True)}
    <div class="sethead"><span>EMAIL</span><em>gateway M7TJF</em></div>
    {srow("Copy-to","me@example.com","confirmed")}{srow("Mail gateway","Auto")}
    <div class="sethead"><span>PHONE</span><em></em></div>
    {srow("Listen in background",sub="persistent notification · uses battery",toggle=True)}{srow("Notifications","mail · direct · TX")}{srow("Time","UTC + local")}
    <div class="sethead"><span>ABOUT</span><em></em></div>
    {srow("Update","up to date","",True,vc="c-gr")}{srow("Identity key","back up",vc="c-or")}{srow("Re-run setup","")}
    </div><div style="position:absolute;left:0;right:0;bottom:0;height:56px;background:linear-gradient(to bottom,rgba(7,7,10,0),#07070a)"></div>'''
    return phone(c,None,top=top,show_nav=False,bt=False,usb=True)

def register(add):
    h,js=radio_usb(); add('13-radio-usb',h,js)
    h,js=radio_bt(); add('14-radio-bluetooth',h,js)
    add('15-radio-stats-on-air',radio_stats())
    add('16-onboarding-1-radio',onb1())
    add('17-onboarding-1b-bluetooth',onb1b())
    add('18-onboarding-2-you',onb2())
    add('19-onboarding-3-levels',onb3())
    add('20-onboarding-4-permissions',onb4())
    add('21-settings',settings())

    try:
        import screens5; screens5.register(add)
    except ImportError as e: print('no screens5',e)
