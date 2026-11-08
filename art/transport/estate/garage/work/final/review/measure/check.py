import numpy as np
o = bpy.data.objects["EG_garage"]; me = o.data; mw = o.matrix_world
co = np.array([(mw @ v.co)[:] for v in me.vertices])
pm = {}
for p in me.polygons:
    pm.setdefault(me.materials[p.material_index].name, set()).update(p.vertices)
def ext(mat, box):
    idx = np.array(sorted(pm.get(mat, [])))
    if len(idx) == 0: return None
    c = co[idx]
    m = np.all((c >= np.array(box[0])) & (c <= np.array(box[1])), axis=1)
    c = c[m]
    if len(c) == 0: return None
    return {"min": c.min(0).round(4).tolist(), "max": c.max(0).round(4).tolist(), "n": int(len(c))}
res = {}
# right fence near y 5..7: boards (fence_*), rails/posts (post)
for k in ("garage_fence_0", "garage_fence_1", "garage_fence_2", "garage_fence_3"):
    res["fence_board_" + k] = ext(k, [(13.0, 5.0, 0.0), (14.0, 7.0, 2.0)])
res["fence_rail_low"] = ext("garage_post", [(13.0, 5.0, 0.35), (14.0, 7.0, 0.55)])
res["fence_rail_high"] = ext("garage_post", [(13.0, 5.0, 1.3), (14.0, 7.0, 1.5)])
res["back_board"] = ext("garage_fence_0", [(0, 14.0, 0), (2, 16, 2)])
res["back_rail"] = ext("garage_post", [(0.5, 14.0, 0.35), (1.5, 16, 0.55)])
# wheel (log near x 9.5..10.8, y 2.0..2.6)
res["wheel"] = ext("garage_log", [(9.4, 1.9, -0.1), (10.9, 2.65, 1.4)])
# workshop window sill (frame) and front wall
res["sill"] = ext("garage_frame", [(8.3, 2.3, 1.0), (9.9, 2.61, 1.2)])
# fire board tools: iron & log in front of board y<4.6
res["fb_iron"] = ext("garage_iron", [(-8.2, 4.4, 0.7), (-6.7, 4.61, 2.0)])
res["fb_log_front"] = ext("garage_log", [(-8.1, 4.4, 0.8), (-6.8, 4.599, 2.0)])
res["fb_board"] = ext("garage_red", [(-8.16, 4.59, 0.74), (-6.74, 4.65, 2.11)])
# fuel shed: left wall planks and roof
res["fuel_leftwall"] = ext("garage_shed_wall", [(-13.2, 2.5, 0.0), (-12.95, 6.2, 3.0)])
res["fuel_roof_left"] = ext("garage_shed_roof", [(-13.35, 2.0, 2.0), (-12.9, 6.3, 3.0)])
# rack drums lower row bottom vs sleepers
res["rack_drums"] = ext("garage_drum", [(-12.7, 2.6, 0.0), (-8.6, 3.6, 0.2)])
res["sleepers"] = ext("garage_log", [(-12.9, 2.6, 0.0), (-8.5, 3.6, 0.13)])
# pulley vs beam
res["pulley"] = ext("garage_iron", [(-0.2, 0.4, 6.9), (0.2, 0.8, 7.35)])
res["hoist_beam"] = ext("garage_log", [(-0.2, 0.4, 7.2), (0.2, 2.2, 7.6)])
# sign
s = bpy.data.objects["EG_garage_sign"]
sc_ = np.array([(s.matrix_world @ v.co)[:] for v in s.data.vertices])
res["sign_text"] = {"min": sc_.min(0).round(4).tolist(), "max": sc_.max(0).round(4).tolist()}
res["enamel"] = ext("garage_enamel", [(-0.6, 1.4, 2.0), (0.6, 1.6, 2.5)])
result = res
