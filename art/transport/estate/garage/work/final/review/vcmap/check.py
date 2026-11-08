import numpy as np, math
from mathutils import Vector
o = bpy.data.objects["EG_garage"]; me = o.data
ca = me.color_attributes[0]
dg = bpy.context.evaluated_depsgraph_get()
oe = o.evaluated_get(dg)
inv = o.matrix_world.inverted()
def face_vc(fi):
    p = me.polygons[fi]
    return float(np.mean([ca.data[li].color[0] for li in p.loop_indices]))
res = {}
for label, d in (("top", Vector((0, 0, -1))), ("game", -_dirv(CAMERA["az"], CAMERA["elev"])), ("front", Vector((0, 1, 0))),
                 ("back", Vector((0, -1, 0))), ("left", Vector((1, 0, 0))), ("right", Vector((-1, 0, 0)))):
    bad = {}
    total = 0
    for x in np.arange(-15, 16, 0.2):
        for y in np.arange(-14, 17, 0.2):
            for z in ([0] if label in ("top", "game") else np.arange(0.1, 9.5, 0.2)):
                if label in ("top", "game"):
                    start = Vector((x, y, 0.5)) - d * 60
                elif label in ("front", "back"):
                    if abs(y) > 0.01: continue
                    start = Vector((x, -d.y * 30, z))
                else:
                    if abs(x) > 0.01: continue
                    start = Vector((-d.x * 30, y, z))
                ok, loc, nrm, fi = o.ray_cast(inv @ start, d, distance=200)
                if not ok: continue
                total += 1
                if face_vc(fi) < 0.42 and me.materials[me.polygons[fi].material_index].name not in ("garage_path",):
                    m = me.materials[me.polygons[fi].material_index].name
                    key = (m, fi)
                    if key not in bad:
                        bad[key] = [round(loc.x, 2), round(loc.y, 2), round(loc.z, 2), round(face_vc(fi), 3), round(me.polygons[fi].area, 3)]
    by = {}
    for (m, fi), v in bad.items():
        by.setdefault(m, []).append(v)
    res[label] = {"hits": total, "backfaces": {m: {"faces": len(v), "area": round(sum(a[4] for a in v), 2), "sample": sorted(v, key=lambda a: -a[4])[:4]} for m, v in by.items()}}
result = res
