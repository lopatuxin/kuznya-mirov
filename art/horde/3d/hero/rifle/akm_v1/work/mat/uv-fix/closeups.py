import hashlib
import json
from pathlib import Path

import bpy
from mathutils import Vector

BM = r"C:\Users\lopat\.codex\plugins\cache\anton-toolkit-marketplace\blender-modeling\0.2.3-codex.ha8b4c4edceb3\scripts"
exec(open(BM + "/bm.py", encoding="utf-8").read())
ROOT = Path(r"C:\projects\kuznya-mirov\art\horde\3d\hero\rifle\akm_v1")
OUT = ROOT / "work" / "mat" / "uv-fix"
load_dims(str(ROOT))
sc = bpy.data.scenes[SCENE]
bpy.context.window.scene = sc
obj = bpy.data.objects[PREFIX + "_envelope"]
exec((ROOT / "materials.py").read_text(encoding="utf-8"), globals())
result = apply(sc, [obj.name])
for name, px, py, az, el, span in (("roof-closeup", 1325, 150, 30, 65, .30),
                                   ("magazine-closeup", 1070, 480, 10, 15, .19),
                                   ("magazine-heel-closeup", 960, 650, 10, -10, .14)):
    center = Vector((0, (px - ANCHOR_PX[0]) / PPM, (ANCHOR_PX[1] - py) / PPM))
    cam = aim(sc, [obj], _dirv(az, el), (1200, 1200), lens=0, center=center)
    cam.data.ortho_scale = span
    render_objs(sc, [obj], cam, (1200,1200), str(OUT / (name + ".png")), mode="albedo")
result["geometry_hash_after"] = hashlib.sha256(json.dumps([tuple(v.co) for v in obj.data.vertices]).encode()).hexdigest()
result["uv_min"] = min(c for value in obj.data.uv_layers.active.data for c in value.uv)
result["uv_max"] = max(c for value in obj.data.uv_layers.active.data for c in value.uv)
result["uv_loops"] = len(obj.data.loops)
uv_area=0.0
world_area=0.0
for polygon in obj.data.polygons:
    a,b,c=[obj.data.uv_layers.active.data[index].uv for index in polygon.loop_indices]
    uv_area+=abs((b.x-a.x)*(c.y-a.y)-(b.y-a.y)*(c.x-a.x))/2
    world_area+=polygon.area
result['uv_fill']=uv_area
result['average_texels_per_m']=4096*(uv_area/world_area)**.5
result['rgba8_with_mips_mib']=4096*4096*4*4/3/1024/1024
result['color_attributes']=[attribute.name for attribute in obj.data.color_attributes]
(OUT / "validation.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
print(json.dumps(result), flush=True)
