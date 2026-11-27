import os
from playwright.sync_api import sync_playwright
import os
def chrome_exe():
    env=os.environ.get("CHROME") or os.environ.get("CHROMIUM_PATH")
    if env: return env
    for pth in ("/usr/bin/google-chrome","/usr/bin/google-chrome-stable","/usr/bin/chromium","/usr/bin/chromium-browser"):
        if os.path.exists(pth): return pth
    return None
rows=[
 ('CHAT','Live chat, one room or direct chat at a time',[('01-chat','01 · Chat','RX idle, receipts, grouped long message'),('02-chat-tx','02 · Chat · on air','TX banner, pieces 2/3, queued line'),('03-chat-rooms','03 · Channels','rooms, direct, heard stations'),('04-chat-key','04 · Key','what the marks mean')],
   'NOTES','The desktop chat line is kept as is: time, [CALL] (own orange, others cyan), text, [rf] / [net], SNR, then marks. Long messages show one line with a 3/3 footer; the [1/3] tags are stripped. The desktop STATION panel shrinks to one strip under the app bar; it turns orange and shows PTT TX while keying.'),
 ('EMAIL','One view at a time: list, open, compose (v1.0.21)',[('05-mail-list','05 · Inbox','unread dot, 📎, waiting header, Check mail'),('06-mail-open','06 · Open mail','< Inbox, Reply, Delete, file'),('07-mail-open-sent','07 · Open sent','status chip + journey in local time'),('08-mail-compose','08 · Compose','attachment, airtime line, Cancel / Send'),('09-mail-settings','09 · Email settings','Copy-to and mail gateway (gear)')],None,None),
 ('MAP AND PLANNER','weechatradio.com planner, in your pocket',[('10-map','10 · Map','path verdict, band chips, sure range'),('11-plan-vhf','11 · Plan · 70cm','terrain profile, Mobile antenna, Tune radio'),('12-plan-hf-sky','12 · Plan · HF sky path','80m NVIS, 24 h strip, VOACAP')],
   'NOTES','Band chips keep the site colours: green works, yellow skips, grey will not work. Path colours: blue clear, yellow marginal, red blocked. New on phone: Tune radio sets frequency and preset on the connected rig over CAT (the site only shows /radio qsy). PMR446 is not offered.'),
 ('RADIO','The old STATION panel plus the waterfall',[('13-radio-usb','13 · Radio · USB','Digirig, waterfall, IN / OUT meters'),('14-radio-bluetooth','14 · Radio · Bluetooth','VR-N76, battery note, vhf-fast caution'),('15-radio-stats-on-air','15 · Stats + On the air','BEACN, HOLD, MAIL GW, frames heard')],
   'NOTES','Bluetooth audio runs the full preset ladder; only vhf-fast carries a small untested-on-Bluetooth caution. USB (Digirig) is the suggested route for mail files.'),
 ('FIRST RUN','Four steps, then Start station',[('16-onboarding-1-radio','16 · 1/4 Radio','Bluetooth, USB or internet only'),('17-onboarding-1b-bluetooth','17 · 1/4 Bluetooth','find, pair, radio checklist'),('18-onboarding-2-you','18 · 2/4 You','callsign, locator, mode'),('19-onboarding-3-levels','19 · 3/4 Levels','preset, IN meter, OUT, PTT / VOX'),('20-onboarding-4-permissions','20 · 4/4 Permissions','mic, USB, location, background')],None,None),
 ('SETTINGS','Gear in the app bar',[('21-settings','21 · Settings','station, radio, audio, email, phone')],'TOKENS',None),
]
def swatch(c,n): return f'<div style="display:flex;align-items:center;gap:8px"><i style="width:22px;height:22px;background:{c};border:1px solid #333;display:block"></i><span><b style="font-weight:400;color:#fff">{n}</b> <span style="color:#7a7388">{c}</span></span></div>'
tokens=f'''<div class="note" style="width:{2*262+22}px;margin-top:46px"><h4>DESIGN TOKENS · same as the desktop app</h4>
<div style="display:grid;grid-template-columns:repeat(2,1fr);gap:10px 18px;margin-top:10px;font-size:12px">
{swatch('#07070a','BG')}{swatch('#15151e','SHELL / card')}{swatch('#212840','LINE')}{swatch('#d8d0e8','FG')}
{swatch('#7d9bff','ACCENT')}{swatch('#ff7a3d','ORANGE · own call, TX')}{swatch('#c4b5fd','PURPLE · values')}{swatch('#7a7388','DIM')}
{swatch('#39ff14','GREEN · ok')}{swatch('#ffc43d','AMBER')}{swatch('#ff4d6d','RED')}{swatch('#00d0d0','CYAN · other calls')}
{swatch('#00e5ff','internet')}{swatch('#39ff14','internet-radio')}{swatch('#ffbf00','radio')}{swatch('#ff4dff','radio-plus')}
</div>
<p style="margin-top:14px;line-height:1.6;color:#b7aec8;font-size:12.5px">Type: Fira Mono everywhere (the website font; the desktop app is all monospace). Hairline 1 px buttons with a coloured stroke, 4 px corners, scan lines at 4 px. Targets are 44 dp or more; buttons keep the lowercase terminal voice. The brand mark wave takes the mode colour, as on the desktop.</p></div>'''
PW=262
def sec(title,sub,items,note=''):
    ph=''.join(f'<div class="cell" style="width:{PW}px"><div class="bz"><img src="{f}.png"></div><div class="cap"><b>{cap}</b><br>{desc}</div></div>' for f,cap,desc in items)
    w=len(items)*(PW+22)-22
    return f'<div class="sc" style="width:{w}px"><div class="sec"><h2>{title}</h2><span>{sub}</span></div><p class="nt">{note}</p><div class="row">{ph}</div></div>'
R={r[0]:r for r in rows}
def S(k,note):
    t,sub,items,_,_=R[k]; return sec(t,sub,items,note)
r1=S('CHAT','The desktop chat line is kept as is: time, [CALL] (own orange, others cyan), text, [rf] and SNR, then marks. Long messages show one line with a 3/3 footer; [1/3] tags are stripped. The STATION panel shrinks to a strip under the app bar and goes orange while keying.')+S('MAP AND PLANNER','Site colours: green works, yellow skips, grey will not work; path blue clear, yellow marginal, red blocked. New: Tune radio sets frequency and preset over CAT. PMR446 is not offered.')
r2=S('EMAIL','v1.0.21 email flow, one view at a time. Gear is contextual: on Email it opens Copy-to and gateway, with a link to app settings.')+S('SETTINGS','')+tokens
r3=S('RADIO','Bluetooth audio runs the whole preset ladder; only vhf-fast carries a small "untested on Bluetooth" caution. USB (Digirig) stays the suggested route for mail files.')+S('FIRST RUN','Four steps, then Start station.')
r4=sec('BLUETOOTH FILES AND BACKGROUND','Added after review',[('08b-mail-compose-bluetooth','08b · Compose on Bluetooth','attachment kept, suggest USB, vhf-fast untested'),('22-notification-listening','22 · Listening notification','foreground service, on by default')],'Attachments stay visible on Bluetooth with a suggestion to switch to USB (Digirig). Background listening is ON by default: a persistent notification reads weechat-radio listening on 433.625, with Open, Mute and Stop.')
cells=f'<div class="r1">{r1}</div><div class="r1">{r2}</div><div class="r1">{r3}</div><div class="r1">{r4}</div>'
html=f'''<!doctype html><html><head><meta charset="utf-8"><style>
@font-face{{font-family:"Fira Mono";src:url("src/fonts/fira-mono-400.woff2") format("woff2")}}
*{{box-sizing:border-box;margin:0;padding:0}} body{{background:#07070a;color:#d8d0e8;font-family:"Fira Mono","Noto Color Emoji",monospace;width:2520px;padding:48px 60px 56px}}
.hd{{display:flex;align-items:flex-end;justify-content:space-between;border-bottom:1px solid #212840;padding-bottom:22px;margin-bottom:8px}}
.hd .t{{font-size:44px;letter-spacing:-.04em;color:#f1ecfa;line-height:1.05}} .hd .t b{{font-weight:400;color:#7d9bff}}
.hd p{{color:#948ca3;font-size:14px;line-height:1.6;max-width:640px;text-align:right}}
.pr{{font-size:14px;color:#948ca3;margin-bottom:10px}} .pr b{{font-weight:400;color:#7d9bff}} .pr i{{font-style:normal;color:#ff7a3d}}
.sec{{margin:30px 0 8px;display:flex;align-items:baseline;gap:16px}} .sec h2{{font-weight:400;font-size:18px;color:#7d9bff;letter-spacing:.08em}} .sec span{{color:#ff7a3d;font-size:13px}}
.r1{{display:flex;gap:44px;align-items:flex-start;margin-top:10px}} .row{{display:flex;gap:22px}} .nt{{font-size:12.5px;line-height:1.55;color:#b7aec8;margin:0 0 14px;max-width:760px;min-height:0}} .nt:empty{{display:none}}
.bz{{border:5px solid #14141c;border-radius:26px;overflow:hidden;background:#000;box-shadow:0 0 0 1px #2a3050}} .bz img{{display:block;width:100%}}
.cap{{margin-top:10px;font-size:12px;color:#948ca3;line-height:1.5}} .cap b{{font-weight:400;color:#d8d0e8;font-size:13px}}
.note{{border:1px solid #212840;background:#0a0a10;border-radius:6px;padding:20px;align-self:start}} .note h4{{font-weight:400;font-size:12px;letter-spacing:.08em;color:#7d9bff}} .note p{{margin-top:10px;font-size:12.5px;line-height:1.65;color:#b7aec8}}
.ft{{margin-top:44px;padding-top:16px;border-top:1px solid #212840;color:#6f6880;font-size:12px;line-height:1.6}}
</style></head><body>
<div class="hd"><div><div class="pr"><b>weechat-radio:</b><i>$</i> ./android --mockups</div><div class="t">weechat<b>radio</b> · Android</div></div><p>The phone is a full station: chat, email, map and planner, radio status and waterfall. Radio over Bluetooth audio+PTT (Vero VR-N76) or USB (Digirig + CAT). 412 × 915 dp frames. Looks like the desktop app and weechatradio.com.</p></div>
{cells}
<div class="ft">All names, callsigns and mail are sample data (XX0AAA, M7TJF, IO81UF, Frome, Bath, 433.625 NFM, wcr-qa@mail.grokbot.com). Map basemap is a generated placeholder, not real terrain. Mockups only: not built from app code, no real radio keyed.</div>
</body></html>'''
open('../board.html','w').write(html)
with sync_playwright() as p:
    exe=chrome_exe(); b=p.chromium.launch(executable_path=exe) if exe else p.chromium.launch()
    pg=b.new_page(viewport={'width':2640,'height':1000},device_scale_factor=1.5)
    pg.goto('file://'+os.path.abspath('../board.html')); pg.wait_for_timeout(1500)
    pg.screenshot(path='../00-overview-board.png',full_page=True)
    b.close()
print('board ok')
