import numpy as np
from scipy.ndimage import gaussian_filter
from PIL import Image
rng=np.random.default_rng(81)
W,H=824,1000
h=np.zeros((H,W))
for s,a in [(90,1.0),(45,.55),(22,.28),(10,.12),(4,.05)]:
    n=gaussian_filter(rng.standard_normal((H,W)),s,mode='wrap'); n/=n.std(); h+=a*n
# ridge: add a NW-SE ridge (Mendips-ish)
yy,xx=np.mgrid[0:H,0:W]
h+=1.2*np.exp(-(((xx*0.6+yy*0.8)-620)/70.0)**2)
gy,gx=np.gradient(gaussian_filter(h,1.2))
shade=np.clip(0.5+(-gx*0.8-gy*0.6)*9,0,1)
elev=(h-h.min())/(h.max()-h.min())
base=np.array([8,12,24]); lo=np.array([14,30,42]); hi=np.array([52,68,82])
col=lo[None,None,:]*(1-elev[...,None])+hi[None,None,:]*elev[...,None]
img=col*(0.55+0.9*shade[...,None])*1.05
# valley river hint
img=np.clip(img,0,255).astype(np.uint8)
Image.fromarray(img).save('img/terrain.png')
print(img.shape)
