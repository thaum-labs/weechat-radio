import os,json
from playwright.sync_api import sync_playwright
import os
def chrome_exe():
    env=os.environ.get("CHROME") or os.environ.get("CHROMIUM_PATH")
    if env: return env
    for pth in ("/usr/bin/google-chrome","/usr/bin/google-chrome-stable","/usr/bin/chromium","/usr/bin/chromium-browser"):
        if os.path.exists(pth): return pth
    return None
P=json.load(open('palette.json'))['True']
PW=262
HEAD='''<style>
*{box-sizing:border-box;margin:0;padding:0} body{background:#121318;color:#e3e1e9;font-family:"Roboto",sans-serif;padding:48px 60px 56px}
.hd{display:flex;align-items:flex-end;justify-content:space-between;padding-bottom:22px;margin-bottom:8px;border-bottom:1px solid #45464f}
.hd .t{font-size:48px;line-height:56px;font-weight:400} .hd .t b{font-weight:400;color:#b5c4ff}
.hd p{color:#c6c6d0;font-size:15px;line-height:23px;max-width:700px;text-align:right}
.sec{margin:34px 0 6px;display:flex;align-items:baseline;gap:16px} .sec h2{font-weight:500;font-size:20px;color:#b5c4ff;letter-spacing:.3px} .sec span{color:#ff8a50;font-size:14px}
.r1{display:flex;gap:44px;align-items:flex-start;margin-top:8px} .row{display:flex;gap:22px} .nt{font-size:13.5px;line-height:20px;color:#c6c6d0;margin:0 0 14px;max-width:760px} .nt:empty{display:none}
.bz{border:6px solid #1e1f25;border-radius:30px;overflow:hidden;background:#000;box-shadow:0 0 0 1px #45464f} .bz img{display:block;width:100%}
.cap{margin-top:10px;font-size:12.5px;color:#c6c6d0;line-height:18px} .cap b{font-weight:500;color:#e3e1e9;font-size:14px}
.note{border-radius:16px;background:#1e1f25;padding:22px;align-self:start} .note h4{font-weight:500;font-size:13px;letter-spacing:.5px;color:#b5c4ff} .note p{margin-top:10px;font-size:13px;line-height:20px;color:#c6c6d0}
.ft{margin-top:44px;padding-top:16px;border-top:1px solid #45464f;color:#8f909a;font-size:12.5px;line-height:19px}
</style>'''
def sec(title,sub,items,note='',pw=PW):
    ph=''.join(f'<div style="width:{pw}px"><div class="bz"><img src="{f}.png"></div><div class="cap"><b>{cap}</b><br>{desc}</div></div>' for f,cap,desc in items)
    w=len(items)*(pw+22)-22
    return f'<div style="width:{w}px"><div class="sec"><h2>{title}</h2><span>{sub}</span></div><p class="nt">{note}</p><div class="row">{ph}</div></div>'
def sw(c,n): return f'<div style="display:flex;align-items:center;gap:8px"><i style="width:24px;height:24px;border-radius:6px;background:{c};display:block;border:1px solid #ffffff22"></i><span style="font-size:12.5px"><b style="font-weight:500">{n}</b> <span style="color:#8f909a">{c}</span></span></div>'
pal=f'''<div class="note" style="width:{2*PW+22}px"><h4>DYNAMIC COLOUR · seeded from #7d9bff</h4><div style="display:grid;grid-template-columns:1fr 1fr;gap:9px 16px;margin-top:12px">
{sw(P['primary'],'primary')}{sw(P['primary_container'],'primary container')}{sw(P['secondary_container'],'secondary container · nav pill')}{sw(P['surface'],'surface')}{sw(P['surface_container_high'],'surface container high')}{sw(P['error'],'error')}
{sw('#ff8a50','TX orange (custom)')}{sw('#78dc66','OK green (custom)')}{sw('#ffd166','Warn amber (custom)')}{sw('#5fd3e0','Other calls (custom)')}{sw('#c4b5fd','Meter peak (custom)')}{sw('#ff4dff','radio-plus mode')}</div>
<p>Tonal palette from the M3 tonal-spot algorithm (2021 spec) on the weechat-radio accent. Radio meaning rides on four fixed custom roles (TX orange, OK green, warn amber, call cyan) harmonised per theme. Type: Roboto; Roboto Mono only for callsigns, frequencies, grid squares and preset codes.</p></div>'''
r1=sec('CHAT','Bubbles, receipt icons and a key sheet instead of [ok] text',[('01-chat','01 · Chat','bubbles, via and SNR, receipts'),('02-chat-tx','02 · On air','TX banner, progress, snackbar'),('03-chat-rooms','03 · Channels','bottom sheet, direct, heard chips'),('04-chat-key','04 · Key','what each icon means')],'Own messages sit right in primary container; others left with a mono cyan callsign. Mark codes become icons: clock queued, tick sent, double tick delivered, repeat relayed, error refused.')+sec('MAP AND PLANNER','Same planner, M3 sheet and chips',[('10-map','10 · Map','search bar, FABs, verdict, band chips'),('11-plan-vhf','11 · Plan · 70cm','terrain, cards, Tune radio'),('12-plan-hf-sky','12 · Plan · HF sky','80m NVIS, 24 h strip')],'Band FilterChips keep green / yellow / grey dots. Path colours stay blue, yellow, red.')
r2=sec('EMAIL','List, read, compose, gear sheet',[('05-mail-list','05 · Inbox','tabs, waiting card, extended FAB'),('06-mail-open','06 · Open mail','back, reply, delete, file card'),('07-mail-open-sent','07 · Sent','Delivered chip, journey stepper'),('08-mail-compose','08 · Compose','outlined fields, input chip'),('08b-mail-compose-bluetooth','08b · Compose on BT','keeps file, suggests USB'),('09-mail-settings','09 · Email settings','modal bottom sheet')],'')+sec('SETTINGS AND BACKGROUND','Listening is on by default',[('21-settings','21 · Settings','grouped list, switches'),('22-notification-listening','22 · Notification shade','foreground service')],'')
r3=sec('RADIO','Status, waterfall and meters',[('13-radio-usb','13 · Radio · USB','Digirig, waterfall, IN/OUT'),('14-radio-bluetooth','14 · Radio · Bluetooth','battery card, vhf-fast caution'),('15-radio-stats-on-air','15 · Stats + On the air','key/value list, frames')],'')+sec('FIRST RUN','Four steps',[('16-onboarding-1-radio','16 · Radio','selectable cards'),('17-onboarding-1b-bluetooth','17 · Bluetooth','segmented, device list'),('18-onboarding-2-you','18 · You','text fields, mode'),('19-onboarding-3-levels','19 · Levels','chips, slider, PTT segments'),('20-onboarding-4-permissions','20 · Permissions','switch on by default')],'')
r4=sec('LIGHT THEME','Material You light, three key screens',[('01-chat-light','01 · Chat (light)','same bubbles, light roles'),('05-mail-list-light','05 · Inbox (light)','tonal avatars'),('13-radio-usb-light','13 · Radio (light)','waterfall keeps its palette')],'')+pal
html=f'''<!doctype html><html><head><meta charset="utf-8">{HEAD}</head><body><div class="hd"><div class="t">weechat<b>radio</b> · Android <span style="font-size:24px;color:#8f909a">Material 3</span></div><p>The same 21 screens, data and states as the desktop-look set, rebuilt with native Material 3 components: top app bar, navigation bar with pill indicator, FAB, chips, cards, list items, bottom sheets, segmented buttons, sliders and switches.</p></div>
<div class="r1" style="width:2480px">{r1}</div><div class="r1" style="width:2480px">{r2}</div><div class="r1" style="width:2480px">{r3}</div><div class="r1" style="width:2480px">{r4}</div>
<div class="ft">Sample data: XX0AAA, XX0BBB, M7TJF, IO81UF, Frome, Bath, 433.625 NFM, wcr-qa@mail.grokbot.com. Map basemap is a generated placeholder. Mockups only: no real radio keyed.</div></body></html>'''
open('../board.html','w').write(html)
with sync_playwright() as p:
    exe=chrome_exe(); b=p.chromium.launch(executable_path=exe) if exe else p.chromium.launch()
    pg=b.new_page(viewport={'width':2600,'height':1000},device_scale_factor=1.5)
    pg.goto('file://'+os.path.abspath('../board.html')); pg.wait_for_timeout(1200)
    pg.screenshot(path='../00-overview-board.png',full_page=True); b.close()
print('overview ok')

# ---- compare board ----
pairs=[('01-chat','Chat','Bubbles and receipt icons replace the terminal line [ok] [x1] and the key.'),('02-chat-tx','Chat · on air','TX banner with Stop, progress, snackbar. Strip turns orange in both.'),('05-mail-list','Mail list','Tabs, tonal waiting card, extended FAB vs terminal rows and + New.'),('10-map','Map and planner','Search bar, FABs and a standard bottom sheet vs hairline site-style panel.'),('13-radio-usb','Radio · USB','Cards, filter chips and sliders-style meters vs mono panel.'),('16-onboarding-1-radio','Onboarding · radio','Selectable cards with radio buttons vs ./setup prompt.')]
CW=330
cells=''
for f,t,d in pairs:
    cells+=f'''<div style="width:{2*CW+24}px"><div style="font-size:22px;font-weight:500;margin-bottom:2px">{t}</div><div style="font-size:13px;color:#c6c6d0;line-height:19px;height:38px;max-width:{2*CW+24}px">{d}</div><div style="display:flex;gap:24px;margin-top:10px">
    <div style="width:{CW}px"><div class="bz" style="border-color:#14141c;border-radius:26px"><img src="../android/{f}.png" style="width:100%;display:block"></div><div class="cap" style="text-align:center"><b style="color:#ff7a3d">Desktop look</b><br>Fira Mono · hairline · scanlines</div></div>
    <div style="width:{CW}px"><div class="bz"><img src="{f}.png"></div><div class="cap" style="text-align:center"><b style="color:#b5c4ff">Material 3</b><br>Roboto · tonal · M3 components</div></div></div></div>'''
html=f'''<!doctype html><html><head><meta charset="utf-8">{HEAD}<style>body{{width:2300px}}.grid{{display:grid;grid-template-columns:repeat(3,{2*CW+24}px);gap:56px 60px;margin-top:30px}}</style></head><body><div class="hd"><div class="t">Desktop look <b>vs</b> Material 3</div><p>Six matching pairs, same data and states. Left: the desktop-app styling carried to the phone. Right: native Android with Material You.</p></div><div class="grid">{cells}</div>
<div class="ft">Sample data. Both sets share the same content, numbering and states.</div></body></html>'''
open('../compare.html','w').write(html)
with sync_playwright() as p:
    exe=chrome_exe(); b=p.chromium.launch(executable_path=exe) if exe else p.chromium.launch()
    pg=b.new_page(viewport={"width":2420,'height':1000},device_scale_factor=1.5)
    pg.goto('file://'+os.path.abspath('../compare.html')); pg.wait_for_timeout(1200)
    pg.screenshot(path='../00-compare-board.png',full_page=True); b.close()
print('compare ok')
