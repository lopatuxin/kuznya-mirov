# Склад порта целиком: бутовый цоколь, стены из вертикальных досок с нащельниками, откатные двери фасада и к морю,
# ворота в правом торце, окошко в левом фронтоне, помост на столбиках с лестницей, навес со столбами и подкосами,
# двускатная железная крыша с фальцами на стропилах, вентиляционная труба на коньке. Строит blender-part-builder;
# общие числа (стены, помост, свесы, конёк, труба) берутся из dims.py.

import math
import random
import bmesh
from mathutils import Vector

# --- толщины и шаги, метры (сняты с видов: доска с нащельником 0,34 м, фальцы и стропила через 1,07 м)
CT = 0.045          # обшивка стены (сплошная доска под нащельниками)
BT = 0.025          # нащельник: толщина
BW = 0.09           # нащельник: ширина
BP = 0.34           # шаг досок / нащельников
ST = 0.05           # настил кровли + железо
RD = 0.18           # глубина стропила
RW = 0.08           # ширина стропила
N_RAFT = 29         # стропил на скат
N_SEAM = 28         # фальцев на скат
POST = 0.2          # сечение столбов помоста и навеса
POST_Y = -7.0       # ось столбов (наружная грань -7.1, край помоста PLATFORM_Y -7.2)
POSTS_X = (-14.85, -11.6, -8.45, -4.72, -1.76, 1.76, 4.72, 8.45, 11.6, 14.85)   # с вида спереди
CANOPY_X = (-14.85, -4.72, 4.72, 14.85)                                            # столбы до крыши
CORNER = 0.18       # угловые стойки сруба стены
PLINTH_OUT = 0.13   # лицо самых выпуклых камней цоколя от плоскости стены (виды с торцов: 4,83 против 4,7)
CORE_OUT = 0.03     # лицо раствора (ядра кладки) от плоскости стены
PLINTH_IN = 0.3     # цоколь внутрь от наружной плоскости стены
CS = 0.47           # высокий угловой камень под задними угловыми стойками, сторона (торцы и зад: 19-21 точка)

FILL_Y = -6.93      # лицо раствора каменной засыпки под помостом (за лицом столбиков -7,1)
FILL_TOP = 1.04     # верх засыпки под обвязкой помоста (вид спереди: 1,0-1,06)
FILL_X = 14.69      # засыпка по X, внутри торцевых зашивок
FILL_T = 0.35       # толщина подпорной стенки (сбоку под зашивкой виден только её торец)
DECK_X = 15.25      # настил помоста по X (вид спереди: ±15,25 на высоте настила)
# без фасок: бюджет 12 000 треугольников уходит на бут; фаска 1,5 см на брусе в игре — 0,1 точки
BEVEL = {}
SMOOTH = {"PW_plinth": 80}      # камни: подушка затеняется гладко, грани читаются скруглёнными

# двери: середина по стене, половина ширины проёма, низ, верх, высота рельса, половина рельса, высота ручек
FRONT_DOORS = [(c, 2.05, PLATFORM_Z, 3.8, 3.9, 2.48, 2.3) for c in (-8.5, 0.0, 8.5)]
BACK_DOORS = [(c, 2.37, 0.0, 3.3, 3.4, 2.9, 1.5) for c in (-7.5, 7.5)]
GATE = (-1.97, 1.73, 0.0, 3.44)        # ворота в правом торце: y от, y до, низ, верх
WINDOW = (-0.98, 0.02, 4.69, 5.75)     # окошко в левом фронтоне: y от, y до, низ, верх

E_F, E_B = EAVE_Y
R_Y, R_Z = RIDGE
SF = (R_Z - EAVE_Z) / (R_Y - E_F)      # уклон переднего ската
SB = (R_Z - EAVE_Z) / (E_B - R_Y)      # уклон заднего ската
CF = 1 / math.sqrt(1 + SF * SF)
CB = 1 / math.sqrt(1 + SB * SB)


def top_f(y):
    return EAVE_Z + (y - E_F) * SF


def top_b(y):
    return EAVE_Z + (E_B - y) * SB


def under(y, d):
    """Высота поверхности на глубине d (по нормали) под верхом кровли."""
    return top_f(y) - d / CF if y <= R_Y else top_b(y) - d / CB


RU = ST + RD        # низ стропил под верхом кровли

# --- плоскости: (u, v, w) -> мир; w наружу от наружной плоскости стены
FR = {
    "front": lambda u, v, w: (u, WALL_Y[0] - w, v),
    "back": lambda u, v, w: (u, WALL_Y[1] + w, v),
    "left": lambda u, v, w: (-WALL_X - w, u, v),
    "right": lambda u, v, w: (WALL_X + w, u, v),
    "YZ": lambda u, v, w: (w, u, v),
    "fill": lambda u, v, w: (u, FILL_Y - w, v),
}
OUT = {"front": (0, -1, 0), "back": (0, 1, 0), "left": (-1, 0, 0), "right": (1, 0, 0), "fill": (0, -1, 0)}


def area(p):
    return abs(sum(p[i][0] * p[(i + 1) % len(p)][1] - p[(i + 1) % len(p)][0] * p[i][1] for i in range(len(p)))) / 2


def clip(poly, u0=-1e9, u1=1e9, v0=-1e9, v1=1e9):
    """Выпуклый многоугольник, обрезанный прямоугольником."""
    def cut(pts, inside, inter):
        out = []
        for i in range(len(pts)):
            a, b = pts[i], pts[(i + 1) % len(pts)]
            if inside(a):
                out.append(a)
            if inside(a) != inside(b):
                out.append(inter(a, b))
        return out

    def xu(c):
        return lambda a, b: (c, a[1] + (b[1] - a[1]) * (c - a[0]) / (b[0] - a[0]))

    def xv(c):
        return lambda a, b: (a[0] + (b[0] - a[0]) * (c - a[1]) / (b[1] - a[1]), c)
    p = list(poly)
    for inside, inter in ((lambda q: q[0] >= u0, xu(u0)), (lambda q: q[0] <= u1, xu(u1)),
                          (lambda q: q[1] >= v0, xv(v0)), (lambda q: q[1] <= v1, xv(v1))):
        if not p:
            return []
        p = cut(p, inside, inter)
    q = []
    for pt in p:
        if not q or abs(pt[0] - q[-1][0]) > 1e-6 or abs(pt[1] - q[-1][1]) > 1e-6:
            q.append(pt)
    if len(q) > 1 and abs(q[0][0] - q[-1][0]) < 1e-6 and abs(q[0][1] - q[-1][1]) < 1e-6:
        q.pop()
    return q if len(q) >= 3 and area(q) > 1e-4 else []


def slab(bm, plane, poly, w0, w1, mi=0):
    """Призма из выпуклого многоугольника (u, v) плоскости `plane` толщиной w0..w1."""
    F = FR[plane]
    lo = [bm.verts.new(F(u, v, w0)) for u, v in poly]
    hi = [bm.verts.new(F(u, v, w1)) for u, v in poly]
    fs = [bm.faces.new(lo), bm.faces.new(hi)]
    n = len(poly)
    for i in range(n):
        j = (i + 1) % n
        fs.append(bm.faces.new((lo[i], lo[j], hi[j], hi[i])))
    for f in fs:
        f.material_index = mi
    bmesh.ops.recalc_face_normals(bm, faces=fs)
    return fs


def wbox(bm, plane, u0, u1, v0, v1, w0, w1, mi=0):
    return slab(bm, plane, [(u0, v0), (u1, v0), (u1, v1), (u0, v1)], w0, w1, mi)


def abox(bm, x0, x1, y0, y1, z0, z1, mi=0):
    return box(bm, ((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2), (x1 - x0, y1 - y0, z1 - z0), mi)


def wquad(wall, u0, u1, v0, v1, w):
    """Четырёхугольник в плоскости стены с нормалью наружу (для boards)."""
    F = FR[wall]
    q = [Vector(F(u0, v0, w)), Vector(F(u1, v0, w)), Vector(F(u1, v1, w)), Vector(F(u0, v1, w))]
    if (q[1] - q[0]).cross(q[3] - q[0]).dot(Vector(OUT[wall])) < 0:
        q = [Vector(F(u1, v0, w)), Vector(F(u0, v0, w)), Vector(F(u0, v1, w)), Vector(F(u1, v1, w))]
    return q


def tri_ngons(bm):
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])


# --- контуры стен
def long_poly(wall, w):
    """Продольная стена между угловыми стойками; верх — низ стропил над наружной гранью."""
    y = WALL_Y[0] - w if wall == "front" else WALL_Y[1] + w
    u = WALL_X - CORNER
    return [(-u, BASE_Z), (u, BASE_Z), (u, under(y, RU)), (-u, under(y, RU))]


def end_poly():
    """Торцевая стена с фронтоном между угловыми стойками; верх — низ крайних стропил."""
    u = WALL_Y[1] - CORNER
    return [(-u, BASE_Z), (u, BASE_Z), (u, under(u, RU)), (R_Y, R_Z - max(RU / CB, RU / CF)),
            (-u, under(-u, RU))]


def openings(wall):
    if wall == "front":
        return [(c - h, c + h, z0, z1) for c, h, z0, z1, *_ in FRONT_DOORS]
    if wall == "back":
        return [(c - h, c + h, z0, z1) for c, h, z0, z1, *_ in BACK_DOORS]
    if wall == "right":
        return [GATE]
    return [WINDOW]


def skin(bm, wall, poly, holes, w0, w1):
    """Сплошная обшивка одной стены одним куском: сетка по краям проёмов, ячейки проёмов выкинуты,
    общие вершины сварены, толщина w0..w1 с торцами только по наружному краю и по проёмам."""
    us = sorted({round(p[0], 6) for p in poly} | {round(x, 6) for h in holes for x in h[:2]})
    vlo, vhi = min(p[1] for p in poly), max(p[1] for p in poly)
    vs = sorted({vlo, vhi} | {round(z, 6) for h in holes for z in h[2:] if vlo < z < vhi})
    key, pts, cells = {}, [], []

    def vid(q):
        k = (round(q[0], 4), round(q[1], 4))
        if k not in key:
            key[k] = len(pts)
            pts.append(q)
        return key[k]
    for i in range(len(us) - 1):
        for j in range(len(vs) - 1):
            mu, mv = (us[i] + us[i + 1]) / 2, (vs[j] + vs[j + 1]) / 2
            if any(h[0] < mu < h[1] and h[2] < mv < h[3] for h in holes):
                continue
            c = clip(poly, us[i], us[i + 1], vs[j], vs[j + 1])
            if c:
                ids = []
                for q in c:
                    k = vid(q)
                    if not ids or ids[-1] != k:
                        ids.append(k)
                if len(ids) > 2 and ids[0] == ids[-1]:
                    ids.pop()
                if len(ids) >= 3:
                    cells.append(ids)
    # вершины соседей, лежащие на ребре ячейки, вставляются в ячейку: сетка без Т-стыков
    full = []
    for ids in cells:
        out = []
        for n, a in enumerate(ids):
            b = ids[(n + 1) % len(ids)]
            out.append(a)
            pa, pb = pts[a], pts[b]
            d = (pb[0] - pa[0], pb[1] - pa[1])
            L2 = d[0] ** 2 + d[1] ** 2
            on = []
            for k, q in enumerate(pts):
                if k in ids:
                    continue
                t = ((q[0] - pa[0]) * d[0] + (q[1] - pa[1]) * d[1]) / L2
                if 1e-6 < t < 1 - 1e-6 and abs((q[0] - pa[0]) * d[1] - (q[1] - pa[1]) * d[0]) / math.sqrt(L2) < 1e-4:
                    on.append((t, k))
            out += [k for _, k in sorted(on)]
        full.append(out)
    F = FR[wall]
    lo = [bm.verts.new(F(u, v, w0)) for u, v in pts]
    hi = [bm.verts.new(F(u, v, w1)) for u, v in pts]
    fs = []
    edges = {}
    for ids in full:
        fs.append(bm.faces.new([hi[k] for k in ids]))
        fs.append(bm.faces.new([lo[k] for k in reversed(ids)]))
        for n, a in enumerate(ids):
            b = ids[(n + 1) % len(ids)]
            edges[(min(a, b), max(a, b))] = edges.get((min(a, b), max(a, b)), 0) + 1
    for (a, b), cnt in edges.items():
        if cnt == 1:
            fs.append(bm.faces.new((lo[a], lo[b], hi[b], hi[a])))
    bmesh.ops.recalc_face_normals(bm, faces=fs)


def walls(bm_core, bm_bat, rnd):
    for wall in ("front", "back", "left", "right"):
        long = wall in ("front", "back")
        core = long_poly(wall, -BT) if long else end_poly()
        bat = long_poly(wall, 0.0) if long else end_poly()
        if wall == "front":                     # под помостом стена стоит на каменной засыпке
            core = [(u, FILL_TOP if v == BASE_Z else v) for u, v in core]
            bat = [(u, PLATFORM_Z if v == BASE_Z else v) for u, v in bat]
        holes = openings(wall)
        skin(bm_core, wall, core, holes, -BT - CT, -BT)
        us = [p[0] for p in bat]
        u0, u1 = min(us), max(us)
        k = 1
        while u0 + k * BP < u1 - 0.06:
            uc = u0 + k * BP
            a, b = uc - BW / 2, uc + BW / 2
            hit = [o for o in holes if o[0] - 0.02 < b and o[1] + 0.02 > a]
            parts = [clip(bat, a, b, v1=hit[0][2] - 0.01), clip(bat, a, b, v0=hit[0][3] + 0.01)] if hit else [clip(bat, a, b)]
            for p in parts:
                if p and max(q[1] for q in p) - min(q[1] for q in p) > 0.08:
                    slab(bm_bat, wall, p, -BT, rnd.uniform(0.0, 0.008))
            k += 1


def frame(bm, bm_dark):
    """Угловые стойки, пояс торцов, окно, косяки и перемычка ворот, тёмные проёмы."""
    for sx in (-1, 1):
        for sy in (-1, 1):
            x0, x1 = sorted((sx * (WALL_X - CORNER), sx * (WALL_X + 0.01)))
            y0, y1 = sorted((sy * (WALL_Y[1] - CORNER), sy * (WALL_Y[1] + 0.01)))
            abox(bm, x0, x1, y0, y1, BASE_Z, under(sy * (WALL_Y[1] + 0.01), RU))
    u = WALL_Y[1] - CORNER
    ub = E_B - (4.35 + RU / CB - EAVE_Z) / SB - 0.05     # пояс кончается под задним скатом
    for wall in ("left", "right"):
        wbox(bm, wall, -u, ub, 4.17, 4.35, 0.0, 0.03)
    # подкосы от угловых стоек под задний свес (видны на торцах)
    for sx in (-1, 1):
        x = sx * (WALL_X - 0.09)
        yb = WALL_Y[1] + 0.01
        beam(bm, (x, yb - 0.02, 3.05), (x, yb + 0.33, under(yb + 0.33, RU) + 0.02), 0.08, 0.08, side=(1, 0, 0))
    # окошко в левом фронтоне: наличник, переплёт, стекло (слот 1 тёмной части), за ним темнота чердака вглубь
    a, b, z0, z1 = WINDOW
    t = 0.09
    wbox(bm, "left", a - t, a, z0, z1 + t, 0.0, 0.04)
    wbox(bm, "left", b, b + t, z0, z1 + t, 0.0, 0.04)
    wbox(bm, "left", a, b, z1, z1 + t, 0.0, 0.04)
    wbox(bm, "left", a - t - 0.03, b + t + 0.03, z0 - t - 0.04, z0, 0.0, 0.07)
    wbox(bm, "left", (a + b) / 2 - 0.025, (a + b) / 2 + 0.025, z0, z1, -0.035, -0.01)
    wbox(bm, "left", a, b, (z0 + z1) / 2 - 0.025, (z0 + z1) / 2 + 0.025, -0.03, -0.015)
    wbox(bm_dark, "left", a, b, z0, z1, -0.042, -0.038, 1)
    # тёмный короб чердака за стеклом: стенки от обшивки вглубь на 0,6 м и задник — глубина, а не плашка
    for u0, u1, v0, v1 in ((a - 0.05, a, z0 - 0.05, z1 + 0.05), (b, b + 0.05, z0 - 0.05, z1 + 0.05),
                           (a, b, z0 - 0.05, z0), (a, b, z1, z1 + 0.05)):
        wbox(bm_dark, "left", u0, u1, v0, v1, -0.62, -0.01)
    wbox(bm_dark, "left", a - 0.05, b + 0.05, z0 - 0.05, z1 + 0.05, -0.64, -0.6)
    # ворота в правом торце: косяки на цоколе, перемычка
    a, b, z0, z1 = GATE
    wbox(bm, "right", a - 0.12, a, BASE_Z, z1, 0.0, 0.05)
    wbox(bm, "right", b, b + 0.12, BASE_Z, z1, 0.0, 0.05)
    wbox(bm, "right", a - 0.18, b + 0.18, z1, z1 + 0.22, 0.0, 0.06)
    # темнота за откатными дверями (в плоскости обшивки)
    for wall, doors in (("front", FRONT_DOORS), ("back", BACK_DOORS)):
        for c, h, z0, z1, *_ in doors:
            wbox(bm_dark, wall, c - h, c + h, z0, z1, -BT - CT + 0.005, -BT - 0.005)


def slide_door(bm, wall, c, h, z0, z1, rz, rh, hz):
    """Откатная дверь: две створки из досок на раме с раскосом, рельс на брусе, подвесы, ручки, уголки."""
    F = FR[wall]
    out = Vector(OUT[wall])
    for s in (-1, 1):
        a, b = sorted((c + s * 0.005, c + s * (h + 0.05)))
        boards(bm, wquad(wall, a, b, z0, z1, 0.01), 6, 0.008, 0.04, along="v", mat_index=0)
        wbox(bm, wall, a, b, z0 + 0.08, z0 + 0.24, 0.05, 0.08, 1)
        wbox(bm, wall, a, b, z1 - 0.24, z1 - 0.08, 0.05, 0.08, 1)
        edge, mid = (a, b) if s < 0 else (b, a)
        p0 = Vector(F(edge - s * 0.12, z0 + 0.24, 0.0635))
        p1 = Vector(F(mid + s * 0.12, z1 - 0.24, 0.0635))
        beam(bm, p0 + out * 0.0035, p1 + out * 0.0035, 0.02, 0.12, side=out, mat_index=1)
        for uh in (a + 0.45, b - 0.45):         # подвесы на рельс
            wbox(bm, wall, uh - 0.03, uh + 0.03, z1 - 0.3, rz + 0.02, 0.08, 0.095, 2)
        for zz in (z0 + 0.1, z1 - 0.1):         # железные уголки на углах створки
            ua, ub = sorted((edge, edge - s * 0.35))
            wbox(bm, wall, ua, ub, zz - 0.035, zz + 0.035, 0.08, 0.09, 2)
        uh = c + s * 0.15                        # ручка
        wbox(bm, wall, uh - 0.02, uh + 0.02, hz - 0.2, hz + 0.2, 0.05, 0.09, 2)
    wbox(bm, wall, c - rh - 0.05, c + rh + 0.05, rz - 0.08, rz + 0.1, 0.0, 0.07, 1)   # брус под рельс
    wbox(bm, wall, c - rh, c + rh, rz - 0.04, rz + 0.04, 0.07, 0.10, 2)               # рельс


def gate(bm):
    """Распашные ворота в правом торце: створки в проёме, петли-полосы, ручки."""
    a, b, z0, z1 = GATE
    c = (a + b) / 2
    for s in (-1, 1):
        u0, u1 = sorted((c + s * 0.005, b if s > 0 else a))
        boards(bm, wquad("right", u0, u1, z0, z1, -0.05), 6, 0.008, 0.045, along="v", mat_index=0)
        hinge = u1 if s > 0 else u0
        for zz in (z0 + 0.45, z1 - 0.45):
            ua, ub = sorted((hinge, hinge - s * 0.85))
            wbox(bm, "right", ua, ub, zz - 0.04, zz + 0.04, -0.005, 0.006, 2)
        wbox(bm, "right", c + s * 0.12 - 0.02, c + s * 0.12 + 0.02, 1.4, 1.8, -0.005, 0.04, 2)


def stone(bm, plane, u0, u1, v0, v1, wb, wf, rnd, nch=2):
    """Бутовый камень: контур на растворе со скруглёнными (срезанными) углами — `nch` случайных углов,
    лицо-подушка утоплено от контура на глубину выступа; гладкое затенение даёт округлость. Тыл закрыт
    и утоплен в раствор."""
    F = FR[plane]
    m = min(u1 - u0, v1 - v0)

    def j():
        return rnd.uniform(-0.012, 0.012)
    cut = set(rnd.sample(range(4), nch))
    corners = [(u0, v0, 1, 1), (u1, v0, -1, 1), (u1, v1, -1, -1), (u0, v1, 1, -1)]
    R, first, last = [], [], []
    for k, (u, v, su, sv) in enumerate(corners):
        if k in cut:
            c = rnd.uniform(0.18, 0.34) * m
            # по обходу: сначала точка на входящей стороне, потом на выходящей
            pair = [(u, v + sv * c), (u + su * c, v)] if k in (0, 2) else [(u + su * c, v), (u, v + sv * c)]
        else:
            pair = [(u, v)]
        idx = []
        for pu, pv in pair:
            R.append(bm.verts.new(F(pu + j(), max(pv + j(), 0.002), wb)))
            idx.append(len(R) - 1)
        first.append(idx[0])
        last.append(idx[-1])
    ins = min(0.36 * m, rnd.uniform(0.9, 1.4) * (wf - wb))
    face = [(u0 + ins, v0 + ins), (u1 - ins, v0 + ins), (u1 - ins, v1 - ins), (u0 + ins, v1 - ins)]
    Q = [bm.verts.new(F(u + j(), v + j(), wf + rnd.uniform(-0.01, 0.01))) for u, v in face]
    fs = [bm.faces.new(Q)]
    for k in range(4):
        n = (k + 1) % 4
        fs.append(bm.faces.new((R[last[k]], R[first[n]], Q[n], Q[k])))
        if first[k] != last[k]:
            fs.append(bm.faces.new((R[first[k]], R[last[k]], Q[k])))
    fs.append(bm.faces.new(list(reversed(R))))
    want = Vector(F(0, 0, 1)) - Vector(F(0, 0, 0))
    fs[0].normal_update()
    if fs[0].normal.dot(want) < 0:
        bmesh.ops.reverse_faces(bm, faces=fs)


def rubble(bm, plane, u0, u1, top, courses, lmin, lmax, wb, wf, rnd, tall=0.0, nch=2):
    """Ряд бута от u0 до u1 высотой 0..top: камни разной длины и высоты, каждый следующий ряд ложится
    на верх камней под ним, высокий камень иногда занимает два ряда."""
    placed = []                                   # (a, b, top)
    g = 0.022                                     # шов
    for k in range(courses):
        nominal = top * (k + 1) / courses
        u = u0
        while u < u1 - 0.12:
            block = [p for p in placed if p[0] - g < u + 0.05 and p[1] > u and p[2] > nominal - 0.14]
            if block:
                u = max(p[1] for p in block) + g
                continue
            L = rnd.uniform(lmin, lmax)
            e = u + L
            nxt = [p[0] for p in placed if p[0] > u + 0.05 and p[2] > nominal - 0.14]
            if nxt and e > min(nxt) - g:
                e = min(nxt) - g
            if u1 - e < lmin * 0.8:
                e = u1
            below = [p[2] for p in placed if p[0] < e and p[1] > u]
            v0 = (max(below) + g) if below else 0.01
            if k == courses - 1:
                v1 = top - rnd.uniform(0.005, 0.03)
            elif k == 0 and courses > 1 and rnd.random() < tall:
                v1 = top * (k + 2) / courses + rnd.uniform(-0.05, 0.0)
            else:
                v1 = nominal + rnd.uniform(-0.06, 0.06)
            if e - u > 0.18 and v1 - v0 > 0.14:
                stone(bm, plane, u, e, v0, v1, wb, rnd.uniform(*wf), rnd, nch)
                placed.append((u, e, v1))
            u = e + g


def corner_stone(bm, x0, x1, y0, y1, z1, rnd):
    """Высокий угловой камень под угловой стойкой: восьмигранник в плане, верх чуть сужен."""
    c = 0.07
    rim = [(x0 + c, y0), (x1 - c, y0), (x1, y0 + c), (x1, y1 - c), (x1 - c, y1), (x0 + c, y1), (x0, y1 - c),
           (x0, y0 + c)]
    mx, my = (x0 + x1) / 2, (y0 + y1) / 2
    lo = [bm.verts.new((x, y, 0.0)) for x, y in rim]
    mid = [bm.verts.new((x + rnd.uniform(-0.01, 0.01), y + rnd.uniform(-0.01, 0.01), z1 - 0.06)) for x, y in rim]
    hi = [bm.verts.new((mx + (x - mx) * 0.8, my + (y - my) * 0.8, z1)) for x, y in rim]
    fs = [bm.faces.new(list(reversed(lo))), bm.faces.new(hi)]
    for a, b in ((lo, mid), (mid, hi)):
        for i in range(8):
            k = (i + 1) % 8
            fs.append(bm.faces.new((a[i], a[k], b[k], b[i])))
    bmesh.ops.recalc_face_normals(bm, faces=fs)


def plinth(bm, core, rnd):
    """Бутовый цоколь на тёмном растворе: ядро кладки, камни выступают из шва; под помостом —
    каменная засыпка до обвязки, лицом между столбиками; высокие камни под задними углами и посередине."""
    yb = WALL_Y[1]
    x_in = WALL_X - CS + 0.04                      # ядро заходит в угловой камень
    co = CORE_OUT
    # ядро: задняя стена между дверями, торцы, засыпка под помостом
    for a, b in ((-x_in, -9.92), (-5.08, 5.08), (9.92, x_in)):
        abox(core, a, b, yb - PLINTH_IN, yb + co, 0.0, BASE_Z)
    for s in (-1, 1):
        segs = [(-4.82, yb - CS + 0.04)] if s < 0 else [(-4.82, GATE[0] - 0.01), (GATE[1] + 0.01, yb - CS + 0.04)]
        for a, b in segs:
            x0, x1 = sorted((s * (WALL_X - PLINTH_IN), s * (WALL_X + co)))
            abox(core, x0, x1, a, b, 0.0, BASE_Z)
    abox(core, -FILL_X, FILL_X, FILL_Y, FILL_Y + FILL_T, 0.0, FILL_TOP)          # подпорная стенка под краем помоста
    abox(core, -FILL_X, FILL_X, WALL_Y[0] - 0.05, WALL_Y[0] + PLINTH_IN, 0.0, FILL_TOP)   # цоколь фасада до верха засыпки
    # высокие камни: задние углы и середина задней стены
    for s in (-1, 1):
        x0, x1 = sorted((s * (WALL_X + PLINTH_OUT), s * (WALL_X + PLINTH_OUT - CS)))
        corner_stone(bm, x0, x1, yb + PLINTH_OUT - CS, yb + PLINTH_OUT, BASE_Z, rnd)
    corner_stone(bm, -0.26, 0.26, yb - 0.1, yb + PLINTH_OUT, BASE_Z - 0.02, rnd)
    # бут по видимым сторонам
    wb, wf = co - 0.015, (co + 0.05, PLINTH_OUT)
    e = WALL_X + PLINTH_OUT - CS - 0.03
    runs = [("back", -e, -9.94), ("back", -5.06, -0.29), ("back", 0.29, 5.06), ("back", 9.94, e),
            ("left", -4.78, yb + PLINTH_OUT - CS - 0.03),
            ("right", -4.78, GATE[0] - 0.14), ("right", GATE[1] + 0.14, yb + PLINTH_OUT - CS - 0.03)]
    for plane, a, b in runs:
        rubble(bm, plane, a, b, BASE_Z, 2, 0.32, 0.72, wb, wf, rnd, tall=0.1)
    xs = sorted(POSTS_X)
    for i in range(1, len(xs) - 1):                # пролёт у лестницы закрыт лестницей: без камня
        rubble(bm, "fill", xs[i] + POST / 2 + 0.02, xs[i + 1] - POST / 2 - 0.02, FILL_TOP - 0.01, 3, 0.36, 0.72,
               -0.015, (0.035, 0.09), rnd, nch=1)


def platform(bm, rnd):
    """Помост: настил вдоль стены на лагах, лаги на обвязке и прогоне у стены, зашивка торцов."""
    y0, y1 = PLATFORM_Y, WALL_Y[0]
    n, g = 11, 0.012
    w = (y1 - y0 - g * (n - 1)) / n
    breaks = ([-8.45, -1.76, 4.72, 11.6], [-11.6, -4.72, 1.76, 8.45])
    for i in range(n):
        yb = y1 - i * (w + g)
        xs = [-DECK_X] + breaks[i % 2] + [DECK_X]
        for k in range(len(xs) - 1):
            xa = xs[k] + (0.005 if k else 0.0)
            xb = xs[k + 1] - (0.005 if k < len(xs) - 2 else 0.0)
            abox(bm, xa, xb, yb - w, yb, PLATFORM_Z - 0.045 - rnd.uniform(0, 0.004), PLATFORM_Z, 0)
    jt = PLATFORM_Z - 0.045                    # верх лаг = низ настила
    for x in POSTS_X:
        abox(bm, x - 0.05, x + 0.05, POST_Y + 0.05, y1 - 0.1, jt - 0.2, jt, 1)
    abox(bm, -WALL_X + CORNER + 0.03, WALL_X - CORNER - 0.03, y1 - 0.1, y1, jt - 0.2, jt, 1)   # прогон по стене
    abox(bm, -15.2, 15.2, POST_Y - POST / 2 - 0.02, POST_Y + POST / 2 - 0.02, 1.05, jt - 0.005, 1)  # обвязка
    for s in (-1, 1):                          # зашивка торцов под помостом
        x = s * (abs(POSTS_X[0]) + POST / 2)
        boards(bm, [Vector((x, POST_Y, 0.15)), Vector((x, y1 - 0.1, 0.15)), Vector((x, y1 - 0.1, 1.105)),
                    Vector((x, POST_Y, 1.105))] if s > 0 else
               [Vector((x, y1 - 0.1, 0.15)), Vector((x, POST_Y, 0.15)), Vector((x, POST_Y, 1.105)),
                Vector((x, y1 - 0.1, 1.105))], 3, 0.004, 0.035, along="u", mat_index=0)


def posts_canopy(bm):
    """Столбы помоста и навеса, балка навеса под стропилами, ригели к стене, подкосы."""
    yo = POST_Y - POST / 2
    plate_top = under(yo - 0.02, RU)
    plate_bot = plate_top - 0.2
    for x in POSTS_X:
        top = plate_top - 0.05 if x in CANOPY_X else PLATFORM_Z - 0.045 - 0.025
        abox(bm, x - POST / 2, x + POST / 2, yo, POST_Y + POST / 2, 0.0, top)
    abox(bm, -15.15, 15.15, yo - 0.02, POST_Y + POST / 2 + 0.02, plate_bot, plate_top)
    tb = plate_bot - 0.18
    for x in CANOPY_X:
        abox(bm, x - 0.075, x + 0.075, POST_Y - 0.05, WALL_Y[0], tb, plate_bot)          # ригель к стене
        beam(bm, (x, POST_Y + POST / 2 - 0.02, tb - 0.6), (x, POST_Y + POST / 2 + 0.6, tb + 0.02), 0.1, 0.1,
             side=(1, 0, 0))
        beam(bm, (x, WALL_Y[0] + 0.02, tb - 0.6), (x, WALL_Y[0] - 0.6, tb + 0.02), 0.1, 0.1, side=(1, 0, 0))
        s = 1 if x in (CANOPY_X[0], CANOPY_X[2]) else -1      # подкос вдоль фасада: как на виде спереди
        beam(bm, (x + s * (POST / 2 - 0.02), POST_Y, plate_bot - 0.75), (x + s * (POST / 2 + 0.75), POST_Y,
             plate_bot + 0.02), 0.1, 0.1, side=(0, 1, 0))
    for s in (-1, 1):                                       # кронштейн под торцевой свес крыши
        x = s * (abs(CANOPY_X[0]) + POST / 2)
        beam(bm, (x - s * 0.02, POST_Y, plate_bot - 0.4), (x + s * 0.5, POST_Y, under(POST_Y, ST) + 0.02),
             0.08, 0.08, side=(0, 1, 0))


def stairs(bm):
    """Лестница на помост: две тетивы, зашитые досками до земли, четыре ступени."""
    x0, x1, yout = STEPS
    rise, n = PLATFORM_Z / 5, 4
    run = (PLATFORM_Y - yout) / n
    k = rise / run

    def zs(y):
        return PLATFORM_Z - k * (PLATFORM_Y - y) - 0.06
    yA = PLATFORM_Y + (PLATFORM_Z - 0.06 - 1.29) / k
    yB = PLATFORM_Y - (PLATFORM_Z - 0.06) / k
    poly = [(POST_Y - 0.1, 1.29), (yA, 1.29), (yB, 0.0), (POST_Y - 0.1, 0.0)]   # зашитые тетивы до земли, как на торце
    for xa in (x0 - 0.02, x1 - 0.06):
        slab(bm, "YZ", poly, xa, xa + 0.08, 0)
    for i in range(1, n + 1):
        zt = PLATFORM_Z - rise * i
        abox(bm, x0 + 0.02, x1 - 0.02, PLATFORM_Y - run * i - 0.02, PLATFORM_Y - run * (i - 1), zt - 0.045, zt, 1)


def seam(bm, p0, p1, n):
    """Фальц: гребень треугольного сечения на железе (основание 4 см, высота 3,5 см), низ чуть утоплен."""
    x = Vector((1, 0, 0))
    vs = []
    for p in (p0, p1):
        vs.append([bm.verts.new(p - x * 0.02 - n * 0.005), bm.verts.new(p + x * 0.02 - n * 0.005),
                   bm.verts.new(p + n * 0.035)])
    a, b = vs
    fs = [bm.faces.new(a), bm.faces.new(list(reversed(b)))]
    for i in range(3):
        k = (i + 1) % 3
        fs.append(bm.faces.new((a[k], a[i], b[i], b[k])))
    bmesh.ops.recalc_face_normals(bm, faces=fs)


def roof(bm, bm_r):
    """Кровля: настил с железом по скатам, фальцы, конёк; стропила, лобовые и ветровые доски."""
    xr = ROOF_X - 0.03
    df, db = ST / CF, ST / CB
    front = [(E_F, top_f(E_F)), (R_Y, R_Z), (R_Y, R_Z - df), (E_F, top_f(E_F) - df)]
    back = [(R_Y, R_Z), (E_B, top_b(E_B)), (E_B, top_b(E_B) - db), (R_Y, R_Z - db)]
    for poly in (front, back):
        for f in slab(bm, "YZ", poly, -xr, xr, 0):
            if f.normal.z < -0.5:
                f.material_index = 1          # снизу — доски настила
    nf = Vector((0, -SF, 1)) * CF
    nb = Vector((0, SB, 1)) * CB
    step = 2 * xr / (N_SEAM + 1)
    for i in range(1, N_SEAM + 1):
        x = -xr + i * step
        for ya, yb, top, n in ((E_F + 0.01, R_Y - 0.2, top_f, nf), (R_Y + 0.2, E_B - 0.01, top_b, nb)):
            seam(bm, Vector((x, ya, top(ya))), Vector((x, yb, top(yb))), n)
    cap = [(R_Y - 0.2, top_f(R_Y - 0.2)), (R_Y + 0.2, top_b(R_Y + 0.2)), (R_Y + 0.2, top_b(R_Y + 0.2) + 0.03),
           (R_Y, R_Z + 0.05), (R_Y - 0.2, top_f(R_Y - 0.2) + 0.03)]
    slab(bm, "YZ", cap, -xr - 0.02, xr + 0.02, 0)
    # стропила с отвесными запилами на свесах и у конька
    rf = [(E_F, top_f(E_F) - df), (R_Y, R_Z - df), (R_Y, R_Z - RU / CF), (E_F, top_f(E_F) - RU / CF)]
    rb = [(R_Y, R_Z - db), (E_B, top_b(E_B) - db), (E_B, top_b(E_B) - RU / CB), (R_Y, R_Z - RU / CB)]
    for i in range(N_RAFT):
        x = -(WALL_X - 0.05) + i * 2 * (WALL_X - 0.05) / (N_RAFT - 1)
        slab(bm_r, "YZ", rf, x - RW / 2, x + RW / 2)
        slab(bm_r, "YZ", rb, x - RW / 2, x + RW / 2)
    abox(bm_r, -xr, xr, E_F - 0.03, E_F, top_f(E_F) - 0.25, top_f(E_F))           # лобовая доска
    abox(bm_r, -xr, xr, E_B, E_B + 0.03, top_b(E_B) - 0.25, top_b(E_B))
    bf = [(E_F - 0.03, top_f(E_F - 0.03) + 0.02), (R_Y, R_Z + 0.02), (R_Y, R_Z - 0.25),
          (E_F - 0.03, top_f(E_F - 0.03) - 0.25)]
    bb = [(R_Y, R_Z + 0.02), (E_B + 0.03, top_b(E_B + 0.03) + 0.02), (E_B + 0.03, top_b(E_B + 0.03) - 0.25),
          (R_Y, R_Z - 0.25)]
    for s in (-1, 1):                                                               # ветровые доски
        for poly in (bf, bb):
            slab(bm_r, "YZ", poly, *sorted((s * xr, s * ROOF_X)))


def vent(bm):
    vx = VENT[0]
    cylinder(bm, (vx, R_Y, R_Z - 0.25), (vx, R_Y, VENT[1] - 0.15), 0.17, segments=10)
    cylinder(bm, (vx, R_Y, VENT[1] - 0.17), (vx, R_Y, VENT[1]), 0.28, segments=10)


def build(sc, col):
    rnd = random.Random(7)
    made = []

    def put(name, bm, roles):
        tri_ngons(bm)
        made.append(obj_from_bm(name, bm, col, [M(r) for r in roles]))

    bm, core = bmesh.new(), bmesh.new()
    plinth(bm, core, rnd)
    put("PW_plinth", bm, ["stone"])
    put("PW_mortar", core, ["dark"])
    core, bat = bmesh.new(), bmesh.new()
    walls(core, bat, rnd)
    put("PW_wall_boards", core, ["board"])
    put("PW_battens", bat, ["board"])
    bm, dark = bmesh.new(), bmesh.new()
    frame(bm, dark)
    put("PW_frame", bm, ["timber"])
    put("PW_dark", dark, ["dark", "glass"])
    bm = bmesh.new()
    for wall, doors in (("front", FRONT_DOORS), ("back", BACK_DOORS)):
        for d in doors:
            slide_door(bm, wall, *d)
    gate(bm)
    put("PW_doors", bm, ["board", "timber", "iron"])
    bm = bmesh.new()
    platform(bm, rnd)
    put("PW_platform", bm, ["board", "timber"])
    bm = bmesh.new()
    posts_canopy(bm)
    put("PW_posts", bm, ["timber"])
    bm = bmesh.new()
    stairs(bm)
    put("PW_stairs", bm, ["timber", "board"])
    bm, raf = bmesh.new(), bmesh.new()
    roof(bm, raf)
    put("PW_roof", bm, ["roof", "board"])
    put("PW_rafters", raf, ["timber"])
    bm = bmesh.new()
    vent(bm)
    put("PW_vent", bm, ["roof"])
    for o in made:
        if o.name in BEVEL:
            add_bevel(o, BEVEL[o.name][0], BEVEL[o.name][1], 'ANGLE', 30)
        finish_shading(o, SMOOTH.get(o.name, 30))
