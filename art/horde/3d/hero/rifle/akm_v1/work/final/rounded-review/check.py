import bpy, bmesh, json, os, struct, sys
from pathlib import Path
import numpy as np
BM = r'C:\Users\lopat\.codex\plugins\cache\anton-toolkit-marketplace\blender-modeling\0.2.3-codex.ha8b4c4edceb3\scripts'
exec(open(BM + '/bm.py',encoding='utf-8').read())
ASSET=r'C:\projects\kuznya-mirov\art\horde\3d\hero\rifle\akm_v1'
OUT=ASSET+r'\work\final\rounded-review'
load_dims(ASSET)
sc=bpy.data.scenes[SCENE]
names=model_names(sc)
gates=whole_gates(sc,OUT)
obj=bpy.data.objects[names[0]]
mesh=obj.data
bm=bmesh.new();bm.from_mesh(mesh)
bad_edges=sum(not e.is_manifold for e in bm.edges)
zero_faces=sum(f.calc_area()<1e-12 for f in bm.faces)
bm.free()
uv=np.asarray([loop.uv[:] for loop in mesh.uv_layers.active.data])
raw=Path(ASSET,'akm-v1.glb').read_bytes();n=struct.unpack_from('<I',raw,12)[0]
glb=json.loads(raw[20:20+n])
r={'gates':gates,'nonmanifold_edges':bad_edges,'zero_faces':zero_faces,'uv_finite':bool(np.isfinite(uv).all()),'uv_range':[uv.min(0).tolist(),uv.max(0).tolist()], 'glb':glb_report(str(Path(ASSET,'akm-v1.glb'))),'nodes':[v.get('name') for v in glb['nodes']],'embedded_images':glb.get('images')}
Path(OUT,'independent-check.json').write_text(json.dumps(r,ensure_ascii=False,indent=2),encoding='utf-8')
print('REVIEW '+json.dumps(r));sys.stdout.flush();os._exit(0)
