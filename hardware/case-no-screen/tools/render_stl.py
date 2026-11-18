import sys, struct, numpy as np
import matplotlib; matplotlib.use("Agg")
import matplotlib.pyplot as plt
from mpl_toolkits.mplot3d.art3d import Poly3DCollection
def load(p):
    f=open(p,'rb'); f.read(80); n=struct.unpack('<I',f.read(4))[0]
    a=np.frombuffer(f.read(50*n),dtype=np.dtype([('n','<3f4'),('v','<9f4'),('a','<u2')]))
    return a['v'].reshape(-1,3,3).astype(float)
for p,out in zip(sys.argv[1::2], sys.argv[2::2]):
    t=load(p); mn=t.reshape(-1,3).min(0); mx=t.reshape(-1,3).max(0)
    print(p, "extents", np.round(mn,2), np.round(mx,2), "span", np.round(mx-mn,2), "tris", len(t))
    fig=plt.figure(figsize=(16,8))
    views=[("top (x right, z up; Y is height)",(0,2),1),("front (x right, y up)",(0,1),1),("side (z right, y up)",(2,1),1)]
    for i,(title,(a,b),_) in enumerate(views,1):
        ax=fig.add_subplot(1,3,i); ax.set_title(title); ax.set_aspect('equal')
        # draw triangles as thin polygons, colored by height along the remaining axis
        c=[k for k in range(3) if k not in (a,b)][0]
        order=np.argsort(t[:,:,c].mean(1))
        from matplotlib.collections import PolyCollection
        pc=PolyCollection(t[order][:,:,[a,b]], array=t[order][:,:,c].mean(1), cmap='viridis', edgecolors='none')
        ax.add_collection(pc); ax.autoscale(); ax.grid(True, alpha=.3)
        ax.set_xticks(np.arange(np.floor(mn[a]/10)*10, mx[a]+10, 10)); ax.set_yticks(np.arange(np.floor(mn[b]/10)*10, mx[b]+10, 10))
    fig.tight_layout(); fig.savefig(out, dpi=110); print("wrote", out)
