# Замеры мест, исправленных по ревью гаража: зазоры забора, щель навеса, колесо, блок, щит, табличка, свес под
# щипцом, бочки, затенение нутра, фальцы. Метры; минус у зазора — детали заходят друг в друга.
import numpy as np
from mathutils import Vector
o = bpy.data.objects["EG_garage"]; me = o.data; mw = o.matrix_world
co = np.array([(mw @ v.co)[:] for v in me.vertices])
pm = {}
for p in me.polygons:
    pm.setdefault(me.materials[p.material_index].name, set()).update(p.vertices)


def pick(prefix, box):
    idx = sorted({i for name, s in pm.items() if name.startswith(prefix) for i in s})
    if not idx:
        return None
    c = co[np.array(idx)]
    m = np.all((c >= np.array(box[0])) & (c <= np.array(box[1])), axis=1)
    return c[m] if m.any() else None


def ext(c):
    return None if c is None else {"min": c.min(0).round(4).tolist(), "max": c.max(0).round(4).tolist()}


res = {}
# 1. Забор справа (линия x 13,5, изнутри — к −X): доски, слеги, столбы около y 5…7.
boards = pick(("garage_board", "garage_fence"), [(13.3, 5.0, 0.0), (13.6, 7.0, 2.0)])
rails = pick("garage_post", [(13.2, 5.0, 0.39), (13.6, 7.0, 0.51)])
posts = pick("garage_post", [(13.2, 4.0, 1.5), (13.6, 8.0, 1.95)])
res["fence"] = {"boards_x": [round(boards[:, 0].min(), 4), round(boards[:, 0].max(), 4)],
                "rail_x": [round(rails[:, 0].min(), 4), round(rails[:, 0].max(), 4)],
                "post_x": [round(posts[:, 0].min(), 4), round(posts[:, 0].max(), 4)]}
res["fence"]["gap_board_rail"] = round(boards[:, 0].min() - rails[:, 0].max(), 4)
res["fence"]["gap_rail_post"] = round(rails[:, 0].min() - posts[:, 0].max(), 4)
back_boards = pick(("garage_board", "garage_fence"), [(0.0, 14.9, 0.0), (2.0, 15.1, 2.0)])
back_rails = pick("garage_post", [(0.0, 14.8, 0.39), (2.0, 15.1, 0.51)])
res["fence_back"] = {"gap_board_rail": round(back_rails[:, 1].max() - back_boards[:, 1].min(), 4) * -1}
# 2. Навес с бочками: щель между левой стеной и крышей — лучи снаружи по +X под низом тёса.
dg = bpy.context.evaluated_depsgraph_get()
inv = o.matrix_world.inverted()
k = 0.45 / 3.5
lift = 0.16 * (1 + k * k) ** 0.5
through, total = 0, 0
for y in np.arange(2.45, 6.1, 0.05):
    under = 2.75 - k * (y - 2.5) + lift
    for z in np.arange(1.9, under - 0.003, 0.01):
        ok, loc, nrm, fi = o.ray_cast(inv @ Vector((-15.0, y, z)), Vector((1.0, 0.0, 0.0)))
        total += 1
        if ok and (mw @ loc).x > -12.95:
            through += 1
res["fuel_shed_side_rays"] = {"through": through, "total": total}
# 3. Колесо: низ шины, ближняя к стене точка, края по X.
wheel = np.concatenate([c for c in (pick("garage_rust", [(9.3, 1.8, -0.1), (10.7, 2.6, 1.2)]), pick("garage_log", [(9.3, 1.8, -0.1), (10.7, 2.59, 1.2)]))
                        if c is not None])
res["wheel"] = {"min_z": round(wheel[:, 2].min(), 4), "max_y": round(wheel[:, 1].max(), 4), "x": [round(wheel[:, 0].min(), 3), round(wheel[:, 0].max(), 3)],
                "wall_batten_face_y": 2.58, "corner_x": 10.6}
# 4. Блок подъёмника: верх шкива, низ балки, хомут.
sheave = pick("garage_steel", [(-0.03, 0.45, 6.9), (0.03, 0.75, 7.3)])
beam = pick("garage_log", [(-0.1, 0.4, 7.25), (0.1, 2.2, 7.55)])
strap = pick("garage_iron", [(-0.1, 0.55, 7.2), (0.1, 0.65, 7.52)])
res["pulley"] = {"sheave": ext(sheave), "beam_bottom": round(beam[:, 2].min(), 4), "strap": ext(strap)}
# 5. Пожарный щит: от инструментов до доски (лицо доски y 4,6).
for name, prefix, box in (("axe_head", "garage_steel", [(-7.35, 4.5, 1.4), (-7.05, 4.62, 1.7)]),
                          ("shovel_blade", "garage_steel", [(-7.06, 4.5, 0.78), (-6.84, 4.62, 1.11)]),
                          ("buckets", "garage_pump_red", [(-8.12, 4.2, 1.15), (-7.4, 4.62, 1.65)]),
                          ("hooks", "garage_iron", [(-8.15, 4.4, 0.75), (-6.75, 4.62, 2.1)])):
    c = pick(prefix, box)
    res["fireboard_" + name] = None if c is None else {"gap_to_board": round(4.6 - c[:, 1].max(), 4), "n": int(len(c))}
# 6. Табличка: лист и буквы.
s = bpy.data.objects["EG_garage_sign"]
sc_ = np.array([(s.matrix_world @ v.co)[:] for v in s.data.vertices])
enamel = pick("garage_enamel", [(-0.6, 1.4, 2.0), (0.6, 1.6, 2.5)])
res["sign"] = {"letters_y": [round(sc_[:, 1].min(), 5), round(sc_[:, 1].max(), 5)], "plate_y": [round(enamel[:, 1].min(), 5), round(enamel[:, 1].max(), 5)]}
res["sign"]["letter_gap_to_plate"] = round(enamel[:, 1].min() - sc_[:, 1].max(), 5)
# 7. Свес под щипцом сеновала: вершины кровли в полосе y 1,05…1,5, |x| < 2,5 ниже 5,45.
ledge = pick("garage_roof", [(-2.5, 1.04, 4.9), (2.5, 1.5, 5.45)])
res["ledge_under_gable_verts"] = 0 if ledge is None else int(len(ledge))
# 8. Бочки: лежащая у навеса, стеллаж против лаг.
lying = pick("garage_drum", [(-9.1, 0.4, -0.1), (-8.0, 1.4, 0.7)])
res["lying_drum_min_z"] = None if lying is None else round(lying[:, 2].min(), 4)
rack = pick("garage_drum", [(-12.8, 2.6, 0.0), (-8.5, 3.6, 0.2)])
res["rack_drums_min_z"] = None if rack is None else round(rack[:, 2].min(), 4)
chime = pick("garage_drum", [(-12.8, 3.48, 0.0), (-8.5, 3.55, 0.2)])
res["rack_chime_min_z_over_sleeper"] = None if chime is None else round(chime[:, 2].min(), 4)
# 9. Нутро: цвет вершин внутренней стороны задней стены ниже и выше 1,4 м.
ca = me.color_attributes[0]
lo_, hi_ = [], []
for p in me.polygons:
    n = (mw.to_3x3() @ p.normal).normalized()
    c = mw @ p.center
    if n.y < -0.9 and abs(c.y - 11.49) < 0.03 and -5.5 < c.x < 5.5:
        for li in p.loop_indices:
            z = (mw @ me.vertices[me.loops[li].vertex_index].co).z
            if abs(z - 1.4) < 0.002:
                (lo_ if (mw @ p.center).z < 1.4 else hi_).append(ca.data[li].color[0])
# у одной и той же вершины на 1,4 м: цвет со стороны грани ниже и грани выше — ступень, если разные
res["interior_back_wall_vc_at_1.4"] = {"from_face_below": round(float(np.mean(lo_)), 3), "from_face_above": round(float(np.mean(hi_)), 3)}
# 10. Фальцы у свеса: насколько выходят за край листа (свес y 1,05).
seams = pick("garage_roof", [(-6.0, 1.0, 5.0), (6.0, 1.1, 5.3)])
res["front_eave_min_y"] = None if seams is None else round(seams[:, 1].min(), 4)
result = res
