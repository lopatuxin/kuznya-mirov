# Первый гараж игрока со всем двором: бывший каретный сарай купеческой усадьбы, переделанный под полуторку
# в 1930 году. Сарай из старого кирпича в облупившейся побелке под ржавой железной крышей с фальцами, щипец
# сеновала с балкой и крюком над воротами, две лучковые арки с кирпичными перемычками и зелёными дощатыми
# воротами — левые распахнуты, за ними верстак и смотровая яма; на столбе между арками синяя эмалевая табличка
# «ГАРАЖ». Справа дощатая мастерская с жестяной трубой и тележным колесом у стены, слева навес с бочками бензина
# и пожарный щит с ящиком песка, коновязь. Забора нет: двор открыт на улицу.
# Двор устроен под проезд полуторки игры (8,5 × 3,3 м, в 1,6 раза крупнее настоящей): от улицы к каждой арке
# и к навесу ведёт свой подъезд с колеями; ручная колонка со стеклянным мерником стоит на островке между
# подъездами к аркам — машина заправляется рядом с ней перед въездом. Земля — сеткой: неровный край сходит
# к земле карты, в колеях грязь и лужи, где не ездят — трава. Образец вида — art/transport/garage/concept.png.
# Размер настоящий; арки — 4 м в ширину. Улица — со стороны −Y Blender, начало — середина сарая по ширине,
# единица — метр. Материалы — карты ambientCG и Poly Haven из art/transport/textures в настоящем размере, как у
# хлебозавода; затенение запечено в цвет вершин.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/garage.py [-- <папка>]
import math
import os
import random
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402
from bakery import GRIME, GROUND, Plant, noise  # noqa: E402
from gaz_aa import glass  # noqa: E402
from gazel import smoothstep  # noqa: E402

OUT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "models"))

# Двор: прямоугольник со скруглёнными углами и неровным краем; за краем земля за `EDGE` м сходит к земле карты.
YARD = (-15.0, -11.8, 14.5, 15.0)
EDGE = 1.6
CELL = 0.3  # шаг сетки земли: колеи уже метра, и затенению в вершинах нужны частые вершины

# Каретный сарай: коробка стен, двускатная крыша с коньком вдоль X, щипец сеновала посередине фасада.
X0, X1, YF, YB = -6.5, 6.5, 1.5, 12.0
WALL = 0.51  # в два кирпича
EAVE = 5.4  # с запасом над сводами: камера игры смотрит сверху, и свес крыши закрывает 0,6 м стены под собой
SLOPE = 0.6  # подъём ската на метр
YM = (YF + YB) / 2
RIDGE = EAVE + (YM - YF) * SLOPE
OVER = 0.45  # свес крыши за стену
RAKE = 0.35  # свес над торцами
EAVE_EDGE = EAVE - OVER * SLOPE
ROOF = 0.08  # толщина кровли
SINK = 0.07  # верх щипцов ниже кровли — чтобы не сливался с ней
GATES = (-3.2, 3.2)  # середины арок: левая распахнута, правая закрыта
GATE_W, SPRING, RISE = 4.0, 3.0, 0.8  # ширина арки, пята свода, подъём свода
VOUSSOIR = 0.25  # длина кирпича арки по радиусу
DORMER = 2.6  # половина ширины щипца сеновала
DORMER_SLOPE = 1.1
DORMER_TOP = EAVE + DORMER * DORMER_SLOPE
DORMER_EAVE = DORMER + OVER * SLOPE / DORMER_SLOPE  # где скат щипца сходится со свесом крыши
DORMER_END = YF + DORMER * DORMER_SLOPE / SLOPE  # где конёк щипца упирается в скат крыши
CEILING = 4.15  # низ пола сеновала
FLOOR = GROUND + 0.03  # пол сарая выше кочек земли двора, что идёт под ним

# Мастерская справа от сарая: стены по высоте `shed_top`, крыша скатом вправо.
WX0, WX1, WY0, WY1 = X1, 10.6, 2.6, 11.0
BOARD = 0.12  # толщина дощатой стены

# Навес с бочками слева от сарая, открыт к улице; колонка — на островке между подъездами к аркам.
FX0, FX1, FY0, FY1 = -13.0, -8.4, 2.5, 6.0
PUMP = (0.0, -2.5)
DRUM_R, DRUM_H = 0.29, 0.44  # бочка на 200 л: 0,58 × 0,88 м

# Подъезды: с улицы к левой и правой арке и к навесу; начинаются за краем двора. Вдоль осей — две колеи по колее
# полуторки игры (2,6 м), в колеях земля просела, посередине протёрта; на концах подъезда колея за `TAPER` м
# сходит на нет.
WEAR = [
    [(-14.9, -13.05), (-12.0, -12.3), (-7.0, -11.0), (-4.2, -8.5), (-3.3, -5.0), (-3.2, -1.0), (-3.2, 2.5)],
    [(13.88, -13.13), (11.0, -12.3), (6.5, -11.0), (4.1, -8.5), (3.3, -5.0), (3.2, -1.0), (3.2, 2.5)],
    [(-16.88, -13.63), (-14.5, -11.8), (-11.5, -9.5), (-10.8, -6.0), (-10.7, 1.8)],
]
TRACK = 1.28  # половина колеи
TAPER = 1.5
# Где земля голая: под навесом, у колонки, у щита, в сараях. Трава и кочки растут вне этих мест и вне колей.
BARE = [(FX0 - 0.3, FY0 - 0.5, FX1 + 0.3, FY1 + 0.3), (PUMP[0] - 1.0, PUMP[1] - 1.6, PUMP[0] + 1.0, PUMP[1] + 1.6),
        (-8.3, 3.6, -6.6, 4.8), (X0, YF, X1, YB), (WX0, WY0, WX1, WY1)]
# Лужи в колеях и масляные пятна между колеями: подъезд, метры от его начала, колея (−1 — левая, 1 — правая,
# 0 — середина), полуоси пятна.
PUDDLES = [(0, 17.0, -1, 0.3, 0.9), (0, 14.0, 1, 0.28, 0.7), (1, 17.5, 1, 0.32, 1.0), (1, 14.5, -1, 0.25, 0.6), (2, 12.0, -1, 0.3, 0.8)]
STAINS = [(0, 20.5, 0, 0.5, 0.35), (1, 20.3, 0, 0.45, 0.3), (0, 18.0, 0, 0.3, 0.25), (2, 15.4, 0, 0.4, 0.3)]


def arc(cx, n=14):
    """Лучковый свод арки шириной `GATE_W` с пятой на `SPRING` и подъёмом `RISE`: точки (x, z) слева направо,
    радиус и высота середины круга."""
    half = GATE_W / 2
    r = (half * half + RISE * RISE) / (2 * RISE)
    zc = SPRING + RISE - r
    a0 = math.atan2(SPRING - zc, half)
    pts = [(cx + r * math.cos(a), zc + r * math.sin(a)) for a in (math.pi - a0 - (math.pi - 2 * a0) * i / n for i in range(n + 1))]
    return pts, r, zc


def arch_z(cx, x):
    """Высота свода арки с серединой `cx` над точкой `x`."""
    _, r, zc = arc(cx)
    return zc + math.sqrt(max(0.0, r * r - (x - cx) ** 2))


def shed_top(x):
    """Верх стен мастерской: у стены сарая 3,9 м, у правой стены 2,8 м."""
    return 3.9 - (x - WX0) / (WX1 - WX0) * 1.1


def nearest_lane(x, y):
    """Ближайший подъезд `WEAR`: расстояние от точки до его оси и сколько колеи там ещё осталось — у начала и конца
    подъезда она за `TAPER` м сходит на нет. За началом и концом расстояние не считается: иначе колея обогнула бы
    конец подъезда полукольцом."""
    best, left = 1e9, 0.0
    for line in WEAR:
        last = len(line) - 2
        for i, ((ax, ay), (bx, by)) in enumerate(zip(line, line[1:])):
            dx, dy = bx - ax, by - ay
            length = math.hypot(dx, dy)
            t = ((x - ax) * dx + (y - ay) * dy) / (length * length)
            if (i == 0 and t < 0.0) or (i == last and t > 1.0):
                continue
            t = max(0.0, min(1.0, t))
            d = math.hypot(x - ax - dx * t, y - ay - dy * t)
            if d < best:
                best, left = d, 1.0
                if i == 0:
                    left = min(left, smoothstep(0.0, TAPER, t * length))
                if i == last:
                    left = min(left, smoothstep(0.0, TAPER, (1.0 - t) * length))
    return best, left


def on_lane(lane, s, side):
    """Точка подъезда `WEAR[lane]` в `s` метрах от его начала (не дальше конца): на оси (`side` = 0) или в колее слева
    (−1) и справа (1)."""
    line = WEAR[lane]
    (ax, ay), (bx, by) = line[0], line[1]
    for following in line[2:]:
        length = math.hypot(bx - ax, by - ay)
        if s <= length:
            break
        s -= length
        (ax, ay), (bx, by) = (bx, by), following
    length = math.hypot(bx - ax, by - ay)
    dx, dy = (bx - ax) / length, (by - ay) / length
    t = min(s, length)
    return ax + dx * t + dy * side * TRACK, ay + dy * t - dx * side * TRACK


def worn(x, y):
    """Насколько точка протёрта колёсами: 1 в колее, меньше половины посередине между колеями, к 2 м за колеёй — 0."""
    d, left = nearest_lane(x, y)
    return left * max(math.exp(-((d - TRACK) / 0.4) ** 2), 0.45 * math.exp(-(d / 1.2) ** 2))


def rut(x, y):
    """Глубина колеи от 0 до 1: 1 посередине следа колеса."""
    d, left = nearest_lane(x, y)
    return left * math.exp(-((d - TRACK) / 0.32) ** 2)


def edge(x, y):
    """Сколько метров от точки до края двора; снаружи — меньше нуля. Край — прямоугольник `YARD` со скруглёнными
    углами, неровный примерно на метр."""
    x0, y0, x1, y1 = YARD
    r = 3.0
    qx = abs(x - (x0 + x1) / 2) - ((x1 - x0) / 2 - r)
    qy = abs(y - (y0 + y1) / 2) - ((y1 - y0) / 2 - r)
    outside = math.hypot(max(qx, 0.0), max(qy, 0.0)) + min(max(qx, qy), 0.0) - r
    return -outside - 1.4 * (noise(x / 4.0, y / 4.0, 11) - 0.5) - 0.6 * (noise(x / 1.5, y / 1.5, 12) - 0.5)


def ground_z(x, y):
    """Высота земли: кочки по 2–3 см, колеи на 3,5 см глубже; к краю двора — к нулю, к земле карты."""
    fade = smoothstep(0.0, EDGE, edge(x, y))
    return fade * (GROUND + 0.025 * (noise(x / 1.3, y / 1.3, 13) - 0.5) - 0.035 * rut(x, y))


def bare(x, y):
    return any(x0 <= x <= x1 and y0 <= y <= y1 for x0, y0, x1, y1 in BARE)


def grassy(x, y):
    """Сколько травы: у края двора и за сараем больше, пятнами в глубине двора, на подъездах нет совсем."""
    g = 1.0 - smoothstep(0.6, 3.2, edge(x, y)) + 0.6 * smoothstep(YB, YB + 2.0, y) + 0.9 * (noise(x / 3.5, y / 3.5, 14) - 0.55)
    return g - 1.6 * worn(x, y)


class Yard(Plant):
    """Сетка двора: к материалам и развёртке хлебозавода — брусья под любым углом, плоские фигуры с толщиной,
    наклонные пластины, тела вращения, своды арок, створки ворот и земля открытой сеткой."""

    def __init__(self, name):
        super().__init__(name)
        self.up = []

    def orient(self):
        """Земля — открытая сетка, пересчёт может развернуть её вниз: её грани разворачиваются вверх."""
        super().orient()
        for f in self.up:
            f.normal_update()
            if f.normal.z < 0:
                f.normal_flip()

    def solid(self, ring0, ring1, mi):
        """Тело между двумя одинаковыми контурами вершин: две крышки и бока."""
        self.bm.faces.new(list(reversed(ring0))).material_index = mi
        self.bm.faces.new(ring1).material_index = mi
        for i in range(len(ring0)):
            j = (i + 1) % len(ring0)
            self.bm.faces.new([ring0[i], ring0[j], ring1[j], ring1[i]]).material_index = mi

    def beam(self, a, b, w, h, mi, up=(0.0, 0.0, 1.0)):
        """Брус от точки `a` до `b`: `w` — поперёк бруса и `up`, `h` — вдоль `up`."""
        a, b = Vector(a), Vector(b)
        d = (b - a).normalized()
        side = d.cross(Vector(up))
        if side.length < 1e-6:
            side = d.cross(Vector((1.0, 0.0, 0.0)))
        side.normalize()
        lift = side.cross(d).normalized()
        corners = [side * sw * w / 2 + lift * sh * h / 2 for sw, sh in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
        self.solid([self.bm.verts.new(a + c) for c in corners], [self.bm.verts.new(b + c) for c in corners], mi)

    def prism(self, points, offset, mi):
        """Плоская выпуклая фигура по точкам (x, y, z), вытянутая на вектор `offset`: щипец, стена со скошенным верхом."""
        front = [self.bm.verts.new(Vector(p)) for p in points]
        self.solid(front, [self.bm.verts.new(v.co + Vector(offset)) for v in front], mi)

    def plate(self, points, thick, mi):
        """Пластина по плоскому выпуклому контуру (x, y, z): верх — контур, низ — на `thick` ниже по нормали; скат крыши."""
        pts = [Vector(p) for p in points]
        n = (pts[1] - pts[0]).cross(pts[2] - pts[0]).normalized()
        if n.z < 0:
            n = -n
        self.solid([self.bm.verts.new(p - n * thick) for p in pts], [self.bm.verts.new(p) for p in pts], mi)

    def lathe(self, center, axis, profile, mi, segments=24, closed=False):
        """Тело вращения вокруг оси `axis` через `center`: профиль — точки (t вдоль оси, радиус); точка с нулевым
        радиусом — середина донышка, `closed` — профиль замкнут в кольцо. Бока гладкие, донышки и торцы плоские;
        развёртка своя — по окружности и вдоль профиля, в метрах материала."""
        c, a = Vector(center), Vector(axis).normalized()
        e1 = a.orthogonal().normalized()
        e2 = a.cross(e1)
        meters = self.tiles.get(mi, (2.0, False))[0]
        dirs = [e1 * math.cos(2 * math.pi * k / segments) + e2 * math.sin(2 * math.pi * k / segments) for k in range(segments)]
        rings = [[self.bm.verts.new(c + a * t)] * segments if r < 1e-6 else [self.bm.verts.new(c + a * t + d * r) for d in dirs]
                 for t, r in profile]
        pairs = list(zip(range(len(profile) - 1), range(1, len(profile)))) + ([(len(profile) - 1, 0)] if closed else [])
        along = [0.0]
        for p, q in pairs:
            along.append(along[-1] + math.hypot(profile[q][0] - profile[p][0], profile[q][1] - profile[p][1]))
        for n, (p, q) in enumerate(pairs):
            flat = abs(profile[q][0] - profile[p][0]) < 1e-6
            for k in range(segments):
                corners, seen = [], set()
                for ring, kk, nn in ((p, k, n), (p, k + 1, n), (q, k + 1, n + 1), (q, k, n + 1)):
                    v = rings[ring][kk % segments]
                    if v not in seen:
                        seen.add(v)
                        corners.append((v, kk, nn, profile[ring][1]))
                f = self.bm.faces.new([v for v, _, _, _ in corners])
                f.material_index = mi
                f.smooth = not flat
                self.manual.add(f)
                for loop, (_, kk, nn, r) in zip(f.loops, corners):
                    loop[self.uv].uv = (2 * math.pi * max(r, 0.05) * kk / segments / meters, along[nn] / meters)
        # Где профиль ломается круче 30°, кольцо рёбер острое: иначе сглаживание тянет тень по всей соседней грани.
        count = len(profile)
        for i in range(count) if closed else range(1, count - 1):
            (t0, r0), (t1, r1), (t2, r2) = profile[i - 1], profile[i], profile[(i + 1) % count]
            turn = abs(math.atan2(r2 - r1, t2 - t1) - math.atan2(r1 - r0, t1 - t0))
            if r1 < 1e-6 or min(turn, 2 * math.pi - turn) < math.radians(30):
                continue
            for k in range(segments):
                ring_edge = self.bm.edges.get((rings[i][k], rings[i][(k + 1) % segments]))
                if ring_edge is not None:
                    ring_edge.smooth = False

    def spandrel(self, cx, mi):
        """Стена над аркой фасада: от свода до карниза, во всю толщину стены."""
        pts, _, _ = arc(cx)
        rows = [[self.bm.verts.new((x, y, z if low else EAVE)) for x, z in pts] for y in (YF, YF + WALL) for low in (True, False)]
        front_low, front_top, back_low, back_top = rows
        for i in range(len(pts) - 1):
            j = i + 1
            for quad in ((front_low[i], front_low[j], front_top[j], front_top[i]), (back_low[i], back_top[i], back_top[j], back_low[j]),
                         (front_low[i], back_low[i], back_low[j], front_low[j]), (front_top[i], front_top[j], back_top[j], back_top[i])):
                self.bm.faces.new(quad).material_index = mi
        for k in (0, -1):
            self.bm.faces.new((front_low[k], front_top[k], back_top[k], back_low[k])).material_index = mi

    def voussoirs(self, cx, bricks, mortar, rnd):
        """Кирпичная перемычка арки: кирпичи на ребро, торцами по радиусу, с швами раствора между ними; раствор —
        полосой под кирпичами. Выступает из побелки на 3,5 см."""
        pts, r, zc = arc(cx, n=40)
        outer = r + VOUSSOIR + 0.01
        mortar_rows = [[self.bm.verts.new((cx + (x - cx) * k, y, zc + (z - zc) * k)) for x, z in pts]
                       for k in (1.0, outer / r) for y in (YF - 0.02, YF)]
        inner_front, inner_back, outer_front, outer_back = mortar_rows
        for i in range(len(pts) - 1):
            j = i + 1
            for quad in ((inner_front[i], outer_front[i], outer_front[j], inner_front[j]), (inner_back[i], inner_back[j], outer_back[j], outer_back[i]),
                         (inner_front[i], inner_front[j], inner_back[j], inner_back[i]), (outer_front[i], outer_back[i], outer_back[j], outer_front[j])):
                self.bm.faces.new(quad).material_index = mortar
        for k in (0, -1):
            self.bm.faces.new((inner_front[k], inner_back[k], outer_back[k], outer_front[k])).material_index = mortar
        left = math.atan2(pts[0][1] - zc, pts[0][0] - cx)
        right = math.atan2(pts[-1][1] - zc, pts[-1][0] - cx)
        count = round(r * (left - right) / 0.078)
        step, joint = (left - right) / count, 0.011 / r
        for i in range(count):
            a0, a1 = left - i * step - joint / 2, left - (i + 1) * step + joint / 2
            outline = [(cx + rr * math.cos(a), zc + rr * math.sin(a)) for rr, a in ((r + 0.004, a0), (r + 0.004, a1), (r + VOUSSOIR, a1), (r + VOUSSOIR, a0))]
            self.solid([self.bm.verts.new((x, YF - 0.035, z)) for x, z in outline], [self.bm.verts.new((x, YF - 0.015, z)) for x, z in outline],
                       rnd.choice(bricks))

    def leaf(self, hinge, sign, inner, angle, width, top, boards, battens):
        """Створка ворот из вертикальных досок по 20 см толщиной 6 см: петли в точке `hinge` (x, y); закрытая створка
        уходит от петель вдоль X со знаком `sign`, её внутренняя сторона — к Y со знаком `inner`; `angle` — поворот
        створки в градусах против часовой. Верх доски на расстоянии s от петель — `top(s)`. Изнутри две планки и раскос."""
        thick, pitch, gap = 0.06, 0.2, 0.012
        rot = math.radians(angle)
        c, s_ = math.cos(rot), math.sin(rot)
        d = Vector((sign * c, sign * s_, 0.0))
        nrm = Vector((-inner * s_, inner * c, 0.0))
        base = Vector((hinge[0], hinge[1], 0.0))

        def at(s, z, off=0.0):
            return base + d * s + nrm * off + Vector((0.0, 0.0, z))

        n = max(1, round(width / pitch))
        step = width / n
        for i in range(n):
            s0, s1 = i * step + gap / 2, (i + 1) * step - gap / 2
            outline = [(s0, GROUND + 0.03), (s1, GROUND + 0.03)] + [(s, top(s)) for s in (s1, (s0 + s1) / 2, s0)]
            self.solid([self.bm.verts.new(at(s, z)) for s, z in outline], [self.bm.verts.new(at(s, z, thick)) for s, z in outline], boards)
        low, high = battens
        off = thick + 0.025
        for z in battens:
            self.beam(at(0.06, z, off), at(width - 0.06, z, off), 0.05, 0.14, boards)
        self.beam(at(0.15, low + 0.08, off), at(width - 0.15, high - 0.08, off), 0.05, 0.12, boards)


def drum(m, center, axis, paint):
    """Стальная бочка на 200 л: обечайка с двумя обручами катания, закатанные края-уторы, донышки утоплены на
    сантиметр; на верхнем донышке (к концу `axis`) две пробки. Её же полуторки повезут в кузове."""
    r, h = DRUM_R, DRUM_H
    profile = [(-h + 0.012, 0.0), (-h + 0.012, r - 0.012), (-h, r - 0.004), (-h, r + 0.008), (-h + 0.022, r + 0.008), (-h + 0.03, r)]
    for t in (-0.15, 0.15):
        profile += [(t - 0.03, r), (t - 0.012, r + 0.014), (t + 0.012, r + 0.014), (t + 0.03, r)]
    profile += [(h - 0.03, r), (h - 0.022, r + 0.008), (h, r + 0.008), (h, r - 0.004), (h - 0.012, r - 0.012), (h - 0.012, 0.0)]
    m.lathe(center, axis, profile, paint, 28)
    a = Vector(axis).normalized()
    lid = Vector(center) + a * (h - 0.012)
    for off, size in ((0.17, 0.032), (-0.19, 0.02)):
        m.lathe(lid + a.orthogonal().normalized() * off, a, [(0.0, 0.0), (0.0, size), (0.018, size), (0.018, 0.0)], paint, 10)


def coach_house(m, mt):
    """Каретный сарай: стены с арками, щипцы, крыша с фальцами и щипцом сеновала, труба, нутро левого проезда."""
    plaster, brick = mt["plaster"], mt["brick"]
    # Стены: столбы между арками, стены над арками, торцы, задняя; кирпичные перемычки арок, цоколь и карниз.
    edges = [X0] + [x for cx in GATES for x in (cx - GATE_W / 2, cx + GATE_W / 2)] + [X1]
    for x0, x1 in zip(edges[::2], edges[1::2]):
        m.box(x0, YF, GROUND, x1, YF + WALL, EAVE, plaster)
        m.box(x0 - (0.04 if x0 == X0 else 0.0), YF - 0.04, 0.0, x1 + (0.04 if x1 == X1 else 0.0), YF, 0.55, brick)
    rnd = random.Random(18)
    for cx in GATES:
        m.spandrel(cx, plaster)
        m.voussoirs(cx, (mt["voussoir"], mt["voussoir_dark"]), mt["mortar"], rnd)
    m.box(X0, YB - WALL, GROUND, X1, YB, EAVE, plaster)
    m.box(X0, YF + WALL, GROUND, X0 + WALL, YB - WALL, EAVE, plaster)
    m.box(X1 - WALL, YF + WALL, GROUND, X1, YB - WALL, EAVE, plaster)
    m.box(X0 - 0.04, YB, 0.0, X1 + 0.04, YB + 0.04, 0.55, brick)
    m.box(X0 - 0.04, YF, 0.0, X0, YB, 0.55, brick)
    m.box(X1, YF, 0.0, X1 + 0.04, YB, 0.55, brick)
    for y in (YF, YB):
        s = -1 if y == YF else 1
        m.box(X0 - 0.1, min(y, y + s * 0.12), EAVE - 0.48, X1 + 0.1, max(y, y + s * 0.12), EAVE - 0.2, brick)
    for x, out in ((X0, WALL), (X1, -WALL)):
        m.prism([(x, YF, EAVE - SINK), (x, YB, EAVE - SINK), (x, YM, RIDGE - SINK)], (out, 0.0, 0.0), plaster)
    m.prism([(-DORMER, YF, EAVE - SINK), (DORMER, YF, EAVE - SINK), (0.0, YF, DORMER_TOP - SINK)], (0.0, WALL, 0.0), plaster)

    # Табличка «ГАРАЖ» на столбе между арками: синяя эмаль с белой каймой; буквы ставит `build`.
    m.box(-0.5, YF - 0.03, 2.05, 0.5, YF, 2.42, mt["enamel"])
    for x0, z0, x1, z1 in ((-0.46, 2.09, 0.46, 2.11), (-0.46, 2.36, 0.46, 2.38), (-0.46, 2.09, -0.44, 2.38), (0.44, 2.09, 0.46, 2.38)):
        m.box(x0, YF - 0.035, z0, x1, YF - 0.03, z1, mt["enamel_white"])

    # Окно в левом торце с решёткой.
    m.box(X0 - 0.06, 6.55, 2.1, X0, 7.45, 3.1, mt["frame_dark"])
    m.box(X0 - 0.07, 6.65, 2.2, X0 - 0.06, 7.35, 3.0, mt["glass"])
    for y in (6.83, 7.0, 7.17):
        m.box(X0 - 0.1, y - 0.012, 2.2, X0 - 0.07, y + 0.012, 3.0, mt["iron"])

    # Крыша: скаты с фальцами через 0,62 м, конёк, скаты щипца сеновала с его фальцами.
    roof = mt["roof"]
    xa, xb = X0 - RAKE, X1 + RAKE
    m.plate([(xa, YF - OVER, EAVE_EDGE), (xb, YF - OVER, EAVE_EDGE), (xb, YM, RIDGE), (xa, YM, RIDGE)], ROOF, roof)
    m.plate([(xa, YB + OVER, EAVE_EDGE), (xb, YB + OVER, EAVE_EDGE), (xb, YM, RIDGE), (xa, YM, RIDGE)], ROOF, roof)
    front_n = Vector((0.0, -SLOPE, 1.0)).normalized()
    back_n = Vector((0.0, SLOPE, 1.0)).normalized()
    count = round((xb - xa - 0.3) / 0.62)
    for i in range(count + 1):
        x = xa + 0.15 + (xb - xa - 0.3) * i / count
        y0 = YF - OVER
        if abs(x) < DORMER_EAVE:
            y0 += (DORMER_EAVE - abs(x)) / DORMER_EAVE * (DORMER_END - (YF - OVER))
        z0 = EAVE + (y0 - YF) * SLOPE
        m.beam(Vector((x, y0, z0)) + front_n * 0.025, Vector((x, YM, RIDGE)) + front_n * 0.025, 0.05, 0.06, roof, up=front_n)
        m.beam(Vector((x, YB + OVER, EAVE_EDGE)) + back_n * 0.025, Vector((x, YM, RIDGE)) + back_n * 0.025, 0.05, 0.06, roof, up=back_n)
    m.beam((xa, YM, RIDGE + 0.02), (xb, YM, RIDGE + 0.02), 0.26, 0.07, roof)
    for sign in (-1, 1):
        m.plate([(0.0, YF - OVER, DORMER_TOP), (sign * DORMER_EAVE, YF - OVER, EAVE_EDGE), (0.0, DORMER_END, DORMER_TOP)], ROOF, roof)
        n = Vector((sign * DORMER_SLOPE, 0.0, 1.0)).normalized()
        y = YF - OVER + 0.3
        while y < DORMER_END - 0.3:
            t = (y - (YF - OVER)) / (DORMER_END - (YF - OVER))
            end = Vector((sign * DORMER_EAVE * (1 - t), y, EAVE_EDGE + (DORMER_TOP - EAVE_EDGE) * t))
            m.beam(Vector((0.0, y, DORMER_TOP)) + n * 0.025, end + n * 0.025, 0.05, 0.06, roof, up=n)
            y += 0.62
    m.beam((0.0, YF - OVER, DORMER_TOP + 0.02), (0.0, DORMER_END, DORMER_TOP + 0.02), 0.22, 0.06, roof)

    # Кирпичная труба на заднем скате.
    m.box(3.9, 8.2, CEILING, 4.55, 8.85, RIDGE + 1.0, brick)
    m.box(3.82, 8.12, RIDGE + 1.0, 4.63, 8.93, RIDGE + 1.12, brick)

    # Сеновал: дверца в щипце, балка с блоком, верёвкой и крюком.
    m.box(-0.7, YF - 0.05, EAVE + 0.25, 0.7, YF, EAVE + 1.7, mt["frame_dark"])
    m.box(-0.6, YF - 0.07, EAVE + 0.33, 0.6, YF - 0.05, EAVE + 1.62, mt["planks"])
    m.beam((0.0, YF + 0.6, EAVE + 2.0), (0.0, YF - 1.05, EAVE + 2.0), 0.18, 0.2, mt["log"])
    m.cylinder_x(0.0, YF - 0.9, EAVE + 1.78, 0.09, 0.08, mt["iron"], 12)
    hook = EAVE - 0.4
    m.beam((0.0, YF - 0.9, EAVE + 1.7), (0.0, YF - 0.9, hook + 0.2), 0.025, 0.025, mt["rope"])
    m.beam((0.0, YF - 0.9, hook + 0.22), (0.0, YF - 0.9, hook), 0.04, 0.04, mt["iron"])
    m.beam((0.0, YF - 0.9, hook + 0.02), (0.0, YF - 0.75, hook + 0.02), 0.04, 0.04, mt["iron"])
    m.beam((0.0, YF - 0.77, hook + 0.01), (0.0, YF - 0.77, hook + 0.12), 0.04, 0.04, mt["iron"])

    # Нутро: пол сеновала, тёмный пол, смотровая яма в левом проезде, верстак у задней стены, колесо.
    m.box(X0 + WALL, YF + WALL, CEILING, X1 - WALL, YB - WALL, CEILING + 0.15, mt["planks"])
    m.box(X0 + WALL, YF + WALL, GROUND, X1 - WALL, YB - WALL, FLOOR, mt["floor"])
    m.box(-3.75, 4.2, FLOOR, -2.65, 10.2, FLOOR + 0.005, mt["pit"])
    for x0, y0, x1, y1 in ((-3.9, 4.05, -2.5, 4.2), (-3.9, 10.2, -2.5, 10.35), (-3.9, 4.2, -3.75, 10.2), (-2.65, 4.2, -2.5, 10.2)):
        m.box(x0, y0, GROUND, x1, y1, FLOOR + 0.06, mt["log"])
    by0, by1 = YB - WALL - 0.65, YB - WALL - 0.02
    m.box(-4.7, by0, 0.86, -1.7, by1, 0.92, mt["planks"])
    for x in (-4.6, -1.8):
        for y in (by0 + 0.06, by1 - 0.06):
            m.cbox(x, y, GROUND, 0.08, 0.08, 0.86, mt["log"])
    m.box(-4.6, by0 + 0.05, 0.3, -1.8, by1 - 0.05, 0.34, mt["planks"])
    m.box(-2.3, by0 - 0.02, 0.92, -2.05, by0 + 0.2, 1.08, mt["iron"])
    m.box(-4.5, by1, 1.2, -2.0, by1 + 0.02, 2.0, mt["planks"])
    for i, x in enumerate((-4.2, -3.8, -3.4, -3.0, -2.6)):
        m.box(x - 0.03, by1 - 0.03, 1.35 + 0.08 * (i % 2), x + 0.03, by1, 1.85, mt["iron"])
    m.cylinder(-5.4, 9.8, FLOOR, 0.42, 0.2, mt["rubber"], 16)
    m.cylinder(-5.4, 9.8, FLOOR + 0.2, 0.24, 0.005, mt["pit"], 16)

    # Ворота: левые распахнуты наружу, правые закрыты.
    left, right = GATES
    half = GATE_W / 2
    battens = (0.5, 2.35)

    def top(cx, x0, sign):
        return lambda s: arch_z(cx, x0 + sign * s) - 0.03

    m.leaf((left - half, YF), 1, 1, -105, half, top(left, left - half, 1), mt["gate"], battens)
    m.leaf((left + half, YF), -1, 1, 105, half, top(left, left + half, -1), mt["gate"], battens)
    m.leaf((right - half, YF + 0.1), 1, 1, 0, half, top(right, right - half, 1), mt["gate"], battens)
    m.leaf((right + half, YF + 0.1), -1, 1, 0, half, top(right, right + half, -1), mt["gate"], battens)


def workshop(m, mt):
    """Дощатая мастерская справа от сарая: односкатная крыша, окно с переплётом, дверь, жестяная труба; у стены —
    тележное колесо, у торца — брошенные доски."""
    walls = mt["shed_wall"]
    m.prism([(WX0, WY0, GROUND), (WX1, WY0, GROUND), (WX1, WY0, shed_top(WX1)), (WX0, WY0, shed_top(WX0))], (0.0, BOARD, 0.0), walls)
    m.prism([(WX0, WY1, GROUND), (WX1, WY1, GROUND), (WX1, WY1, shed_top(WX1)), (WX0, WY1, shed_top(WX0))], (0.0, -BOARD, 0.0), walls)
    m.box(WX1 - BOARD, WY0 + BOARD, GROUND, WX1, WY1 - BOARD, shed_top(WX1), walls)
    over = 0.35

    def roof_z(x):
        return shed_top(x) + 0.055

    m.plate([(WX0, WY0 - over, roof_z(WX0)), (WX1 + over, WY0 - over, roof_z(WX1 + over)),
             (WX1 + over, WY1 + over, roof_z(WX1 + over)), (WX0, WY1 + over, roof_z(WX0))], 0.06, mt["shed_roof"])
    # Окно: рама, стекло, переплёт крестом, подоконная доска.
    frame, glass_mi = mt["frame"], mt["glass"]
    wx, wz0, wz1, ww = 9.1, 1.25, 2.15, 1.0
    m.box(wx - ww / 2, WY0 - 0.02, wz0, wx + ww / 2, WY0, wz1, glass_mi)
    for x0, z0, x1, z1 in ((wx - ww / 2 - 0.08, wz0 - 0.08, wx + ww / 2 + 0.08, wz0), (wx - ww / 2 - 0.08, wz1, wx + ww / 2 + 0.08, wz1 + 0.08),
                           (wx - ww / 2 - 0.08, wz0, wx - ww / 2, wz1), (wx + ww / 2, wz0, wx + ww / 2 + 0.08, wz1),
                           (wx - 0.025, wz0, wx + 0.025, wz1), (wx - ww / 2, (wz0 + wz1) / 2 + 0.1, wx + ww / 2, (wz0 + wz1) / 2 + 0.15)):
        m.box(x0, WY0 - 0.06, z0, x1, WY0, z1, frame)
    m.box(wx - ww / 2 - 0.12, WY0 - 0.14, wz0 - 0.12, wx + ww / 2 + 0.12, WY0, wz0 - 0.08, frame)
    # Дверь из досок с планками.
    m.box(7.0, WY0 - 0.04, GROUND, 7.95, WY0, 2.05, mt["planks"])
    for z in (0.35, 1.65):
        m.box(7.05, WY0 - 0.07, z, 7.9, WY0 - 0.04, z + 0.12, mt["planks"])
    m.box(7.8, WY0 - 0.1, 1.0, 7.86, WY0 - 0.07, 1.12, mt["iron"])
    # Жестяная труба печки с колпаком.
    m.cylinder(9.6, 8.5, 2.6, 0.1, 2.3, mt["tin"], 12)
    m.cylinder(9.6, 8.5, 4.9, 0.2, 0.18, mt["tin"], 12, r_top=0.05)
    # Тележное колесо, прислонённое к стене справа от окна: обод, ступица, двенадцать спиц.
    center, axis = Vector((10.15, WY0 - 0.2, 0.66)), Vector((0.0, -0.96, 0.28))
    m.lathe(center, axis, [(-0.035, 0.56), (0.035, 0.56), (0.035, 0.62), (-0.035, 0.62)], mt["log"], 32, closed=True)
    m.lathe(center, axis, [(-0.11, 0.0), (-0.11, 0.09), (0.11, 0.09), (0.11, 0.0)], mt["log"], 12)
    side = axis.orthogonal().normalized()
    other = axis.normalized().cross(side)
    for k in range(12):
        d = side * math.cos(2 * math.pi * k / 12) + other * math.sin(2 * math.pi * k / 12)
        m.beam(center + d * 0.08, center + d * 0.57, 0.035, 0.035, mt["log"], up=axis)
    for dx, dy, length in ((0.0, 0.0, 3.2), (0.15, 0.22, 2.7)):
        m.beam((WX1 + 0.6 + dx, 4.0 + dy, GROUND + 0.02), (WX1 + 0.6 + dx + 0.2, 4.0 + dy + length, GROUND + 0.02), 0.2, 0.04, mt["planks"])


def fuel_shed(m, mt):
    """Навес слева от сарая, открытый к улице: задняя и левая стены из досок, бочки бензина на лагах в два ряда
    торцами вперёд; у навеса три стоячие бочки и одна на боку, готовая в кузов."""
    log, x0, x1, front, back = mt["log"], FX0, FX1, FY0, FY1
    for x in (x0, (x0 + x1) / 2, x1):
        m.beam((x, front, 0.0), (x, front, 2.75), 0.14, 0.14, log)
    for x in (x0, x1):
        m.beam((x, back, 0.0), (x, back, 2.3), 0.14, 0.14, log)
    m.beam((x0 - 0.1, front, 2.67), (x1 + 0.1, front, 2.67), 0.14, 0.16, log)
    m.beam((x0 - 0.1, back, 2.22), (x1 + 0.1, back, 2.22), 0.14, 0.16, log)
    m.prism([(x0, back, 0.0), (x1, back, 0.0), (x1, back, 2.15), (x0, back, 2.15)], (0.0, BOARD, 0.0), mt["shed_wall"])
    m.prism([(x0, front + 0.1, 0.0), (x0, back, 0.0), (x0, back, 2.15), (x0, front + 0.1, 2.6)], (-BOARD, 0.0, 0.0), mt["shed_wall"])
    m.plate([(x0 - 0.3, front - 0.35, 2.84), (x1 + 0.3, front - 0.35, 2.84), (x1 + 0.3, back + 0.25, 2.28), (x0 - 0.3, back + 0.25, 2.28)],
            0.05, mt["shed_roof"])
    mid = front + 0.6  # у самого края навеса: глубже крыша закрывает бочки от камеры игры
    for y in (mid - 0.35, mid + 0.35):
        m.box(x0 + 0.15, y - 0.07, 0.0, x1 - 0.15, y + 0.07, GROUND + 0.12, log)
    paints = (mt["drum"], mt["drum_grey"], mt["drum"], mt["drum_rust"], mt["drum"], mt["drum_grey"])
    base = GROUND + 0.12 + DRUM_R + 0.008
    row = [(x0 + 0.45 + 0.63 * i, base) for i in range(6)] + [(x0 + 0.765 + 0.63 * i, base + 0.52) for i in range(5)]
    for i, (x, z) in enumerate(row):
        drum(m, (x, mid, z), (0.0, -1.0, 0.0), paints[i % len(paints)])
    for i, (x, y) in enumerate(((-12.75, 1.65), (-12.85, 1.05), (-13.35, 1.4))):
        drum(m, (x, y, GROUND + DRUM_H), (0.0, 0.0, 1.0), paints[i + 1])
    drum(m, (-8.55, 0.9, GROUND + DRUM_R + 0.008), (1.0, 0.0, 0.0), mt["drum"])


def pump(m, mt):
    """Ручная бензоколонка на бетонном островке между подъездами: чугунная тумба, колонна с поясками, стеклянный
    мерник с бензином внутри, рамой и шкалой, колпак с белым шаром; рычаг насоса справа, шланг с пистолетом на
    крюке слева. Насосом бензин качают в мерник, из него он самотёком идёт в бак."""
    x, y = PUMP
    red, iron, upward = mt["red"], mt["iron"], (0.0, 0.0, 1.0)
    m.box(x - 0.6, y - 1.25, 0.0, x + 0.6, y + 1.25, GROUND + 0.15, mt["concrete"])
    base = GROUND + 0.15
    m.box(x - 0.27, y - 0.27, base, x + 0.27, y + 0.27, base + 0.32, red)
    m.box(x - 0.23, y - 0.23, base + 0.32, x + 0.23, y + 0.23, base + 0.38, red)
    col = base + 0.38
    m.lathe((x, y, col), upward, [(0.0, 0.0), (0.0, 0.2), (0.07, 0.2), (0.1, 0.165), (0.62, 0.165), (0.64, 0.18), (0.69, 0.18),
                                  (0.71, 0.165), (1.2, 0.165), (1.23, 0.205), (1.3, 0.205), (1.3, 0.0)], red)
    g0 = col + 1.3
    m.lathe((x, y, g0), upward, [(0.0, 0.0), (0.0, 0.19), (0.8, 0.19), (0.8, 0.0)], mt["fuel_glass"])
    m.lathe((x, y, g0 + 0.01), upward, [(0.0, 0.0), (0.0, 0.17), (0.46, 0.17), (0.46, 0.0)], mt["fuel"])
    for k in range(4):
        a = math.pi / 4 + k * math.pi / 2
        px, py = x + 0.205 * math.cos(a), y + 0.205 * math.sin(a)
        m.beam((px, py, g0), (px, py, g0 + 0.8), 0.035, 0.035, red)
    m.box(x - 0.025, y - 0.21, g0 + 0.06, x + 0.025, y - 0.19, g0 + 0.74, mt["enamel_white"])
    for i in range(9):
        z = g0 + 0.08 + i * 0.08
        m.box(x - 0.018, y - 0.215, z, x + (0.006 if i % 2 else 0.018), y - 0.21, z + 0.008, iron)
    cap = g0 + 0.8
    m.lathe((x, y, cap), upward, [(0.0, 0.0), (0.0, 0.235), (0.06, 0.235), (0.09, 0.2), (0.2, 0.09), (0.22, 0.06), (0.22, 0.0)], red)
    m.lathe((x, y, cap + 0.19), upward, [(0.16 - 0.16 * math.cos(math.pi * k / 10), 0.16 * math.sin(math.pi * k / 10)) for k in range(11)],
            mt["globe"], 20)
    # Рычаг качается вокруг вала, торчащего из колонны вбок, — в плоскости вдоль её бока.
    m.lathe((x + 0.165, y, col + 0.55), (1.0, 0.0, 0.0), [(0.0, 0.0), (0.0, 0.05), (0.06, 0.05), (0.06, 0.0)], iron, 12)
    m.beam((x + 0.2425, y, col + 0.55), (x + 0.2425, y - 0.3, col + 1.0), 0.035, 0.035, iron)
    m.beam((x + 0.2425, y - 0.3, col + 1.0), (x + 0.2425, y - 0.36, col + 1.12), 0.05, 0.05, mt["log"])
    hose = [(x - 0.17, y, g0 - 0.05), (x - 0.33, y, g0 - 0.2), (x - 0.42, y, col + 0.75), (x - 0.4, y, col + 0.35),
            (x - 0.3, y, col + 0.18), (x - 0.22, y, col + 0.25), (x - 0.21, y, col + 0.6)]
    for a, b in zip(hose, hose[1:]):
        m.beam(a, b, 0.045, 0.045, mt["rubber"])
    m.beam((x - 0.21, y, col + 0.6), (x - 0.21, y, col + 0.85), 0.05, 0.05, iron)
    m.beam((x - 0.21, y, col + 0.85), (x - 0.27, y, col + 0.93), 0.03, 0.03, iron)
    m.box(x - 0.2, y - 0.03, col + 0.8, x - 0.16, y + 0.03, col + 0.86, iron)


def fire_board(m, mt):
    """Пожарный щит на двух столбах между навесом и сараем: красная доска под козырьком, конусные вёдра, топор,
    багор, лопата; перед щитом ящик с песком."""
    x0, x1, y = -8.15, -6.75, 4.6
    log, red, iron = mt["log"], mt["red"], mt["iron"]
    for x in (x0 + 0.1, x1 - 0.1):
        m.beam((x, y + 0.08, 0.0), (x, y + 0.08, 2.3), 0.1, 0.1, log)
    m.box(x0, y, 0.75, x1, y + 0.04, 2.1, red)
    m.plate([(x0 - 0.1, y - 0.25, 2.18), (x1 + 0.1, y - 0.25, 2.18), (x1 + 0.1, y + 0.1, 2.32), (x0 - 0.1, y + 0.1, 2.32)], 0.04, red)
    for x in (x0 + 0.25, x0 + 0.6):
        m.cylinder(x, y - 0.17, 1.25, 0.005, 0.38, red, 12, r_top=0.15)
        m.box(x - 0.01, y - 0.05, 1.63, x + 0.01, y, 1.7, iron)
    m.beam((x0 + 0.92, y - 0.03, 1.0), (x0 + 0.92, y - 0.03, 1.6), 0.04, 0.03, log)
    m.box(x0 + 0.82, y - 0.06, 1.48, x0 + 1.02, y - 0.02, 1.62, iron)
    m.beam((x0 + 0.1, y - 0.04, 1.9), (x1 - 0.1, y - 0.04, 1.9), 0.04, 0.04, log)
    m.beam((x1 - 0.1, y - 0.04, 1.9), (x1 - 0.02, y - 0.04, 1.78), 0.03, 0.03, iron)
    m.beam((x1 - 0.18, y - 0.04, 0.85), (x1 - 0.18, y - 0.04, 1.7), 0.04, 0.03, log)
    m.box(x1 - 0.31, y - 0.06, 0.8, x1 - 0.05, y - 0.03, 1.05, iron)
    m.box(-8.05, 3.75, 0.0, -6.85, 4.35, 0.72, mt["planks"])
    m.box(-7.97, 3.83, 0.72, -6.93, 4.27, 0.73, mt["sand"])


def hitching_post(m, mt):
    """Коновязь с лошадиных времён справа во дворе: два столба и жердь."""
    for x in (9.0, 12.0):
        m.cylinder(x, -4.0, 0.0, 0.1, 1.15, mt["log"], 10)
    m.cylinder_x(10.5, -4.0, 1.0, 0.075, 3.6, mt["log"], 10)


def clip(polygon, field):
    """Часть выпуклого многоугольника из точек (x, y, z), где `field(x, y)` больше нуля: края режутся там, где поле
    переходит через ноль, — граница идёт плавной линией, а не ступеньками клеток. Точка разреза лежит на ребре и
    по высоте, поэтому части одного треугольника остаются в его плоскости и сходятся без щелей."""
    out = []
    for p, q in zip(polygon, polygon[1:] + polygon[:1]):
        fp, fq = field(p[0], p[1]), field(q[0], q[1])
        if fp > 0:
            out.append(p)
        if (fp > 0) != (fq > 0):
            t = fp / (fp - fq)
            out.append(tuple(a + (b - a) * t for a, b in zip(p, q)))
    return out


def mud_field(x, y):
    """Больше нуля там, где на подъезде грязь: пятнами в метр-три с рваным краем, гуще на оси подъезда, за
    его краем — нет."""
    d, left = nearest_lane(x, y)
    lane = left * math.exp(-(d / 2.0) ** 2)
    patches = 2.0 * (noise(x / 3.0, y / 3.0, 15) - 0.6) + 0.3 * (noise(x / 0.9, y / 0.9, 16) - 0.5)
    return patches + 1.2 * (lane - 0.6) - 2.0 * (1.0 - lane) ** 4 - (2.0 if bare(x, y) else 0.0)


def turf_field(x, y):
    """Больше нуля там, где растёт трава."""
    return grassy(x, y) - 0.5 - (2.0 if bare(x, y) else 0.0)


def ground(m, mt):
    """Земля двора открытой гладкой сеткой `CELL` по высоте `ground_z`: клетка — два треугольника, обрезанных по
    краю двора и разрезанных по границам грязи и травы; вершины на общих рёбрах общие. Развёртка — в метрах, своя."""
    x0, y0 = YARD[0] - 1.0, YARD[1] - 1.0
    nx, ny = round((YARD[2] - YARD[0] + 2.0) / CELL), round((YARD[3] - YARD[1] + 2.0) / CELL)
    corners, verts = {}, {}

    def corner(i, j):
        if (i, j) not in corners:
            x, y = x0 + i * CELL, y0 + j * CELL
            corners[(i, j)] = (x, y, ground_z(x, y))
        return corners[(i, j)]

    def vert(p):
        key = (round(p[0], 4), round(p[1], 4))
        if key not in verts:
            verts[key] = m.bm.verts.new(p)
        return verts[key]

    def face(polygon, kind):
        ring = []
        for p in polygon:
            v = vert(p)
            if v not in ring:
                ring.append(v)
        if len(ring) < 3:
            return
        f = m.bm.faces.new(ring)
        f.material_index = mt[kind]
        f.smooth = True
        m.manual.add(f)
        m.up.append(f)
        meters = m.tiles[mt[kind]][0]
        for loop in f.loops:
            loop[m.uv].uv = (loop.vert.co.x / meters, loop.vert.co.y / meters)

    for i in range(nx):
        for j in range(ny):
            a, b, c, d = corner(i, j), corner(i + 1, j), corner(i + 1, j + 1), corner(i, j + 1)
            for triangle in ([a, b, c], [a, c, d]):
                inside_yard = clip(triangle, edge)
                if len(inside_yard) < 3:
                    continue
                face(clip(inside_yard, mud_field), "mud")
                rest = clip(inside_yard, lambda x, y: -mud_field(x, y))
                face(clip(rest, turf_field), "turf")
                face(clip(rest, lambda x, y: -turf_field(x, y)), "dirt")
    for spots, mi, seed, lift in ((PUDDLES, mt["puddle"], 600, 0.012), (STAINS, mt["oil"], 700, 0.01)):
        for i, (lane, s, side, rx, ry) in enumerate(spots):
            x, y = on_lane(lane, s, side)
            m.blot(x, y, rx, ry, mi, seed + i, z=ground_z(x, y) + lift)


def grass(m, mt):
    """Кочки травы у края двора, у стен и пятнами там, где трава в земле; на подъездах и в голых местах — нет."""
    rnd = random.Random(7)
    x0, y0, x1, y1 = YARD
    placed = 0
    for _ in range(4000):
        x, y = rnd.uniform(x0, x1), rnd.uniform(y0, y1)
        if edge(x, y) < 0.3 or bare(x, y) or worn(x, y) > 0.12 or grassy(x, y) < 0.3:
            continue
        placed += 1
        if placed > 340:
            break
        blade = mt["grass"] if rnd.random() < 0.7 else mt["grass_dry"]
        z = ground_z(x, y) - 0.01
        for _ in range(rnd.randint(5, 9)):
            m.cylinder(x + rnd.uniform(-0.15, 0.15), y + rnd.uniform(-0.15, 0.15), z, rnd.uniform(0.03, 0.05), rnd.uniform(0.2, 0.45),
                       blade, 3, r_top=0.002)


def materials(m):
    return {
        "plaster": m.tex("plaster", "PaintedPlaster014", 2.5, tint="#f6efe6"),
        "brick": m.tex("brick", "brick_wall_001", 3.0),
        "voussoir": m.tex("voussoir", "brick_wall_001", 12.0),
        "voussoir_dark": m.tex("voussoir_dark", "brick_wall_001", 12.0, tint="#c9b5a8"),
        "mortar": m.mat("mortar", "#b3aa9c", 0.95),
        "roof": m.tex("roof", "MetalPlates011", 2.0, tint="#b8a8a8"),
        "gate": m.tex("gate", "PaintedWood009C", 1.2, tint="#6b896b", swap=True),
        "planks": m.tex("planks", "Planks012", 2.1, swap=True),
        "shed_wall": m.tex("shed_wall", "Planks012", 2.1, tint="#c8c2ba", swap=True),
        "shed_roof": m.tex("shed_roof", "Planks012", 2.1, tint="#b0aca6"),
        "log": m.tex("log", "Wood062", 1.6),
        "red": m.tex("red", "PaintedWood003", 1.2),
        "drum": m.tex("drum", "green_metal_rust", 1.6),
        "drum_grey": m.tex("drum_grey", "green_metal_rust", 1.6, tint="#a9b4bd"),
        "drum_rust": m.tex("drum_rust", "MetalPlates011", 2.0),
        "concrete": m.tex("concrete", "garage_floor", 1.89),
        "dirt": m.tex("dirt", "Ground103", 2.5),
        "turf": m.tex("turf", "Ground003", 2.5),
        "mud": m.tex("mud", "Ground051", 2.0),
        "floor": m.tex("floor", "Ground103", 2.5, tint="#9a948c"),
        "puddle": m.mat("puddle", "#86919a", 0.03),  # вода отражает небо
        "oil": m.mat("oil", "#3b3631", 0.35, 0.1),
        "tin": m.mat("tin", "#8d9396", 0.45, 0.6),
        "iron": m.mat("iron", "#2c2d2f", 0.55, 0.5),
        "rubber": m.mat("rubber", "#1d1d1e", 0.85),
        "rope": m.mat("rope", "#8a7a5c", 0.95),
        "pit": m.mat("pit", "#0f0f10", 0.9),
        "sand": m.mat("sand", "#c4a873", 0.95),
        "frame": m.mat("frame", "#d8d0bf", 0.7),
        "frame_dark": m.mat("frame_dark", "#4a3f35", 0.85),
        "glass": m.mat("glass", "#2b3438", 0.15, 0.3),
        "fuel_glass": glass(m, "fuel_glass", "#d6e2e0", 0.28),
        "fuel": m.mat("fuel", "#c8862e", 0.12),
        "globe": m.mat("globe", "#f1ece0", 0.25),
        "enamel": m.mat("enamel", "#1c3c8c", 0.25),
        "enamel_white": m.mat("enamel_white", "#f2efe6", 0.3),
        "grass": m.mat("grass", "#5d7a35", 0.9),
        "grass_dry": m.mat("grass_dry", "#8c8a4c", 0.9),
    }


def macro(x, y):
    """Крупные светлые и тёмные пятна земли 3–8 м — гасят повтор картинки грунта — и протёртые колёсами подъезды."""
    spots = 0.82 + 0.24 * (0.6 * noise(x / 7.0, y / 7.0, 3) + 0.4 * noise(x / 3.0, y / 3.0, 4))
    return spots * (1.0 - 0.3 * worn(x, y))


def inside(co):
    """Точка внутри сарая — под полом сеновала, между стенами: туда свет попадает только через левую арку."""
    return X0 + WALL - 0.02 < co.x < X1 - WALL + 0.02 and YF + WALL - 0.02 < co.y < YB - WALL + 0.02 and co.z < CEILING + 0.01


def bake_occlusion(obj):
    """Затенение в цвет вершин: запечённое Cycles для земли, крыш и стен, у земли — пятна и колеи `macro`, стены
    темнеют к земле до `GRIME`; всё внутри сарая — вдвое темнее, свет туда идёт только через открытую арку."""
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 96
    world = bpy.data.worlds.new("bake")
    world.light_settings.distance = 3.0
    scene.world = world
    bpy.ops.mesh.primitive_plane_add(size=300.0, location=(0.0, 0.0, -0.002))
    plane = bpy.context.active_object
    attr = obj.data.color_attributes.new("occlusion", "BYTE_COLOR", "CORNER")
    obj.data.color_attributes.active_color = attr
    obj.data.color_attributes.render_color_index = obj.data.color_attributes.active_color_index
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    scene.render.bake.target = "VERTEX_COLORS"
    bpy.ops.object.bake(type="AO")
    mesh = obj.data
    for poly in mesh.polygons:
        flat = abs(poly.normal.z) > 0.5
        shade = 0.5 if inside(poly.center) else 1.0
        for li in poly.loop_indices:
            co = mesh.vertices[mesh.loops[li].vertex_index].co
            ao = attr.data[li].color[0]
            if flat:
                v = 0.45 + 0.55 * ao
                if co.z < 0.2:
                    v *= macro(co.x, co.y)
            else:
                t = min(1.0, max(0.0, co.z / GRIME))
                v = (0.72 + 0.28 * t * t * (3 - 2 * t)) * (0.55 + 0.45 * ao)
            v *= shade
            attr.data[li].color = (v, v, v, 1.0)
    bpy.data.objects.remove(plane)


def build(out=OUT):
    lib.reset()
    m = Yard("garage")
    mt = materials(m)
    coach_house(m, mt)
    workshop(m, mt)
    fuel_shed(m, mt)
    pump(m, mt)
    fire_board(m, mt)
    hitching_post(m, mt)
    m.project()
    m.grid()
    ground(m, mt)
    grass(m, mt)
    obj = m.finish()
    bake_occlusion(obj)
    lib.text(obj, "ГАРАЖ", 0.2, (0.0, YF - 0.045, 2.235), 0, "#f2efe6", depth=0.008)
    pts = [obj.matrix_world @ Vector(c) for c in obj.bound_box]
    print("garage size", *(round(max(p[i] for p in pts) - min(p[i] for p in pts), 2) for i in range(3)))
    os.makedirs(out, exist_ok=True)
    lib.export(os.path.join(out, "garage.glb"), vertex_color="ACTIVE")
    print("written garage")


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    build(args[0] if args else OUT)


if __name__ == "__main__":
    main()
