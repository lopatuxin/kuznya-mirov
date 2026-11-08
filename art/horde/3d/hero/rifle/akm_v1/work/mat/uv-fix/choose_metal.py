from pathlib import Path
import bpy
import numpy as np
BM=r"C:\Users\lopat\.codex\plugins\cache\anton-toolkit-marketplace\blender-modeling\0.2.3-codex.ha8b4c4edceb3\scripts"
exec(open(BM+'/bm.py',encoding='utf-8').read())
root=Path(r'C:\projects\kuznya-mirov\art\horde\3d\hero\rifle\akm_v1')
a=load_rgba(str(root/'akm-basecolor.png'))
for y in range(100,241,5):
    v=a[y:y+5,1100:1530,:3]
    print(y,float(v.mean()),float(np.quantile(v,.95)),flush=True)
save_rgba(a[90:250,1030:1600],str(root/'work/mat/uv-fix/metal-patch.png'))
