from lib import *
from screens1 import compose_bar

CALL='M7TJF@mail.weechatradio.com'

def chip(label,col): return f'<span class="chip {col}">{label}</span>'

def mrow(who,subj,when,unread=False,files=False,chipl=None,word='',whoc=''):
    dot='●' if unread else ''
    cl='unread' if unread else ''
    f=' <span class="c-di">📎</span>' if files else ''
    l3=''
    if chipl or word:
        l3=f'<div class="l3">{chipl or ""}<span>{word}</span></div>'
    return f'''<div class="mrow {cl}"><div class="dotc">{dot}</div><div class="mid"><div class="l1"><span class="who {whoc}">{who}</span><span class="when">{when}</span></div><div class="l2">{subj}{f}</div>{l3}</div></div>'''

def list_bar(active='Inbox',unread=2,waiting=1):
    tabs=''
    for t in ['Inbox','Outbox','Sent']:
        on='on' if t==active else ''
        n=f'<span class="n">{unread}</span>' if (t=='Inbox' and unread) else ''
        tabs+=f'<div class="tab {on}">{t}{n}</div>'
    return f'''<div class="tabs">{tabs}<div class="btn s c-or" style="min-height:38px;margin-left:auto">Check mail</div></div>'''

def modbar(right=CALL):
    return f'<div class="mod" style="padding-bottom:2px"><span class="l">EMAIL</span><span class="r" style="font-size:11px">{right}</span></div>'

def fab(): return '<div class="mfab btn c-gr" style="background:#0a1a08;min-height:50px;padding:0 20px;font-size:14px;border-radius:6px">+ New</div>'

def mail_list(sheet=False):
    rows=''
    rows+=f'''<div class="mrow unread" style="background:rgba(196,181,253,.06)"><div class="dotc">●</div><div class="mid"><div class="l1"><span class="who">ann@example.com</span><span class="when"></span></div><div class="l2">Photos from the Bath walk</div><div class="l3">{chip("Waiting · Get","c-pu")}<span>18.2 KB · at gateway M7TJF</span></div></div></div>'''
    rows+=mrow('wcr-qa@mail.grokbot.com','Re: phone attachment test','23:12',True,True)
    rows+=mrow('XX0BBB@mail.weechatradio.com','Net notes for Sunday','22:40',True,True)
    rows+=mrow('XX0AAA@mail.weechatradio.com','Re: aerial height at Frome','Fri')
    rows+=mrow('ann@example.com','Hello from the hill','Thu',False,True)
    rows+=mrow('newsletter@rsgb.example','Your callsign renewal reminder','14 Nov')
    toast='<div class="pad" style="padding-top:2px;padding-bottom:6px;font-size:12px;color:var(--green)">1 mail waiting at the gateway. Tap a line to Get it.</div>'
    foot='<div class="pad c-di" style="padding-top:12px;font-size:11px">Tap a mail to read it. <span class="c-or">●</span> not read yet · 📎 has a file.</div>'
    c=modbar()+list_bar()+toast+f'<div style="flex:1;overflow:hidden;border-top:1px solid var(--line)">{rows}{foot}</div>'+fab()
    return c

def mailbtn(label,col,extra=''): return f'<div class="btn s {col}" style="min-height:40px;{extra}">{label}</div>'

def mail_open():
    bar=f'''<div style="display:flex;align-items:center;gap:8px;padding:10px 14px">{mailbtn("&lt; Inbox","c-pu")}<span class="c-di" style="font-size:12px">2 of 5</span><span style="flex:1"></span>{mailbtn("Reply","c-or")}{mailbtn("Delete","c-re")}</div>'''
    hdr='''<div style="padding:4px 14px 10px"><div class="hdr"><span class="k">From</span><span style="overflow-wrap:anywhere">XX0BBB@mail.weechatradio.com</span><span class="k">To</span><span style="overflow-wrap:anywhere">M7TJF@mail.weechatradio.com</span><span class="k">Subject</span><span class="c-wh">Net notes for Sunday</span><span class="k">Received</span><span>Today 22:40</span></div></div>'''
    body='''<div style="border-top:1px solid var(--line);padding:12px 14px"><div class="body">Hi Tobias,

Notes for the Sunday net:

1. 433.625 NFM, vhf-fast, 10:00
2. Low power, handhelds rest 6 s
   between blocks
3. You run net control from Frome,
   I relay from Bath if the ridge
   gets in the way ⛰️

Plan attached. Good luck with the
phone app! 73
XX0BBB</div>
<div class="attach" style="margin-top:14px"><span class="c-gr" style="font-size:13px;flex:1">📎 net-plan.txt · 1.2 KB</span><div class="btn s c-gr" style="min-height:38px">Open</div></div></div>'''
    return bar+hdr+body

def mail_sent_open():
    bar=f'''<div style="display:flex;align-items:center;gap:8px;padding:10px 14px">{mailbtn("&lt; Sent","c-pu")}<span class="c-di" style="font-size:12px">1 of 3</span><span style="flex:1"></span>{mailbtn("Delete","c-re")}</div>'''
    hdr=f'''<div style="padding:4px 14px 8px"><div class="hdr"><span class="k">From</span><span style="overflow-wrap:anywhere">M7TJF@mail.weechatradio.com</span><span class="k">To</span><span style="overflow-wrap:anywhere">wcr-qa@mail.grokbot.com</span><span class="k">Subject</span><span class="c-wh">Phone attachment test</span><span class="k">Written</span><span>Today 23:04</span><span class="k">Status</span><span>{chip("Delivered","c-gr")} <span class="c-di" style="font-size:12px;margin-left:4px">via M7TJF</span></span></div>
    <div class="c-di" style="margin-top:10px;font-size:12px;line-height:1.55">23:04 written → 23:05 on air → 23:07 at gateway → 23:07 sent → 23:08 delivered</div></div>'''
    body='''<div style="border-top:1px solid var(--line);padding:12px 14px"><div class="body">Sending qa-32k.bin from the phone
over vhf-fast. Please confirm it
arrives intact. 73 de M7TJF</div>
<div class="attach" style="margin-top:14px"><span class="c-gr" style="font-size:13px;flex:1">📎 qa-32k.bin · 32.0 KB</span></div></div>'''
    return bar+hdr+body

def mail_compose():
    bar=f'''<div style="display:flex;align-items:center;gap:8px;padding:10px 14px">{mailbtn("&lt; Back","c-pu")}<span class="c-wh" style="font-size:14px">New mail</span><span style="flex:1"></span></div>'''
    form='''<div class="field pad" style="padding-top:0">
    <label style="margin-top:2px">To</label><div class="input">wcr-qa@mail.grokbot.com</div>
    <label>Subject</label><div class="input">Phone attachment test</div>
    <label>Message</label><div class="input" style="align-items:flex-start;padding:12px;min-height:116px;line-height:1.5">Sending qa-32k.bin from the phone over vhf-fast. Please confirm it arrives intact. 73 de M7TJF</div></div>'''
    att=f'''<div class="pad" style="margin-top:12px"><div class="attach"><span style="font-size:13px;flex:1">📎 qa-32k.bin (32.0 KB)</span><div class="btn s c-pu" style="min-height:36px">remove</div></div>
    <div style="display:flex;align-items:center;gap:10px;margin-top:10px"><div class="btn s c-gr" style="min-height:40px">{ic("clip").replace('class="ic "','class="ic" style="width:16px;height:16px"')} Attach</div><span class="sub" style="font-size:11px;line-height:1.35">Files up to 512 KB in all on vhf-fast; pictures are shrunk to fit.</span></div></div>'''
    foot=f'''<div style="margin-top:auto;border-top:1px solid var(--line);background:var(--shell);padding:10px 14px 12px"><div class="sub" style="font-size:11.5px;line-height:1.4;margin-bottom:8px">33201 B · ~1 min 45 s on vhf-fast (46 frames in 3 bursts, incl. 12 s rest) · via gateway M7TJF</div><div style="display:flex;gap:10px"><div class="btn c-pu" style="flex:1">Cancel</div><div class="btn fillo" style="flex:2">Send</div></div></div>'''
    return bar+form+att+foot

def mail_settings_sheet():
    return f'''<div class="scrim"></div><div class="sheet" style="padding-bottom:10px"><div class="grab"></div>
    <h3><span style="color:var(--purple);display:flex;align-items:center;gap:8px">{ic("gear").replace('class="ic "','class="ic" style="width:18px;height:18px"')}Email settings</span><span class="c-di">x</span></h3>
    <div style="padding:2px 16px 4px;font-size:12px;color:var(--dim)">Copy-to: a copy of every mail you send and get</div>
    <div style="display:flex;gap:8px;padding:6px 16px"><div class="input" style="flex:1;color:var(--fg)">me@example.com</div><div class="btn s c-pu" style="min-height:46px">save</div></div>
    <div style="display:flex;gap:8px;padding:6px 16px"><div class="input" style="flex:1;color:var(--fg);letter-spacing:.3em">482913</div><div class="btn s c-or" style="min-height:46px">confirm</div></div>
    <div class="sub" style="padding:2px 16px 4px;font-size:11px">copy-to: waiting for the code sent to me@example.com</div>
    <div style="padding:14px 16px 4px;font-size:12px;color:var(--dim)">Mail gateway</div>
    <div style="padding:0 16px;color:var(--fg);font-size:14px">M7TJF · Pi, Frome <span class="c-di" style="font-size:12px">· heard 10 dB</span></div>
    <div class="kv" style="padding:8px 16px 2px"><span class="k" style="width:64px;color:var(--dim);font-size:12px">MAIL GW</span><div class="seg3"><i style="background:var(--green)"></i><i style="background:var(--green)"></i><i style="background:var(--green)"></i></div><span style="width:30px"></span></div>
    <div class="sub" style="padding:4px 16px 2px;font-size:11px">Change it in Settings → Email.</div><div class="row" style="margin-top:10px;min-height:48px"><span class="grow c-acc" style="font-size:13px">All app settings</span><span class="chev">›</span></div></div>'''

def register(add):
    P=lambda c,n='mail',**k: phone(c,n,strip('usb'),top=appbar(PLUS,gear='mail'),**k)
    add('05-mail-list', P(mail_list()))
    add('06-mail-open', P(modbar()+mail_open()))
    add('07-mail-open-sent', P(modbar()+mail_sent_open()))
    add('08-mail-compose', P(modbar('')+mail_compose()))
    base=P(mail_list())
    add('09-mail-settings', base.replace('</div><div class="nav">', mail_settings_sheet()+'</div><div class="nav">',1))
