import json, html
PAL=json.load(open(__file__.rsplit('/',1)[0]+'/palette.json'))
def vars_css(d):
    return ';'.join('--'+k.replace('_','-')+':'+v for k,v in d.items())
DARK_X='--tx:#ff8a50;--tx-c:#5d2c12;--on-tx-c:#ffdbc9;--ok:#78dc66;--ok-c:#1b5220;--on-ok-c:#b8f3a9;--warn:#ffd166;--warn-c:#574300;--on-warn-c:#ffe39a;--cy:#5fd3e0;--pu:#c4b5fd;--meter-ok:#6fdc5a;--scrim:rgba(0,0,0,.52)'
LIGHT_X='--tx:#b3410a;--tx-c:#ffdbc9;--on-tx-c:#3a1300;--ok:#1d7a22;--ok-c:#b8f3a9;--on-ok-c:#00210a;--warn:#7a5900;--warn-c:#ffe39a;--on-warn-c:#251a00;--cy:#006a73;--pu:#6a4fc4;--meter-ok:#2f9e24;--scrim:rgba(0,0,0,.32)'
CSS='''
@font-face{font-family:"Roboto";src:url("fonts/Roboto-Variable.ttf") format("truetype");font-weight:100 900;font-style:normal}
@font-face{font-family:"Roboto Mono";src:url("fonts/RobotoMono-Variable.ttf") format("truetype");font-weight:100 900;font-style:normal}
*{box-sizing:border-box;margin:0;padding:0}
body{background:#000;font-family:"Roboto",sans-serif;-webkit-font-smoothing:antialiased}
.phone{width:412px;height:915px;position:relative;overflow:hidden;background:var(--surface);color:var(--on-surface);display:flex;flex-direction:column;font-size:14px;line-height:20px}
.phone.dark{'''+vars_css(PAL['True'])+';'+DARK_X+'''}
.phone.light{'''+vars_css(PAL['False'])+';'+LIGHT_X+'''}
.mono{font-family:"Roboto Mono",monospace;letter-spacing:0}
svg.i{width:24px;height:24px;fill:currentColor;flex:none}
.sb{height:32px;flex:none;display:flex;align-items:center;justify-content:space-between;padding:0 20px 0 24px;font-size:13px;font-weight:500;color:var(--on-surface)}
.sb .l{display:flex;align-items:center;gap:6px}.sb .r{display:flex;align-items:center;gap:6px}.sb svg{width:15px;height:15px;fill:currentColor}
.gest{position:absolute;left:0;right:0;bottom:0;height:20px;display:flex;align-items:flex-end;justify-content:center;padding-bottom:7px;pointer-events:none}.gest i{display:block;width:120px;height:4px;border-radius:2px;background:var(--on-surface);opacity:.85}
.tbar{height:64px;flex:none;display:flex;align-items:center;padding:0 4px 0 4px;gap:0;background:var(--surface)}
.tbar .tt{flex:1;min-width:0;padding-left:12px}.tbar .tt.nob{padding-left:12px}
.tbar h1{font-size:22px;line-height:28px;font-weight:400;color:var(--on-surface);white-space:nowrap}
.tbar .sub{font-size:12px;line-height:16px;color:var(--on-surface-variant);white-space:nowrap}
.ib{width:48px;height:48px;border-radius:24px;display:flex;align-items:center;justify-content:center;color:var(--on-surface-variant);flex:none}
.ib.t{background:var(--secondary-container);color:var(--on-secondary-container);width:40px;height:40px;margin:0 4px}
.logo{width:48px;display:flex;align-items:center;justify-content:center;flex:none}
.content{flex:1;min-height:0;display:flex;flex-direction:column;position:relative;overflow:hidden}
.nav{height:80px;flex:none;background:var(--surface-container);display:flex;padding:12px 8px 16px;gap:8px;margin-bottom:20px;position:relative}
.navbg{position:absolute;left:0;right:0;bottom:0;height:20px;background:var(--surface-container)}
.nav a{flex:1;display:flex;flex-direction:column;align-items:center;gap:4px;font-size:12px;line-height:16px;font-weight:500;color:var(--on-surface-variant);position:relative}
.nav a .pill{width:64px;height:32px;border-radius:16px;display:flex;align-items:center;justify-content:center;position:relative}
.nav a.on{color:var(--on-surface)}.nav a.on .pill{background:var(--secondary-container);color:var(--on-secondary-container)}
.bdg{position:absolute;top:-2px;left:34px;min-width:16px;height:16px;border-radius:8px;background:var(--error);color:var(--on-error);font-size:11px;line-height:16px;text-align:center;padding:0 4px;font-weight:500}
.bdg.dot{min-width:6px;width:6px;height:6px;padding:0;top:3px;left:38px}
.btn{height:40px;border-radius:20px;padding:0 24px;display:inline-flex;align-items:center;justify-content:center;gap:8px;font-size:14px;font-weight:500;white-space:nowrap;flex:none}
.btn svg.i{width:18px;height:18px}.btn.ic{padding:0 24px 0 16px}
.b-fill{background:var(--primary);color:var(--on-primary)}
.b-tonal{background:var(--secondary-container);color:var(--on-secondary-container)}
.b-out{border:1px solid var(--outline);color:var(--primary)}
.b-text{color:var(--primary);padding:0 12px}
.b-tx{background:var(--tx);color:#2a0d00}
.b-err{color:var(--error)}
.chip{height:32px;border-radius:8px;border:1px solid var(--outline-variant);display:inline-flex;align-items:center;gap:6px;padding:0 12px;font-size:14px;font-weight:500;color:var(--on-surface-variant);white-space:nowrap;flex:none}
.chip svg.i{width:18px;height:18px}
.chip.sel{background:var(--secondary-container);color:var(--on-secondary-container);border-color:transparent}
.chip.sm{height:24px;border-radius:6px;font-size:12px;padding:0 8px;gap:4px}.chip.sm svg.i{width:14px;height:14px}
.chip.ok{background:var(--ok-c);color:var(--on-ok-c);border-color:transparent}
.chip.warn{background:var(--warn-c);color:var(--on-warn-c);border-color:transparent}
.chip.tx{background:var(--tx-c);color:var(--on-tx-c);border-color:transparent}
.chip.pri{background:var(--primary-container);color:var(--on-primary-container);border-color:transparent}
.chip.err{background:var(--error-container);color:var(--on-error-container);border-color:transparent}
.dotc{width:8px;height:8px;border-radius:4px;display:inline-block;flex:none}
.card{border-radius:12px;background:var(--surface-container-high);padding:16px}
.card.o{background:transparent;border:1px solid var(--outline-variant)}
.card.low{background:var(--surface-container)}
.lbl{font-size:12px;line-height:16px;font-weight:500;letter-spacing:.5px;color:var(--on-surface-variant)}
.sec{padding:16px 16px 8px;font-size:14px;line-height:20px;font-weight:500;color:var(--primary)}
.li{display:flex;align-items:center;gap:16px;padding:8px 16px;min-height:56px}
.li .lead{width:40px;height:40px;border-radius:20px;background:var(--primary-container);color:var(--on-primary-container);display:flex;align-items:center;justify-content:center;flex:none;font-size:16px;font-weight:500}
.li .lead.pl{background:none;color:var(--on-surface-variant);width:24px;height:24px;border-radius:0}
.li .mid{flex:1;min-width:0}
.li .hl{font-size:16px;line-height:24px;color:var(--on-surface);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.li .sp{font-size:14px;line-height:20px;color:var(--on-surface-variant)}
.li .sp.one{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.li .tr{font-size:12px;line-height:16px;color:var(--on-surface-variant);text-align:right;flex:none;display:flex;flex-direction:column;align-items:flex-end;gap:4px}
.div{height:1px;background:var(--outline-variant);margin:0 16px}
.tf{position:relative;height:56px;border:1px solid var(--outline);border-radius:4px;padding:0 16px;display:flex;align-items:center;font-size:16px;color:var(--on-surface)}
.tf .fl{position:absolute;top:-9px;left:12px;padding:0 4px;background:var(--surface);font-size:12px;line-height:16px;color:var(--primary)}
.tf.sheetbg .fl{background:var(--surface-container-low)}
.tf .tl{margin-left:auto;color:var(--on-surface-variant);display:flex}
.sheet{position:absolute;left:0;right:0;bottom:0;background:var(--surface-container-low);border-radius:28px 28px 0 0;z-index:20;padding:0 0 20px}
.sheet .h{width:32px;height:4px;border-radius:2px;background:var(--on-surface-variant);opacity:.4;margin:16px auto 8px}
.scrim{position:absolute;inset:0;background:var(--scrim);z-index:10}
.fab{position:absolute;right:16px;height:56px;border-radius:16px;background:var(--primary-container);color:var(--on-primary-container);display:flex;align-items:center;gap:12px;padding:0 20px 0 16px;font-size:14px;font-weight:500;z-index:5;box-shadow:0 3px 8px rgba(0,0,0,.35)}
.fab.sm{width:40px;height:40px;border-radius:12px;padding:0;justify-content:center;background:var(--surface-container-high);color:var(--primary)}
.snack{position:absolute;left:8px;right:8px;min-height:48px;border-radius:4px;background:var(--inverse-surface);color:var(--inverse-on-surface);display:flex;align-items:center;padding:0 8px 0 16px;font-size:14px;z-index:8;box-shadow:0 3px 8px rgba(0,0,0,.35)}
.snack b{margin-left:auto;padding:0 12px;color:var(--inverse-primary);font-weight:500}
.seg{display:flex;height:40px;border:1px solid var(--outline);border-radius:20px;overflow:hidden}
.seg div{flex:1;display:flex;align-items:center;justify-content:center;gap:6px;font-size:14px;font-weight:500;color:var(--on-surface);border-left:1px solid var(--outline)}
.seg div:first-child{border-left:0}.seg div.on{background:var(--secondary-container);color:var(--on-secondary-container)}.seg svg.i{width:18px;height:18px}
.sw{width:52px;height:32px;border-radius:16px;background:var(--surface-container-highest);border:2px solid var(--outline);position:relative;flex:none}
.sw::after{content:"";position:absolute;left:6px;top:8px;width:12px;height:12px;border-radius:6px;background:var(--outline)}
.sw.on{background:var(--primary);border-color:var(--primary)}.sw.on::after{left:26px;top:4px;width:24px;height:24px;border-radius:12px;background:var(--on-primary)}
.sw.on i{position:absolute;left:31px;top:9px;width:14px;height:14px;color:var(--primary);z-index:2}.sw.on i svg{width:14px;height:14px;fill:var(--primary)}
.lpi{height:4px;border-radius:2px;background:var(--secondary-container);position:relative;overflow:hidden}
.lpi i{position:absolute;left:0;top:0;bottom:0;border-radius:2px;background:var(--primary)}
.meter{height:10px;border-radius:5px;background:var(--surface-container-highest);position:relative;overflow:hidden}
.meter i{position:absolute;top:0;bottom:0}
.rad{width:20px;height:20px;border-radius:10px;border:2px solid var(--on-surface-variant);position:relative;flex:none}
.rad.on{border-color:var(--primary)}.rad.on::after{content:"";position:absolute;inset:3px;border-radius:50%;background:var(--primary)}
.slider{position:relative;height:44px}
.slider .tr1{position:absolute;left:0;right:0;top:18px;height:8px;border-radius:4px;background:var(--secondary-container)}
.slider .tr1 i{position:absolute;left:0;top:0;bottom:0;border-radius:4px 0 0 4px;background:var(--primary)}
.slider .th{position:absolute;top:10px;width:4px;height:24px;border-radius:2px;background:var(--primary)}
.slider .vl{position:absolute;top:-22px;transform:translateX(-50%);background:var(--inverse-surface);color:var(--inverse-on-surface);font-size:12px;padding:3px 9px;border-radius:14px;white-space:nowrap}
.mark{display:inline-block}
.ts{font-size:11px;line-height:16px;color:var(--on-surface-variant)}
.ok{color:var(--ok)}.txc{color:var(--tx)}.wn{color:var(--warn)}.cy{color:var(--cy)}.pri{color:var(--primary)}.osv{color:var(--on-surface-variant)}.er{color:var(--error)}
'''
ICONS={
'chat':'M20 2H4c-1.1 0-1.99.9-1.99 2L2 22l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zM6 9h12v2H6V9zm8 5H6v-2h8v2zm4-6H6V6h12v2z',
'mail':'M20 4H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 4l-8 5-8-5V6l8 5 8-5v2z',
'map':'M20.5 3l-.16.03L15 5.1 9 3 3.36 4.9c-.21.07-.36.25-.36.48V20.5c0 .28.22.5.5.5l.16-.03L9 18.9l6 2.1 5.64-1.9c.21-.07.36-.25.36-.48V3.5c0-.28-.22-.5-.5-.5zM15 19l-6-2.11V5l6 2.11V19z',
'radio':'M12 5c-3.87 0-7 3.13-7 7h2c0-2.76 2.24-5 5-5s5 2.24 5 5h2c0-3.87-3.13-7-7-7zm1 9.29c.88-.39 1.5-1.26 1.5-2.29 0-1.38-1.12-2.5-2.5-2.5S9.5 10.62 9.5 12c0 1.02.62 1.9 1.5 2.29v3.3L7.59 21 9 22.41l3-3 3 3L16.41 21 13 17.59v-3.3zM12 1C5.93 1 1 5.93 1 12h2c0-4.97 4.03-9 9-9s9 4.03 9 9h2c0-6.07-4.93-11-11-11z',
'gear':'M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z',
'done':'M9 16.2 4.8 12l-1.4 1.4L9 19 21 7l-1.4-1.4z',
'done_all':'M18 7l-1.41-1.41-6.34 6.34 1.41 1.41L18 7zm4.24-1.41L11.66 16.17 7.48 12l-1.41 1.41L11.66 19l12-12-1.42-1.41zM.41 13.41L6 19l1.41-1.41L1.83 12 .41 13.41z',
'sched':'M11.99 2C6.47 2 2 6.48 2 12s4.47 10 9.99 10C17.52 22 22 17.52 22 12S17.52 2 11.99 2zM12 20c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8zm.5-13H11v6l5.25 3.15.75-1.23-4.5-2.67z',
'error':'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z',
'send':'M2.01 21L23 12 2.01 3 2 10l15 2-15 2z',
'back':'M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z',
'close':'M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z',
'check':'M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z',
'add':'M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z',
'minus':'M19 13H5v-2h14v2z',
'dots':'M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z',
'clip':'M16.5 6v11.5c0 2.21-1.79 4-4 4s-4-1.79-4-4V5c0-1.38 1.12-2.5 2.5-2.5s2.5 1.12 2.5 2.5v10.5c0 .55-.45 1-1 1s-1-.45-1-1V6H10v9.5c0 1.38 1.12 2.5 2.5 2.5s2.5-1.12 2.5-2.5V5c0-2.21-1.79-4-4-4S7 2.79 7 5v12.5c0 3.04 2.46 5.5 5.5 5.5s5.5-2.46 5.5-5.5V6h-1.5z',
'drop':'M7 10l5 5 5-5z',
'bt':'M17.71 7.71L12 2h-1v7.59L6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 11 14.41V22h1l5.71-5.71-4.3-4.29 4.3-4.29zM13 5.83l1.88 1.88L13 9.59V5.83zm1.88 10.46L13 18.17v-3.76l1.88 1.88z',
'usb':'M15 7v4h1v2h-3V5h2l-3-4-3 4h2v8H8v-2.07c.7-.37 1.2-1.08 1.2-1.93 0-1.21-.99-2.2-2.2-2.2-1.21 0-2.2.99-2.2 2.2 0 .85.5 1.56 1.2 1.93V13c0 1.11.89 2 2 2h3v3.05c-.71.37-1.2 1.1-1.2 1.95 0 1.22.99 2.2 2.2 2.2 1.21 0 2.2-.98 2.2-2.2 0-.85-.49-1.58-1.2-1.95V15h3c1.11 0 2-.89 2-2v-2h1V7h-4z',
'mic':'M12 14c1.66 0 2.99-1.34 2.99-3L15 5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3zm5.3-3c0 3-2.54 5.1-5.3 5.1S6.7 14 6.7 11H5c0 3.41 2.72 6.23 6 6.72V21h2v-3.28c3.28-.48 6-3.3 6-6.72h-1.7z',
'pin':'M12 2C8.13 2 5 5.13 5 9c0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7zm0 9.5c-1.38 0-2.5-1.12-2.5-2.5s1.12-2.5 2.5-2.5 2.5 1.12 2.5 2.5-1.12 2.5-2.5 2.5z',
'bell':'M12 22c1.1 0 2-.9 2-2h-4c0 1.1.89 2 2 2zm6-6v-5c0-3.07-1.64-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.63 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z',
'batt':'M15.67 4H14V2h-4v2H8.33C7.6 4 7 4.6 7 5.33v15.33C7 21.4 7.6 22 8.33 22h7.33c.74 0 1.34-.6 1.34-1.33V5.33C17 4.6 16.4 4 15.67 4z',
'reply':'M10 9V5l-7 7 7 7v-4.1c5 0 8.5 1.6 11 5.1-1-5-4-10-11-11z',
'del':'M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z',
'edit':'M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z',
'sync':'M12 4V1L8 5l4 4V6c3.31 0 6 2.69 6 6 0 1.01-.25 1.97-.7 2.8l1.46 1.46C19.54 15.03 20 13.57 20 12c0-4.42-3.58-8-8-8zm0 14c-3.31 0-6-2.69-6-6 0-1.01.25-1.97.7-2.8L5.24 7.74C4.46 8.97 4 10.43 4 12c0 4.42 3.58 8 8 8v3l4-4-4-4v3z',
'loc':'M12 8c-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4-1.79-4-4-4zm8.94 3A8.994 8.994 0 0013 3.06V1h-2v2.06A8.994 8.994 0 003.06 11H1v2h2.06A8.994 8.994 0 0011 20.94V23h2v-2.06A8.994 8.994 0 0020.94 13H23v-2h-2.06zM12 19c-3.87 0-7-3.13-7-7s3.13-7 7-7 7 3.13 7 7-3.13 7-7 7z',
'copy':'M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z',
'swap':'M6.99 11L3 15l3.99 4v-3H14v-2H6.99v-3zM21 9l-3.99-4v3H10v2h7.01v3L21 9z',
'warn':'M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z',
'info':'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z',
'help':'M11 18h2v-2h-2v2zm1-16C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8zm0-14c-2.21 0-4 1.79-4 4h2c0-1.1.9-2 2-2s2 .9 2 2c0 2-3 1.75-3 5h2c0-2.25 3-2.5 3-5 0-2.21-1.79-4-4-4z',
'cloud':'M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96z',
'wifi':'M1 9l2 2c4.97-4.97 13.03-4.97 18 0l2-2C16.93 2.93 7.08 2.93 1 9zm8 8l3 3 3-3c-1.65-1.66-4.34-1.66-6 0zm-4-4l2 2c2.76-2.76 7.24-2.76 10 0l2-2C15.14 9.14 8.87 9.14 5 13z',
'group':'M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5c-1.66 0-3 1.34-3 3s1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5C6.34 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z',
'person':'M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z',
'tag':'M20 10V8h-4V4h-2v4h-4V4H8v4H4v2h4v4H4v2h4v4h2v-4h4v4h2v-4h4v-2h-4v-4h4zm-6 4h-4v-4h4v4z',
'inbox':'M19 3H4.99c-1.11 0-1.98.89-1.98 2L3 19c0 1.1.88 2 1.99 2H19c1.1 0 2-.9 2-2V5c0-1.11-.9-2-2-2zm0 12h-4c0 1.66-1.35 3-3 3s-3-1.34-3-3H4.99V5H19v10z',
'dl':'M19.35 10.04C18.67 6.59 15.64 4 12 4 9.11 4 6.6 5.64 5.35 8.04 2.34 8.36 0 10.91 0 14c0 3.31 2.69 6 6 6h13c2.76 0 5-2.24 5-5 0-2.64-2.05-4.78-4.65-4.96zM17 13l-5 5-5-5h3V9h4v4h3z',
'play':'M8 5v14l11-7z',
'tune':'M3 17v2h6v-2H3zM3 5v2h10V5H3zm10 16v-2h8v-2h-8v-2h-2v6h2zM7 9v2H3v2h4v2h2V9H7zm14 4v-2H11v2h10zm-6-4h2V7h4V5h-4V3h-2v6z',
'file':'M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2H18c1.1 0 2-.9 2-2V8l-6-6zm2 16H8v-2h8v2zm0-4H8v-2h8v2zm-3-5V3.5L18.5 9H13z',
'chev':'M10 6L8.59 7.41 13.17 12l-4.58 4.59L10 18l6-6z',
'up':'M12 8l-6 6 1.41 1.41L12 10.83l4.59 4.58L18 14z',
'search':'M15.5 14h-.79l-.28-.27A6.471 6.471 0 0016 9.5 6.5 6.5 0 109.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z',
'repeat':'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z',
'hub':'M21 12l-3-3v2h-4V7h2l-3-3-3 3h2v4H6V9l-3 3 3 3v-2h4v4H8l3 3 3-3h-2v-4h4v2l3-3z',
'shield':'M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm-2 16l-4-4 1.41-1.41L10 14.17l6.59-6.59L18 9l-8 8z',
'time':'M11.99 2C6.47 2 2 6.48 2 12s4.47 10 9.99 10C17.52 22 22 17.52 22 12S17.52 2 11.99 2zM12 20c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8zm.5-13H11v6l5.25 3.15.75-1.23-4.5-2.67z',
'ant':'M12 5c-3.87 0-7 3.13-7 7h2c0-2.76 2.24-5 5-5s5 2.24 5 5h2c0-3.87-3.13-7-7-7zm1 9.29c.88-.39 1.5-1.26 1.5-2.29 0-1.38-1.12-2.5-2.5-2.5S9.5 10.62 9.5 12c0 1.02.62 1.9 1.5 2.29v3.3L7.59 21 9 22.41l3-3 3 3L16.41 21 13 17.59v-3.3z',
'phone':'M17 1.01L7 1c-1.1 0-2 .9-2 2v18c0 1.1.9 2 2 2h10c1.1 0 2-.9 2-2V3c0-1.1-.9-1.99-2-1.99zM17 19H7V5h10v14z',
'graph':'M7 18h2V6H7v12zm4 4h2V2h-2v20zm-8-8h2v-4H3v4zm12 4h2V6h-2v12zm4-8v4h2v-4h-2z',
'key':'M12.65 10C11.83 7.67 9.61 6 7 6c-3.31 0-6 2.69-6 6s2.69 6 6 6c2.61 0 4.83-1.67 5.65-4H17v4h4v-4h2v-4H12.65zM7 14c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2z',
'refresh':'M17.65 6.35C16.2 4.9 14.21 4 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08c-.82 2.33-3.04 4-5.65 4-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z',
'wave':'M7 18h2V6H7v12zm4 4h2V2h-2v20zm-8-8h2v-4H3v4zm12 4h2V6h-2v12zm4-8v4h2v-4h-2z',
}
def ic(n,size=None,color=None,cls=''):
    st=''
    if size: st+=f'width:{size}px;height:{size}px;'
    if color: st+=f'color:{color};'
    return f'<svg class="i {cls}" viewBox="0 0 24 24" style="{st}"><path d="{ICONS[n]}"/></svg>'

def mark(wave='#ff4dff',w=22,fg=None):
    fg=fg or 'currentColor'
    return f'''<svg class="mark" viewBox="0 0 64 60" width="{w}" height="{w*60/64:.0f}" fill="none"><path d="M8 20 L20 30 L8 40" stroke="{fg}" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/><rect x="25" y="36" width="15" height="5" rx="1" fill="{wave}"/><path d="M40 29 A7 7 0 0 1 47 36" stroke="{wave}" stroke-width="4" stroke-linecap="round"/><path d="M40 23 A13 13 0 0 1 53 36" stroke="{wave}" stroke-width="4" stroke-linecap="round" opacity="0.7"/><path d="M40 17 A19 19 0 0 1 59 36" stroke="{wave}" stroke-width="4" stroke-linecap="round" opacity="0.4"/></svg>'''

def statusbar(bt=False,usb=True,time='23:31'):
    ex=''
    if bt: ex+='<svg viewBox="0 0 24 24"><path d="'+ICONS['bt']+'"/></svg>'
    if usb: ex+='<svg viewBox="0 0 24 24"><path d="'+ICONS['usb']+'"/></svg>'
    return f'''<div class="sb"><div class="l">{time}{mark("#ff4dff",14,"currentColor")}</div><div class="r">{ex}<svg viewBox="0 0 24 24"><path d="M1 9l2 2c4.97-4.97 13.03-4.97 18 0l2-2C16.93 2.93 7.08 2.93 1 9zm8 8l3 3 3-3c-1.65-1.66-4.34-1.66-6 0zm-4-4l2 2c2.76-2.76 7.24-2.76 10 0l2-2C15.14 9.14 8.87 9.14 5 13z"/></svg><svg viewBox="0 0 24 24"><path d="M2 22V12l20-10v20z"/></svg><span style="font-size:12px">78%</span></div></div>'''

def nav(active,mail=1,chatdot=True):
    out=''
    for k,l in (('chat','Chat'),('mail','Mail'),('map','Map'),('radio','Radio')):
        b=''
        if k=='mail' and mail: b=f'<span class="bdg">{mail}</span>'
        if k=='chat' and chatdot: b='<span class="bdg dot"></span>'
        out+=f'<a class="{"on" if k==active else ""}"><div class="pill">{ic(k)}{b}</div>{l}</a>'
    return f'<div class="nav">{out}</div><div class="navbg"></div>'

def topbar(title,sub='',lead=None,actions='',logo=False,wave='#ff4dff'):
    if logo: l=f'<div class="logo">{mark(wave,26,"var(--on-surface)")}</div>'
    elif lead: l=f'<div class="ib">{ic(lead)}</div>'
    else: l='<div style="width:12px"></div>'
    s=f'<div class="sub">{sub}</div>' if sub else ''
    return f'<div class="tbar">{l}<div class="tt"><h1>{title}</h1>{s}</div>{actions}</div>'

def gear(tonal=False): return f'<div class="ib {"t" if tonal else ""}">{ic("gear")}</div>'

def meterbar(frac,h=10):
    segs=[(0,.66,'var(--meter-ok)'),(.66,.88,'var(--pu)'),(.88,1,'var(--tx)')]
    o=''
    for a,b,c in segs:
        e=min(frac,b)
        if e>a: o+=f'<i style="left:{a*100}%;width:{(e-a)*100}%;background:{c}"></i>'
    return f'<div class="meter" style="height:{h}px">{o}</div>'

def phone(content,active=None,theme='dark',top='',sb=None,overlay='',navbar=True,mail=1,chatdot=True):
    sb=sb if sb is not None else statusbar()
    n=nav(active,mail,chatdot) if (navbar and active) else ''
    return f'<div class="phone {theme}">{sb}{top}<div class="content">{content}</div>{n}{overlay}<div class="gest"><i></i></div></div>'

def page(body,js=''):
    return f'<!doctype html><html><head><meta charset="utf-8"><style>{CSS}</style></head><body>{body}<script>{js}</script></body></html>'

def rigstrip(kind='usb',tx=False):
    rig='Digirig' if kind=='usb' else 'VR-N76'
    pre='vhf-fast' if kind=='usb' else 'vhf-fm'
    bg='var(--tx-c)' if tx else 'var(--surface-container-high)'
    fg='var(--on-tx-c)' if tx else 'var(--on-surface)'
    ptt='<b style="color:var(--tx);font-weight:700">PTT TX</b>' if tx else '<span class="osv">PTT</span> <b style="font-weight:500">RX</b>'
    return f'''<div style="margin:0 12px 6px;height:40px;border-radius:12px;background:{bg};color:{fg};display:flex;align-items:center;gap:10px;padding:0 14px;font-size:13px;flex:none"><span class="mono" style="font-size:13px;font-weight:500">M7TJF</span><span style="display:flex;align-items:center;gap:5px"><i class="dotc" style="background:var(--ok)"></i>{rig}</span>{ptt}<span style="display:flex;align-items:center;gap:5px"><span class="osv">IN</span><span style="width:40px">{meterbar(.2 if tx else .52,6)}</span></span><span class="mono" style="margin-left:auto;font-size:12px;color:{'var(--on-tx-c)' if tx else 'var(--primary)'}">{pre}</span></div>'''
