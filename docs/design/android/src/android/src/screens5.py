from lib import *
from screens2 import *

def mail_compose_bt():
    bar=f'''<div style="display:flex;align-items:center;gap:8px;padding:10px 14px">{mailbtn("&lt; Back","c-pu")}<span class="c-wh" style="font-size:14px">New mail</span><span style="flex:1"></span></div>'''
    form='''<div class="field pad" style="padding-top:0">
    <label style="margin-top:2px">To</label><div class="input">wcr-qa@mail.grokbot.com</div>
    <label>Subject</label><div class="input">Phone attachment test</div>
    <label>Message</label><div class="input" style="align-items:flex-start;padding:10px 12px;min-height:72px;line-height:1.5">Sending qa-32k.bin from the phone. Please confirm it arrives intact. 73 de M7TJF</div></div>'''
    att=f'''<div class="pad" style="margin-top:10px"><div class="attach"><span style="font-size:13px;flex:1">📎 qa-32k.bin (32.0 KB)</span><div class="btn s c-pu" style="min-height:36px">remove</div></div>
    <div style="border:1px solid var(--amber);background:rgba(255,196,61,.07);border-radius:6px;padding:10px 12px;margin-top:10px;font-size:12px;line-height:1.5">
      <div style="color:var(--amber);display:flex;align-items:center;gap:8px;font-size:12.5px">{ic("usb").replace('class="ic "','class="ic" style="width:18px;height:18px"')}Files travel best over USB</div>
      <div class="sub" style="margin-top:3px;line-height:1.45">Mail files go on the radio at vhf-fast. That preset is <span class="c-am">untested on Bluetooth</span>: you can try it, but a Digirig over USB is the reliable way to send files.</div>
      <div style="display:flex;gap:8px;margin-top:8px"><div class="btn s c-pu" style="flex:1;min-height:38px">Try vhf-fast on Bluetooth</div><div class="btn s c-gr" style="flex:1;min-height:38px">Set up USB</div></div></div></div>'''
    foot=f'''<div style="margin-top:auto;border-top:1px solid var(--line);background:var(--shell);padding:10px 14px 12px"><div class="sub" style="font-size:11.5px;line-height:1.4;margin-bottom:8px">33201 B · ~1 min 45 s on vhf-fast (46 frames in 3 bursts, incl. 12 s rest) · via gateway M7TJF</div><div style="display:flex;gap:10px"><div class="btn c-pu" style="flex:1">Cancel</div><div class="btn fillo" style="flex:2">Send</div></div></div>'''
    return bar+form+att+foot

def shade():
    def n(icon_html,app,when,title,body,actions=''):
        return f'''<div style="background:var(--shell);border:1px solid var(--line);border-radius:10px;padding:11px 12px;margin:0 12px 10px"><div style="display:flex;align-items:center;gap:8px;font-size:11px;color:var(--dim)">{icon_html}<span>{app}</span><span>·</span><span>{when}</span></div><div style="font-size:13.5px;margin-top:6px;color:var(--fg)">{title}</div><div style="font-size:12px;color:#b7aec8;margin-top:2px;line-height:1.4">{body}</div>{actions}</div>'''
    act=lambda *a:'<div style="display:flex;gap:8px;margin-top:9px">'+''.join(f'<div class="btn s {c}" style="min-height:34px;padding:0 14px;font-size:12px">{t}</div>' for t,c in a)+'</div>'
    m=mark(PLUS,16)
    c=f'''<div style="flex:1;background:var(--bg);padding-top:10px">
    <div style="display:flex;justify-content:space-between;padding:6px 18px 14px;color:var(--dim);font-size:12px"><span>Saturday 21 Nov</span><span>Silent · Notifications</span></div>
    {n(m,"weechat radio","now","weechat-radio listening on 433.625","M7TJF · Digirig · vhf-fast · RX · IN ok. Keeps listening with the screen off and uses battery.",act(("Open","c-acc"),("Mute","c-pu"),("Stop","c-or")))}
    {n(m,"weechat radio","23:12","New mail · wcr-qa@mail.grokbot.com","Re: phone attachment test · 📎 1 file")}
    {n(m,"weechat radio","22:52","XX0BBB","Net notes for Sunday: we meet on 433.625 NFM at 10:00, low power only…")}
    <div style="margin:4px 12px;color:var(--dim);font-size:11px;line-height:1.45;padding:0 4px">The first notification is the foreground service: Android requires it to keep the radio audio alive with the screen off. Background listening is on by default; turn it off in Settings.</div></div>'''
    return phone(c,None,show_nav=False,top='',usb=True)

def register(add):
    P=lambda c,n='mail',**k: phone(c,n,strip('bt'),top=appbar(PLUS,gear='mail'),bt=True,**k)
    add('08b-mail-compose-bluetooth', P(modbar('')+mail_compose_bt()))
    add('22-notification-listening', shade())
