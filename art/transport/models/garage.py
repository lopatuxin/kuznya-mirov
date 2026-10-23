# Первый гараж игрока на две машины со всем двором: бывший каретный сарай купеческой усадьбы, переделанный под
# полуторку в 1930 году; рядом, слева, — дом усадьбы с конторой (`kontora.py`). Сарай из старого голого красного
# кирпича под ржавой железной крышей с фальцами, щипец сеновала с балкой и крюком над воротами, две лучковые арки
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
# настоящем размере, как у хлебозавода; затенение запечено в цвет вершин.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/garage.py [-- <папка>]
import math
import os
import random
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402
from bakery import GRIME, Plant, noise  # noqa: E402
from gaz_aa import glass  # noqa: E402
from gazel import smoothstep  # noqa: E402

OUT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "models"))

GROUND = 0.0  # земля двора — земля карты движка; ниже неё в модели ничего нет: движок ставит самую низкую точку модели на землю
# Край двора со стороны улицы: от него до оси главной улицы 9,3 м, как у двора избы, и подъезды начинаются за ним
# в 5,6 м — под укатанным краем дороги.
FRONT = -11.8

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
FLOOR = GROUND + 0.03  # пол сарая выше подъездов, что уходят под ворота

# Мастерская справа от сарая: стены по высоте `shed_top`, крыша скатом вправо.
WX0, WX1, WY0, WY1 = X1, 10.6, 2.6, 11.0
BOARD = 0.12  # толщина дощатой стены

# Навес с бочками слева от сарая, открыт к улице; колонка — на островке между подъездами к аркам.
FX0, FX1, FY0, FY1 = -13.0, -8.4, 2.5, 6.0
PUMP = (0.0, -2.5)
DRUM_R, DRUM_H = 0.29, 0.44  # бочка на 200 л: 0,58 × 0,88 м

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


def shed_top(x):
    """Верх стен мастерской: у стены сарая 3,9 м, у правой стены 2,8 м."""
    return 3.9 - (x - WX0) / (WX1 - WX0) * 1.1


class Yard(Plant):
    """Сетка двора: к материалам и развёртке хлебозавода — брусья под любым углом, плоские фигуры с толщиной,
    наклонные пластины, тела вращения, своды арок, створки ворот и тропы открытой сеткой."""

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
        полосой под кирпичами. Выступает из стены на 3,5 см."""
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


def plank_fence(m, a, b, h, boards, post, rnd):
    """Дощатый забор от `a` до `b` на плане высотой около `h`: столбы через ~2,5 м, две слеги изнутри (к +Y или +X
    от линии забора — по левую руку от `a` к `b`), вертикальные доски по 18 см со щелями в полтора сантиметра, верх
    чуть неровный; каждая доска — одним из тонов `boards`."""
    a, b = Vector((a[0], a[1], 0.0)), Vector((b[0], b[1], 0.0))
    along = b - a
    length = along.length
    d = along / length
    inner = Vector((-d.y, d.x, 0.0))
    up = Vector((0.0, 0.0, 1.0))
    n = max(1, math.ceil(length / 2.5))
    for i in range(n + 1):
        p = a + along * (i / n) + inner * 0.09
        m.beam(p, p + up * (h + 0.08), 0.12, 0.12, post, up=tuple(inner))
    for z in (0.45, h - 0.4):
        m.beam(a + inner * 0.06 + up * z, b + inner * 0.06 + up * z, 0.05, 0.1, post)
    count = round(length / 0.195)
    for i in range(count):
        p = a + d * (length * (i + 0.5) / count)
        top = h + rnd.uniform(-0.04, 0.03)
        m.beam(p + up * 0.05, p + up * top, 0.18, 0.025, rnd.choice(boards), up=tuple(inner))


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
        profile += [(t - 0.03, r), (t - 0.012, r + 0.014), (t + 0.012, r + 0.014), (t + 0.03, r)]
    profile += [(h - 0.03, r), (h - 0.022, r + 0.008), (h, r + 0.008), (h, r - 0.004), (h - 0.012, r - 0.012), (h - 0.012, 0.0)]
    m.lathe(center, axis, profile, paint, 28)
    a = Vector(axis).normalized()
    lid = Vector(center) + a * (h - 0.012)
    for off, size in ((0.17, 0.032), (-0.19, 0.02)):
        m.lathe(lid + a.orthogonal().normalized() * off, a, [(0.0, 0.0), (0.0, size), (0.018, size), (0.018, 0.0)], paint, 10)


def coach_house(m, mt):
    """Каретный сарай: стены с арками, щипцы, крыша с фальцами и щипцом сеновала, труба, нутро левого проезда."""
    wall, brick = mt["wall"], mt["brick"]
    # Стены голого кирпича: столбы между арками, стены над арками, торцы, задняя; перемычки арок кирпичом на ребро,
    # цоколь и карниз — кирпичом темнее.
    edges = [X0] + [x for cx in GATES for x in (cx - GATE_W / 2, cx + GATE_W / 2)] + [X1]
    for x0, x1 in zip(edges[::2], edges[1::2]):
        m.box(x0, YF, GROUND, x1, YF + WALL, EAVE, wall)
        m.box(x0 - (0.04 if x0 == X0 else 0.0), YF - 0.04, 0.0, x1 + (0.04 if x1 == X1 else 0.0), YF, 0.55, brick)
    rnd = random.Random(18)
    for cx in GATES:
        m.spandrel(cx, wall)
        m.voussoirs(cx, (mt["voussoir"], mt["voussoir_dark"]), mt["mortar"], rnd)
    m.box(X0, YB - WALL, GROUND, X1, YB, EAVE, wall)
    m.box(X0, YF + WALL, GROUND, X0 + WALL, YB - WALL, EAVE, wall)
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
    chimney(m, 3.9, 8.2, 4.55, 8.85, CEILING, RIDGE + 1.0, brick, mt["soot"], wall=0.15, cap=0.12, over=0.08)

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
    """Дощатая мастерская справа от сарая: односкатная крыша, окно с переплётом, дверь, ржавая жестяная труба; у стены —
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
    # Ржавая жестяная труба печки с колпаком.
    m.cylinder(9.6, 8.5, 2.6, 0.1, 2.3, mt["rust"], 12)
    m.cylinder(9.6, 8.5, 4.9, 0.2, 0.18, mt["rust"], 12, r_top=0.05)
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
    paints = (mt["drum"], mt["drum_grey"], mt["drum"], mt["rust"], mt["drum"], mt["drum_grey"])
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


def materials(m):
    return {
        "wall": m.tex("wall", "Bricks085", 2.0, tint="#e0c4b8"),
        "brick": m.tex("brick", "Bricks085", 2.0, tint="#b8a096"),
        "voussoir": m.mat("voussoir", "#8a4634", 0.85),
        "voussoir_dark": m.mat("voussoir_dark", "#76392b", 0.85),
        "mortar": m.mat("mortar", "#b3aa9c", 0.95),
        "roof": m.tex("roof", "MetalPlates011", 2.0, tint="#b8a8a8"),
        "gate": m.tex("gate", "PaintedWood009C", 1.2, tint="#6b896b", swap=True),
        "planks": m.tex("planks", "Planks012", 2.1, swap=True),
        "shed_wall": m.tex("shed_wall", "Planks012", 2.1, tint="#c8c2ba", swap=True),
        "shed_roof": m.tex("shed_roof", "Planks012", 2.1, tint="#b0aca6"),
        "fence": [m.tex(f"fence_{i}", "Wood033", 1.6, tint=t, swap=True) for i, t in enumerate(("#b8b0a4", "#a9a195", "#c3bbaf", "#a19a8f"))],
        "post": m.tex("post", "Wood059", 1.4, tint="#8b8079"),
        "log": m.tex("log", "Wood062", 1.6),
        "red": m.tex("red", "PaintedWood003", 1.2),
        "drum": m.tex("drum", "green_metal_rust", 1.6),
        "drum_grey": m.tex("drum_grey", "green_metal_rust", 1.6, tint="#a9b4bd"),
        "rust": m.tex("rust", "MetalPlates011", 2.0),
        "concrete": m.tex("concrete", "garage_floor", 1.89),
        "floor": m.tex("floor", "Ground103", 2.5, tint="#9a948c"),
        "path": faded(m, "path", "Ground103", 2.5, "#e6dccd"),
        "soot": m.mat("soot", "#0e0d0c", 0.95),
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
    }


def inside(co):
    """Точка внутри сарая — под полом сеновала, между стенами: туда свет попадает только через левую арку."""
    return X0 + WALL - 0.02 < co.x < X1 - WALL + 0.02 and YF + WALL - 0.02 < co.y < YB - WALL + 0.02 and co.z < CEILING + 0.01


def bake_occlusion(obj, path):
    """Затенение в цвет вершин: запечённое Cycles над плоскостью на месте земли карты, стены темнеют к земле до
    `GRIME`; всё внутри сарая — вдвое темнее, свет туда идёт только через открытую арку. У граней подъездов (материал
    `path`) в альфу цвета ложится слой вершин `fade` — как колея видна; сам слой после этого убирается."""
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
        shade = 0.5 if inside(poly.center) else 1.0
        for li in poly.loop_indices:
            vi = mesh.loops[li].vertex_index
            co = mesh.vertices[vi].co
            ao = attr.data[li].color[0]
            if flat:
                v = 0.45 + 0.55 * ao
            else:
                v = (0.72 + 0.28 * smoothstep(0.0, GRIME, co.z - GROUND)) * (0.55 + 0.45 * ao)
            v *= shade
            attr.data[li].color = (v, v, v, fade[vi].value if poly.material_index == path else 1.0)
    mesh.attributes.remove(mesh.attributes["fade"])
    bpy.data.objects.remove(plane)


def fences(m, mt):
    """Дощатый забор справа от мастерской и сзади за сараем — до забора дома усадьбы."""
    rnd = random.Random(19)
    plank_fence(m, (FENCE_X, FENCE_Y0), (FENCE_X, FENCE_Y1), 1.8, mt["fence"], mt["post"], rnd)
    plank_fence(m, (FENCE_X, FENCE_Y1), (FENCE_BACK_X0, FENCE_Y1), 1.8, mt["fence"], mt["post"], rnd)


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
    for i, line in enumerate(WEAR):
        m.path(line, LANE, mt["path"], 60 + i, ruts=True)
    obj = m.finish()
    bake_occlusion(obj, mt["path"])
    lib.text(obj, "ГАРАЖ", 0.2, (0.0, YF - 0.045, 2.235), 0, "#f2efe6", depth=0.008)
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
