import sys, os, importlib
from lib import *
import screens1
SCREENS=[]
def add(name, html, js=''):
    SCREENS.append((name, html, js))
add('01-chat', screens1.chat_main())
add('02-chat-tx', screens1.chat_tx())
add('03-chat-rooms', screens1.chat_rooms())
add('04-chat-key', screens1.chat_key())
try:
    import screens2; screens2.register(add)
except ImportError as e:
    print('no screens2',e)
try:
    import screens3; screens3.register(add)
except ImportError as e:
    print('no screens3',e)

only=sys.argv[1:] 
files=[]
for name,h,js in SCREENS:
    if only and not any(o in name for o in only): continue
    path=f'_{name}.html'
    open(path,'w').write(page(h,'',WF_JS+js))
    files.append((path,f'../{name}.png'))
from render import render
render(files)
print('rendered',len(files))
