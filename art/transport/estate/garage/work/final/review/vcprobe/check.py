import numpy as np
o = bpy.data.objects["EG_garage"]; me = o.data; mw = o.matrix_world
ca = me.color_attributes[0]
out = []
def probe(label, test):
    rows = []
    for p in me.polygons:
        n = (mw.to_3x3() @ p.normal).normalized()
        c = mw @ p.center
        if test(n, c):
            for li in p.loop_indices:
                v = mw @ me.vertices[me.loops[li].vertex_index].co
                rows.append((round(v.x,2), round(v.y,2), round(v.z,2), round(ca.data[li].color[0],3), me.materials[p.material_index].name, round(c.z,2)))
    return {"label": label, "rows": sorted(rows, key=lambda r: (r[0], r[2]))[:80]}
# interior back wall, faces facing -Y near y=11.49, x in -4..-2
out.append(probe("back_inner", lambda n, c: n.y < -0.9 and abs(c.y - 11.49) < 0.03 and -4.5 < c.x < -2.0))
# outer facade pier left, faces facing -Y at y=1.5, x in -6.5..-5.2
out.append(probe("facade_pier", lambda n, c: n.y < -0.9 and abs(c.y - 1.5) < 0.01 and -6.5 < c.x < -5.2 and c.z < 3))
result = out
