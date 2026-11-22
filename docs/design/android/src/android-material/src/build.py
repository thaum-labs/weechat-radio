import sys,os
from m3 import *
SCREENS=[]
def add(name,html,js=''): SCREENS.append((name,html,js))
import s1
add('01-chat',s1.chat_main()); add('02-chat-tx',s1.chat_tx()); add('03-chat-rooms',s1.chat_rooms()); add('04-chat-key',s1.chat_key())
add('01-chat-light',s1.chat_main('light'))
for mod in ['s2','s3','s4']:
    try:
        m=__import__(mod); m.register(add)
    except ImportError as e: print('skip',mod,e)
only=sys.argv[1:]
from playwright.sync_api import sync_playwright
import wf
def chrome_exe():
    env=os.environ.get("CHROME") or os.environ.get("CHROMIUM_PATH")
    if env: return env
    for p in ("/usr/bin/google-chrome","/usr/bin/google-chrome-stable","/usr/bin/chromium","/usr/bin/chromium-browser"):
        if os.path.exists(p): return p
    return None
with sync_playwright() as p:
    exe=chrome_exe()
    b=p.chromium.launch(executable_path=exe) if exe else p.chromium.launch()
    n=0
    for name,h,js in SCREENS:
        if only and not any(o in name for o in only): continue
        path=f'_{name}.html'
        open(path,'w').write(page(h,wf.JS+js))
        pg=b.new_page(viewport={'width':412,'height':915},device_scale_factor=2)
        pg.goto('file://'+os.path.abspath(path)); pg.wait_for_timeout(500)
        pg.evaluate("document.fonts.ready")
        pg.locator('.phone').screenshot(path=f'../{name}.png'); pg.close(); n+=1
    b.close()
print('rendered',n)
