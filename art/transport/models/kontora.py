# Дом купеческой усадьбы начальной деревушки, 1890 год; в 1930-м в нём строительная контора артели игрока. По образцу
# art/transport/estate/concept.png, но кирпич голый, без побелки. Двухэтажный дом 13 × 11 м у самой улицы: низ из
# старого красного кирпича на цоколе потемнее, лучковые окна под перемычками из кирпича на ребро, с распахнутыми
# железными ставнями; верх — сруб «в обло» из круглых выветренных брёвен с выпусками на углах, окна с белыми резными
# наличниками, как у изб деревни; между этажами кирпичный поясок под жестяным отливом. Вальмовая крыша из старого
# железа с фальцами, выцветшая красно-коричневая, как у каретника-гаража, две кирпичные трубы, водосточные трубы по
# углам. К улице посередине — крыльцо на точёных столбах под железным козырьком, двустворчатая филёнчатая дверь со
# стеклом в своде; на щипце козырька синяя эмалевая табличка «КОНТОРА». Сзади — дверь во двор с каменными ступенями.
# Справа от дома, за проходом, — навес с бочками гаража (`garage.py`), сам каретник-гараж — в глубине участка;
# дощатый забор — слева от дома и сзади, до забора гаража.
# Земли, травы и деревьев в модели нет: их рисует движок; тропы с прозрачным краем — от улицы к крыльцу и от задней
# двери к навесу гаража. Улица — со стороны −Y Blender, начало — середина дома по ширине на краю участка у улицы
# (край — в 9,3 м от оси главной улицы, как у дворов), единица — метр; верх земли — ноль; размер настоящий.
# Материалы — карты ambientCG и Poly Haven из art/transport/textures; затенение запечено в цвет вершин.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/kontora.py [-- <папка>]
import math
import os
import random
import sys

from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402
from garage import chimney, faded, plank_fence  # noqa: E402
from yard import Face, Homestead, bake_occlusion, cabin, tones, window  # noqa: E402

OUT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "models"))

GROUND = 0.0  # земля участка — земля карты движка; ниже неё в модели ничего нет

# Кирпичный низ: наружные грани стен, толщина стены в два с половиной кирпича, верх цоколя — он же пол первого
# этажа, верх кирпича — под срубом.
X0, X1, Y0, Y1 = -6.5, 6.5, 0.5, 11.5
WALL = 0.64
PLINTH = 0.6
TOP = 3.6
# Окна низа — середины по сторонам, ширина, низ проёма, пята и подъём лучкового свода; двери — середина, ширина,
# пята и подъём свода.
LOW = {"front": [-5.0, -3.0, 3.0, 5.0], "back": [-4.6, -1.6, 1.2], "left": [3.2, 8.2], "right": [3.2, 8.2]}
WIN, SILL, SPRING, RISE = 0.9, 1.25, 2.45, 0.26
DOOR, DOOR_W, DOOR_SPRING, DOOR_RISE = 0.0, 1.4, 2.6, 0.3
BACK_DOOR, BACK_W, BACK_SPRING = 3.6, 1.1, 2.45
VOUSSOIR = 0.24  # длина кирпича перемычки по радиусу
SHUTTER = 0.035  # толщина ставни

# Сруб верха: оси стен — на 0,2 м внутри от граней кирпича, радиус бревна, шаг венца, выпуск за угол, число венцов
# боковых стен; окна — середины по сторонам, ширина, низ и верх проёма.
LX0, LX1, LY0, LY1 = X0 + 0.2, X1 - 0.2, Y0 + 0.2, Y1 - 0.2
LOG_R, STEP, OUTLET, COURSES = 0.14, 0.235, 0.3, 13
UP = {"front": [-4.4, -2.2, 0.0, 2.2, 4.4], "back": [-3.6, 0.0, 3.6], "left": [2.4, 6.0, 9.6], "right": [2.4, 6.0, 9.6]}
UP_W, UP_Z0, UP_Z1 = 0.8, TOP + 0.85, TOP + 2.05

# Крыша: свес от граней кирпича, подъём ската на метр, толщина кровли, шаг фальцев.
OVER, SLOPE, ROOF, PITCH = 0.75, 0.58, 0.06, 0.62

# Крыльцо: половина ширины площадки, её вылет от стены, места столбов; козырёк — свес за столбы, высота свеса и конька.
PORCH_HALF, PORCH_D, POST_U, POST_D = 1.45, 1.9, 1.3, 1.75
CANOPY_D, CANOPY_EAVE, CANOPY_RIDGE = 2.0, 3.1, 3.55

# Забор: слева от дома и сзади — до забора гаража (его задний забор в этой же линии).
FENCE_X, FENCE_Y0, FENCE_BACK_Y, FENCE_BACK_X1 = X0 - 1.5, 1.0, 26.8, 9.0

# Тропы: ломаная, полуширина — от улицы к ступеням крыльца, от задней двери к навесу с бочками гаража.
STREET = -5.6
PATHS = [
    ([(DOOR, STREET), (DOOR + 0.1, -2.6), (DOOR, Y0 - PORCH_D - 0.4)], 0.75),
    ([(BACK_DOOR, Y1 + 0.7), (5.2, 12.9), (7.4, 13.5), (9.6, 13.6)], 0.45),
]


def materials(m):
    return {
        # Светлота частей после карты и подкраски, примерно: кровля 0,45, кирпич 0,3, брёвна 0,33, наличники 0,85.
        "wall": m.tex("wall", "Bricks085", 2.0, tint="#e0c4b8"),
        "brick": m.tex("brick", "Bricks085", 2.0, tint="#b8a096"),
        "voussoir": m.mat("voussoir", "#8a4634", 0.85),
        "voussoir_dark": m.mat("voussoir_dark", "#76392b", 0.85),
        "logs": tones(m, "log", "Wood033", 1.6, ("#8f7c66", "#83715d", "#9a8771", "#7a6a59")),
        "log_end": m.mat("log_end", "#8a7660", 0.9),
        "boards": tones(m, "board", "Wood033", 1.6, ("#b8b0a4", "#a9a195", "#c3bbaf", "#a19a8f"), swap=True),
        "fence": tones(m, "fence", "Wood033", 1.6, ("#b8b0a4", "#a9a195", "#c3bbaf", "#a19a8f"), swap=True),
        "post": m.tex("post", "Wood059", 1.4, tint="#8b8079"),
        "plank": m.tex("plank", "Planks012", 2.1, tint="#c8c2ba"),
        "paint": m.tex("paint", "PaintedWood009C", 1.2, tint="#fbf9f4"),
        "door": m.tex("door", "PaintedWood009C", 1.2, tint="#8c6a4c", swap=True),
        "shutter": m.tex("shutter", "green_metal_rust", 1.6, tint="#8a9a8e"),
        "roof": m.tex("roof", "MetalPlates011", 2.0, tint="#b8a8a8"),
        "stone": m.tex("stone", "white_sandstone_bricks_03", 2.0, tint="#a39e95"),
        "path": faded(m, "path", "Ground103", 2.5, "#e6dccd"),
        "glass": m.mat("glass", "#222b2b", 0.1, 0.3),
        "soot": m.mat("soot", "#0e0d0c", 0.95),
        "tin": m.mat("tin", "#55595b", 0.7, 0.2),
        "iron": m.mat("iron", "#2c2d2f", 0.55, 0.5),
        "enamel": m.mat("enamel", "#1c3c8c", 0.25),
        "enamel_white": m.mat("enamel_white", "#f2efe6", 0.3),
    }


def faces():
    """Наружные плоскости кирпичных стен и пролёт каждой вдоль себя: торцевые стены — между уличной и задней."""
    return {
        "front": (Face("front", Y0), X0, X1),
        "back": (Face("back", Y1), X0, X1),
        "left": (Face("left", X0), Y0 + WALL, Y1 - WALL),
        "right": (Face("right", X1), Y0 + WALL, Y1 - WALL),
    }


def arch(u0, u1, zs, rise, n=12):
    """Лучковый свод проёма от `u0` до `u1` с пятой на `zs` и подъёмом `rise`: точки (u, z) слева направо, середина,
    радиус и высота середины круга."""
    half, um = (u1 - u0) / 2, (u0 + u1) / 2
    r = (half * half + rise * rise) / (2 * rise)
    zc = zs + rise - r
    a0 = math.atan2(zs - zc, half)
    pts = [(um + r * math.cos(a), zc + r * math.sin(a)) for a in (math.pi - a0 - (math.pi - 2 * a0) * i / n for i in range(n + 1))]
    return pts, um, r, zc


def holes():
    """Проёмы кирпичного низа по сторонам: (u0, u1, низ, пята, подъём, вид)."""
    out = {side: [(u - WIN / 2, u + WIN / 2, SILL, SPRING, RISE, "window") for u in places] for side, places in LOW.items()}
    out["front"].append((DOOR - DOOR_W / 2, DOOR + DOOR_W / 2, PLINTH, DOOR_SPRING, DOOR_RISE, "door"))
    out["back"].append((BACK_DOOR - BACK_W / 2, BACK_DOOR + BACK_W / 2, PLINTH, BACK_SPRING, RISE, "back_door"))
    return out


def spandrel(m, face, u0, u1, zs, rise, ztop, mi):
    """Кирпич над сводом проёма: от свода до `ztop`, во всю толщину стены."""
    pts, _, _, _ = arch(u0, u1, zs, rise)
    rows = [[m.bm.verts.new(face.at(u, z if low else ztop, d)) for u, z in pts] for d in (0.0, -WALL) for low in (True, False)]
    front_low, front_top, back_low, back_top = rows
    for i in range(len(pts) - 1):
        j = i + 1
        for quad in ((front_low[i], front_low[j], front_top[j], front_top[i]), (back_low[i], back_top[i], back_top[j], back_low[j]),
                     (front_low[i], back_low[i], back_low[j], front_low[j]), (front_top[i], front_top[j], back_top[j], back_top[i])):
            m.bm.faces.new(quad).material_index = mi
    for k in (0, -1):
        m.bm.faces.new((front_low[k], front_top[k], back_top[k], back_low[k])).material_index = mi


def voussoirs(m, mt, face, u0, u1, zs, rise, rnd):
    """Перемычка над сводом — кирпичи на ребро торцами по радиусу, с швами, выступает из стены на 2 см."""
    pts, um, r, zc = arch(u0, u1, zs, rise)
    left = math.atan2(pts[0][1] - zc, pts[0][0] - um)
    right = math.atan2(pts[-1][1] - zc, pts[-1][0] - um)
    count = max(3, round(r * (left - right) / 0.078))
    step, joint = (left - right) / count, 0.011 / r
    for i in range(count):
        a0, a1 = left - i * step - joint / 2, left - (i + 1) * step + joint / 2
        outline = [(um + rr * math.cos(a), zc + rr * math.sin(a)) for rr, a in ((r + 0.004, a0), (r + 0.004, a1), (r + VOUSSOIR, a1), (r + VOUSSOIR, a0))]
        face.shape(m, outline, 0.0, 0.02, rnd.choice((mt["voussoir"], mt["voussoir_dark"])))


def window_low(m, mt, face, u0, u1, zb, zs, rise):
    """Окно низа в глубине проёма: стекло под сводом, белая рама с переплётом и дугой по своду, жестяной отлив;
    по сторонам — распахнутые железные ставни с двумя полосами."""
    pts, um, r, zc = arch(u0, u1, zs, rise)
    depth = -0.16
    face.shape(m, [(u0, zb), (u1, zb)] + list(reversed(pts)), depth - 0.01, depth, mt["glass"])
    paint = mt["paint"]
    for a0, a1, b0, b1 in ((u0, u0 + 0.07, zb, zs), (u1 - 0.07, u1, zb, zs), (u0, u1, zb, zb + 0.07), (u0, u1, zs - 0.035, zs + 0.035),
                           (um - 0.03, um + 0.03, zb, zs + rise - 0.05)):
        face.box(m, a0, a1, b0, b1, depth, depth + 0.05, paint)
    normal = tuple(face.at(0.0, 0.0, 1.0) - face.at(0.0, 0.0, 0.0))
    inner = r - 0.035
    angles = [math.atan2(z - zc, u - um) for u, z in pts]
    for a, b in zip(angles, angles[1:]):
        m.beam(face.at(um + inner * math.cos(a), zc + inner * math.sin(a), depth + 0.025),
               face.at(um + inner * math.cos(b), zc + inner * math.sin(b), depth + 0.025), 0.07, 0.05, paint, up=normal)
    face.box(m, u0 - 0.06, u1 + 0.06, zb - 0.05, zb, depth, 0.09, mt["tin"])
    half = (u1 - u0) / 2
    for s0, s1 in ((u0 - 0.03 - half, u0 - 0.03), (u1 + 0.03, u1 + 0.03 + half)):
        face.box(m, s0, s1, zb - 0.02, zs, 0.005, 0.005 + SHUTTER, mt["shutter"])
        for z in (zb + 0.2, zs - 0.26):
            face.box(m, s0 + 0.02, s1 - 0.02, z, z + 0.06, 0.005 + SHUTTER, 0.015 + SHUTTER, mt["iron"])


def door_leaves(m, mt, face, u0, u1, zb, zs, rise, double):
    """Филёнчатая дверь в глубине проёма — в две створки или одна — с рамой и стеклом в своде над поперечиной."""
    pts, um, _, _ = arch(u0, u1, zs, rise)
    depth = -0.2
    face.shape(m, pts, depth - 0.01, depth, mt["glass"])
    paint, door = mt["paint"], mt["door"]
    face.box(m, u0, u1, zs - 0.05, zs + 0.03, depth, depth + 0.06, paint)
    face.box(m, um - 0.025, um + 0.025, zs, zs + rise - 0.04, depth, depth + 0.04, paint)
    leaves = ((u0 + 0.02, um - 0.005), (um + 0.005, u1 - 0.02)) if double else ((u0 + 0.02, u1 - 0.02),)
    for a, b in leaves:
        face.box(m, a, b, zb, zs - 0.05, depth, depth + 0.05, door)
        for c0, c1 in ((zb + 0.15, zb + 0.8), (zb + 0.95, zs - 0.2)):
            face.box(m, a + 0.09, b - 0.09, c0, c1, depth + 0.05, depth + 0.07, door)
    handles = (um - 0.09, um + 0.07) if double else (u1 - 0.17,)
    for u in handles:
        face.box(m, u, u + 0.02, zb + 0.95, zb + 1.1, depth + 0.05, depth + 0.1, mt["iron"])


def lower(m, mt, rnd):
    """Кирпичный низ: стены с проёмами — куски по сторонам проёмов, под окнами и над сводами, — цоколь потемнее на
    3,5 см наружу, поясок под срубом с жестяным отливом; в проёмах окна и двери, над сводами перемычки."""
    for side, (face, a0, a1) in faces().items():
        openings = holes()[side]
        cuts = sorted({a0, a1} | {u for h in openings for u in h[:2]})
        for s0, s1 in zip(cuts, cuts[1:]):
            hole = next((h for h in openings if h[0] - 1e-6 <= s0 and s1 <= h[1] + 1e-6), None)
            if hole is None:
                face.box(m, s0, s1, GROUND, TOP, -WALL, 0.0, mt["wall"])
                continue
            u0, u1, zb, zs, rise, _ = hole
            if zb > GROUND:
                face.box(m, s0, s1, GROUND, zb, -WALL, 0.0, mt["wall"])
            spandrel(m, face, u0, u1, zs, rise, TOP, mt["wall"])
        ends = 0.035 if side in ("front", "back") else 0.0
        b0, b1 = (X0, X1) if side in ("front", "back") else (Y0, Y1)
        face.box(m, b0 - ends, b1 + ends, GROUND, PLINTH, 0.0, 0.035, mt["brick"])
        face.box(m, b0 - 0.07 * (ends > 0), b1 + 0.07 * (ends > 0), TOP - 0.34, TOP - 0.1, 0.0, 0.07, mt["brick"])
        face.box(m, b0 - 0.1 * (ends > 0), b1 + 0.1 * (ends > 0), TOP - 0.1, TOP - 0.06, -0.06, 0.1, mt["tin"])
        for u0, u1, zb, zs, rise, kind in openings:
            voussoirs(m, mt, face, u0, u1, zs, rise, rnd)
            if kind == "window":
                window_low(m, mt, face, u0, u1, zb, zs, rise)
            else:
                door_leaves(m, mt, face, u0, u1, zb, zs, rise, kind == "door")


def upper(m, mt, rnd):
    """Сруб второго этажа на кирпичном низе с окнами в белых резных наличниках. Возвращает высоту верха сруба."""
    openings = {side: [(u - UP_W / 2, u + UP_W / 2, UP_Z0, UP_Z1) for u in places] for side, places in UP.items()}
    top = cabin(m, mt, (LX0, LX1, LY0, LY1), LOG_R, STEP, COURSES, TOP, OUTLET, openings, mt["logs"], rnd)
    walls = {"front": Face("front", LY0), "back": Face("back", LY1), "left": Face("left", LX0), "right": Face("right", LX1)}
    for side, places in openings.items():
        for u0, u1, z0, z1 in places:
            window(m, mt, walls[side], LOG_R, u0, u1, z0, z1)
    return top


def roof(m, mt, top):
    """Вальмовая крыша из железа над срубом: скаты подняты над верхним венцом, чтобы под ними прошли выпуски брёвен
    на углах; фальцы через `PITCH` от свеса до конька и до рёбер вальм, рёбра и конёк — накладками; две кирпичные
    трубы у конька. Возвращает высоту свеса."""
    ex0, ey0, ex1, ey1 = X0 - OVER, Y0 - OVER, X1 + OVER, Y1 + OVER
    ze = top + 0.25 - (OVER + 0.2) * SLOPE
    inset = (ey1 - ey0) / 2
    ym, rx0, rx1 = (ey0 + ey1) / 2, ex0 + inset, ex1 - inset
    zr = ze + inset * SLOPE
    mi = mt["roof"]
    m.plate([(ex0, ey0, ze), (ex1, ey0, ze), (rx1, ym, zr), (rx0, ym, zr)], ROOF, mi)
    m.plate([(ex1, ey1, ze), (ex0, ey1, ze), (rx0, ym, zr), (rx1, ym, zr)], ROOF, mi)
    m.plate([(ex0, ey1, ze), (ex0, ey0, ze), (rx0, ym, zr)], ROOF, mi)
    m.plate([(ex1, ey0, ze), (ex1, ey1, ze), (rx1, ym, zr)], ROOF, mi)
    nf, nb = Vector((0.0, -SLOPE, 1.0)).normalized(), Vector((0.0, SLOPE, 1.0)).normalized()
    nl, nr = Vector((-SLOPE, 0.0, 1.0)).normalized(), Vector((SLOPE, 0.0, 1.0)).normalized()
    count = round((ex1 - ex0 - 0.3) / PITCH)
    for i in range(count + 1):
        x = ex0 + 0.15 + (ex1 - ex0 - 0.3) * i / count
        rise = min(x - ex0, ex1 - x, inset)  # на сколько метров по плану фальц уходит от свеса к ребру или коньку
        if rise < 0.2:
            continue
        z = ze + rise * SLOPE
        m.beam(Vector((x, ey0, ze)) + nf * 0.025, Vector((x, ey0 + rise, z)) + nf * 0.025, 0.05, 0.06, mi, up=tuple(nf))
        m.beam(Vector((x, ey1, ze)) + nb * 0.025, Vector((x, ey1 - rise, z)) + nb * 0.025, 0.05, 0.06, mi, up=tuple(nb))
    count = round((ey1 - ey0 - 0.3) / PITCH)
    for i in range(count + 1):
        y = ey0 + 0.15 + (ey1 - ey0 - 0.3) * i / count
        rise = min(y - ey0, ey1 - y)
        if rise < 0.2:
            continue
        z = ze + rise * SLOPE
        m.beam(Vector((ex0, y, ze)) + nl * 0.025, Vector((ex0 + rise, y, z)) + nl * 0.025, 0.05, 0.06, mi, up=tuple(nl))
        m.beam(Vector((ex1, y, ze)) + nr * 0.025, Vector((ex1 - rise, y, z)) + nr * 0.025, 0.05, 0.06, mi, up=tuple(nr))
    for (cx, cy), (rx, ry) in (((ex0, ey0), (rx0, ym)), ((ex1, ey0), (rx1, ym)), ((ex0, ey1), (rx0, ym)), ((ex1, ey1), (rx1, ym))):
        m.beam((cx, cy, ze + 0.03), (rx, ry, zr + 0.03), 0.18, 0.06, mi)
    m.beam((rx0, ym, zr + 0.03), (rx1, ym, zr + 0.03), 0.22, 0.07, mi)
    for cx, cy in ((-1.7, ym + 0.9), (1.7, ym - 0.9)):
        chimney(m, cx - 0.33, cy - 0.33, cx + 0.33, cy + 0.33, ze, zr + 0.85, mt["brick"], mt["soot"], wall=0.15, cap=0.12, over=0.07)
    return ze


def downpipes(m, mt, ze):
    """Водосточные трубы из того же старого железа, что крыша, по углам уличной и задней стен: от свеса колено к
    стене, труба вниз, внизу колено от стены."""
    for side in ("front", "back"):
        face = faces()[side][0]
        for u in (X0 + 0.5, X1 - 0.5):
            face.box(m, u - 0.06, u + 0.06, 0.12, ze - 0.2, 0.1, 0.22, mt["roof"])
            face.box(m, u - 0.14, u + 0.14, ze - 0.32, ze - 0.12, 0.1, OVER - 0.05, mt["roof"])
            face.box(m, u - 0.06, u + 0.06, 0.02, 0.14, 0.1, 0.42, mt["roof"])


def porch(m, mt):
    """Крыльцо к улице: дощатая площадка на уровне пола, обшитая снизу, три ступени, два точёных столба, перила с
    балясинами по бокам; козырёк из железа щипцом к улице, щипец зашит досками, по краям белые причелины, на щипце
    синяя эмалевая табличка — буквы ставит `build`."""
    face, _, _ = faces()["front"]
    boards = mt["boards"]
    face.box(m, -PORCH_HALF, PORCH_HALF, PLINTH - 0.07, PLINTH, 0.0, PORCH_D, boards[0])
    face.box(m, -PORCH_HALF + 0.03, PORCH_HALF - 0.03, GROUND, PLINTH - 0.07, 0.0, PORCH_D - 0.04, mt["plank"])
    for z, d0, d1 in ((0.4, PORCH_D, PORCH_D + 0.3), (0.2, PORCH_D + 0.3, PORCH_D + 0.6)):
        face.box(m, -0.95, 0.95, GROUND, z, d0, d1, boards[1])
    profile = [(0.0, 0.0), (0.0, 0.085), (0.16, 0.085), (0.2, 0.06), (0.55, 0.055), (0.6, 0.075), (0.68, 0.055), (1.55, 0.05),
               (1.62, 0.072), (1.7, 0.05), (2.3, 0.05), (2.36, 0.08), (CANOPY_EAVE - 0.1 - PLINTH, 0.08), (CANOPY_EAVE - 0.1 - PLINTH, 0.0)]
    for u in (-POST_U, POST_U):
        base = face.at(u, PLINTH, POST_D)
        m.lathe(base, (0.0, 0.0, 1.0), profile, boards[2], 16)
        for z in (PLINTH + 0.1, PLINTH + 0.88):
            m.beam(face.at(u, z, POST_D), face.at(u, z, 0.0), 0.06, 0.06, boards[3])
        d = 0.15
        while d < POST_D - 0.08:
            face.box(m, u - 0.02, u + 0.02, PLINTH + 0.1, PLINTH + 0.88, d - 0.02, d + 0.02, boards[3])
            d += 0.16
        rail_end = 0.95 if u > 0 else -0.95
        m.beam(face.at(u, PLINTH + 0.88, POST_D), face.at(rail_end, PLINTH + 0.88, POST_D), 0.06, 0.06, boards[3])
    face.box(m, -PORCH_HALF - 0.05, PORCH_HALF + 0.05, CANOPY_EAVE - 0.1, CANOPY_EAVE, POST_D - 0.06, POST_D + 0.06, boards[2])
    half = PORCH_HALF + 0.2
    for sign in (-1, 1):
        m.plate([face.at(sign * half, CANOPY_EAVE, CANOPY_D), face.at(sign * half, CANOPY_EAVE, 0.0),
                 face.at(0.0, CANOPY_RIDGE, 0.0), face.at(0.0, CANOPY_RIDGE, CANOPY_D)], 0.04, mt["roof"])
        normal = tuple(face.at(0.0, 0.0, 1.0) - face.at(0.0, 0.0, 0.0))
        m.beam(face.at(sign * half, CANOPY_EAVE - 0.08, CANOPY_D + 0.02), face.at(0.0, CANOPY_RIDGE - 0.08, CANOPY_D + 0.02), 0.16, 0.03,
               mt["paint"], up=normal)
    apex = CANOPY_RIDGE - 0.09
    face.shape(m, [(-PORCH_HALF, CANOPY_EAVE), (PORCH_HALF, CANOPY_EAVE), (0.0, apex)], CANOPY_D - 0.1, CANOPY_D - 0.05, boards[0])
    face.box(m, -0.35, 0.35, CANOPY_EAVE + 0.02, CANOPY_EAVE + 0.22, CANOPY_D - 0.05, CANOPY_D - 0.03, mt["enamel"])
    for a0, a1, b0, b1 in ((-0.32, 0.32, 0.04, 0.055), (-0.32, 0.32, 0.185, 0.2), (-0.32, -0.305, 0.04, 0.2), (0.305, 0.32, 0.04, 0.2)):
        face.box(m, a0, a1, CANOPY_EAVE + b0, CANOPY_EAVE + b1, CANOPY_D - 0.03, CANOPY_D - 0.025, mt["enamel_white"])


def back_steps(m, mt):
    """Две каменные ступени у задней двери."""
    face, _, _ = faces()["back"]
    for z, d1 in ((0.3, 0.9), (PLINTH, 0.5)):
        face.box(m, BACK_DOOR - 0.75, BACK_DOOR + 0.75, GROUND, z, 0.0, d1, mt["stone"])


def fences(m, mt):
    rnd = random.Random(23)
    plank_fence(m, (FENCE_X, FENCE_BACK_Y), (FENCE_X, FENCE_Y0), 1.8, mt["fence"], mt["post"], rnd)
    plank_fence(m, (FENCE_BACK_X1, FENCE_BACK_Y), (FENCE_X, FENCE_BACK_Y), 1.8, mt["fence"], mt["post"], rnd)


def build(out=OUT):
    lib.reset()
    rnd = random.Random(1890)
    m = Homestead("kontora")
    mt = materials(m)
    lower(m, mt, rnd)
    top = upper(m, mt, rnd)
    ze = roof(m, mt, top)
    downpipes(m, mt, ze)
    porch(m, mt)
    back_steps(m, mt)
    fences(m, mt)
    m.project()
    m.grid()
    for i, (line, half) in enumerate(PATHS):
        m.path(line, half, mt["path"], 80 + i)
    obj = m.finish()
    bake_occlusion(obj, mt["path"])
    lib.text(obj, "КОНТОРА", 0.13, (0.0, Y0 - CANOPY_D + 0.02, CANOPY_EAVE + 0.12), 0, "#f2efe6", depth=0.006)
    pts = [obj.matrix_world @ Vector(c) for c in obj.bound_box]
    print("kontora size", *(round(max(p[i] for p in pts) - min(p[i] for p in pts), 2) for i in range(3)), "faces", len(obj.data.polygons))
    os.makedirs(out, exist_ok=True)
    lib.export(os.path.join(out, "kontora.glb"), vertex_color="ACTIVE")
    print("written kontora")


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    build(args[0] if args else OUT)


if __name__ == "__main__":
    main()
