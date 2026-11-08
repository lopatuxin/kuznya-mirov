import hashlib
import json
from pathlib import Path

import bpy

root=Path(r"C:\projects\kuznya-mirov\art\horde\3d\hero\rifle\akm_v1\work\mat\uv-fix")
mesh=bpy.data.objects['HordeAKMv1_envelope'].data
layer=mesh.uv_layers['UVMap']
uv=[[float(c) for c in item.uv] for item in layer.data]
digest=hashlib.sha256(json.dumps([tuple(v.co) for v in mesh.vertices]).encode()).hexdigest()
(root/'uv-map.json').write_text(json.dumps({'geometry_hash':digest,'uv':uv},separators=(',',':')),encoding='utf-8')
print(min(c for item in uv for c in item), max(c for item in uv for c in item), flush=True)
