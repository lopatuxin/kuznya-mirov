# Первый гараж игрока на две машины со всем двором: бывший каретный сарай купеческой усадьбы, переделанный под
# полуторку в 1930 году; рядом, слева, — дом усадьбы с конторой (`kontora.py`). Сарай из старого голого красного
# кирпича под ржавой железной крышей с фальцами, щипец сеновала с балкой и блоком над воротами, две лучковые арки
# с кирпичными перемычками и зелёными дощатыми воротами — левые распахнуты, за ними верстак и смотровая яма; на
# столбе между арками синяя эмалевая табличка «ГАРАЖ». Справа дощатая мастерская с жестяной трубой и тележным
# колесом у стены, слева навес с бочками бензина и пожарный щит с ящиком песка, коновязь. С улицы двор открыт,
# дощатый забор — только сбоку справа и сзади.
# Двор устроен под проезд полуторки игры (8,5 × 3,3 м, в 1,6 раза крупнее настоящей): от улицы к каждой арке
# и к навесу ведёт свой подъезд — колея с прозрачным краем, как тропы двора избы; ручная колонка со стеклянным
# мерником стоит на островке между подъездами к аркам — машина заправляется рядом с ней перед въездом. Земли и
# травы в модели нет: их рисует движок. Образец вида — art/transport/estate/concept.png, кирпич — без побелки.
# Размер настоящий; арки — 4 м в ширину. Улица — со стороны −Y Blender, начало — середина сарая по ширине,
# единица — метр; верх земли — ноль. Материалы — карты ambientCG и Poly Haven из art/transport/textures в
# настоящем размере, как у хлебозавода; доски, листы и кирпичи перемычек развёрнуты каждый сам по себе, затенение
# запечено в цвет вершин.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/garage.py [-- <папка>]
import math
import os
import random
import sys

import bmesh
import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402
from bakery import GRIME, TEXTURES, Plant, noise  # noqa: E402
from gaz_aa import glass  # noqa: E402
from gazel import smoothstep  # noqa: E402

OUT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "models"))

GROUND = 0.0  # земля двора — земля карты движка; ниже неё в модели ничего нет: движок ставит самую низкую точку модели на землю
# Край двора со стороны улицы: от него до оси главной улицы 9,3 м, как у двора избы, и подъезды начинаются за ним
# в 5,6 м — под укатанным краем дороги.
FRONT = -11.8
Z = Vector((0.0, 0.0, 1.0))

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
SINK = 0.03  # верх щипцов ниже кровли — уходит под её настил
GATES = (-3.2, 3.2)  # середины арок: левая распахнута, правая закрыта
GATE_W, SPRING, RISE = 4.0, 3.0, 0.8  # ширина арки, пята свода, подъём свода
VOUSSOIR = 0.25  # длина кирпича арки по радиусу
PROUD = 0.02  # кирпичи перемычек выступают из стены
DORMER = 2.6  # половина ширины щипца сеновала
DORMER_SLOPE = 1.1
DORMER_TOP = EAVE + DORMER * DORMER_SLOPE
DORMER_EAVE = DORMER + OVER * SLOPE / DORMER_SLOPE  # где скат щипца сходится со свесом крыши
DORMER_END = YF + DORMER * DORMER_SLOPE / SLOPE  # где конёк щипца упирается в скат крыши
VALLEY_RISE = DORMER_END - (YF - OVER)  # на сколько ендова уходит по плану вглубь от угла свеса до конька щипца
CEILING = 4.15  # низ пола сеновала
FLOOR = GROUND + 0.03  # пол сарая выше подъездов, что уходят под ворота
WINDOW = (6.6, 7.4, 2.15, 3.05)  # окно в левом торце: края проёма по Y и по высоте
PLATE = 0.004  # лист эмалевой таблички

# Кровля сарая: железные листы между стоячими фальцами по сплошному дощатому настилу на стропилах.
SHEET = 0.004  # лист — толще настоящего на пару миллиметров, чтобы не мерцал с настилом
DECK = 0.025  # сплошной настил под железом
SEAM_W, SEAM_H = 0.012, 0.025  # стоячий фальц: ширина и высота над листом
PITCH = 0.62  # шаг фальцев — ширина листа
DRIP = 0.035  # свес и края кровли загнуты вниз капельником
VALLEY = 0.08  # полоса ендовы — по столько с каждой стороны стыка скатов
RAFTER = 0.9  # шаг стропил

# Кирпичи-ложки на фото кладки Bricks085 (точки картинки 1024², x0, y0, x1, y1 сверху вниз): из них по одному
# кладутся перемычки арок и окна, с половиной шва вокруг; ряды кладки на фото — через 35,1 точки, середина шва
# верхнего ряда — на 38-й.
BRICK_FACES = ((170, 44, 278, 70), (288, 44, 397, 70), (558, 44, 665, 70), (273, 113, 381, 140), (738, 113, 842, 140),
               (98, 183, 205, 209), (447, 183, 551, 209), (676, 183, 784, 209), (390, 255, 499, 281), (222, 328, 327, 354),
               (680, 328, 786, 354), (56, 403, 165, 429), (516, 403, 624, 429), (746, 403, 856, 429), (343, 479, 447, 506),
               (911, 479, 1017, 506))
BRICK_PX, COURSE_PX, COURSE_TOP = 1024, 35.1, 38.0

# Мастерская справа от сарая: каркас из бруса, обшивка досками с нащельниками, тёсовая крыша скатом вправо.
WX0, WX1, WY0, WY1 = X1, 10.6, 2.6, 11.0
BOARD = 0.025  # толщина доски обшивки
STUD = 0.12  # брус каркаса
DOOR = (7.0, 7.95, 2.05)  # дверь мастерской: края по X и высота
SHOP_WINDOW = (8.35, 9.35, 1.2, 2.1)  # окно мастерской: края проёма по X и по высоте
WHEEL_R = 0.5  # тележное колесо: радиус обода по дереву, шина — поверх

# Навес с бочками слева от сарая, открыт к улице; колонка — на островке между подъездами к аркам.
FX0, FX1, FY0, FY1 = -13.0, -8.4, 2.5, 6.0
PUMP = (0.0, -2.5)
DRUM_R, DRUM_H = 0.29, 0.44  # бочка на 200 л: 0,58 × 0,88 м
HOOP = 0.014  # обручи катания выступают из обечайки — бочка лежит на них

# Подъезды: с улицы к левой и правой арке и к навесу — колеи по колее полуторки игры (2,6 м) с прозрачным краем;
# начинаются под укатанным краем улицы и уходят под ворота и под навес.
WEAR = [
    [(-7.5, FRONT - 5.6), (-5.2, -12.5), (-3.6, -7.0), (-3.2, -2.0), (-3.2, 2.5)],
    [(7.5, FRONT - 5.6), (5.2, -12.5), (3.6, -7.0), (3.2, -2.0), (3.2, 2.5)],
    [(-14.0, FRONT - 5.6), (-12.0, -11.0), (-10.9, -5.0), (-10.7, 1.8)],
]
LANE = 1.9  # полуширина колеи подъезда
# Дощатый забор: справа от мастерской и сзади за сараем, до забора дома усадьбы; с улицы двор открыт.
FENCE_X, FENCE_Y0, FENCE_Y1, FENCE_BACK_X0 = 13.5, -8.0, 15.0, -13.0


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


def arch_bricks(cx):
    """Кирпичи перемычки арки: сколько их и углы левой и правой пяты от середины круга свода."""
    pts, r, zc = arc(cx)
    left = math.atan2(pts[0][1] - zc, pts[0][0] - cx)
    right = math.atan2(pts[-1][1] - zc, pts[-1][0] - cx)
    return round(r * (left - right) / 0.078), left, right


def shed_top(x):
    """Низ стропил мастерской — верх её стен: у стены сарая 3,9 м, у правой стены 2,8 м."""
    return 3.9 - (x - WX0) / (WX1 - WX0) * 1.1


def front_z(y):
    """Высота переднего ската крыши сарая над точкой `y`."""
    return EAVE + (y - YF) * SLOPE


def back_z(y):
    """Высота заднего ската над точкой `y`."""
    return EAVE + (YB - y) * SLOPE


def dormer_z(x):
    """Высота ската щипца сеновала над точкой `x`."""
    return DORMER_TOP - abs(x) * DORMER_SLOPE


def valley_y(x):
    """Ендова щипца сеновала на плане: над точкой `x` она на этом Y — прямая от угла свеса до конца конька щипца,
    за углом свеса — её продолжение."""
    return YF - OVER + (DORMER_EAVE - abs(x)) / DORMER_EAVE * VALLEY_RISE


def curve(points, steps=6):
    """Плавная линия через точки (x, y, z) — сплайн Катмулла — Рома, `steps` отрезков между соседними точками."""
    pts = [Vector(p) for p in points]
    ext = [pts[0] * 2 - pts[1]] + pts + [pts[-1] * 2 - pts[-2]]
    out = []
    for i in range(1, len(ext) - 2):
        p0, p1, p2, p3 = ext[i - 1], ext[i], ext[i + 1], ext[i + 2]
        for s in range(steps):
            t = s / steps
            out.append(0.5 * (2 * p1 + (p2 - p0) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t + (3 * p1 - p0 - 3 * p2 + p3) * t ** 3))
    out.append(pts[-1])
    return out


class Yard(Plant):
    """Сетка двора: к материалам и развёртке хлебозавода — брусья под любым углом, доски и листы со своей
    развёрткой, круглые жгуты, плоские фигуры с толщиной, наклонные пластины, тела вращения, своды арок с
    кирпичными перемычками, створки ворот и тропы открытой сеткой."""

    def __init__(self, name):
        super().__init__(name)
        self.up = []

    def orient(self):
        """Тропы — открытая сетка, пересчёт может развернуть её вниз: её грани разворачиваются вверх."""
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

    def grain_uv(self, faces, origin, axes, grain, mi, rnd):
        """Развёртка детали в осях `axes` от точки `origin`: волокно картинки (её U) — вдоль `axes[grain]`, поперёк —
        вдоль второй оси, лежащей в грани; на торцах — две другие оси. У каждой детали свой сдвиг по картинке от
        `rnd` — соседние доски не повторяют рисунок."""
        meters = self.tiles.get(mi, (2.0, False))[0]
        su, sv = (rnd.uniform(0.0, 4.0), rnd.uniform(0.0, 4.0)) if rnd else (0.0, 0.0)
        g = axes[grain]
        others = [ax for k, ax in enumerate(axes) if k != grain]
        for f in faces:
            f.material_index = mi
            f.normal_update()
            self.manual.add(f)
            if abs(f.normal.dot(g)) > 0.7:
                ua, va = others
            else:
                ua, va = g, min(others, key=lambda ax: abs(f.normal.dot(ax)))
            for loop in f.loops:
                p = loop.vert.co - origin
                loop[self.uv].uv = (p.dot(ua) / meters + su, p.dot(va) / meters + sv)

    def panel(self, origin, ax_u, ax_v, out, outline, d0, d1, mi, grain=1, rnd=None):
        """Доска, планка или лист по плоскому контуру `outline` — точки (u, v) в плоскости `origin` + `ax_u`·u +
        `ax_v`·v — толщиной от `d0` до `d1` по `out`; контур может быть вогнутым. Волокно картинки — вдоль оси
        `grain`: 0 — u, 1 — v, 2 — `out`. Возвращает грани детали."""
        o, eu, ev, en = Vector(origin), Vector(ax_u), Vector(ax_v), Vector(out)
        ring0 = [self.bm.verts.new(o + eu * u + ev * v + en * d0) for u, v in outline]
        ring1 = [self.bm.verts.new(o + eu * u + ev * v + en * d1) for u, v in outline]
        faces = [self.bm.faces.new(list(reversed(ring0))), self.bm.faces.new(ring1)]
        for i in range(len(ring0)):
            j = (i + 1) % len(ring0)
            faces.append(self.bm.faces.new([ring0[i], ring0[j], ring1[j], ring1[i]]))
        self.grain_uv(faces, o, (eu, ev, en), grain, mi, rnd)
        return faces

    def board(self, a, b, w, h, mi, up=(0.0, 0.0, 1.0), rnd=None):
        """Брус или доска как `beam`, но со своей развёрткой: волокно картинки идёт вдоль доски."""
        a, b = Vector(a), Vector(b)
        d = (b - a).normalized()
        side = d.cross(Vector(up))
        if side.length < 1e-6:
            side = d.cross(Vector((1.0, 0.0, 0.0)))
        side.normalize()
        lift = side.cross(d).normalized()
        return self.panel(a, side, lift, d, [(-w / 2, -h / 2), (w / 2, -h / 2), (w / 2, h / 2), (-w / 2, h / 2)], 0.0, (b - a).length,
                          mi, 2, rnd)

    def split(self, faces, planes):
        """Режет грани `faces` плоскостями (точка, нормаль). Затенение запекается в вершины: край детали, ушедший под
        соседнюю (доска под нащельником, нижняя доска тёса под верхней), тёмен, и без вершин рядом с ним эта
        темнота растягивалась бы на всю деталь. Куски граней со своей развёрткой остаются со своей."""
        manual = all(f in self.manual for f in faces)
        for co, no in planes:
            edges = list({e for f in faces for e in f.edges})
            verts = list({v for e in edges for v in e.verts})
            geom = bmesh.ops.bisect_plane(self.bm, geom=verts + edges + faces, plane_co=co, plane_no=no)["geom"]
            faces = [g for g in geom if isinstance(g, bmesh.types.BMFace)]
            if manual:
                self.manual.update(faces)

    def sheet(self, points, thick, mi, along, rnd=None):
        """Лист по плоскому контуру (x, y, z): верх — контур, низ — на `thick` ниже по нормали; картинка ложится
        осью U вдоль `along`."""
        pts = [Vector(p) for p in points]
        n = Vector((0.0, 0.0, 0.0))
        for i, p in enumerate(pts):
            q = pts[(i + 1) % len(pts)]
            n += Vector(((p.y - q.y) * (p.z + q.z), (p.z - q.z) * (p.x + q.x), (p.x - q.x) * (p.y + q.y)))
        n.normalize()
        if n.z < 0:
            n = -n
        eu = (Vector(along) - n * n.dot(Vector(along))).normalized()
        ev = n.cross(eu)
        o = pts[0]
        self.panel(o, eu, ev, n, [((p - o).dot(eu), (p - o).dot(ev)) for p in pts], -thick, 0.0, mi, 0, rnd)

    def cord(self, points, r, mi, segments=8):
        """Круглый жгут радиуса `r` по ломаной (x, y, z): шланг, верёвка, пруток. Сечение идёт по линии без
        скручивания, концы закрыты; развёртка — вдоль жгута и по окружности, в метрах материала."""
        pts = [Vector(p) for p in points]
        tangents = [(pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]).normalized() for i in range(len(pts))]
        meters = self.tiles.get(mi, (2.0, False))[0]
        normal = tangents[0].orthogonal().normalized()
        rings, along = [], 0.0
        for i, (p, t) in enumerate(zip(pts, tangents)):
            if i:
                normal = tangents[i - 1].rotation_difference(t) @ normal
                normal = (normal - t * normal.dot(t)).normalized()
                along += (p - pts[i - 1]).length
            binormal = t.cross(normal)
            ring = [self.bm.verts.new(p + (normal * math.cos(2 * math.pi * k / segments) + binormal * math.sin(2 * math.pi * k / segments)) * r)
                    for k in range(segments)]
            rings.append((ring, along))
        for (r0, s0), (r1, s1) in zip(rings, rings[1:]):
            for k in range(segments):
                kk = (k + 1) % segments
                f = self.bm.faces.new([r0[k], r0[kk], r1[kk], r1[k]])
                f.material_index = mi
                f.smooth = True
                self.manual.add(f)
                for loop, (s, c) in zip(f.loops, ((s0, k), (s0, k + 1), (s1, k + 1), (s1, k))):
                    loop[self.uv].uv = (s / meters, 2 * math.pi * r * c / segments / meters)
        self.bm.faces.new(list(reversed(rings[0][0]))).material_index = mi
        self.bm.faces.new(rings[-1][0]).material_index = mi

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
        радиусом — середина донышка, `closed` — профиль замкнут в кольцо. Бока гладкие, донышки и торцы плоские.
        Развёртка своя, в метрах материала: бока — по окружности и вдоль профиля, а у материала с поворотом `swap`
        наоборот — волокно картинки идёт вдоль оси, как у точёного столба; донышки и торцы — плоской проекцией. У
        каждого тела свой сдвиг и поворот картинки — одинаковые бочки не повторяют друг друга."""
        c, a = Vector(center), Vector(axis).normalized()
        e1 = a.orthogonal().normalized()
        e2 = a.cross(e1)
        meters, swap = self.tiles.get(mi, (2.0, False))
        rnd = random.Random(f"{c.x:.3f} {c.y:.3f} {c.z:.3f}")
        turn, su, sv = rnd.uniform(0.0, 2 * math.pi), rnd.random(), rnd.random()
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
                for loop, (v, kk, nn, r) in zip(f.loops, corners):
                    if flat:
                        x, y = (v.co - c).dot(e1), (v.co - c).dot(e2)
                        u, w = x * math.cos(turn) - y * math.sin(turn), x * math.sin(turn) + y * math.cos(turn)
                    else:
                        u, w = 2 * math.pi * max(r, 0.05) * kk / segments, along[nn]
                        if swap:
                            u, w = w, u
                    loop[self.uv].uv = (u / meters + su, w / meters + sv)
        # Где профиль ломается круче 30°, кольцо рёбер острое: иначе сглаживание тянет тень по всей соседней грани.
        count = len(profile)
        for i in range(count) if closed else range(1, count - 1):
            (t0, r0), (t1, r1), (t2, r2) = profile[i - 1], profile[i], profile[(i + 1) % count]
            turn_at = abs(math.atan2(r2 - r1, t2 - t1) - math.atan2(r1 - r0, t1 - t0))
            if r1 < 1e-6 or min(turn_at, 2 * math.pi - turn_at) < math.radians(30):
                continue
            for k in range(segments):
                ring_edge = self.bm.edges.get((rings[i][k], rings[i][(k + 1) % segments]))
                if ring_edge is not None:
                    ring_edge.smooth = False

    def spandrel(self, cx, mi):
        """Стена над аркой фасада: от свода до карниза, во всю толщину стены; спереди — ряд вершин над перемычкой, чтобы
        затенение у её кирпичей не тянулось по стене до карниза. Исподу свода — своя развёртка: ряды кладки с фото идут
        поперёк стены, по ряду на кирпич перемычки, — свод выложен теми же кирпичами на ребро."""
        pts, r, zc = arc(cx)
        count, left, right = arch_bricks(cx)
        brick = r * (left - right) / count
        meters = self.tiles.get(mi, (2.0, False))[0]
        rows = [[self.bm.verts.new((x, y, z if low else EAVE)) for x, z in pts] for y in (YF, YF + WALL) for low in (True, False)]
        front_low, front_top, back_low, back_top = rows
        front_mid = [self.bm.verts.new((x, YF, z + VOUSSOIR + 0.05)) for x, z in pts]
        for i in range(len(pts) - 1):
            j = i + 1
            for quad in ((front_low[i], front_low[j], front_mid[j], front_mid[i]), (front_mid[i], front_mid[j], front_top[j], front_top[i]),
                         (back_low[i], back_top[i], back_top[j], back_low[j]), (front_top[i], front_top[j], back_top[j], back_top[i])):
                self.bm.faces.new(quad).material_index = mi
            soffit = self.bm.faces.new((front_low[i], back_low[i], back_low[j], front_low[j]))
            soffit.material_index = mi
            self.manual.add(soffit)
            for loop in soffit.loops:
                x, y, z = loop.vert.co
                s = r * (left - math.atan2(z - zc, x - cx))
                loop[self.uv].uv = ((y - YF) / meters, 1 - (COURSE_TOP + s / brick * COURSE_PX) / BRICK_PX)
        for k in (0, -1):
            self.bm.faces.new((front_low[k], front_mid[k], front_top[k], back_top[k], back_low[k])).material_index = mi

    def brick(self, quad, back, mi, rnd):
        """Кирпич перемычки на ребро: лицо `quad` — четыре точки: два угла одного торца, потом второго, вдоль кирпича —
        от первого торца ко второму; `back` — вглубь стены. На лицо ложится случайный кирпич-ложок с фото кладки с
        половиной шва вокруг, на торцы и бока — его края: шов между соседями — раствор с фото."""
        x0, y0, x1, y1 = rnd.choice(BRICK_FACES)
        ua, ub = (x0 - 3) / BRICK_PX, (x1 + 3) / BRICK_PX
        if rnd.random() < 0.5:
            ua, ub = ub, ua
        va, vb = 1 - (y0 - 3) / BRICK_PX, 1 - (y1 + 3) / BRICK_PX
        f_ = [self.bm.verts.new(Vector(p)) for p in quad]
        b_ = [self.bm.verts.new(Vector(p) + Vector(back)) for p in quad]
        # Грани и место каждого их угла на кирпиче: вдоль кирпича (0 — первый торец) и поперёк.
        for verts, spots in (((f_[0], f_[1], f_[2], f_[3]), ((0, 0), (0, 1), (1, 1), (1, 0))),
                             ((b_[3], b_[2], b_[1], b_[0]), ((1, 0), (1, 1), (0, 1), (0, 0))),
                             ((f_[0], b_[0], b_[1], f_[1]), ((0, 0), (0.06, 0), (0.06, 1), (0, 1))),
                             ((f_[3], f_[2], b_[2], b_[3]), ((1, 0), (1, 1), (0.94, 1), (0.94, 0))),
                             ((f_[0], f_[3], b_[3], b_[0]), ((0, 0), (1, 0), (1, 0.25), (0, 0.25))),
                             ((f_[1], b_[1], b_[2], f_[2]), ((0, 1), (0, 0.75), (1, 0.75), (1, 1)))):
            f = self.bm.faces.new(verts)
            f.material_index = mi
            self.manual.add(f)
            for loop, (s, t) in zip(f.loops, spots):
                loop[self.uv].uv = (ua + s * (ub - ua), va + t * (vb - va))

    def voussoirs(self, cx, bricks, rnd):
        """Кирпичная перемычка арки: кирпичи на ребро торцами по радиусу вплотную друг к другу, выступают из стены на
        `PROUD`; каждый — одним из тонов `bricks`."""
        _, r, zc = arc(cx)
        count, left, right = arch_bricks(cx)
        step = (left - right) / count
        for i in range(count):
            a0, a1 = left - i * step, left - (i + 1) * step
            quad = [(cx + rr * math.cos(a), YF - PROUD, zc + rr * math.sin(a)) for rr, a in ((r, a0), (r, a1), (r + VOUSSOIR, a1), (r + VOUSSOIR, a0))]
            self.brick(quad, (0.0, PROUD, 0.0), rnd.choice(bricks), rnd)

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

    def path(self, line, half, mi, seed, ruts=False, step=0.25, columns=12):
        """Тропа по ломаной `line` на плане: полоса с поперечными рядами вершин через `step` м, середина гуляет в
        стороны, ширина около `2 × half` меняется по длине. Насколько тропа видна, пишется в слой вершин `fade`: в
        середине 1, к неровному краю и к концам — 0; у колеи (`ruts`) между колеями трава — тропа там бледнее.
        Развёртка — в метрах материала по плану."""
        fade = self.bm.verts.layers.float.get("fade") or self.bm.verts.layers.float.new("fade")
        meters = self.tiles[mi][0]
        lengths = [0.0]
        for (ax, ay), (bx, by) in zip(line, line[1:]):
            lengths.append(lengths[-1] + math.hypot(bx - ax, by - ay))
        total = lengths[-1]

        def at(s):
            """Точка ломаной на `s` м от начала и направление там."""
            i = max(0, min(len(line) - 2, next((k for k in range(len(lengths) - 1) if s <= lengths[k + 1]), len(line) - 2)))
            (ax, ay), (bx, by) = line[i], line[i + 1]
            t = (s - lengths[i]) / max(1e-6, lengths[i + 1] - lengths[i])
            d = Vector((bx - ax, by - ay)).normalized()
            return Vector((ax + (bx - ax) * t, ay + (by - ay) * t)), d

        rows = max(2, round(total / step))
        grid = []
        for r in range(rows + 1):
            s = total * r / rows
            c, d = at(s)
            n = Vector((-d.y, d.x))
            c = c + n * (0.3 * half * (noise(s / 2.2, 0.5, seed) - 0.5))
            w = half * (0.8 + 0.4 * noise(s / 3.0, 1.5, seed + 1))
            ends = smoothstep(0.0, 0.9, s) * smoothstep(0.0, 0.9, total - s)
            ring = []
            for j in range(columns + 1):
                t = -1.35 + 2.7 * j / columns
                p = c + n * (t * w)
                ragged = abs(t) + 0.5 * (noise(s * 1.4, t * 2.0 + 3.0, seed + 2) - 0.5)
                a = (1.0 - smoothstep(0.55, 1.25, ragged)) * ends
                if ruts:
                    a *= 1.0 - 0.6 * math.exp(-(t / 0.3) ** 2) * (0.6 + 0.4 * noise(s / 1.5, 7.0, seed + 3))
                v = self.bm.verts.new((p.x, p.y, GROUND + 0.012 + 0.006 * a))
                v[fade] = a
                ring.append(v)
            grid.append(ring)
        for r in range(rows):
            for j in range(columns):
                f = self.bm.faces.new((grid[r][j], grid[r][j + 1], grid[r + 1][j + 1], grid[r + 1][j]))
                f.material_index = mi
                f.smooth = True
                self.manual.add(f)
                self.up.append(f)
                for loop in f.loops:
                    loop[self.uv].uv = (loop.vert.co.x / meters, loop.vert.co.y / meters)


class Side:
    """Стена на плане: наружная плоскость от `a` до `b` (x, y), `out` — наружу. В её осях t — вдоль стены от `a`,
    z — высота, d — от наружной плоскости наружу (минус — вглубь)."""

    def __init__(self, a, b, out):
        self.a = Vector((a[0], a[1], 0.0))
        along = Vector((b[0], b[1], 0.0)) - self.a
        self.length = along.length
        self.d = along / self.length
        self.out = Vector((out[0], out[1], 0.0))

    def at(self, t, z, d=0.0):
        return self.a + self.d * t + Z * z + self.out * d

    def shape(self, m, outline, d0, d1, mi, rnd=None, grain=1):
        """Деталь по контуру (t, z) толщиной от `d0` до `d1`; волокно — по высоте (`grain` 1) или вдоль стены (0).
        Возвращает её грани."""
        return m.panel(self.a, self.d, Z, self.out, outline, d0, d1, mi, grain, rnd)

    def box(self, m, t0, t1, z0, z1, d0, d1, mi, rnd=None, grain=1):
        self.shape(m, [(t0, z0), (t1, z0), (t1, z1), (t0, z1)], d0, d1, mi, rnd, grain)

    def strip(self, m, t0, t1, top, d0, d1, mi, rnd=None, z0=0.02):
        """Вертикальная доска от `z0` до верха стены `top(t)` — по скату крыши."""
        self.shape(m, [(t0, z0), (t1, z0), (t1, top(t1)), (t0, top(t0))], d0, d1, mi, rnd)


def faded(m, key, folder, meters, tint):
    """Материал, прозрачный по альфе цвета вершин `occlusion`: край тропы растворяется в земле карты. Выгрузка пишет
    его режимом BLEND, движок умножает прозрачность на альфу цвета вершины, как у кусков дорог. Цвет вершин умножен и
    на цвет материала — так его понимает glTF; без этого выгрузка теряет альфу («Vertex Color alpha without color»)."""
    mi = m.tex(key, folder, meters, tint=tint)
    mat = m.materials[mi]
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = lib.bsdf(mat)
    attr = nodes.new("ShaderNodeVertexColor")
    attr.layer_name = "occlusion"
    mix = nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs[0].default_value = 1.0
    links.new(bsdf.inputs["Base Color"].links[0].from_socket, mix.inputs[6])
    links.new(attr.outputs["Color"], mix.inputs[7])
    links.new(mix.outputs[2], bsdf.inputs["Base Color"])
    links.new(attr.outputs["Alpha"], bsdf.inputs["Alpha"])
    mat.surface_render_method = "BLENDED"
    return mi


def iron(m, key, meters, tint, arm):
    """Ржавое железо с фото Metal025, как в порту: своя карта шероховатости и металла папки — `arm`
    (arm_roof.jpg — кровля почти без металла, arm_iron.jpg — кованое железо)."""
    mi = m.tex(key, "Metal025", meters, tint=tint)
    node = next(n for n in m.materials[mi].node_tree.nodes
                if n.bl_idname == "ShaderNodeTexImage" and os.path.basename(n.image.filepath) == "arm.jpg")
    node.image = bpy.data.images.load(os.path.join(TEXTURES, "Metal025", arm), check_existing=True)
    node.image.colorspace_settings.name = "Non-Color"
    return mi


def painted(m, key, folder, meters, tint):
    """Крашеное железо: карты папки `folder` без металла — сверху краска, а не голый металл."""
    mi = m.tex(key, folder, meters, tint=tint)
    mat = m.materials[mi]
    bsdf = lib.bsdf(mat)
    for link in list(bsdf.inputs["Metallic"].links):
        mat.node_tree.links.remove(link)
    bsdf.inputs["Metallic"].default_value = 0.0
    return mi


def plank_fence(m, a, b, h, boards, post, rnd):
    """Дощатый забор от `a` до `b` на плане высотой около `h`: вертикальные доски по 18 см со щелями в полтора
    сантиметра стоят на линии забора и прибиты к двум слегам изнутри (к +Y или +X от линии — по левую руку от `a`
    к `b`), слеги — к столбам через ~2,5 м вплотную за ними; верх досок чуть неровный, каждая доска — одним из
    тонов `boards`."""
    a, b = Vector((a[0], a[1], 0.0)), Vector((b[0], b[1], 0.0))
    along = b - a
    length = along.length
    d = along / length
    inner = Vector((-d.y, d.x, 0.0))
    up = Vector((0.0, 0.0, 1.0))
    thick, rail = 0.025, 0.05
    rail_mid = thick / 2 + rail / 2 - 0.001  # слега на миллиметр в досках
    post_mid = thick / 2 + rail + 0.06 - 0.004  # столб на 4 мм в слегах
    n = max(1, math.ceil(length / 2.5))
    for i in range(n + 1):
        p = a + along * (i / n) + inner * post_mid
        m.beam(p, p + up * (h + 0.08), 0.12, 0.12, post, up=tuple(inner))
    for z in (0.45, h - 0.55):  # верхняя слега ниже среза затенения на `GRIME`: вершины досок там не уходят в слегу
        m.beam(a + inner * rail_mid + up * z, b + inner * rail_mid + up * z, rail, 0.1, post)
    count = round(length / 0.195)
    for i in range(count):
        p = a + d * (length * (i + 0.5) / count)
        top = h + rnd.uniform(-0.04, 0.03)
        m.beam(p + up * 0.05, p + up * top, 0.18, thick, rnd.choice(boards), up=tuple(inner))


def chimney(m, x0, y0, x1, y1, z0, z1, brick, soot, wall=0.14, cap=0.1, over=0.06):
    """Кирпичная труба над крышей: четыре стенки толщиной `wall` вокруг дымохода от `z0` до `z1`, сверху напуск на
    `over` наружу высотой `cap` с тем же отверстием; дымоход изнутри закопчён на 0,4 м вглубь, там тёмное дно — сверху
    в трубе видно чёрное отверстие."""
    ix0, iy0, ix1, iy1 = x0 + wall, y0 + wall, x1 - wall, y1 - wall
    for a0, b0, a1, b1 in ((x0, y0, x1, iy0), (x0, iy1, x1, y1), (x0, iy0, ix0, iy1), (ix1, iy0, x1, iy1)):
        m.box(a0, b0, z0, a1, b1, z1, brick)
    top = z1 + cap
    for a0, b0, a1, b1 in ((x0 - over, y0 - over, x1 + over, iy0), (x0 - over, iy1, x1 + over, y1 + over), (x0 - over, iy0, ix0, iy1),
                           (ix1, iy0, x1 + over, iy1)):
        m.box(a0, b0, z1, a1, b1, top, brick)
    deep = top - 0.4
    m.box(ix0, iy0, deep - 0.05, ix1, iy1, deep, soot)
    t = 0.006
    for a0, b0, a1, b1 in ((ix0, iy0, ix1, iy0 + t), (ix0, iy1 - t, ix1, iy1), (ix0, iy0 + t, ix0 + t, iy1 - t), (ix1 - t, iy0 + t, ix1, iy1 - t)):
        m.box(a0, b0, deep, a1, b1, top - 0.002, soot)


def drum(m, center, axis, paint):
    """Стальная бочка на 200 л: обечайка с двумя обручами катания, закатанные края-уторы, донышки утоплены на
    сантиметр; на верхнем донышке (к концу `axis`) две пробки. Её же полуторки повезут в кузове."""
    r, h = DRUM_R, DRUM_H
    profile = [(-h + 0.012, 0.0), (-h + 0.012, r - 0.012), (-h, r - 0.004), (-h, r + 0.008), (-h + 0.022, r + 0.008), (-h + 0.03, r)]
    for t in (-0.15, 0.15):
        profile += [(t - 0.03, r), (t - 0.012, r + HOOP), (t + 0.012, r + HOOP), (t + 0.03, r)]
    profile += [(h - 0.03, r), (h - 0.022, r + 0.008), (h, r + 0.008), (h, r - 0.004), (h - 0.012, r - 0.012), (h - 0.012, 0.0)]
    m.lathe(center, axis, profile, paint, 28)
    a = Vector(axis).normalized()
    lid = Vector(center) + a * (h - 0.012)
    for off, size in ((0.17, 0.032), (-0.19, 0.02)):
        m.lathe(lid + a.orthogonal().normalized() * off, a, [(0.0, 0.0), (0.0, size), (0.018, size), (0.018, 0.0)], paint, 10)


def handle(m, side, t, z, d, mi, height=0.2, reach=0.05):
    """Скоба-ручка из прутка на двери или створке `side` (у неё есть `at(t, z, d)`): ножки из плоскости `d` наружу
    на `reach`, хват по высоте `height` с серединой на `z`."""
    lo, hi = z - height / 2, z + height / 2
    pts = [side.at(t, lo, d - 0.01), side.at(t, lo, d + reach * 0.6), side.at(t, lo + 0.03, d + reach), side.at(t, hi - 0.03, d + reach),
           side.at(t, hi, d + reach * 0.6), side.at(t, hi, d - 0.01)]
    m.cord(curve(pts, 3), 0.008, mi)


def strap_hinge(m, side, t, z, d, length, mi, knuckle=0.01):
    """Полосовая петля на створке или двери `side`: полоса шириной 5 см с острым концом лежит на плоскости `d` от
    оси на `t` на `length` вдоль t; ушко на оси — на `knuckle` перед плоскостью."""
    side.shape(m, [(t, z - 0.025), (t + length - 0.06, z - 0.018), (t + length, z), (t + length - 0.06, z + 0.018), (t, z + 0.025)],
               d, d + 0.005, mi, grain=0)
    m.lathe(side.at(t, z - 0.035, d + knuckle), (0.0, 0.0, 1.0), [(0.0, 0.0), (0.0, 0.016), (0.07, 0.016), (0.07, 0.0)], mi, 12)


HANG = 0.02  # от оси навеса ворот до наружной плоскости створки


class Leaf:
    """Створка ворот, повёрнутая вокруг штыря навеса `pin` (x, y): закрытой она уходит от штыря вдоль X со знаком
    `sign`, изнутри — к +Y; `angle` — поворот в градусах против часовой. В её осях t — от штыря вдоль створки, z —
    высота, d — от оси навеса к улице у закрытой створки (наружная плоскость досок — на −`HANG`)."""

    def __init__(self, pin, sign, angle):
        rot = math.radians(angle)
        self.a = Vector((pin[0], pin[1], 0.0))
        self.d = Vector((sign * math.cos(rot), sign * math.sin(rot), 0.0))
        self.out = Vector((math.sin(rot), -math.cos(rot), 0.0))

    def at(self, t, z, d=0.0):
        return self.a + self.d * t + Z * z + self.out * d

    def shape(self, m, outline, d0, d1, mi, rnd=None, grain=1):
        m.panel(self.a, self.d, Z, self.out, outline, d0, d1, mi, grain, rnd)


def gate_leaf(m, mt, pin, sign, angle, width, top, rnd):
    """Створка ворот на двух навесах: вертикальные доски по 20 см толщиной 5 см, каждая своим тоном; изнутри две
    планки и раскос. Снаружи на высоте планок — полосовые петли, их ушки надеты на штыри навесов на оси `pin`, а
    стержни штырей вбиты в откос арки (от штыря в сторону −`sign` по X); у притвора — скоба-ручка. Верх доски на
    расстоянии t от штыря — `top(t)`, остальное — как у `Leaf`."""
    leaf = Leaf(pin, sign, angle)
    thick, pitch, gap = 0.05, 0.2, 0.012
    n = max(1, round(width / pitch))
    step = width / n
    for i in range(n):
        s0, s1 = i * step + gap / 2, (i + 1) * step - gap / 2
        outline = [(s0, GROUND + 0.03), (s1, GROUND + 0.03), (s1, top(s1)), ((s0 + s1) / 2, top((s0 + s1) / 2)), (s0, top(s0))]
        leaf.shape(m, outline, -HANG - thick, -HANG, rnd.choice(mt["gate"]), rnd)
    battens = (0.5, 2.35)
    off = -HANG - thick - 0.025
    for z in battens:
        m.board(leaf.at(0.06, z, off), leaf.at(width - 0.06, z, off), 0.05, 0.14, mt["gate"][0], rnd=rnd)
    m.board(leaf.at(0.15, battens[0] + 0.08, off), leaf.at(width - 0.15, battens[1] - 0.08, off), 0.05, 0.12, mt["gate"][1], rnd=rnd)
    for z in battens:
        strap_hinge(m, leaf, 0.0, z, -HANG, 0.86, mt["iron"], HANG)
        # Навес: штырь сквозь ушко, его стержень — горизонтально в кладку откоса.
        m.lathe((pin[0], pin[1], z - 0.06), (0.0, 0.0, 1.0), [(0.0, 0.0), (0.0, 0.009), (0.13, 0.009), (0.13, 0.0)], mt["iron"], 8)
        m.beam((pin[0], pin[1], z - 0.05), (pin[0] - sign * 0.14, pin[1], z - 0.05), 0.022, 0.022, mt["iron"])
    handle(m, leaf, width - 0.15, 1.15, -HANG, mt["iron"])


def iron_roof(m, mt):
    """Крыша сарая: железо листами между стоячими фальцами по сплошному дощатому настилу на стропилах, свесы и
    края загнуты вниз капельником. Передний скат вырезан под щипцом сеновала, скаты щипца сходятся с ним ендовами —
    полосами железа на обоих скатах; на коньках — накладки, загнутые на лист. Каждый лист — своего тона."""
    roofs, deck, log = mt["roof"], mt["deck"], mt["log"]
    rnd = random.Random(26)
    xa, xb = X0 - RAKE, X1 + RAKE
    nf = Vector((0.0, -SLOPE, 1.0)).normalized()
    nb = Vector((0.0, SLOPE, 1.0)).normalized()
    kk = VALLEY_RISE / DORMER_EAVE
    off_y = VALLEY * math.sqrt(1 + kk * kk)  # полоса ендовы на переднем скате: от стыка по Y
    off_x = VALLEY * math.sqrt(1 + 1 / (kk * kk))  # на скате щипца: от стыка по X

    def front_edge(x):
        return max(YF - OVER, valley_y(x))

    def on_front(x, y, lift=0.0):
        return Vector((x, y, front_z(y))) + nf * lift

    def on_back(x, y, lift=0.0):
        return Vector((x, y, back_z(y))) + nb * lift

    count = round((xb - xa - 0.3) / PITCH)
    seams = [xa + 0.15 + (xb - xa - 0.3) * i / count for i in range(count + 1)]
    edges = [xa] + seams + [xb]
    down_f, down_b = (0.0, -1.0, -SLOPE), (0.0, 1.0, -SLOPE)
    # Листы скатов — полосы между фальцами; передние под щипцом обрезаны по ендовам, по изломам края — на куски.
    for e0, e1 in zip(edges, edges[1:]):
        tone, seed = rnd.choice(roofs), rnd.random()
        cuts = [e0] + [c for c in (-DORMER_EAVE, 0.0, DORMER_EAVE) if e0 < c < e1] + [e1]
        for c0, c1 in zip(cuts, cuts[1:]):
            m.sheet([on_front(c0, front_edge(c0)), on_front(c1, front_edge(c1)), on_front(c1, YM), on_front(c0, YM)], SHEET, tone, down_f,
                    random.Random(seed))
        m.sheet([on_back(e0, YB + OVER), on_back(e1, YB + OVER), on_back(e1, YM), on_back(e0, YM)], SHEET, tone, down_b, random.Random(seed + 1))
    # Скаты щипца: полосы между фальцами поперёк конька щипца, от конька до ендовы.
    dormer_seams = []
    y = YF - OVER + 0.3
    while y < DORMER_END - 0.3:
        dormer_seams.append(y)
        y += PITCH
    rows = [YF - OVER] + dormer_seams + [DORMER_END]

    def valley_x(y_):
        return DORMER_EAVE * (1 - (y_ - (YF - OVER)) / VALLEY_RISE)

    for sign in (-1, 1):
        nd = Vector((sign * DORMER_SLOPE, 0.0, 1.0)).normalized()
        for y0, y1 in zip(rows, rows[1:]):
            pts = [(0.0, y0), (sign * valley_x(y0), y0), (sign * valley_x(y1), y1), (0.0, y1)]
            if abs(pts[2][0]) < 1e-6:
                pts = pts[:2] + [pts[3]]
            m.sheet([(x_, y_, dormer_z(x_)) for x_, y_ in pts], SHEET, rnd.choice(roofs), (sign, 0.0, -DORMER_SLOPE), rnd)
        # Настил под скатом щипца и прогоны под его свесом над щипцом.
        m.sheet([Vector((x_, y_, dormer_z(x_))) - nd * SHEET for x_, y_ in ((0.0, YF - OVER + 0.003), (sign * (DORMER_EAVE - 0.003), YF - OVER + 0.003),
                                                                            (0.0, DORMER_END))], DECK, deck, (0.0, 1.0, 0.0), rnd)
        x_ = sign * 1.3
        lift = SHEET + DECK + 0.06
        m.board(Vector((x_, YF - OVER + 0.03, dormer_z(x_))) - nd * lift, Vector((x_, YF + 0.15, dormer_z(x_))) - nd * lift, 0.08, 0.12, log,
                up=tuple(nd), rnd=rnd)
        for y_ in dormer_seams:
            x_end = valley_x(y_) - off_x
            if x_end < 0.05:
                continue
            p0, q0 = (Vector((sign * 0.01, y_ + w, dormer_z(0.01))) for w in (-SEAM_W / 2, SEAM_W / 2))
            p1, q1 = (Vector((sign * (valley_x(y_ + w) - off_x), y_ + w, dormer_z(valley_x(y_ + w) - off_x))) for w in (-SEAM_W / 2, SEAM_W / 2))
            seam(m, p0, p1, q0, q1, nd, roofs[0])
        # Ендова: полоса железа сложена по стыку, на переднем скате и на скате щипца.
        e, t = Vector((sign * DORMER_EAVE, YF - OVER, 0.0)), Vector((0.0, DORMER_END, 0.0))
        v = (t - e).normalized()
        across = Vector((v.y, -v.x, 0.0)) * (1 if sign > 0 else -1)  # к переднему скату
        e_d, t_d = e - across * VALLEY, t - across * VALLEY
        e_d += v * ((YF - OVER - e_d.y) / v.y)
        t_d -= v * (abs(t_d.x) / abs(v.x))
        lift = 0.0025
        m.sheet([on_front(p.x, p.y, lift) for p in (e, t, t + across * VALLEY, e + across * VALLEY)], SHEET, roofs[0], v, rnd)
        m.sheet([Vector((p.x, p.y, dormer_z(p.x) + lift)) for p in (e, e_d, t_d, t)], SHEET, roofs[0], v, rnd)
    # Настил под скатами, по нему — стропила под свесами.
    for sign in (-1, 1):
        pts = [(sign * (-xa - 0.003), YF - OVER + 0.003), (sign * DORMER_EAVE, YF - OVER + 0.003), (0.0, DORMER_END), (0.0, YM),
               (sign * (-xa - 0.003), YM)]
        m.sheet([on_front(x_, y_, -SHEET) for x_, y_ in pts], DECK, deck, (1.0, 0.0, 0.0), rnd)
    m.sheet([on_back(x_, y_, -SHEET) for x_, y_ in ((xa + 0.003, YB + OVER - 0.003), (xb - 0.003, YB + OVER - 0.003), (xb - 0.003, YM),
                                                    (xa + 0.003, YM))], DECK, deck, (1.0, 0.0, 0.0), rnd)
    lift = SHEET + DECK + 0.07
    x_ = X0 + 0.25
    while x_ < X1:
        if abs(x_) > DORMER_EAVE + 0.1:
            m.board(on_front(x_, YF - OVER + 0.02, -lift), on_front(x_, YF + 0.12, -lift), 0.06, 0.14, log, up=tuple(nf), rnd=rnd)
        m.board(on_back(x_, YB + OVER - 0.02, -lift), on_back(x_, YB - 0.12, -lift), 0.06, 0.14, log, up=tuple(nb), rnd=rnd)
        x_ += RAFTER
    z_ = DORMER_TOP - (SHEET + DECK) * math.sqrt(1 + DORMER_SLOPE ** 2) - 0.06
    m.board((0.0, YF - OVER + 0.03, z_), (0.0, YF + 0.15, z_), 0.1, 0.12, log, rnd=rnd)
    # Фальцы: на переднем скате от свеса или от полосы ендовы до конька, на заднем — от свеса до конька.
    for x_ in seams:
        sides = (x_ - SEAM_W / 2, x_ + SEAM_W / 2)
        start = [max(YF - OVER, valley_y(s) + off_y) for s in sides]
        seam(m, on_front(sides[0], start[0]), on_front(sides[0], YM - 0.02), on_front(sides[1], start[1]), on_front(sides[1], YM - 0.02), nf, roofs[0])
        seam(m, on_back(sides[0], YB + OVER), on_back(sides[0], YM + 0.02), on_back(sides[1], YB + OVER), on_back(sides[1], YM + 0.02), nb, roofs[0])
    # Капельники: свесы, торцы крыши и передние края щипца.
    for x0_, x1_ in ((xa, -DORMER_EAVE), (DORMER_EAVE, xb)):
        drip(m, on_front(x0_, YF - OVER), on_front(x1_, YF - OVER), nf, Vector(down_f).normalized(), roofs[0])
    drip(m, on_back(xa, YB + OVER), on_back(xb, YB + OVER), nb, Vector(down_b).normalized(), roofs[0])
    for x_, out in ((xa, Vector((-1.0, 0.0, 0.0))), (xb, Vector((1.0, 0.0, 0.0)))):
        drip(m, on_front(x_, YF - OVER), on_front(x_, YM), nf, out, roofs[0])
        drip(m, on_back(x_, YB + OVER), on_back(x_, YM), nb, out, roofs[0])
    for sign in (-1, 1):
        nd = Vector((sign * DORMER_SLOPE, 0.0, 1.0)).normalized()
        drip(m, Vector((0.0, YF - OVER, DORMER_TOP)), Vector((sign * DORMER_EAVE, YF - OVER, EAVE_EDGE)), nd, Vector((0.0, -1.0, 0.0)), roofs[0])
    # Коньки: накладка над фальцами, края загнуты на лист.
    rise = (SEAM_H + 0.004) * math.sqrt(1 + SLOPE * SLOPE)
    w0, w1 = 0.135, 0.15
    profile = [(YM - w1, front_z(YM - w1) + 0.001), (YM - w0, front_z(YM - w0) + rise), (YM, RIDGE + rise), (YM + w0, back_z(YM + w0) + rise),
               (YM + w1, back_z(YM + w1) + 0.001)]
    profile += [(YM + w1 - SHEET, back_z(YM + w1 - SHEET) + 0.001), (YM + w0, back_z(YM + w0) + rise - SHEET), (YM, RIDGE + rise - SHEET),
                (YM - w0, front_z(YM - w0) + rise - SHEET), (YM - w1 + SHEET, front_z(YM - w1 + SHEET) + 0.001)]
    m.solid([m.bm.verts.new((xa - 0.005, y_, z_)) for y_, z_ in profile], [m.bm.verts.new((xb + 0.005, y_, z_)) for y_, z_ in profile], roofs[0])
    rise = (SEAM_H + 0.004) * math.sqrt(1 + DORMER_SLOPE ** 2)
    profile = [(-w1, dormer_z(w1) + 0.001), (-w0, dormer_z(w0) + rise), (0.0, DORMER_TOP + rise), (w0, dormer_z(w0) + rise), (w1, dormer_z(w1) + 0.001)]
    profile += [(w1 - SHEET, dormer_z(w1 - SHEET) + 0.001), (w0, dormer_z(w0) + rise - SHEET), (0.0, DORMER_TOP + rise - SHEET),
                (-w0, dormer_z(w0) + rise - SHEET), (-w1 + SHEET, dormer_z(w1 - SHEET) + 0.001)]
    m.solid([m.bm.verts.new((x_, YF - OVER - 0.005, z_)) for x_, z_ in profile], [m.bm.verts.new((x_, DORMER_END + 0.06, z_)) for x_, z_ in profile],
            roofs[0])
    # Листы режутся вдоль: на четверти и рядом с фальцами, у края накладки конька и у краёв ендов. Края листов уходят
    # под фальцы и накладки, вершины там тёмные — без вершин рядом темнота растягивалась бы по всему листу.
    tones = set(roofs)
    planes = []  # (точка, нормаль, режет ли скаты щипца — иначе скаты крыши)
    for lines, axis, steep in ((edges, Vector((1.0, 0.0, 0.0)), False), (rows, Vector((0.0, 1.0, 0.0)), True)):
        for c0, c1 in zip(lines, lines[1:]):
            planes += [(axis * (c0 + (c1 - c0) * q), axis, steep) for q in (0.25, 0.5, 0.75)]
        planes += [(axis * (c + s * (SEAM_W / 2 + 0.012)), axis, steep) for c in lines[1:-1] for s in (-1, 1)]
    planes += [(Vector((0.0, YM + s * (w1 + 0.012), 0.0)), Vector((0.0, 1.0, 0.0)), False) for s in (-1, 1)]
    for sign in (-1, 1):
        e, t = Vector((sign * DORMER_EAVE, YF - OVER, 0.0)), Vector((0.0, DORMER_END, 0.0))
        v = (t - e).normalized()
        across = Vector((v.y, -v.x, 0.0)) * sign
        planes += [(e + across * s * (VALLEY + 0.012), across, s < 0) for s in (-1, 1)]
    for co, no, steep in planes:
        faces = []
        for f in m.bm.faces:
            if f.material_index in tones and f in m.manual:
                f.normal_update()
                if f.normal.z > 0.3 and (abs(f.normal.x) > 0.3) == steep:
                    faces.append(f)
        m.split(faces, [(co, no)])


def seam(m, p0, p1, q0, q1, n, mi):
    """Стоячий фальц: стороны от `p0` до `p1` и от `q0` до `q1` на листе, высота `SEAM_H` по нормали `n`."""
    h = Vector(n) * SEAM_H
    m.solid([m.bm.verts.new(v) for v in (p0, q0, q0 + h, p0 + h)], [m.bm.verts.new(v) for v in (p1, q1, q1 + h, p1 + h)], mi)


def drip(m, a, b, n, out, mi):
    """Капельник: край листа от `a` до `b` (по верху листа) загнут вниз по нормали `n` на `DRIP`, наружу — `out`."""
    shift = Vector(out) * SHEET / 2 - Vector(n) * DRIP / 2
    m.board(Vector(a) + shift, Vector(b) + shift, SHEET, DRIP, mi, up=tuple(n))


def coach_house(m, mt):
    """Каретный сарай: стены с арками и окном в торце, щипцы, крыша с фальцами и щипцом сеновала, труба, дверца
    сеновала с подъёмником, нутро левого проезда, ворота."""
    wall, brick = mt["wall"], mt["brick"]
    rnd = random.Random(18)
    # Стены голого кирпича: столбы между арками, стены над арками, торцы, задняя; перемычки арок кирпичом на ребро,
    # цоколь и карниз — кирпичом темнее.
    edges = [X0] + [x for cx in GATES for x in (cx - GATE_W / 2, cx + GATE_W / 2)] + [X1]
    for x0, x1 in zip(edges[::2], edges[1::2]):
        m.box(x0, YF, GROUND, x1, YF + WALL, EAVE, wall)
        m.box(x0 - (0.04 if x0 == X0 else 0.0), YF - 0.04, 0.0, x1 + (0.04 if x1 == X1 else 0.0), YF, 0.55, brick)
    for cx in GATES:
        m.spandrel(cx, wall)
        m.voussoirs(cx, mt["voussoir"], rnd)
    m.box(X0, YB - WALL, GROUND, X1, YB, EAVE, wall)
    wy0, wy1, wz0, wz1 = WINDOW
    for y0, y1, z0, z1 in ((YF + WALL, wy0, GROUND, EAVE), (wy1, YB - WALL, GROUND, EAVE), (wy0, wy1, GROUND, wz0 - 0.035),
                           (wy0, wy1, wz1, wz1 + VOUSSOIR + 0.05), (wy0, wy1, wz1 + VOUSSOIR + 0.05, EAVE)):
        m.box(X0, y0, z0, X0 + WALL, y1, z1, wall)
    m.box(X1 - WALL, YF + WALL, GROUND, X1, YB - WALL, EAVE, wall)
    m.box(X0 - 0.04, YB, 0.0, X1 + 0.04, YB + 0.04, 0.55, brick)
    m.box(X0 - 0.04, YF, 0.0, X0, YB, 0.55, brick)
    m.box(X1, YF, 0.0, X1 + 0.04, YB, 0.55, brick)
    for y in (YF, YB):
        s = -1 if y == YF else 1
        m.box(X0 - 0.1, min(y, y + s * 0.12), EAVE - 0.48, X1 + 0.1, max(y, y + s * 0.12), EAVE - 0.2, brick)
    for x, out in ((X0, WALL), (X1, -WALL)):
        m.prism([(x, YF, EAVE - SINK), (x, YB, EAVE - SINK), (x, YM, RIDGE - SINK)], (out, 0.0, 0.0), wall)
    m.prism([(-DORMER, YF, EAVE - SINK), (DORMER, YF, EAVE - SINK), (0.0, YF, DORMER_TOP - SINK)], (0.0, WALL, 0.0), wall)

    # Табличка «ГАРАЖ» на столбе между арками: тонкий лист синей эмали с белой каймой; буквы кладёт на лист `build`.
    m.box(-0.5, YF - PLATE, 2.05, 0.5, YF, 2.42, mt["enamel"])
    for x0, z0, x1, z1 in ((-0.46, 2.09, 0.46, 2.11), (-0.46, 2.36, 0.46, 2.38), (-0.46, 2.09, -0.44, 2.38), (0.44, 2.09, 0.46, 2.38)):
        m.box(x0, YF - PLATE - 0.0008, z0, x1, YF - PLATE, z1, mt["enamel_white"])

    # Окно в левом торце: проём в кладке с откосами, над ним перемычка — кирпичи на торец; в глубине проёма рама с
    # переплётом и стеклом, перед ней решётка, внизу — жестяной отлив.
    for i in range(13):
        ya, yb = wy0 - 0.1 + i / 13, wy0 - 0.1 + (i + 1) / 13
        m.brick([(X0 - PROUD, ya, wz1), (X0 - PROUD, yb, wz1), (X0 - PROUD, yb, wz1 + VOUSSOIR), (X0 - PROUD, ya, wz1 + VOUSSOIR)], (PROUD, 0.0, 0.0),
                rnd.choice(mt["voussoir"]), rnd)
    m.sheet([(X0 + 0.12, wy0 - 0.03, wz0), (X0 + 0.12, wy1 + 0.03, wz0), (X0 - 0.05, wy1 + 0.03, wz0 - 0.03), (X0 - 0.05, wy0 - 0.03, wz0 - 0.03)],
            0.003, mt["rust"], (-1.0, 0.0, 0.0), rnd)
    m.beam((X0 - 0.0515, wy0 - 0.03, wz0 - 0.045), (X0 - 0.0515, wy1 + 0.03, wz0 - 0.045), 0.003, 0.03, mt["rust"])
    side = Side((X0, wy1), (X0, wy0), (-1.0, 0.0))
    t0, t1, f0, f1 = 0.0, wy1 - wy0, -0.19, -0.12
    for a0, a1, b0, b1 in ((t0, t1, wz0, wz0 + 0.06), (t0, t1, wz1 - 0.06, wz1), (t0, t0 + 0.06, wz0, wz1), (t1 - 0.06, t1, wz0, wz1),
                           ((t0 + t1) / 2 - 0.022, (t0 + t1) / 2 + 0.022, wz0, wz1), (t0, t1, wz0 + 0.56, wz0 + 0.6)):
        side.box(m, a0, a1, b0, b1, f0, f1, mt["frame_dark"], rnd, grain=0 if a1 - a0 > b1 - b0 else 1)
    side.box(m, t0, t1, wz0, wz1, -0.157, -0.153, mt["glass"])
    for t in (0.2, 0.4, 0.6):
        m.cord([side.at(t, wz0 - 0.03, -0.05), side.at(t, wz1 + 0.03, -0.05)], 0.008, mt["iron"])
    m.cord([side.at(-0.03, (wz0 + wz1) / 2, -0.065), side.at(t1 + 0.03, (wz0 + wz1) / 2, -0.065)], 0.008, mt["iron"])

    iron_roof(m, mt)

    # Кирпичная труба на заднем скате.
    chimney(m, 3.9, 8.2, 4.55, 8.85, CEILING, RIDGE + 1.0, brick, mt["soot"], wall=0.15, cap=0.12, over=0.08)

    hayloft(m, mt, rnd)

    # Нутро: пол сеновала, тёмный пол, смотровая яма в левом проезде, верстак у задней стены, колесо.
    m.box(X0 + WALL, YF + WALL, CEILING, X1 - WALL, YB - WALL, CEILING + 0.15, mt["planks"])
    m.box(X0 + WALL, YF + WALL, GROUND, X1 - WALL, YB - WALL, FLOOR, mt["floor"])
    m.box(-3.75, 4.2, FLOOR, -2.65, 10.2, FLOOR + 0.005, mt["pit"])
    for x0, y0, x1, y1 in ((-3.9, 4.05, -2.5, 4.2), (-3.9, 10.2, -2.5, 10.35), (-3.9, 4.2, -3.75, 10.2), (-2.65, 4.2, -2.5, 10.2)):
        m.box(x0, y0, GROUND, x1, y1, FLOOR + 0.06, mt["log"])
    by0, by1 = YB - WALL - 0.65, YB - WALL - 0.02
    for i in range(4):
        y = by0 + (by1 - by0) * (i + 0.5) / 4
        m.board((-4.7, y, 0.89), (-1.7, y, 0.89), (by1 - by0) / 4 - 0.004, 0.06, rnd.choice(mt["boards"]), rnd=rnd)
    for x in (-4.6, -1.8):
        for y in (by0 + 0.06, by1 - 0.06):
            m.cbox(x, y, GROUND, 0.08, 0.08, 0.86, mt["log"])
        m.box(x - 0.025, by0 + 0.02, 0.78, x + 0.025, by1 - 0.02, 0.86, mt["log"])
    m.board((-4.6, (by0 + by1) / 2, 0.32), (-1.8, (by0 + by1) / 2, 0.32), by1 - by0 - 0.1, 0.04, mt["boards"][1], rnd=rnd)
    m.box(-2.3, by0 - 0.02, 0.92, -2.05, by0 + 0.2, 1.08, mt["iron"])
    m.board((-4.5, by1 + 0.01, 1.6), (-2.0, by1 + 0.01, 1.6), 0.8, 0.02, mt["boards"][2], up=(0.0, 1.0, 0.0), rnd=rnd)
    for i, x in enumerate((-4.2, -3.8, -3.4, -3.0, -2.6)):
        m.box(x - 0.03, by1 - 0.03, 1.35 + 0.08 * (i % 2), x + 0.03, by1, 1.85, mt["iron"])
    m.cylinder(-5.4, 9.8, FLOOR, 0.42, 0.2, mt["rubber"], 16)
    m.cylinder(-5.4, 9.8, FLOOR + 0.2, 0.24, 0.005, mt["pit"], 16)

    # Ворота на навесах в откосах арок: левые распахнуты наружу, правые закрыты на засов.
    left, right = GATES
    half = GATE_W / 2
    width = half - 0.02

    def top(cx, x0, sign):
        return lambda s: arch_z(cx, x0 + sign * s) - 0.03

    for cx, y, angle in ((left, YF + HANG + 0.005, -105), (right, YF + 0.075, 0)):
        for sign in (1, -1):
            px = cx - sign * (half - 0.02)
            gate_leaf(m, mt, (px, y), sign, angle * sign, width, top(cx, px, sign), rnd)
    y = YF + 0.075 + HANG  # наружная плоскость закрытых створок
    m.box(right - 0.32, y - 0.008, 1.38, right + 0.18, y, 1.42, mt["iron"])
    for x in (right - 0.26, right + 0.1):
        staple = [(x - 0.025, y + 0.01, 1.4), (x - 0.025, y - 0.02, 1.4), (x, y - 0.025, 1.4), (x + 0.025, y - 0.02, 1.4), (x + 0.025, y + 0.01, 1.4)]
        m.cord(curve(staple, 3),
               0.006, mt["iron"])
    m.cord([(right - 0.3, y - 0.008, 1.4), (right - 0.3, y - 0.06, 1.4)], 0.012, mt["iron"])


def hayloft(m, mt, rnd):
    """Сеновал: дверца в щипце — наличник, полотно из досок с двумя планками и раскосом на полосовых петлях; над ней
    балка подъёмника. На балке хомут из полосы с болтом, на болте блок: две щёки, ось и шкив с желобом; верёвка
    лежит в желобе — спереди спускается к крюку, сзади — к утке на столбе между арками."""
    boards, iron, log = mt["boards"], mt["iron"], mt["log"]
    front = Side((-0.8, YF), (0.8, YF), (0.0, -1.0))
    z0, z1 = EAVE + 0.3, EAVE + 1.65
    for t0, t1, b0, b1, grain in ((0.1, 0.2, z0, z1 + 0.1, 1), (1.4, 1.5, z0, z1 + 0.1, 1), (0.1, 1.5, z1, z1 + 0.1, 0)):
        front.box(m, t0, t1, b0, b1, 0.0, 0.035, rnd.choice(boards), rnd, grain)
    front.box(m, 0.05, 1.55, z0 - 0.07, z0, 0.0, 0.06, boards[0], rnd, 0)
    for i in range(6):
        front.box(m, 0.2 + i * 0.2 + 0.004, 0.2 + (i + 1) * 0.2 - 0.004, z0, z1, 0.0, 0.03, rnd.choice(boards), rnd)
    for z in (z0 + 0.15, z1 - 0.15):
        front.box(m, 0.24, 1.36, z - 0.06, z + 0.06, 0.03, 0.055, boards[1], rnd, 0)
        strap_hinge(m, front, 0.15, z, 0.055, 0.6, iron)
    m.board(front.at(0.3, z0 + 0.21, 0.0425), front.at(1.3, z1 - 0.21, 0.0425), 0.1, 0.025, boards[2], up=(0.0, -1.0, 0.0), rnd=rnd)
    handle(m, front, 1.28, (z0 + z1) / 2, 0.03, iron, 0.16, 0.04)

    m.beam((0.0, YF + 0.6, EAVE + 2.0), (0.0, YF - 1.05, EAVE + 2.0), 0.18, 0.2, log)
    py, top, bottom = YF - 0.9, EAVE + 2.1, EAVE + 1.9
    m.box(-0.096, py - 0.02, top, 0.096, py + 0.02, top + 0.006, iron)
    for x0, x1 in ((-0.096, -0.09), (0.09, 0.096)):
        m.box(x0, py - 0.02, bottom - 0.07, x1, py + 0.02, top + 0.006, iron)
    bolt = bottom - 0.045
    m.cylinder_x(0.0, py, bolt, 0.009, 0.23, iron, 10)
    hub = bolt - 0.15
    cheek = [(py - 0.035, bolt + 0.025), (py + 0.035, bolt + 0.025), (py + 0.118, hub + 0.03), (py + 0.112, hub - 0.07), (py + 0.06, hub - 0.12),
             (py - 0.06, hub - 0.12), (py - 0.112, hub - 0.07), (py - 0.118, hub + 0.03)]
    for sx in (-1, 1):
        m.panel((sx * 0.035, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0), (sx, 0.0, 0.0), cheek, 0.0, 0.008, iron, 0)
    m.cylinder_x(0.0, py, hub, 0.012, 0.1, iron, 10)
    m.lathe((0.0, py, hub), (1.0, 0.0, 0.0), [(-0.025, 0.0), (-0.025, 0.1), (-0.02, 0.098), (-0.012, 0.089), (0.0, 0.082), (0.012, 0.089),
                                             (0.02, 0.098), (0.025, 0.1), (0.025, 0.0)], mt["steel"], 32)
    rr = 0.082 + 0.011
    hook = EAVE - 0.4
    eye = hook + 0.045 + 0.12 + 0.027
    rope = [(0.0, py - rr, eye + 0.027), (0.0, py - rr, hub)]
    rope += [(0.0, py + rr * math.cos(a), hub + rr * math.sin(a)) for a in (math.pi * (1 - i / 12) for i in range(1, 13))]
    cleat = (0.9, YF - 0.045, 2.03)
    rope += [(0.0, py + rr, hub - 0.2), cleat]
    m.cord(rope, 0.011, mt["rope"], 8)
    m.cord(curve([cleat, (0.97, YF - 0.065, 2.0), (0.9, YF - 0.075, 1.97), (0.83, YF - 0.065, 2.0), (0.86, YF - 0.05, 2.03), (0.84, YF - 0.062, 1.95),
                  (0.835, YF - 0.06, 1.6)], 3), 0.011, mt["rope"], 8)
    m.board((0.78, YF - 0.05, 2.0), (1.02, YF - 0.05, 2.0), 0.03, 0.035, log, rnd=rnd)
    for x in (0.86, 0.94):
        m.box(x - 0.012, YF - 0.035, 1.985, x + 0.012, YF + 0.01, 2.015, log)
    # Крюк: кольцо на конце верёвки, стержень и загнутый крюк из круглого прутка.
    yh = py - rr
    m.lathe((0.0, yh, eye), (1.0, 0.0, 0.0), [(-0.007, 0.02), (0.007, 0.02), (0.007, 0.034), (-0.007, 0.034)], iron, 16, closed=True)
    centre = Vector((0.0, yh + 0.045, hook + 0.045))
    bend = [centre + Vector((0.0, 0.045 * math.cos(a), 0.045 * math.sin(a))) for a in (math.pi + (math.pi + 1.0) * i / 12 for i in range(13))]
    m.cord([(0.0, yh, eye - 0.02)] + [tuple(p) for p in bend], 0.01, iron, 10)


def clad(m, mt, side, top, holes, rnd):
    """Обшивка стены `side` вертикальными досками с нащельниками: доски толщиной `BOARD` около 20 см вглубь от
    наружной плоскости со щелями по 2 см, щели закрыты снаружи планками по 7 см; верх доски над точкой t — `top(t)`,
    по скату крыши. Проёмы `holes` — (t0, t1, z0, z1): доски и планки над и под проёмом прерываются, края проёма
    закрывает наличник по 8 см. Каждая доска — своим тоном."""
    cuts = sorted({0.0, side.length, *(t for h in holes for t in h[:2])})

    def spans(hole, gap=0.0):
        """Куски по высоте вне проёма `hole` (без запаса `gap` вокруг него): (низ, верх), верх None — до верха стены."""
        if hole is None:
            return [(0.02, None)]
        return ([(0.02, hole[2] - gap)] if hole[2] - gap > 0.05 else []) + [(hole[3] + gap, None)]

    def piece(t0, t1, lo, hi, d0, d1, mi):
        return side.shape(m, [(t0, lo), (t1, lo), (t1, hi if hi else top(t1)), (t0, hi if hi else top(t0))], d0, d1, mi, rnd)

    for t0, t1 in zip(cuts, cuts[1:]):
        hole = next((h for h in holes if h[0] <= t0 + 1e-6 and h[1] >= t1 - 1e-6), None)
        n = max(1, round((t1 - t0) / 0.2))
        w = (t1 - t0) / n
        for i in range(n):
            a, b = t0 + i * w + 0.01, t0 + (i + 1) * w - 0.01
            for lo, hi in spans(hole):
                m.split(piece(a, b, lo, hi, -BOARD, 0.0, rnd.choice(mt["boards"])), [(side.at(t, 0.0), side.d) for t in (a + 0.03, b - 0.03)])
            if i:
                for lo, hi in spans(hole):
                    piece(t0 + i * w - 0.035, t0 + i * w + 0.035, lo, hi, 0.0, 0.02, rnd.choice(mt["boards"]))
    for t in cuts[1:-1]:
        hole = next(h for h in holes if abs(h[0] - t) < 1e-6 or abs(h[1] - t) < 1e-6)
        for lo, hi in spans(hole, 0.08):
            piece(t - 0.035, t + 0.035, lo, hi, 0.0, 0.02, rnd.choice(mt["boards"]))
    for t0, t1, z0, z1 in holes:
        for a0, a1 in ((t0 - 0.08, t0), (t1, t1 + 0.08)):
            side.box(m, a0, a1, z0 if z0 > 0.05 else 0.02, z1 + 0.08, 0.0, 0.022, rnd.choice(mt["boards"]), rnd)
        side.box(m, t0 - 0.08, t1 + 0.08, z1, z1 + 0.08, 0.0, 0.022, rnd.choice(mt["boards"]), rnd, grain=0)


def frame(m, mt, side, posts, plate, holes, rnd):
    """Каркас за обшивкой стены `side`: брус `STUD` — нижняя обвязка по земле (кроме двери), стойки на местах `posts`,
    верхняя обвязка под стропилами (её верх над точкой t — `plate(t)`), ригели между стойками на середине высоты, у
    проёмов — подоконный и надпроёмный ригели."""
    log = mt["log"]
    d0, d1 = -BOARD - STUD, -BOARD
    lo = 0.0
    for t0, t1, z0, _ in sorted(holes):
        if z0 < 0.05:
            side.box(m, lo, t0, 0.0, STUD, d0, d1, log, rnd, 0)
            lo = t1
    side.box(m, lo, side.length, 0.0, STUD, d0, d1, log, rnd, 0)
    side.shape(m, [(0.0, plate(0.0) - STUD), (side.length, plate(side.length) - STUD), (side.length, plate(side.length)), (0.0, plate(0.0))],
               d0, d1, log, rnd, 0)
    posts = sorted(posts)
    for t in posts:
        side.box(m, t - STUD / 2, t + STUD / 2, STUD, plate(t) - STUD, d0, d1, log, rnd)
    for ta, tb in zip(posts, posts[1:]):
        hole = next((h for h in holes if ta < h[0] and h[1] < tb), None)
        if hole is None:
            side.box(m, ta + STUD / 2, tb - STUD / 2, 1.0, 1.1, d0, d1 + 0.0, log, rnd, 0)
        else:
            if hole[2] > 0.05:
                side.box(m, ta + STUD / 2, tb - STUD / 2, hole[2] - 0.1, hole[2], d0, d1, log, rnd, 0)
            side.box(m, ta + STUD / 2, tb - STUD / 2, hole[3], hole[3] + 0.1, d0, d1, log, rnd, 0)


def board_roof(m, mt, base, along, down, k, u0, u1, s0, s1, rafters, depth, rnd):
    """Тёсовая крыша в один скат: стропила 6 × `depth` см вниз по скату на местах `rafters` (от `base` вдоль конька)
    низом на плоскости через `base` с уклоном `k` вниз по `down`; поперёк них обрешётка 8 × 4 см через 45 см, по ней
    тёс в два слоя вдоль ската: нижние доски по 20 см с зазором около 10 см, верхние по 18 см — на зазорах. u — от
    `base` вдоль конька (`along`), s — от него вниз по скату на плане. Каждая доска — своим тоном."""
    base, along, down = Vector(base), Vector(along), Vector(down)
    n = (down * k + Z).normalized()

    def at(u, s, h):
        return base + along * u + down * s - Z * (k * s) + n * h

    for u in rafters:
        m.board(at(u, s0 + 0.02, depth / 2), at(u, s1 - 0.02, depth / 2), 0.06, depth, mt["log"], up=tuple(n), rnd=rnd)
    s = s0 + 0.06
    while s < s1 - 0.03:
        m.board(at(u0 + 0.03, s, depth + 0.02), at(u1 - 0.03, s, depth + 0.02), 0.08, 0.04, mt["log"], up=tuple(n), rnd=rnd)
        s += 0.45
    count = max(2, round((u1 - u0 - 0.2) / 0.3) + 1)
    pitch = (u1 - u0 - 0.2) / (count - 1)
    for i in range(count):
        u = u0 + 0.1 + i * pitch
        lower = m.board(at(u, s0, depth + 0.051), at(u, s1, depth + 0.051), 0.2, 0.022, rnd.choice(mt["boards"]), up=tuple(n), rnd=rnd)
        m.split(lower, [(at(u + w, s0, 0.0), along) for w in (-0.055, 0.055)])  # края нижней доски — под верхними
        if i:
            m.board(at(u - pitch / 2, s0, depth + 0.073), at(u - pitch / 2, s1, depth + 0.073), 0.18, 0.022, rnd.choice(mt["boards"]), up=tuple(n),
                    rnd=rnd)


def corner_boards(m, mt, sides, top, rnd):
    """Угловые доски по 10 см на концах стен: (стена, конец — 0 или 1, заходит ли за угол на толщину соседней
    обшивки с планкой)."""
    reach = BOARD + 0.022
    for side, end, over in sides:
        t0, t1 = (side.length - 0.1, side.length + (reach if over else 0.0)) if end else (-(reach if over else 0.0), 0.1)
        side.strip(m, t0, t1, lambda t, s=side: top(s, t), 0.0, 0.022, rnd.choice(mt["boards"]), rnd)


def workshop(m, mt):
    """Дощатая мастерская справа от сарая на каркасе из бруса: стены — вертикальные доски с нащельниками и
    угловыми досками, тёсовая крыша скатом вправо на стропилах, окно с переплётом в проёме, дверь из досок в коробке
    с наличником; внутри дощатый пол, верстак под окном и печка-буржуйка с жестяной трубой через крышу. У стены —
    тележное колесо, у торца — брошенные доски."""
    rnd = random.Random(27)
    k = (shed_top(WX0) - shed_top(WX1)) / (WX1 - WX0)
    depth = 0.14
    lift = (depth + 0.04) * math.sqrt(1 + k * k)  # от низа стропил до низа тёса по вертикали
    start = WX0 + 0.04  # от цоколя сарая
    front = Side((start, WY0), (WX1, WY0), (0.0, -1.0))
    back = Side((WX1, WY1), (start, WY1), (0.0, 1.0))
    right = Side((WX1, WY0), (WX1, WY1), (1.0, 0.0))

    def x_of(side, t):
        return (side.a + side.d * t).x

    def top(side, t):
        return shed_top(x_of(side, t)) + lift

    dx0, dx1, dh = DOOR
    wx0, wx1, wz0, wz1 = SHOP_WINDOW
    door = (dx0 - start, dx1 - start, 0.0, dh)
    win = (wx0 - start, wx1 - start, wz0, wz1)
    for side, holes in ((front, [door, win]), (back, []), (right, [])):
        clad(m, mt, side, lambda t, s=side: top(s, t), holes, rnd)
    corner_boards(m, mt, ((front, 1, True), (right, 0, False), (back, 0, True), (right, 1, False), (front, 0, False), (back, 1, False)), top, rnd)
    inset = BOARD + STUD / 2
    frame(m, mt, front, [STUD / 2, door[0] - STUD / 2, door[1] + STUD / 2, win[0] - STUD / 2, win[1] + STUD / 2, front.length - inset],
          lambda t: shed_top(x_of(front, t)), [door, win], rnd)
    frame(m, mt, back, [inset, 1.4, 2.6, back.length - STUD / 2], lambda t: shed_top(x_of(back, t)), [], rnd)
    frame(m, mt, right, [2.1, 4.2, 6.3], lambda t: shed_top(WX1), [], rnd)
    m.board((start + STUD / 2, WY0 + BOARD, shed_top(start + STUD) - STUD / 2), (start + STUD / 2, WY1 - BOARD, shed_top(start + STUD) - STUD / 2),
            STUD, STUD, mt["log"], rnd=rnd)
    count = 10
    rafters = [WY0 + inset + (WY1 - WY0 - 2 * inset) * i / (count - 1) for i in range(count)]
    board_roof(m, mt, (WX0, 0.0, shed_top(WX0)), (0.0, 1.0, 0.0), (1.0, 0.0, 0.0), k, WY0 - 0.35, WY1 + 0.35, 0.005, WX1 - WX0 + 0.35, rafters,
               depth, rnd)

    # Окно: коробка обшивает откосы проёма, в ней рама с переплётом и стеклом; снаружи наличник и подоконная доска.
    t0, t1 = win[0], win[1]
    reveal = BOARD + STUD
    for a0, a1, b0, b1, grain in ((t0, t1, wz0, wz0 + 0.03, 0), (t0, t1, wz1 - 0.03, wz1, 0), (t0, t0 + 0.03, wz0, wz1, 1),
                                  (t1 - 0.03, t1, wz0, wz1, 1)):
        front.box(m, a0, a1, b0, b1, -reveal, 0.0, mt["boards"][0], rnd, grain)
    mid, cross = (t0 + t1) / 2, wz0 + 0.6 * (wz1 - wz0)
    for a0, a1, b0, b1, grain in ((t0 + 0.03, t1 - 0.03, wz0 + 0.03, wz0 + 0.08, 0), (t0 + 0.03, t1 - 0.03, wz1 - 0.08, wz1 - 0.03, 0),
                                  (t0 + 0.03, t0 + 0.08, wz0 + 0.08, wz1 - 0.08, 1), (t1 - 0.08, t1 - 0.03, wz0 + 0.08, wz1 - 0.08, 1),
                                  (mid - 0.02, mid + 0.02, wz0 + 0.08, wz1 - 0.08, 1), (t0 + 0.08, t1 - 0.08, cross - 0.02, cross + 0.02, 0)):
        front.box(m, a0, a1, b0, b1, -0.09, -0.04, mt["paint"], rnd, grain)
    front.box(m, t0 + 0.06, t1 - 0.06, wz0 + 0.06, wz1 - 0.06, -0.067, -0.063, mt["glass"])
    front.box(m, t0 - 0.1, t1 + 0.1, wz0 - 0.025, wz0, -0.03, 0.07, mt["boards"][3], rnd, 0)

    # Дверь: коробка по откосам, полотно из четырёх досок вровень с обшивкой, снаружи две планки и раскос, полосовые
    # петли на ушках у наличника, скоба.
    t0, t1 = door[0], door[1]
    for a0, a1, b0, b1, grain in ((t0, t0 + 0.03, 0.0, dh, 1), (t1 - 0.03, t1, 0.0, dh, 1), (t0, t1, dh - 0.03, dh, 0)):
        front.box(m, a0, a1, b0, b1, -reveal, 0.0, mt["boards"][0], rnd, grain)
    width = t1 - t0 - 0.07
    for i in range(4):
        a = t0 + 0.035 + width * i / 4
        front.box(m, a + 0.003, a + width / 4 - 0.003, 0.01, dh - 0.035, -0.04, 0.0, rnd.choice(mt["boards"]), rnd)
    for z in (0.36, 1.66):
        front.box(m, t0 + 0.05, t1 - 0.05, z - 0.06, z + 0.06, 0.0, 0.025, mt["boards"][1], rnd, 0)
        strap_hinge(m, front, t0 - 0.03, z, 0.025, 0.5, mt["iron"])
    m.board(front.at(t0 + 0.14, 0.42, 0.0125), front.at(t1 - 0.14, 1.6, 0.0125), 0.1, 0.025, mt["boards"][2], up=(0.0, -1.0, 0.0), rnd=rnd)
    handle(m, front, t1 - 0.12, 1.0, 0.0, mt["iron"])

    # Внутри: дощатый пол, верстак под окном, печка-буржуйка на ножках, её труба уходит сквозь крышу.
    m.box(start + STUD, WY0 + BOARD + STUD, 0.0, WX1 - BOARD - STUD, WY1 - BOARD - STUD, 0.04, mt["planks"])
    for i in range(3):
        y0 = WY0 + 0.2 + i * 0.15
        m.board((wx0 - 0.05, y0 + 0.07, 0.86), (wx1 + 0.05, y0 + 0.07, 0.86), 0.146, 0.05, rnd.choice(mt["boards"]), rnd=rnd)
    for x in (wx0, wx1):
        for y in (WY0 + 0.27, WY0 + 0.58):
            m.cbox(x, y, 0.04, 0.07, 0.07, 0.795, mt["log"])
        m.box(x - 0.025, WY0 + 0.22, 0.775, x + 0.025, WY0 + 0.64, 0.835, mt["log"])
    m.box(9.35, 8.3, 0.2, 9.85, 8.7, 0.75, mt["rust"])
    for x in (9.39, 9.81):
        for y in (8.34, 8.66):
            m.cbox(x, y, 0.04, 0.04, 0.04, 0.16, mt["iron"])
    m.cylinder(9.6, 8.5, 0.75, 0.08, 4.15, mt["rust"], 12)
    m.cylinder(9.6, 8.5, 4.9, 0.2, 0.18, mt["rust"], 12, r_top=0.05)

    # Тележное колесо прислонено к стене справа от окна, верхом к нащельнику, низом — шиной на земле: обод, железная
    # шина, ступица, двенадцать спиц.
    seg = front.length - win[1]
    pitch = seg / max(1, round(seg / 0.2))
    x = start + win[1] + pitch * round((WX1 - WHEEL_R - 0.05 - start - win[1]) / pitch)
    lean = math.radians(12)
    axis = Vector((0.0, -math.cos(lean), math.sin(lean)))
    e1 = axis.orthogonal().normalized()
    e2 = axis.cross(e1)
    tyre = [e1 * math.cos(2 * math.pi * i / 40) * (WHEEL_R + 0.012) + e2 * math.sin(2 * math.pi * i / 40) * (WHEEL_R + 0.012) + axis * t
            for i in range(40) for t in (-0.032, 0.032)]
    center = Vector((x, WY0 - 0.02 - 0.0005 - max(p.y for p in tyre), 0.0005 - min(p.z for p in tyre)))
    m.lathe(center, axis, [(-0.035, WHEEL_R - 0.06), (0.035, WHEEL_R - 0.06), (0.035, WHEEL_R), (-0.035, WHEEL_R)], mt["log"], 40, closed=True)
    m.lathe(center, axis, [(-0.032, WHEEL_R - 0.0005), (0.032, WHEEL_R - 0.0005), (0.032, WHEEL_R + 0.012), (-0.032, WHEEL_R + 0.012)], mt["rust"], 40,
            closed=True)
    m.lathe(center, axis, [(-0.12, 0.0), (-0.12, 0.06), (-0.09, 0.085), (0.0, 0.095), (0.09, 0.085), (0.12, 0.06), (0.12, 0.0)], mt["log"], 16)
    for t in (-0.1, 0.1):
        m.lathe(center + axis * t, axis, [(-0.012, 0.07), (0.012, 0.07), (0.012, 0.078), (-0.012, 0.078)], mt["iron"], 16, closed=True)
    for i in range(12):
        d = e1 * math.cos(2 * math.pi * i / 12) + e2 * math.sin(2 * math.pi * i / 12)
        m.cord([center + d * 0.08, center + d * (WHEEL_R - 0.05)], 0.017, mt["log"], 6)
    for dx, dy, length in ((0.0, 0.0, 3.2), (0.15, 0.22, 2.7)):
        m.board((WX1 + 0.6 + dx, 4.0 + dy, GROUND + 0.02), (WX1 + 0.6 + dx + 0.2, 4.0 + dy + length, GROUND + 0.02), 0.2, 0.04,
                rnd.choice(mt["boards"]), rnd=rnd)


def fuel_shed(m, mt):
    """Навес слева от сарая, открытый к улице и вправо: столбы с обвязкой, тёсовая крыша скатом назад на стропилах,
    задняя и левая стены из досок с нащельниками на ригелях; бочки бензина на лагах в два ряда торцами вперёд; у
    навеса три стоячие бочки и одна на боку, готовая в кузов."""
    rnd = random.Random(28)
    log, x0, x1, front, back = mt["log"], FX0, FX1, FY0, FY1
    xm = (x0 + x1) / 2
    hi, low = 2.75, 2.3  # верх обвязки спереди и сзади
    for x in (x0, xm, x1):
        m.beam((x, front, 0.0), (x, front, hi - 0.14), 0.14, 0.14, log)
        m.beam((x, back, 0.0), (x, back, low - 0.14), 0.14, 0.14, log)
    m.beam((x0 - 0.07, front, hi - 0.08), (x1 + 0.1, front, hi - 0.08), 0.14, 0.16, log)
    m.beam((x0 - 0.07, back, low - 0.08), (x1 + 0.1, back, low - 0.08), 0.14, 0.16, log)
    for z in (0.6, 1.6):
        m.beam((x0 - 0.045, front + 0.07, z), (x0 - 0.045, back - 0.07, z), 0.05, 0.1, log)
        m.beam((x0 + 0.07, back + 0.045, z), (x1 - 0.07, back + 0.045, z), 0.05, 0.1, log)
    k = (hi - low) / (back - front)
    depth = 0.12
    lift = (depth + 0.04) * math.sqrt(1 + k * k)
    left = Side((x0 - 0.07 - BOARD, back + 0.07 + BOARD), (x0 - 0.07 - BOARD, front - 0.07), (-1.0, 0.0))
    rear = Side((x1 + 0.07, back + 0.07 + BOARD), (x0 - 0.07 - BOARD, back + 0.07 + BOARD), (0.0, 1.0))

    def top(side, t):
        return hi - k * ((side.a + side.d * t).y - front) + lift

    for side in (left, rear):
        clad(m, mt, side, lambda t, s=side: top(s, t), [], rnd)
    corner_boards(m, mt, ((rear, 1, True), (left, 0, False), (rear, 0, False), (left, 1, False)), top, rnd)
    rafters = [x0 + (x1 - x0) * i / 5 for i in range(6)]
    board_roof(m, mt, (0.0, front, hi), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0), k, x0 - 0.3, x1 + 0.3, -0.35, back - front + 0.25, rafters, depth, rnd)
    # Бочки лежат уторами на лагах — лаги под краями бочек.
    mid = front + 0.6  # у самого края навеса: глубже крыша закрывает бочки от камеры игры
    for y in (mid - 0.43, mid + 0.43):
        m.box(x0 + 0.15, y - 0.07, 0.0, x1 - 0.15, y + 0.07, GROUND + 0.12, log)
    paints = (mt["drum"], mt["drum_grey"], mt["drum"], mt["rust"], mt["drum"], mt["drum_grey"])
    facet = math.cos(math.pi / 28)  # обод бочки — 28-угольник, лежит нижней гранью: она ближе к оси, чем круг
    base = GROUND + 0.12 + (DRUM_R + 0.008) * facet
    row = [(x0 + 0.45 + 0.63 * i, base) for i in range(6)] + [(x0 + 0.765 + 0.63 * i, base + 0.52) for i in range(5)]
    for i, (x, z) in enumerate(row):
        drum(m, (x, mid, z), (0.0, -1.0, 0.0), paints[i % len(paints)])
    for i, (x, y) in enumerate(((-12.75, 1.65), (-12.85, 1.05), (-13.35, 1.4))):
        drum(m, (x, y, GROUND + DRUM_H), (0.0, 0.0, 1.0), paints[i + 1])
    drum(m, (-8.55, 0.9, GROUND + (DRUM_R + HOOP) * facet), (1.0, 0.0, 0.0), mt["drum"])


def pump(m, mt):
    """Ручная бензоколонка на бетонном островке между подъездами: чугунная тумба, колонна с поясками, стеклянный
    мерник с бензином внутри, рамой и шкалой, колпак с белым шаром — всё крашеный чугун; рычаг насоса справа,
    резиновый шланг с пистолетом в кольце на колонне слева. Насосом бензин качают в мерник, из него он самотёком
    идёт в бак."""
    x, y = PUMP
    red, iron, upward = mt["pump_red"], mt["iron"], (0.0, 0.0, 1.0)
    m.box(x - 0.6, y - 1.25, 0.0, x + 0.6, y + 1.25, GROUND + 0.15, mt["concrete"])
    base = GROUND + 0.15
    m.box(x - 0.27, y - 0.27, base, x + 0.27, y + 0.27, base + 0.32, red)
    m.box(x - 0.23, y - 0.23, base + 0.32, x + 0.23, y + 0.23, base + 0.38, red)
    col = base + 0.38
    m.lathe((x, y, col), upward, [(0.0, 0.0), (0.0, 0.2), (0.07, 0.2), (0.1, 0.165), (0.62, 0.165), (0.64, 0.18), (0.69, 0.18),
                                  (0.71, 0.165), (1.2, 0.165), (1.23, 0.205), (1.3, 0.205), (1.3, 0.0)], red)
    g0 = col + 1.3
    m.lathe((x, y, g0), upward, [(0.0, 0.0), (0.0, 0.19), (0.8, 0.19), (0.8, 0.0)], mt["fuel_glass"])
    m.lathe((x, y, g0 - 0.005), upward, [(0.0, 0.0), (0.0, 0.17), (0.475, 0.17), (0.475, 0.0)], mt["fuel"])
    for k in range(4):
        a = math.pi / 4 + k * math.pi / 2
        px, py = x + 0.205 * math.cos(a), y + 0.205 * math.sin(a)
        m.cord([(px, py, g0), (px, py, g0 + 0.8)], 0.017, red, 10)
    m.box(x - 0.025, y - 0.21, g0 + 0.06, x + 0.025, y - 0.19, g0 + 0.74, mt["enamel_white"])
    for i in range(9):
        z = g0 + 0.08 + i * 0.08
        m.box(x - 0.018, y - 0.215, z, x + (0.006 if i % 2 else 0.018), y - 0.21, z + 0.008, iron)
    cap = g0 + 0.8
    m.lathe((x, y, cap), upward, [(0.0, 0.0), (0.0, 0.235), (0.06, 0.235), (0.09, 0.2), (0.2, 0.09), (0.22, 0.06), (0.22, 0.0)], red)
    m.lathe((x, y, cap + 0.19), upward, [(0.16 - 0.16 * math.cos(math.pi * k / 10), 0.16 * math.sin(math.pi * k / 10)) for k in range(11)],
            mt["globe"], 20)
    # Рычаг качается вокруг вала, торчащего из колонны вбок, — в плоскости вдоль её бока; ручка деревянная.
    m.lathe((x + 0.165, y, col + 0.55), (1.0, 0.0, 0.0), [(0.0, 0.0), (0.0, 0.05), (0.06, 0.05), (0.06, 0.0)], iron, 12)
    m.cord([(x + 0.2425, y, col + 0.55), (x + 0.2425, y - 0.3, col + 1.0)], 0.016, iron, 10)
    m.cord([(x + 0.2425, y - 0.29, col + 0.985), (x + 0.2425, y - 0.37, col + 1.13)], 0.022, mt["log"], 10)
    # Шланг из колонны петлёй вниз к пистолету; пистолет — трубка с изогнутым носиком в кольце на колонне.
    hose = curve([(x - 0.15, y, g0 - 0.05), (x - 0.33, y, g0 - 0.2), (x - 0.42, y, col + 0.75), (x - 0.4, y, col + 0.35), (x - 0.3, y, col + 0.18),
                  (x - 0.22, y, col + 0.27), (x - 0.21, y, col + 0.55)], 6)
    m.cord(hose, 0.02, mt["rubber"], 12)
    m.lathe((x - 0.21, y, col + 0.53), upward, [(0.0, 0.0), (0.0, 0.025), (0.07, 0.025), (0.07, 0.0)], iron, 12)
    m.cord(curve([(x - 0.21, y, col + 0.6), (x - 0.21, y, col + 0.82), (x - 0.225, y, col + 0.885), (x - 0.27, y, col + 0.93)], 4), 0.013, iron, 10)
    m.lathe((x - 0.21, y, col + 0.78), upward, [(-0.006, 0.019), (0.006, 0.019), (0.006, 0.027), (-0.006, 0.027)], iron, 12, closed=True)
    m.cord([(x - 0.16, y, col + 0.78), (x - 0.186, y, col + 0.78)], 0.007, iron, 8)


def fire_board(m, mt):
    """Пожарный щит на двух столбах между навесом и сараем: красная доска под козырьком; на крюках из прутка — два
    конусных ведра за дужки, топор, лопата и багор; перед щитом ящик с песком."""
    x0, x1, y = -8.15, -6.75, 4.6
    log, red, iron, steel = mt["log"], mt["red"], mt["iron"], mt["steel"]
    for x in (x0 + 0.1, x1 - 0.1):
        m.beam((x, y + 0.08, 0.0), (x, y + 0.08, 2.3), 0.1, 0.1, log)
    m.box(x0, y, 0.75, x1, y + 0.04, 2.1, red)
    m.plate([(x0 - 0.1, y - 0.25, 2.18), (x1 + 0.1, y - 0.25, 2.18), (x1 + 0.1, y + 0.1, 2.32), (x0 - 0.1, y + 0.1, 2.32)], 0.04, red)

    def hook(x, z, reach, rise=0.035):
        """Крюк из прутка: из доски щита вперёд на `reach`, конец загнут вверх на `rise`."""
        m.cord(curve([(x, y + 0.015, z), (x, y - reach * 0.75, z), (x, y - reach, z + rise * 0.4), (x, y - reach, z + rise)], 4), 0.006, iron, 6)

    # Конусные вёдра: полые, с закатанным краем, дужки висят на крюках, край ведра — у доски.
    for bx in (x0 + 0.2, x0 + 0.55):
        cy = y - 0.158
        m.lathe((bx, cy, 1.2), (0.0, 0.0, 1.0), [(0.0, 0.0), (0.01, 0.006), (0.39, 0.146), (0.396, 0.152), (0.4, 0.158), (0.404, 0.152),
                                                 (0.4, 0.144), (0.02, 0.006), (0.02, 0.0)], mt["pump_red"], 24)
        for sx in (-1, 1):
            m.cbox(bx + sx * 0.152, cy, 1.57, 0.012, 0.016, 0.03, iron)
        m.cord([(bx + 0.158 * math.cos(math.pi * i / 16), cy, 1.585 + 0.16 * math.sin(math.pi * i / 16)) for i in range(17)], 0.004, iron, 6)
        hook(bx, 1.735, 0.18)
    # Топор висит обухом на двух крюках, топорище — вниз; лезвие клином от обуха к острию, с бородкой.
    ax, ay = -7.27, y - 0.019
    m.cord([(ax, ay, 1.0), (ax, ay, 1.6)], 0.016, log, 8)
    outline = [(-0.03, 1.528), (-0.03, 1.612), (0.03, 1.62), (0.1, 1.626), (0.165, 1.645), (0.176, 1.6), (0.178, 1.55), (0.17, 1.5), (0.155, 1.458),
               (0.11, 1.472), (0.06, 1.503), (0.03, 1.52)]

    def thick(u):
        return 0.035 - 0.031 * min(1.0, max(0.0, (u - 0.03) / 0.135))

    m.solid([m.bm.verts.new((ax + u, ay - thick(u) / 2, z)) for u, z in outline], [m.bm.verts.new((ax + u, ay + thick(u) / 2, z)) for u, z in outline],
            steel)
    hook(ax - 0.022, 1.521, 0.05, 0.02)
    hook(ax + 0.045, 1.5055, 0.05, 0.02)
    # Лопата штыковая, полотном вниз: черенок в двух скобах, тулейка, полотно чуть выгнуто.
    sx_, sy = -6.95, y - 0.026
    m.cord([(sx_, sy, 1.15), (sx_, sy, 1.88)], 0.018, log, 8)
    m.lathe((sx_, sy, 1.07), (0.0, 0.0, 1.0), [(0.0, 0.0), (0.0, 0.016), (0.11, 0.022), (0.11, 0.0)], steel, 12)
    rows, cols = 10, 8
    grid = []
    for i in range(rows + 1):
        v = i / rows
        half = 0.004 + 0.096 * min(1.0, v / 0.4) ** 0.6
        grid.append([(sx_ + half * (2 * j / cols - 1), sy - 0.012 * (1 - (2 * j / cols - 1) ** 2), 0.8 + 0.3 * v) for j in range(cols + 1)])
    fr = [[m.bm.verts.new(p) for p in row] for row in grid]
    bk = [[m.bm.verts.new((p[0], p[1] + 0.003, p[2])) for p in row] for row in grid]
    for i in range(rows):
        for j in range(cols):
            m.bm.faces.new((fr[i][j], fr[i][j + 1], fr[i + 1][j + 1], fr[i + 1][j])).material_index = steel
            m.bm.faces.new((bk[i][j], bk[i + 1][j], bk[i + 1][j + 1], bk[i][j + 1])).material_index = steel
    rim = ([(i, 0) for i in range(rows)] + [(rows, j) for j in range(cols)] + [(i, cols) for i in range(rows, 0, -1)]
           + [(0, j) for j in range(cols, 0, -1)])
    for (i0, j0), (i1, j1) in zip(rim, rim[1:] + rim[:1]):
        m.bm.faces.new((fr[i0][j0], fr[i1][j1], bk[i1][j1], bk[i0][j0])).material_index = steel
    for z in (1.35, 1.78):
        arc_ = [(sx_ + 0.024 * math.cos(a), sy + 0.024 * math.sin(a), z) for a in (math.pi + math.pi * i / 8 for i in range(9))]
        m.cord([(sx_ - 0.024, y + 0.01, z)] + arc_ + [(sx_ + 0.024, y + 0.01, z)], 0.006, iron, 6)
    # Багор на двух крюках над вёдрами: древко, железная тулейка, остриё и загнутый вверх крюк.
    bz, by_ = 1.98, y - 0.026
    m.cord([(x0 + 0.07, by_, bz), (x1 - 0.22, by_, bz)], 0.02, log, 8)
    m.lathe((x1 - 0.24, by_, bz), (1.0, 0.0, 0.0), [(0.0, 0.0), (0.0, 0.024), (0.1, 0.018), (0.1, 0.0)], steel, 12)
    m.lathe((x1 - 0.14, by_, bz), (1.0, 0.0, 0.0), [(0.0, 0.0), (0.0, 0.013), (0.09, 0.0)], steel, 10)
    m.cord(curve([(x1 - 0.17, by_, bz + 0.01), (x1 - 0.16, by_, bz + 0.06), (x1 - 0.19, by_, bz + 0.095), (x1 - 0.225, by_, bz + 0.075)], 4), 0.008,
           steel, 8)
    for x in (x0 + 0.2, x1 - 0.35):
        hook(x, bz - 0.026, 0.055, 0.03)
    m.box(-8.05, 3.75, 0.0, -6.85, 4.35, 0.72, mt["boards"][0])
    m.box(-7.97, 3.83, 0.72, -6.93, 4.27, 0.73, mt["sand"])


def hitching_post(m, mt):
    """Коновязь с лошадиных времён справа во дворе: два столба и жердь."""
    for x in (9.0, 12.0):
        m.cylinder(x, -4.0, 0.0, 0.1, 1.15, mt["log"], 10)
    m.cylinder_x(10.5, -4.0, 1.0, 0.075, 3.6, mt["log"], 10)


def shades(m, key, folder, meters, tints, swap=False):
    """Несколько материалов одной картинки разными тонами: каждая доска или лист берёт один из них."""
    return [m.tex(f"{key}_{i}", folder, meters, tint=t, swap=swap) for i, t in enumerate(tints)]


def materials(m):
    # Кирпич — тот же Bricks085, что у дома усадьбы; перемычки — с той же картинки, развёрнутой по кирпичу.
    # Кровля — ржавое железо Metal025 с картой arm_roof, как в порту: картинка на 4 м, три тона листов.
    # Серые выветренные доски — Wood033 с тонами досок дома и дворов деревни; ворота — крашеные доски PaintedWood009C.
    return {
        "wall": m.tex("wall", "Bricks085", 2.0, tint="#ffd0c0"),
        "brick": m.tex("brick", "Bricks085", 2.0, tint="#e0b4a6"),
        "voussoir": shades(m, "voussoir", "Bricks085", 2.0, ("#ffd0c0", "#e8b8a8")),
        "roof": [iron(m, f"roof_{i}", 4.0, t, "arm_roof.jpg") for i, t in enumerate(("#ffa47e", "#f29a76", "#ffae8a"))],
        "deck": m.tex("deck", "Wood033", 1.6, tint="#a19a8f"),
        "gate": shades(m, "gate", "PaintedWood009C_1k", 1.2, ("#77ae77", "#6fa46f", "#80b67c", "#72a872"), swap=True),
        "boards": shades(m, "board", "Wood033", 1.6, ("#b8b0a4", "#a9a195", "#c3bbaf", "#a19a8f"), swap=True),
        "post": m.tex("post", "Wood059", 1.4, tint="#8b8079"),
        "log": m.tex("log", "Wood062", 1.6),
        "planks": m.tex("planks", "Planks012", 2.1),
        "paint": m.tex("paint", "PaintedWood009C_1k", 1.2, tint="#e8e2d6"),
        "frame_dark": m.tex("frame_dark", "PaintedWood009C_1k", 1.2, tint="#7a6a5c"),
        "red": m.tex("red", "PaintedWood003", 1.2),
        "pump_red": painted(m, "pump_red", "Metal049A", 1.0, "#a8382c"),
        "steel": iron(m, "steel", 0.8, "#7a6e66", "arm_iron.jpg"),
        "drum": m.tex("drum", "green_metal_rust_2k", 1.6),
        "drum_grey": m.tex("drum_grey", "green_metal_rust_2k", 1.6, tint="#a9b4bd"),
        "rust": iron(m, "rust", 0.8, "#c09078", "arm_roof.jpg"),
        "concrete": m.tex("concrete", "garage_floor", 1.89),
        "floor": m.tex("floor", "Ground103", 2.5, tint="#9a948c"),
        "path": faded(m, "path", "Ground103", 2.5, "#e6dccd"),
        "soot": m.mat("soot", "#0e0d0c", 0.95),
        "iron": m.mat("iron", "#2c2d2f", 0.55, 0.5),
        "rubber": m.tex("rubber", "Rubber004", 0.5, tint="#7a7a7a"),
        "rope": m.tex("rope", "Rope001", 0.25, tint="#d8c8a8"),
        "pit": m.mat("pit", "#0f0f10", 0.9),
        "sand": m.mat("sand", "#c4a873", 0.95),
        "glass": glass(m, "glass", "#33403f", 0.35),
        "fuel_glass": glass(m, "fuel_glass", "#d6e2e0", 0.28),
        "fuel": m.mat("fuel", "#c8862e", 0.12),
        "globe": m.mat("globe", "#f1ece0", 0.25),
        "enamel": m.mat("enamel", "#1c3c8c", 0.25),
        "enamel_white": m.mat("enamel_white", "#f2efe6", 0.3),
    }


def inside(co):
    """Точка внутри сарая — под полом сеновала, между стенами: туда свет попадает только через левую арку."""
    return X0 + WALL - 0.02 < co.x < X1 - WALL + 0.02 and YF + WALL - 0.02 < co.y < YB - WALL + 0.02 and co.z < CEILING + 0.01


def cut_walls(m):
    """Режет грани сарая для запекания затенения в вершины. По краю нутра — по внутренним плоскостям стен и по полу
    сеновала: затенение нутра решается по вершинам, и грань по обе стороны края тянула бы его полосой через себя.
    Над цоколем и под карнизом: низ и верх стены ушли под них, там вершины тёмные, и без вершин рядом эта темнота
    растягивалась бы на всю стену."""
    for axis, c in ((0, X0 + WALL), (0, X1 - WALL), (1, YF + WALL), (1, YB - WALL), (2, CEILING), (2, 0.6), (2, EAVE - 0.5)):
        co, no = [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]
        co[axis], no[axis] = c, 1.0
        m.split([f for f in m.bm.faces if all(X0 - 0.5 < v.co.x < X1 + 0.5 and YF - 0.5 < v.co.y < YB + 0.5 for v in f.verts)], [(co, no)])


def bake_occlusion(obj, path):
    """Затенение в цвет вершин: запечённое Cycles над плоскостью на месте земли карты, стены темнеют к земле до
    `GRIME`; нутро сарая — вдвое темнее, свет туда идёт только через открытую арку: решается по каждой вершине
    грани. У граней подъездов (материал `path`) в альфу цвета ложится слой вершин `fade` — как колея видна; сам
    слой после этого убирается."""
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 96
    world = bpy.data.worlds.new("bake")
    world.light_settings.distance = 3.0
    scene.world = world
    bpy.ops.mesh.primitive_plane_add(size=300.0, location=(0.0, 0.0, GROUND - 0.002))
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
    fade = mesh.attributes["fade"].data
    for poly in mesh.polygons:
        flat = abs(poly.normal.z) > 0.5
        for li in poly.loop_indices:
            vi = mesh.loops[li].vertex_index
            co = mesh.vertices[vi].co
            ao = attr.data[li].color[0]
            if flat:
                v = 0.45 + 0.55 * ao
            else:
                v = (0.75 + 0.25 * smoothstep(0.0, GRIME, co.z - GROUND)) * (0.55 + 0.45 * ao)
            if inside(co):
                v *= 0.5
            attr.data[li].color = (v, v, v, fade[vi].value if poly.material_index == path else 1.0)
    mesh.attributes.remove(mesh.attributes["fade"])
    bpy.data.objects.remove(plane)


def fences(m, mt):
    """Дощатый забор справа от мастерской и сзади за сараем — до забора дома усадьбы."""
    rnd = random.Random(19)
    plank_fence(m, (FENCE_X, FENCE_Y0), (FENCE_X, FENCE_Y1), 1.8, mt["boards"], mt["post"], rnd)
    plank_fence(m, (FENCE_X, FENCE_Y1), (FENCE_BACK_X0, FENCE_Y1), 1.8, mt["boards"], mt["post"], rnd)


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
    fences(m, mt)
    m.project()
    m.grid()
    cut_walls(m)
    for i, line in enumerate(WEAR):
        m.path(line, LANE, mt["path"], 60 + i, ruts=True)
    obj = m.finish()
    bake_occlusion(obj, mt["path"])
    lib.text(obj, "ГАРАЖ", 0.2, (0.0, YF - PLATE - 0.0006, 2.235), 0, "#f2efe6", depth=0.0004)
    pts = [obj.matrix_world @ Vector(c) for c in obj.bound_box]
    print("garage size", *(round(max(p[i] for p in pts) - min(p[i] for p in pts), 2) for i in range(3)), "faces", len(obj.data.polygons))
    os.makedirs(out, exist_ok=True)
    lib.export(os.path.join(out, "garage.glb"), vertex_color="ACTIVE")
    print("written garage")


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    build(args[0] if args else OUT)


if __name__ == "__main__":
    main()
