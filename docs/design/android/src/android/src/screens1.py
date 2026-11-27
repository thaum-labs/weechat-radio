from lib import *

def line(t,nick,text,me=False,marks='',via='',snr='',grp='',extra=''):
    n=f'<span class="n {"me" if me else "o"}">[{nick}]</span>'
    v=f'<span class="via {via}">[{via}]</span>' if via else ''
    s=f'<span class="snr">{snr}</span>' if snr else ''
    return f'<div class="ln"><span class="t">{t}</span>{n}{text}{extra}{v}{s}{marks}{grp}</div>'

def mk(txt,cls=''): return f'<span class="mk {cls}">{txt}</span>'

def compose_bar(text=None, cursor=False, hint='message #bulletin, or /commands'):
    if text:
        inner=f'<span class="pr">$</span><span>{text}</span>'+('<i class="cursor"></i>' if cursor else '')
    else:
        inner=f'<span class="pr">$</span><span class="ph">{hint}</span>'
    return f'<div class="compose"><div class="box">{inner}</div><div class="btn c-acc" style="min-width:64px;min-height:46px">send</div></div>'

def chanbar(name='#bulletin', sub='everyone'):
    return f'''<div class="chanbar"><div class="chansel"><span>{name}<small>{sub}</small></span>{ic("down","")}</div><div class="btn s c-acc" style="min-height:40px">key</div><div class="iconbtn" style="width:36px;color:var(--dim)">{ic("dots")}</div></div>'''

def seclines(txt):
    return f'<div class="secline">{txt}</div>'

def chat_main():
    L=[
     '<div class="ln sys">— Saturday 21 Nov —</div>',
     line('22:31','XX0CCC','Anyone on 433.625 tonight? Trying the new phone app from Bristol',via='net',snr=''),
     line('22:33','XX0BBB','Heard you XX0CCC, you are 4/5 here. Try NARROW, low power 🔋',via='rf',snr='6 dB'),
     line('22:41','XX0AAA','Pi gateway in Frome is up if anyone wants to test mail ✉️',via='rf',snr='10 dB'),
     line('22:48','XX0BBB','cq cq from Bath, anyone about on 433.625? ☀️',via='rf',snr='11 dB'),
     line('22:49','XX0AAA','Evening XX0BBB! Strong in Frome, 5/9 👍',via='rf',snr='9 dB'),
     line('22:51','M7TJF','Hi both, M7TJF here. Phone station now live 📱',me=True,marks=mk('[ok]','ok')+mk('[x1]','d')),
     line('22:52','XX0BBB','Net notes for Sunday: we meet on 433.625 NFM at 10:00, low power only, handhelds rest between blocks. M7TJF, can you run net control from Frome? I will relay from Bath if the ridge gets in the way.',via='rf',snr='-4 dB',grp=mk('3/3','ok')),
     line('22:54','XX0AAA','👍👍',via='rf',snr='-7 dB'),
     line('22:55','M7TJF','Net control is fine, see you at 10:00. 73',me=True,marks=mk('[rl]')+mk('[ok]','ok')+mk('[x1]','d')),
     '<div class="ln sys">XX0CCC joined #bulletin</div>',
    ]
    c=chanbar()+seclines('<span class="c-pu">anyone on the network can read #bulletin</span>')
    c+=f'<div class="lines">{"".join(L)}</div>'+compose_bar()
    return phone(c,'chat',strip('usb'))

def chat_tx():
    L=[
     '<div class="ln sys">— Saturday 21 Nov —</div>',
     line('22:41','XX0AAA','Pi gateway in Frome is up if anyone wants to test mail ✉️',via='rf',snr='10 dB'),
     line('22:48','XX0BBB','cq cq from Bath, anyone about on 433.625? ☀️',via='rf',snr='11 dB'),
     line('22:49','XX0AAA','Evening XX0BBB! Strong in Frome, 5/9 👍',via='rf',snr='9 dB'),
     line('22:51','M7TJF','Hi both, M7TJF here. Phone station now live 📱',me=True,marks=mk('[ok]','ok')+mk('[x1]','d')),
     line('22:52','XX0BBB','Net notes for Sunday: we meet on 433.625 NFM at 10:00, low power only.',via='rf',snr='-4 dB'),
     line('23:30','M7TJF','Net control is fine. I will set up in Frome by 09:30, bring a flask and the Pi gateway so mail works too. If the ridge blocks Bath, XX0BBB relays. Please check IN levels before TX and keep to NARROW, low power indoors. 73 and see you then ☕',me=True,marks=mk('[tx]'),grp='<span class="grp c-am">■■□ 2/3</span>'),
     '<div class="ln sys">rest 6 s between blocks (handheld duty)</div>',
     line('23:31','M7TJF','Also testing the phone mail screen tonight 📧',me=True,marks=mk('[..]','d')+mk('queued','d')),
    ]
    banner='''<div class="txbanner"><span class="pulse"></span><div class="tt"><b>ON AIR · keying via Digirig CAT</b><span>433.625 NFM · piece 2/3 · 00:07</span></div><div class="btn s c-or" style="min-height:38px">stop</div></div><div class="progress"><i style="width:66%"></i></div>'''
    c=banner+chanbar()+seclines('<span class="c-pu">anyone on the network can read #bulletin</span>')
    c+=f'<div class="lines">{"".join(L)}</div>'+compose_bar('Sorry, one more thing: the Pi gateway',True)
    return phone(c,'chat',strip('usb',tx=True),badge=1,chat_pip=False)

def chat_rooms():
    base=chat_main().replace('<div class="gesture">','<div class="gesture">')
    def crow(name,sub,right='',sel=False,col='c-acc',badge=''):
        b=f'<span class="badge" style="position:static">{badge}</span>' if badge else ''
        bg='background:rgba(125,155,255,.09);' if sel else ''
        return f'<div class="row" style="{bg}min-height:56px"><div class="grow"><div class="{col}" style="font-size:14px">{name}</div><div class="sub">{sub}</div></div><span class="sub">{right}</span>{b}</div>'
    sheet=f'''<div class="scrim"></div><div class="sheet" style="padding-bottom:6px"><div class="grab"></div>
    <h3><span>CHANNELS</span><em>radio-plus</em></h3>
    {crow('#bulletin','everyone · radio + hub','22:55',True)}
    {crow('#frome-bath-net','closed room · 3 members · [all] receipts','22:31')}
    <h3 style="padding-top:12px"><span>DIRECT</span><em>tap a callsign</em></h3>
    {crow('XX0BBB','Net notes for Sunday: we meet on…','22:52',False,'c-cy','1')}
    {crow('XX0AAA','👍👍','22:54',False,'c-cy')}
    <div class="row" style="padding-top:10px;padding-bottom:10px;flex-wrap:wrap;gap:6px"><span class="sub" style="width:100%">heard in the last 30 min</span>
      <span class="chip c-cy">XX0BBB <span class="c-gr">11 dB</span></span><span class="chip c-cy">XX0AAA <span class="c-gr">9 dB</span></span><span class="chip c-cy">XX0CCC <span class="c-re">2 dB</span></span><span class="chip c-cy">XX0DDD <span class="c-am">6 dB</span></span></div>
    <div class="row" style="gap:8px;padding-top:8px;padding-bottom:12px"><div class="input grow" style="min-height:46px"><span class="c-pu" style="margin-right:8px">direct</span><span style="color:#5d576b">callsign</span></div><div class="btn c-or">open</div><div class="btn c-or">+ room</div></div>
    </div>'''
    # place overlay into content
    return base.replace('</div><div class="nav">', sheet+'</div><div class="nav">',1)

MARKS=[('[..]','waiting to go out','d'),('[tx]','sent, no receipt yet',''),('[x2]','how many times it was keyed','d'),('[rl]','heard again on the radio (a relay or an echo), not a receipt',''),('[gw]','a gateway put it on the hub',''),('[ok]','delivered, they sent a receipt','ok'),('[all]','every member of the closed room got it','ok'),('[no]','refused, or no receipt after the last try','no'),('[rf]','reached you by radio','rf'),('[net]','reached you over the internet','net'),('[lan]','reached you on the local network','net'),('-4 dB','signal-to-noise of a radio line (/snr off hides it)','d')]
def chat_key():
    base=chat_tx()
    rows=''
    colmap={'ok':'c-gr','no':'c-re','rf':'c-or','net':'c-acc','d':'c-di','':'c-pu'}
    for m,t,k in MARKS:
        rows+=f'<div class="row" style="min-height:42px;align-items:flex-start;padding-top:9px;padding-bottom:7px"><span class="{colmap[k]}" style="width:54px;flex:none;font-size:13px">{m}</span><span class="grow" style="font-size:12px;color:#b7aec8;line-height:1.35">{t}</span></div>'
    sheet=f'''<div class="scrim"></div><div class="sheet"><div class="grab"></div><h3><span>KEY · what the marks mean</span><em>tap to close</em></h3>{rows}
    <div class="row" style="min-height:52px"><span class="grp c-am" style="width:54px;font-size:13px">■■□</span><span class="grow" style="font-size:12px;color:#b7aec8">pieces of one long message on the air (2/3). Tags like [1/3] are stripped from the text.</span></div><div style="height:6px"></div></div>'''
    return base.replace('</div><div class="nav">', sheet+'</div><div class="nav">',1)
