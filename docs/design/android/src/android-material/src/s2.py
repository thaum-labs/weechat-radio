from m3 import *
from s1 import composer

CALLM='M7TJF@mail.weechatradio.com'
def mailbar(title,sub=CALLM,lead=None,acts=None):
    acts=acts if acts is not None else f'<div class="ib">{ic("sync")}</div>{gear(True)}'
    return f'<div class="tbar">{("<div class=ib>"+ic(lead)+"</div>") if lead else "<div style=width:12px></div>"}<div class="tt" style="padding-left:{4 if lead else 8}px"><h1>{title}</h1><div class="sub mono" style="font-size:11px">{sub}</div></div>{acts}</div>'

def tabs(active='Inbox',unread=2):
    o=''
    for t in ('Inbox','Outbox','Sent'):
        on=t==active
        b=f'<span class="bdg" style="position:static;display:inline-block;margin-left:6px;vertical-align:1px">{unread}</span>' if (t=='Inbox' and unread) else ''
        o+=f'<div style="flex:1;height:48px;display:flex;align-items:center;justify-content:center;font-size:14px;font-weight:500;color:{"var(--primary)" if on else "var(--on-surface-variant)"};position:relative">{t}{b}{"<i style=position:absolute;left:20%;right:20%;bottom:0;height:3px;border-radius:3px 3px 0 0;background:var(--primary)></i>" if on else ""}</div>'
    return f'<div style="display:flex;flex:none;border-bottom:1px solid var(--outline-variant)">{o}</div>'

def avatar(init,col):
    return f'<div class="lead" style="background:{col[0]};color:{col[1]}">{init}</div>'
AV={'a':('var(--primary-container)','var(--on-primary-container)'),'b':('var(--ok-c)','var(--on-ok-c)'),'c':('var(--tx-c)','var(--on-tx-c)'),'d':('var(--secondary-container)','var(--on-secondary-container)')}
def mrow(init,col,who,subj,when,unread=False,files=False,l3=''):
    w='font-weight:500;' if unread else ''
    dot='<i class="dotc" style="background:var(--tx)"></i>' if unread else ''
    att=ic('clip',16,'var(--on-surface-variant)') if files else ''
    return f'''<div class="li" style="align-items:flex-start;padding:10px 16px;min-height:72px">{avatar(init,AV[col])}<div class="mid"><div class="hl" style="{w}font-size:15px;font-family:{'inherit'}">{who}</div><div class="sp one" style="color:{'var(--on-surface)' if unread else 'var(--on-surface-variant)'};{w}">{subj}</div>{l3}</div><div class="tr"><span style="{'color:var(--on-surface);font-weight:500' if unread else ''}">{when}</span><div style="display:flex;gap:6px;align-items:center">{att}{dot}</div></div></div>'''

def mail_list(theme='dark'):
    waiting=f'''<div style="margin:12px 12px 4px;border-radius:16px;background:var(--primary-container);color:var(--on-primary-container);padding:10px 8px 10px 16px;display:flex;align-items:center;gap:12px">{ic("dl")}<div style="flex:1;font-size:14px;line-height:19px"><b style="font-weight:500">1 mail waiting at the gateway</b><br><span style="opacity:.85">ann@example.com · 18.2 KB · via M7TJF</span></div><div class="btn b-fill" style="height:36px;padding:0 18px">Get</div></div>'''
    l3=f'<div style="margin-top:4px"><span class="chip sm pri">{ic("dl",14)}Waiting · Get</span></div>'
    rows=mrow('A','a','ann@example.com','Photos from the Bath walk','',True,False,l3)
    rows+=mrow('W','b','wcr-qa@mail.grokbot.com','Re: phone attachment test','23:12',True,True)
    rows+=mrow('X','c','XX0BBB@mail.weechatradio.com','Net notes for Sunday','22:40',True,True)
    rows+=mrow('X','d','XX0AAA@mail.weechatradio.com','Re: aerial height at Frome','Fri')
    rows+=mrow('A','a','ann@example.com','Hello from the hill','Thu',False,True)
    rows+=mrow('N','d','newsletter@rsgb.example','Your callsign renewal reminder','14 Nov')
    fab=f'<div class="fab" style="bottom:16px">{ic("edit")}Compose</div>'
    c=tabs()+waiting+f'<div style="flex:1;overflow:hidden">{rows}</div>'+fab
    return phone(c,'mail',theme,mailbar('Email'))

def hdrrow(k,v,mono=False):
    return f'<div style="display:flex;gap:12px;padding:3px 0;font-size:14px"><span class="osv" style="width:64px;flex:none">{k}</span><span style="flex:1;min-width:0;overflow-wrap:anywhere;color:var(--on-surface)" class="{"mono" if mono else ""}">{v}</span></div>'.replace('class="mono"','class="mono" style="font-size:13px"') if mono else f'<div style="display:flex;gap:12px;padding:3px 0;font-size:14px"><span class="osv" style="width:64px;flex:none">{k}</span><span style="flex:1;min-width:0;color:var(--on-surface)">{v}</span></div>'
def mono(t): return f'<span class="mono" style="font-size:12.5px">{t}</span>'

def attach_card(name,size,open_=True):
    b='<div class="btn b-tonal" style="height:36px;padding:0 18px">Open</div>' if open_ else ''
    return f'''<div style="margin:16px 16px 0;border-radius:12px;background:var(--surface-container-high);padding:12px 12px 12px 16px;display:flex;align-items:center;gap:12px"><div style="width:40px;height:40px;border-radius:10px;background:var(--ok-c);color:var(--on-ok-c);display:flex;align-items:center;justify-content:center">{ic("file")}</div><div style="flex:1;min-width:0"><div style="font-size:15px;line-height:20px">{name}</div><div class="osv" style="font-size:13px">{size}</div></div>{b}</div>'''

def mail_open():
    top=f'<div class="tbar"><div class="ib">{ic("back")}</div><div class="tt" style="padding-left:4px"><h1 style="font-size:18px;line-height:24px">Inbox</h1><div class="sub">2 of 5</div></div><div class="ib">{ic("reply")}</div><div class="ib">{ic("del")}</div></div>'
    body=f'''<div style="padding:8px 16px 0"><div style="font-size:24px;line-height:32px;color:var(--on-surface)">Net notes for Sunday</div>
    <div style="display:flex;align-items:center;gap:12px;margin:14px 0 10px"><div class="lead" style="width:40px;height:40px;border-radius:20px;background:{AV['c'][0]};color:{AV['c'][1]};display:flex;align-items:center;justify-content:center;font-weight:500">M</div><div style="flex:1;min-width:0"><div class="mono" style="font-size:13px">XX0BBB@mail.weechatradio.com</div><div class="osv" style="font-size:13px">to {mono('M7TJF@mail.weechatradio.com')}</div></div><div class="osv" style="font-size:12px;text-align:right">Today<br>22:40</div></div></div>
    <div class="div"></div>
    <div style="padding:16px 16px 0;font-size:15px;line-height:23px;white-space:pre-wrap">Hi Tobias,

Notes for the Sunday net:

1. <span class="mono" style="font-size:13.5px">433.625</span> NFM, vhf-fast, 10:00
2. Low power, handhelds rest 6 s between blocks
3. You run net control from Frome, I relay from Bath if the ridge gets in the way ⛰️

Plan attached. Good luck with the phone app! 73
XX0BBB</div>{attach_card('net-plan.txt','1.2 KB')}'''
    return phone(f'<div style="flex:1;overflow:hidden">{body}</div>','mail','dark',top)

def stepper(items):
    o=''
    for i,(t,s,last) in enumerate(items):
        o+=f'''<div style="display:flex;gap:14px"><div style="display:flex;flex-direction:column;align-items:center;width:16px"><i class="dotc" style="width:12px;height:12px;border-radius:6px;margin-top:5px;background:{'var(--ok)' if last else 'var(--primary)'}"></i>{'' if last else '<i style="flex:1;width:2px;background:var(--outline-variant);margin:3px 0"></i>'}</div><div style="padding-bottom:{0 if last else 16}px;display:flex;gap:10px;align-items:baseline"><span class="mono" style="font-size:12.5px;color:var(--on-surface-variant);width:42px">{t}</span><span style="font-size:14px;{'font-weight:500' if last else ''}">{s}</span></div></div>'''
    return o

def mail_sent_open():
    top=f'<div class="tbar"><div class="ib">{ic("back")}</div><div class="tt" style="padding-left:4px"><h1 style="font-size:18px;line-height:24px">Sent</h1><div class="sub">1 of 3</div></div><div class="ib">{ic("del")}</div></div>'
    st=stepper([('23:04','Written',0),('23:05','On air',0),('23:07','At gateway M7TJF',0),('23:07','Sent on',0),('23:08','Delivered',1)])
    body=f'''<div style="padding:8px 16px 0"><div style="font-size:24px;line-height:32px">Phone attachment test</div>
    <div style="display:flex;align-items:center;gap:8px;margin:12px 0 4px"><span class="chip ok">{ic("done_all",18)}Delivered</span><span class="osv" style="font-size:13px">via gateway M7TJF</span></div>
    <div style="margin-top:8px">{hdrrow('From',mono('M7TJF@mail.weechatradio.com'))}{hdrrow('To',mono('wcr-qa@mail.grokbot.com'))}{hdrrow('Written','Today 23:04')}</div></div>
    <div style="margin:12px 16px 0" class="card low"><div class="lbl" style="margin-bottom:10px">JOURNEY · LOCAL TIME</div>{st}</div>
    <div style="padding:16px 16px 0;font-size:15px;line-height:23px">Sending qa-32k.bin from the phone over <span class="mono" style="font-size:13.5px">vhf-fast</span>. Please confirm it arrives intact. 73 de M7TJF</div>{attach_card('qa-32k.bin','32.0 KB',False)}'''
    return phone(f'<div style="flex:1;overflow:hidden">{body}</div>','mail','dark',top)

def tfield(label,val,h=56,mono_=False,tl=''):
    return f'<div class="tf" style="height:{h}px;{"align-items:flex-start;padding-top:14px;line-height:22px;" if h>60 else ""}"><span class="fl">{label}</span><span class="{"mono" if mono_ else ""}" style="{"font-size:15px" if mono_ else ""}">{val}</span>{tl}</div>'

def mail_compose(bt=False):
    top=f'<div class="tbar"><div class="ib">{ic("close")}</div><div class="tt" style="padding-left:4px"><h1 style="font-size:20px">New mail</h1></div></div>'
    msg='Sending qa-32k.bin from the phone. Please confirm it arrives intact. 73 de M7TJF' if bt else 'Sending qa-32k.bin from the phone over vhf-fast. Please confirm it arrives intact. 73 de M7TJF'
    chipf=f'<span class="chip sel" style="padding:0 8px 0 12px;height:36px;border:1px solid var(--outline-variant);background:transparent;color:var(--on-surface)">{ic("file",18,"var(--primary)")}qa-32k.bin · 32.0 KB{ic("close",18)}</span>'
    attrow=f'<div style="display:flex;align-items:center;gap:12px;margin-top:14px">{chipf}</div>'
    if not bt:
        helper=f'<div style="display:flex;align-items:center;gap:12px;margin-top:12px"><div class="btn b-out ic" style="flex:none">{ic("clip")}Attach file</div><span class="osv" style="font-size:12px;line-height:16px">Files up to 512 KB in all on vhf-fast; pictures are shrunk to fit.</span></div>'
        info=''
        air='33201 B · ~1 min 45 s on <span class="mono" style="font-size:12px">vhf-fast</span> (46 frames in 3 bursts, incl. 12 s rest) · via gateway M7TJF'
    else:
        helper=''
        info=f'''<div style="margin-top:14px;border-radius:16px;background:var(--warn-c);color:var(--on-warn-c);padding:14px 16px"><div style="display:flex;align-items:center;gap:10px;font-size:15px;font-weight:500">{ic("usb")}Files travel best over USB</div><div style="font-size:13.5px;line-height:19px;margin-top:6px;opacity:.92">Mail files go on the radio at <span class="mono" style="font-size:12.5px">vhf-fast</span>. That preset is untested on Bluetooth: you can try it, but a Digirig over USB is the reliable way to send files.</div><div style="display:flex;gap:8px;margin-top:12px"><div class="btn b-out" style="flex:1;padding:0 8px;height:38px;border-color:var(--on-warn-c);color:var(--on-warn-c);font-size:13px">Try on Bluetooth</div><div class="btn b-fill" style="flex:1;height:38px;padding:0 8px;background:var(--on-warn-c);color:var(--warn-c);font-size:13px">{ic("usb",16)}Set up USB</div></div></div>'''
        air='33201 B · ~1 min 45 s on <span class="mono" style="font-size:12px">vhf-fast</span> (46 frames in 3 bursts, incl. 12 s rest) · via gateway M7TJF'
    body=f'''<div style="padding:12px 16px 0;display:flex;flex-direction:column;gap:18px">{tfield('To','wcr-qa@mail.grokbot.com',mono_=True)}{tfield('Subject','Phone attachment test')}{tfield('Message',msg,h=104)}</div><div style="padding:0 16px">{attrow}{helper}{info}</div>'''
    foot=f'''<div style="margin-top:auto;background:var(--surface-container);padding:12px 16px 14px;border-radius:16px 16px 0 0"><div class="osv" style="font-size:12px;line-height:17px;margin-bottom:10px">{air}</div><div style="display:flex;gap:12px"><div class="btn b-out" style="flex:1">Cancel</div><div class="btn b-fill ic" style="flex:1.4">{ic("send")}Send</div></div></div>'''
    return phone(f'<div style="flex:1;display:flex;flex-direction:column;overflow:hidden"><div style="flex:1;overflow:hidden">{body}</div>{foot}</div>','mail','dark',top,sb=statusbar(bt=bt,usb=not bt))

def mail_settings():
    gwbar='<div style="display:flex;gap:4px;flex:1"><i style="flex:1;height:6px;border-radius:3px;background:var(--ok)"></i><i style="flex:1;height:6px;border-radius:3px;background:var(--ok)"></i><i style="flex:1;height:6px;border-radius:3px;background:var(--ok)"></i></div>'
    sheet=f'''<div class="scrim"></div><div class="sheet" style="padding-bottom:30px"><div class="h"></div>
    <div style="display:flex;align-items:center;padding:0 24px 8px;gap:12px">{ic("gear",24,"var(--primary)")}<div style="font-size:22px;line-height:28px;flex:1">Email settings</div></div>
    <div style="padding:6px 24px 0"><div class="lbl" style="margin-bottom:12px">COPY-TO · a copy of every mail you send and get</div>
    <div style="display:flex;gap:8px;align-items:center"><div class="tf sheetbg" style="flex:1"><span class="fl">Copy-to address</span>me@example.com</div><div class="btn b-tonal">Save</div></div>
    <div style="display:flex;gap:8px;align-items:center;margin-top:18px"><div class="tf sheetbg" style="flex:1"><span class="fl">Confirm code</span><span class="mono" style="letter-spacing:4px">482913</span></div><div class="btn b-fill">Confirm</div></div>
    <div class="osv" style="font-size:12px;margin:6px 0 0 16px">Waiting for the code sent to me@example.com</div></div>
    <div class="sec" style="padding-top:18px">Mail gateway</div>
    <div class="li" style="min-height:56px"><div class="lead">{ic("hub",22)}</div><div class="mid"><div class="hl"><span class="mono" style="font-size:15px">M7TJF</span> · Pi, Frome</div><div class="sp">heard 10 dB</div></div></div>
    <div style="display:flex;align-items:center;gap:12px;padding:2px 24px 4px"><span class="lbl" style="width:68px">MAIL GW</span>{gwbar}</div>
    <div class="div" style="margin:12px 0 0"></div>
    <div class="li" style="min-height:56px"><div class="mid"><div class="hl" style="color:var(--primary);font-size:15px">All app settings</div><div class="sp" style="font-size:12px">Change the gateway in Settings → Email</div></div>{ic("chev",24,"var(--on-surface-variant)")}</div></div>'''
    base=mail_list()
    return base.replace('<div class="gest">',sheet+'<div class="gest">',1)

def register(add):
    add('05-mail-list',mail_list()); add('05-mail-list-light',mail_list('light'))
    add('06-mail-open',mail_open()); add('07-mail-open-sent',mail_sent_open())
    add('08-mail-compose',mail_compose()); add('08b-mail-compose-bluetooth',mail_compose(True))
    add('09-mail-settings',mail_settings())
