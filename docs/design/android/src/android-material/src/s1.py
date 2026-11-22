from m3 import *

REC={'ok':('done_all','var(--ok)'),'tx':('done','var(--on-surface-variant)'),'rl':('repeat','var(--on-surface-variant)'),'q':('sched','var(--on-surface-variant)'),'no':('error','var(--error)'),'all':('done_all','var(--ok)')}
def via_ic(v):
    if v=='rf': return ic('ant',14,'var(--tx)')
    if v=='net': return ic('cloud',14,'var(--primary)')
    return ''
def bub(t,nick,text,me=False,via='',snr='',rec=None,x='',grp='',extra=''):
    meta=f'<span class="ts">{t}</span>'
    if via: meta+=via_ic(via)
    if snr: meta+=f'<span class="ts mono" style="font-size:10.5px">{snr}</span>'
    if grp: meta+=grp
    if x: meta+=f'<span class="ts mono" style="font-size:10.5px">{x}</span>'
    for r in (rec or []):
        n,c=REC[r]; meta+=ic(n,15,c)
    if me:
        bg='var(--primary-container)';fg='var(--on-primary-container)';al='flex-end';rad='18px 18px 4px 18px';head=''
    else:
        bg='var(--surface-container-high)';fg='var(--on-surface)';al='flex-start';rad='18px 18px 18px 4px'
        head=f'<div class="mono" style="font-size:12px;font-weight:500;color:var(--cy);margin-bottom:2px">{nick}</div>'
    return f'''<div style="display:flex;justify-content:{al};padding:0 12px;margin-top:6px"><div style="max-width:84%;background:{bg};color:{fg};border-radius:{rad};padding:8px 12px 6px">{head}<div style="font-size:14.5px;line-height:20px;text-wrap:pretty">{text}</div>{extra}<div style="display:flex;align-items:center;gap:6px;justify-content:flex-end;margin-top:3px">{meta}</div></div></div>'''
def datechip(t): return f'<div style="display:flex;justify-content:center;margin:10px 0 2px"><span class="chip sm" style="border:0;background:var(--surface-container-high)">{t}</span></div>'
def sysline(t): return f'<div style="text-align:center;margin-top:8px;font-size:12px;color:var(--on-surface-variant)">{t}</div>'
def g3(txt='3 of 3',col='var(--ok)'): return f'<span class="chip sm" style="border:0;background:var(--surface-container-highest);color:{col}">{ic("group",12)}{txt}</span>'

def composer(text=None,cursor=False,hint='Message #bulletin or /command'):
    inner=f'<span>{text}</span>'+('<i style="display:inline-block;width:2px;height:20px;background:var(--primary);margin-left:2px;vertical-align:-4px"></i>' if cursor else '') if text else f'<span style="color:var(--on-surface-variant)">{hint}</span>'
    return f'''<div style="display:flex;align-items:center;gap:8px;padding:8px 12px 10px;flex:none;background:var(--surface)"><div style="flex:1;height:48px;border-radius:24px;background:var(--surface-container-high);display:flex;align-items:center;padding:0 16px;font-size:15px;overflow:hidden;white-space:nowrap">{inner}</div><div style="width:48px;height:48px;border-radius:24px;background:var(--primary);color:var(--on-primary);display:flex;align-items:center;justify-content:center">{ic("send")}</div></div>'''

def chat_top(): 
    return f'''<div class="tbar"><div class="logo">{mark('#ff4dff',26,'var(--on-surface)')}</div><div class="tt" style="padding-left:8px"><h1 style="display:flex;align-items:center;gap:2px">#bulletin{ic("drop",24,'var(--on-surface-variant)')}</h1><div class="sub">everyone can read · radio + hub</div></div><div class="ib">{ic("help")}</div>{gear()}</div>'''

def msgs(L): return f'<div style="flex:1;min-height:0;overflow:hidden;display:flex;flex-direction:column;justify-content:flex-end;padding-bottom:6px">{"".join(L)}</div>'

def chat_main(theme='dark'):
    L=[datechip('Saturday 21 Nov'),
     bub('22:41','XX0AAA','Pi gateway in Frome is up if anyone wants to test mail ✉️',via='rf',snr='10 dB'),
     bub('22:48','XX0BBB','cq cq from Bath, anyone about on 433.625? ☀️',via='rf',snr='11 dB'),
     bub('22:49','XX0AAA','Evening XX0BBB! Strong in Frome, 5/9 👍',via='rf',snr='9 dB'),
     bub('22:51','M7TJF','Hi both, M7TJF here. Phone station now live 📱',me=True,rec=['ok'],x='×1'),
     bub('22:52','XX0BBB','Net notes for Sunday: we meet on 433.625 NFM at 10:00, low power only, handhelds rest between blocks. M7TJF, can you run net control from Frome? I will relay from Bath if the ridge gets in the way.',via='rf',snr='-4 dB',grp=g3()),
     bub('22:54','XX0AAA','👍👍',via='rf',snr='-7 dB'),
     bub('22:55','M7TJF','Net control is fine, see you at 10:00. 73',me=True,rec=['ok'],x='×1'),
     sysline('XX0CCC joined #bulletin')]
    c=rigstrip('usb')+msgs(L)+composer()
    return phone(c,'chat',theme,chat_top())

def chat_tx():
    L=[datechip('Saturday 21 Nov'),
     bub('22:49','XX0AAA','Evening XX0BBB! Strong in Frome, 5/9 👍',via='rf',snr='9 dB'),
     bub('22:51','M7TJF','Hi both, M7TJF here. Phone station now live 📱',me=True,rec=['ok'],x='×1'),
     bub('22:52','XX0BBB','Net notes for Sunday: we meet on 433.625 NFM at 10:00, low power only.',via='rf',snr='-4 dB'),
     bub('23:30','M7TJF','Net control is fine. I will set up in Frome by 09:30, bring a flask and the Pi gateway so mail works too. If the ridge blocks Bath, XX0BBB relays. Please check IN levels before TX and keep to NARROW, low power indoors. 73 and see you then ☕',me=True,rec=['tx'],
         extra='<div style="display:flex;align-items:center;gap:8px;margin-top:6px"><div style="flex:1;display:flex;gap:3px"><i style="flex:1;height:4px;border-radius:2px;background:var(--tx)"></i><i style="flex:1;height:4px;border-radius:2px;background:var(--tx)"></i><i style="flex:1;height:4px;border-radius:2px;background:rgba(255,255,255,.18)"></i></div><span class="ts" style="color:var(--tx);font-weight:500">sending 2 of 3</span></div>'),
     f'<div style="display:flex;justify-content:center;margin-top:8px"><span class="chip sm" style="border:0;background:var(--surface-container-high)">{ic("time",14)}Rest 6 s between blocks (handheld duty)</span></div>',
     bub('23:31','M7TJF','Also testing the phone mail screen tonight 📧',me=True,rec=['q'],x='queued')]
    banner=f'''<div style="margin:0 12px 6px;border-radius:16px;background:var(--tx-c);color:var(--on-tx-c);padding:12px 12px 12px 16px;display:flex;align-items:center;gap:14px;flex:none;position:relative;overflow:hidden"><div style="width:40px;height:40px;border-radius:20px;background:var(--tx);color:#2a0d00;display:flex;align-items:center;justify-content:center;flex:none;box-shadow:0 0 0 6px rgba(255,138,80,.22)">{ic("ant")}</div><div style="flex:1;min-width:0"><div style="font-size:16px;line-height:22px;font-weight:500">On air · keying via Digirig CAT</div><div style="font-size:13px;line-height:18px;opacity:.85"><span class="mono" style="font-size:12.5px">433.625</span> NFM · piece 2 of 3 · 00:07</div></div><div class="btn b-tx" style="height:36px;padding:0 16px">Stop</div><div style="position:absolute;left:0;bottom:0;height:4px;width:66%;background:var(--tx)"></div></div>'''
    c=banner+rigstrip('usb',True)+msgs(L)+composer('Sorry, one more thing: the Pi gateway',True)
    snack=f'<div class="snack" style="bottom:{20+80+66}px">Queued behind the message on air<b>UNDO</b></div>'
    snack=f'<div class="snack" style="bottom:176px">Queued behind the message on air<b>UNDO</b></div>'
    return phone(c,'chat','dark',chat_top(),overlay=snack,chatdot=False)

def chat_rooms():
    def row(lead,hl,sp,tr='',badge='',sel=False):
        bg='background:var(--secondary-container);border-radius:28px;margin:0 8px;padding:8px 16px 8px 16px;' if sel else ''
        b=f'<span class="bdg" style="position:static;display:inline-block;margin-top:2px">{badge}</span>' if badge else ''
        return f'<div class="li" style="{bg}min-height:64px"><div class="lead">{lead}</div><div class="mid"><div class="hl {"mono" if hl[0] in "MG" and hl[1:2].isalnum() and not hl.startswith("#") else ""}" style="{"color:var(--cy)" if hl[0] in "MG" else ""}">{hl}</div><div class="sp one">{sp}</div></div><div class="tr"><span>{tr}</span>{b}</div></div>'
    heard=''.join(f'<span class="chip" style="height:32px"><span class="mono" style="color:var(--cy);font-size:13px">{c}</span><span class="mono" style="color:{col};font-size:12px">{d}</span></span>' for c,d,col in (('XX0BBB','11 dB','var(--ok)'),('XX0AAA','9 dB','var(--ok)'),('XX0CCC','2 dB','var(--error)'),('XX0DDD','6 dB','var(--warn)')))
    sheet=f'''<div class="scrim"></div><div class="sheet" style="padding-bottom:28px"><div class="h"></div>
    <div style="display:flex;align-items:center;padding:0 24px 4px"><div style="font-size:22px;line-height:28px;flex:1">Channels</div><span class="chip pri sm">radio-plus</span></div>
    {row(ic('tag',22),'#bulletin','everyone · radio + hub','22:55','',True)}
    {row(ic('group',22),'#frome-bath-net','closed room · 3 members · [all] receipts','22:31')}
    <div class="sec" style="padding-top:12px">Direct</div>
    {row('X','XX0BBB','Net notes for Sunday: we meet on…','22:52','1')}
    {row('X','XX0AAA','👍👍','22:54')}
    <div style="padding:10px 24px 2px" class="lbl">Heard in the last 30 min</div>
    <div style="display:flex;gap:8px;padding:6px 24px 12px;flex-wrap:wrap">{heard}</div>
    <div style="display:flex;gap:8px;padding:0 24px;align-items:center"><div class="tf sheetbg" style="flex:1;height:48px;color:var(--on-surface-variant);font-size:15px"><span class="fl">Direct chat</span>Callsign</div><div class="btn b-tonal" style="padding:0 18px">Open</div><div class="btn b-out" style="padding:0 14px 0 10px;gap:4px">{ic("add")}Room</div></div></div>'''
    return chat_overlay(sheet)

def chat_overlay(sheet,theme='dark'):
    base=chat_main(theme)
    return base.replace('<div class="gest">',sheet+'<div class="gest">',1)

MARKS=[('sched','Waiting to go out','var(--on-surface-variant)'),('done','Sent, no receipt yet','var(--on-surface-variant)'),('x2','Times it was keyed (×2)',None),('repeat','Heard again on the radio (a relay or an echo), not a receipt','var(--on-surface-variant)'),('hub','A gateway put it on the hub','var(--on-surface-variant)'),('done_all','Delivered: they sent a receipt','var(--ok)'),('done_all2','Every member of the closed room got it','var(--ok)'),('error','Refused, or no receipt after the last try','var(--error)'),('ant','Reached you by radio (rf)','var(--tx)'),('cloud','Reached you over the internet (net) or the local network (lan)','var(--primary)'),('snr','Signal-to-noise of a radio line (/snr off hides it)',None),('grp','Pieces of one long message on the air; [1/3] tags are stripped',None)]
def chat_key():
    rows=''
    for k,t,c in MARKS:
        if k=='x2': lead='<span class="mono" style="font-size:12px;color:var(--on-surface-variant)">×2</span>'
        elif k=='snr': lead='<span class="mono" style="font-size:12px;color:var(--on-surface-variant)">-4 dB</span>'
        elif k=='grp': lead=g3('2/3','var(--warn)')
        elif k=='done_all2': lead=ic('done_all',22,c)+''
        else: lead=ic(k,22,c)
        rows+=f'<div class="li" style="min-height:48px;padding-top:6px;padding-bottom:6px"><div style="width:56px;flex:none;display:flex;justify-content:flex-start">{lead}</div><div class="mid" style="font-size:14px;line-height:19px;color:var(--on-surface)">{t}</div></div>'
    sheet=f'''<div class="scrim"></div><div class="sheet" style="padding-bottom:26px"><div class="h"></div><div style="display:flex;align-items:center;padding:0 24px 6px"><div style="font-size:22px;line-height:28px;flex:1">What the marks mean</div><div class="ib" style="margin-right:-12px">{ic("close")}</div></div>{rows}</div>'''
    return chat_overlay(sheet)
