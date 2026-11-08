import bpy, numpy as np, os, sys
d = sys.argv[sys.argv.index("--")+1]
for f in sorted(os.listdir(d)):
    if not (f.startswith("v_") and f.endswith(".png")) or f.endswith("_x.png"):
        continue
    im = bpy.data.images.load(os.path.join(d, f))
    w, h = im.size
    a = np.array(im.pixels[:], np.float32).reshape(h, w, 4)
    rgb = a[..., :3]
    lo, hi = 0.55, 0.97
    rgb = np.clip((rgb - lo) / (hi - lo), 0, 1)
    a[..., :3] = rgb
    out = bpy.data.images.new(f + "_x", w, h, alpha=True)
    out.pixels = a.ravel()
    out.filepath_raw = os.path.join(d, f[:-4] + "_x.png"); out.file_format = "PNG"; out.save()
    print(f, float(a[...,0][a[...,3]>0.5].min()), float(a[...,0][a[...,3]>0.5].mean()))
