import sys, os
from playwright.sync_api import sync_playwright
def chrome_exe():
    env=os.environ.get("CHROME") or os.environ.get("CHROMIUM_PATH")
    if env: return env
    for p in ("/usr/bin/google-chrome","/usr/bin/google-chrome-stable","/usr/bin/chromium","/usr/bin/chromium-browser"):
        if os.path.exists(p): return p
    return None
def render(files, out_dir='..'):
    exe=chrome_exe()
    with sync_playwright() as p:
        b=p.chromium.launch(executable_path=exe) if exe else p.chromium.launch()
        for html_path,png in files:
            pg=b.new_page(viewport={'width':412,'height':915},device_scale_factor=2)
            pg.goto('file://'+os.path.abspath(html_path)); pg.wait_for_timeout(500)
            pg.evaluate("document.fonts.ready")
            pg.locator('.phone').screenshot(path=png)
            pg.close()
        b.close()
