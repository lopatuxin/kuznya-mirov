# ГАЗ-АА «полуторка» довоенного выпуска и машины на её шасси: бортовая, фургон и самосвал ГАЗ-410.
# Размеры по паспорту: длина 5,335 м, база 3,44, ширина 2,04, высота по кабине 1,97; колея передних колёс
# 1,405, задних 1,6; шины 6,50-20, задние спаренные. Форма — по фотографиям сохранившихся машин: узкий капот
# с жалюзи сбоку, высокая облицовка радиатора, фары на поперечине, круглые чёрные крылья, переходящие
# в подножки, плоская кабина с козырьком крыши над стеклом. У самосвала ГАЗ-410 база 3,34, рама укорочена сзади, длина 4,67.
# Материалы — карты Poly Haven и ambientCG из art/transport/textures в настоящем размере, как у хлебозавода:
# крашеное зелёное и чёрное железо с потёртостями, крашеные и некрашеные доски, кожзаменитель крыши, резина.
# У каждой детали свой сдвиг рисунка и свой тон; затенение стыков и потемнение к земле — в цвет вершин, как у гаража.
# Перед — −Y Blender, левый борт (сторона водителя) — +X, основание — в нуле, середина длины — в нуле. Единица — метр.
# Запуск только полуторок: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/gaz_aa.py [-- <папка>]
# Без папки модели ложатся в games/transport/models; копии с затенением для показа в Blender — в art/transport/gaz_aa/work/surface.
import math
import os
import random
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402
from bakery import Plant  # noqa: E402
from gazel import Car, catmull, clamp, smoothstep  # noqa: E402

OUT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "models"))
PREVIEW = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "gaz_aa", "work", "surface"))
AO_STEP = 0.2  # крупные грани режутся сеткой с таким шагом: затенению в вершинах нужны вершины внутри граней
AO_DISTANCE = 0.5  # дальше этого стыки и навесы не затеняют
GRIME = 0.9  # до этой высоты борта, колёса и рама темнеют к земле
# Разброс тона деталей по материалу: доля, на которую самая тёмная деталь темнее самой светлой.
TONES = {"body": 0.06, "black": 0.10, "trim": 0.06, "dump_in": 0.06, "bed": 0.08, "bed_b": 0.08, "bed_c": 0.08,
         "van_top": 0.06, "van_low": 0.06, "floor": 0.14, "rubber": 0.06, "roof": 0.04}
CLEAR = ("glass", "lens", "bulb")  # прозрачное и светящееся не темнеет

LENGTH = 5.335
FRONT_AXLE = 0.62  # от переднего края бампера
WHEELBASE = 3.44
DUMP_WHEELBASE = 3.34  # ГАЗ-410 по паспорту: база 3,34, длина 4,67, ширина 1,96 (autoussr.ru)
DUMP_LENGTH = 4.67
WHEEL_R = 0.41
FRONT_TRACK = 1.405
REAR_TRACK = 1.60
DUAL_GAP = 0.20  # между серединами спаренных задних шин

# Профиль вдоль машины, s — от переднего края бампера назад.
S_GRILLE = 0.40
S_HOOD = 0.50
S_COWL = 1.44
S_CAB = 1.60  # лобовое стекло
S_CAB_BACK = 2.62
S_BED = 2.70
HOOD_BOTTOM = 0.80
HOOD_TOP = 1.31  # верх капота у облицовки; к щитку он поднимается до пояса кабины
HOOD_CROWN = 0.03
BELT = 1.43
ROOF_TOP = 1.97
CAB_HALF = 0.68
SILL = 0.64
FENDER_X = 0.735  # середина крыла поперёк
CAB_WALL = 0.03
FLOOR = 0.73  # пол кабины поверх лонжеронов рамы
SCREEN_HALF = 0.59  # полуширина лобового стекла
GLASS_Z = (1.45, 1.82)  # низ и верх лобового стекла и окон дверей
DOOR = (1.80, 2.545)  # дверь вдоль машины: впереди щиток, за ней узкая стойка до задней стенки
DOOR_GLASS = (1.86, 2.48)  # окно двери вдоль машины
REAR_GLASS_HALF = 0.28
REAR_GLASS_Z = (1.55, 1.78)
WOOD_GREEN = "#6b896b"  # подкраска светлых досок под зелень кузова


class Truck(Car, Plant):
    """Полуторка: точки задаются отметкой `s` от бампера назад, модель стоит серединой длины в нуле. Поверхности
    строит `Car`, материалы с картами и развёртку в метрах даёт `Plant` хлебозавода."""

    def __init__(self, name, length):
        super().__init__(name)
        self.length = length

    def P(self, x, s, z):
        return Vector((x, s - self.length / 2, z))

    def sbox(self, x0, s0, z0, x1, s1, z1, mi):
        """Коробка в отметках `s`; `x` можно давать в любом порядке."""
        x0, x1 = sorted((x0, x1))
        self.box(x0, s0 - self.length / 2, z0, x1, s1 - self.length / 2, z1, mi)

    def both(self, x0, s0, z0, x1, s1, z1, mi):
        """Пара коробок по обоим бортам: `x0`, `x1` — для левого борта (+X)."""
        self.sbox(x0, s0, z0, x1, s1, z1, mi)
        self.sbox(-x0, s0, z0, -x1, s1, z1, mi)

    def revolve_y(self, center, profile, mi_of, segments=20):
        """Тело вращения вокруг оси вдоль машины: профиль (радиус, выступ вперёд), лицом вперёд; фара, знак."""
        rings = [[self.bm.verts.new(center + Vector((r * math.cos(a), -d, r * math.sin(a)))) for r, d in profile]
                 for a in (2 * math.pi * k / segments for k in range(segments))]
        faces = []
        for k in range(segments):
            a, b = rings[k], rings[(k + 1) % segments]
            for i in range(len(profile) - 1):
                faces.append(self.face([a[i], a[i + 1], b[i + 1], b[i]], mi_of(i)))
        self.bm.normal_update()
        if sum(f.normal.y * f.calc_area() for f in faces) > 0:
            for f in faces:
                f.normal_flip()
        return faces

    def bar_x(self, s, z, x0, x1, r, mi, segments=10):
        """Круглый брус поперёк машины: поперечина фар, балка моста."""
        res = bmesh.ops.create_cone(self.bm, cap_ends=True, segments=segments, radius1=r, radius2=r, depth=x1 - x0,
                                    matrix=Matrix.Translation(self.P((x0 + x1) / 2, s, z)) @ Matrix.Rotation(math.pi / 2, 4, "Y"))
        self._paint(res["verts"], mi)

    def disc_y(self, s0, s1, x, z, r, mi, segments=16):
        """Круглая шайба поперёк оси машины, от отметки s0 до s1: циферблат прибора."""
        res = bmesh.ops.create_cone(self.bm, cap_ends=True, segments=segments, radius1=r, radius2=r, depth=s1 - s0,
                                    matrix=Matrix.Translation(self.P(x, (s0 + s1) / 2, z)) @ Matrix.Rotation(math.pi / 2, 4, "X"))
        self._paint(res["verts"], mi)

    def pane(self, corners, mi):
        """Стекло одной гранью по четырём точкам (x, s, z), обход — против часовой, если смотреть снаружи:
        прозрачное движок рисует без отсечения граней, и у коробки стекло легло бы двумя слоями."""
        self.face([self.bm.verts.new(self.P(*p)) for p in corners], mi, smooth=False)

    def refine(self, step=AO_STEP, area=0.02):
        """Режет грани крупнее `area` плоскостями через `step` по трём осям: затенение запекается в вершины, а на
        большой грани без вершин внутри — борт, дверь, доска — его не видно."""
        bm = self.bm
        for axis in range(3):
            lo = min(v.co[axis] for v in bm.verts)
            hi = max(v.co[axis] for v in bm.verts)
            c = math.floor(lo / step) * step + step / 2 + 0.0013  # мимо ровных отметок, где лежат рёбра деталей
            while c < hi:
                faces = [f for f in bm.faces if f.calc_area() > area
                         and min(v.co[axis] for v in f.verts) < c - 1e-4 < c + 1e-4 < max(v.co[axis] for v in f.verts)]
                if faces:
                    edges = list({e for f in faces for e in f.edges})
                    verts = list({v for e in edges for v in e.verts})
                    co, no = [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]
                    co[axis], no[axis] = c, 1.0
                    bmesh.ops.bisect_plane(bm, geom=verts + edges + faces, plane_co=co, plane_no=no)
                c += step

    def pieces(self, seed=1932):
        """Свой сдвиг рисунка у каждой детали — соседние доски и листы не продолжают одну картинку, — и свой тон
        в слое граней `tone`: деталь темнее самой светлой не больше, чем на разброс её материала из TONES."""
        rnd = random.Random(seed)
        tone = self.bm.faces.layers.float.new("tone")
        spread = {mi: TONES.get(key, 0.0) for key, mi in self.index.items()}
        seen = set()
        for start in self.bm.faces:
            if start in seen:
                continue
            island, stack = [], [start]
            seen.add(start)
            while stack:
                f = stack.pop()
                island.append(f)
                for e in f.edges:
                    for g in e.link_faces:
                        if g not in seen:
                            seen.add(g)
                            stack.append(g)
            du, dv = rnd.random(), rnd.random()
            k = 1.0 - spread.get(start.material_index, 0.0) * rnd.random()
            for f in island:
                f[tone] = k
                for loop in f.loops:
                    u, v = loop[self.uv].uv
                    loop[self.uv].uv = (u + du, v + dv)

    def strip(self, pairs, mi):
        """Лента по обе стороны из точек (s, (x, z) внутреннего края, (x, z) внешнего): щиток между капотом и крылом.
        Лицом вверх на обоих бортах — на правом обход зеркальный."""
        for sign in (1, -1):
            rows = [(self.bm.verts.new(self.P(sign * xa, s, za)), self.bm.verts.new(self.P(sign * xb, s, zb)))
                    for s, (xa, za), (xb, zb) in pairs]
            for (a0, b0), (a1, b1) in zip(rows, rows[1:]):
                self.face([a0, b0, b1, a1] if sign > 0 else [a0, a1, b1, b0], mi, smooth=False)


def rounded_rect(hw, zb, zt, rt, rb, crown=0.0, n=4):
    """Замкнутое сечение (x, z): прямоугольник шириной 2·hw со скруглёнными углами и выпуклым на `crown` верхом."""
    def lift(x, z):
        return z + crown * (1 - (x / hw) ** 2)

    right = [((hw - rb) * i / 3, zb) for i in range(3)]
    right += [(hw - rb + rb * math.cos(a), zb + rb + rb * math.sin(a)) for a in (-math.pi / 2 + math.pi / 2 * i / n for i in range(n + 1))]
    right += [(x, lift(x, z)) for x, z in ((hw - rt + rt * math.cos(a), zt - rt + rt * math.sin(a))
                                           for a in (math.pi / 2 * i / n for i in range(n + 1)))]
    right += [(x, lift(x, zt)) for x in ((hw - rt) * (1 - i / 4) for i in range(1, 5))]
    return right + [(-x, z) for x, z in reversed(right[1:-1])]


def hood_half(s):
    return 0.28 + 0.12 * clamp((s - S_HOOD) / (S_COWL - S_HOOD))


def hood_top(s):
    """Капот полого поднимается от облицовки к щитку и у щитка выходит на пояс кабины."""
    return HOOD_TOP + (BELT - HOOD_CROWN - HOOD_TOP) * clamp((s - S_HOOD) / (S_COWL - S_HOOD))


def fender_path():
    """Средняя линия крыла (s, z): спереди опущена к оси, дуга над колесом, сзади стекает в подножку."""
    rho = 0.50
    arc = [(FRONT_AXLE + rho * math.cos(math.radians(a)), WHEEL_R + rho * math.sin(math.radians(a)))
           for a in (140, 126, 110, 90, 70, 52, 38)]  # передний край — над колесом, низ колеса спереди открыт
    return catmull(arc + [(FRONT_AXLE + 0.58, 0.66), (FRONT_AXLE + 0.76, 0.61), (S_CAB - 0.06, 0.59)], 5)


def hood(t, m):
    """Капот от облицовки до щитка: чуть шире и выше к кабине, выпуклый верх, шов петли посередине, жалюзи сбоку."""
    stations = [S_HOOD + (S_COWL + 0.02 - S_HOOD) * i / 24 for i in range(25)]
    t.loft([[t.P(x, s, z) for x, z in rounded_rect(hood_half(s), HOOD_BOTTOM, hood_top(s), 0.10, 0.01, HOOD_CROWN)]
            for s in stations], m["body"])
    hinge = [(t.P(0.0, s, hood_top(s) + HOOD_CROWN), Vector((1.0, 0.0, 0.0)), Vector((0.0, 0.0, 1.0)))
             for s in (S_HOOD + 0.03 + (S_COWL - S_HOOD - 0.03) * i / 12 for i in range(13))]
    t.sweep(hinge, [(-0.008, -0.004), (0.008, -0.004), (0.008, 0.007), (-0.008, 0.007)], m["chrome_dark"], smooth=False)
    for k in range(30):
        s0 = S_HOOD + 0.03 + (S_COWL - S_HOOD - 0.03) * k / 30
        s1 = s0 + (S_COWL - S_HOOD - 0.03) / 30
        x = hood_half(s1)
        zs = hood_top((s0 + s1) / 2) - 0.115  # шов боковины — под скруглением верха
        t.both(x - 0.006, s0, zs, x + 0.003, s1, zs + 0.007, m["gap"])
    for k in range(21):  # жалюзи на всю заднюю половину боковины
        s = 0.78 + k * 0.026
        x = hood_half(s) + 0.002
        t.both(x - 0.004, s, 0.84, x + 0.004, s + 0.011, 1.16, m["gap"])
    t.cylinder(0.0, 1.52 - t.length / 2, BELT - 0.01, 0.035, 0.05, m["chrome"], 12)  # пробка бака на щитке


def radiator(t, m):
    """Высокая узкая чёрная облицовка, сплошная сетка в прутках, знак ГАЗ сверху и крышка горловины."""
    t.rounded_box(t.P(0.0, (S_GRILLE + S_HOOD + 0.02) / 2, 0.98), (0.29, (S_HOOD + 0.02 - S_GRILLE) / 2, 0.39), 0.06,
                  m["black"], m=3, flat=(3, 2, 6))
    # Сердцевина радиатора и сетка перед ней — прутки через 2,4 см.
    t.sbox(-0.235, S_GRILLE - 0.006, 0.66, 0.235, S_GRILLE + 0.02, 1.30, m["grille"])
    for k in range(20):
        x = -0.232 + k * 0.024
        t.sbox(x, S_GRILLE - 0.010, 0.66, x + 0.005, S_GRILLE - 0.006, 1.30, m["grille_bar"])
    for k in range(27):
        z = 0.665 + k * 0.024
        t.sbox(-0.235, S_GRILLE - 0.009, z, 0.235, S_GRILLE - 0.006, z + 0.005, m["grille_bar"])
    badge = t.P(0.0, S_GRILLE - 0.003, 1.335)
    t.revolve_y(badge, [(0.0, 0.010), (0.032, 0.010), (0.036, 0.0)], lambda i: m["chrome"], 16)
    t.revolve_y(badge, [(0.0, 0.013), (0.024, 0.013), (0.026, 0.010)], lambda i: m["badge"], 16)
    t.cylinder(0.0, 0.47 - t.length / 2, 1.37, 0.04, 0.06, m["chrome"], 12)


def lamps(t, m):
    """Две круглые фары на поперечине между крыльями, перед облицовкой: хромовый корпус, за прозрачным стеклом —
    зеркальная чаша отражателя и лампа."""
    lamp_x = 0.44
    t.bar_x(S_GRILLE - 0.01, 1.04, -lamp_x, lamp_x, 0.016, m["black"])  # поперечина фар — перед сеткой
    shell = [(0.02, -0.10), (0.06, -0.09), (0.088, -0.065), (0.102, -0.03), (0.105, 0.0), (0.1, 0.011), (0.088, 0.013),
             (0.045, 0.026), (0.0, 0.028)]
    bowl = [(0.0, -0.075), (0.03, -0.07), (0.06, -0.05), (0.085, -0.02), (0.1, 0.011)]  # край чаши — по ободу корпуса
    for sign in (1, -1):
        c = t.P(sign * lamp_x, S_GRILLE - 0.03, 1.12)
        # корпус крашен чёрным, хромирован только ободок вокруг стекла
        t.revolve_y(c, shell, lambda i: m["black"] if i < 3 else m["chrome"] if i < 5 else m["lens"], 24)
        t.revolve_y(c, bowl, lambda i: m["reflector"], 24)
        t.blob(c.x, c.y + 0.03, c.z, 0.016, 0.022, 0.016, m["bulb"], 1, 0, jitter=0.0)
        t.disc_y(S_GRILLE, S_GRILLE + 0.06, sign * lamp_x, 1.12, 0.008, m["reflector"], 8)  # патрон лампы в дне чаши


def fenders(t, m):
    """Передние крылья протяжкой профиля вдоль дуги, щитки между капотом и крыльями, подножки."""
    path = fender_path()
    # Профиль (наружу от колеса, поперёк наружу): выпуклый верх, внешний край загнут вниз.
    profile = [(-0.012, -0.175), (0.012, -0.12), (0.026, -0.05), (0.03, 0.0), (0.025, 0.06), (0.010, 0.12), (-0.025, 0.163),
               (-0.07, 0.178), (-0.07, 0.166), (-0.03, 0.150), (-0.004, 0.11), (0.010, 0.06), (0.014, 0.0), (0.012, -0.05),
               (0.0, -0.12), (-0.024, -0.175)]
    for sign in (1, -1):
        loops = []
        for i, (s, z) in enumerate(path):
            tan = (Vector(path[min(len(path) - 1, i + 1)]) - Vector(path[max(0, i - 1)])).normalized()
            o, na, nb = t.P(sign * FENDER_X, s, z), Vector((0.0, -tan.y, tan.x)), Vector((sign, 0.0, 0.0))
            k = 1 - 0.65 * smoothstep(FRONT_AXLE + 0.55, S_CAB - 0.06, s)  # к подножке крыло сплющивается
            loops.append([o + na * (a * k) + nb * b for a, b in profile])
        t.loft(loops, m["fender"])
        t.sbox(sign * 0.62, S_CAB - 0.08, 0.555, sign * 0.925, S_CAB_BACK + 0.10, 0.585, m["fender"])
        for k in range(4):  # резиновые полосы подножки
            x = sign * (0.66 + k * 0.07)
            t.sbox(x, S_CAB - 0.04, 0.585, x + sign * 0.035, S_CAB_BACK + 0.06, 0.592, m["rubber"])
        # Фартук от подножки до низа кабины — щели под кабиной не видно; кронштейны подножки к лонжерону.
        t.sbox(sign * (CAB_HALF - CAB_WALL), S_CAB - 0.08, 0.585, sign * CAB_HALF, S_CAB_BACK + 0.10, SILL, m["fender"])
        for s in (S_CAB + 0.15, S_CAB_BACK - 0.10):
            t.sbox(sign * 0.47, s, 0.555, sign * 0.62, s + 0.04, 0.585, m["chassis"])
    # Щиток от низа капота к внутреннему краю крыла — сбоку не видно колеса над капотом насквозь.
    inner = FENDER_X - 0.17
    pairs = [(s, (hood_half(s) - 0.01, HOOD_BOTTOM), (inner, z - 0.01)) for s, z in path if S_GRILLE + 0.02 <= s <= S_COWL]
    t.strip(pairs, m["black"])


def roof(t, m, top):
    """Мягкая крыша вровень со стенками кабины, со скруглёнными краями: спереди опущенный над стеклом козырёк,
    сзади край закруглён к задней стенке. `top` — верх стенок."""
    visor, back_r = 0.12, 0.10
    crown = 0.015  # выпуклость крыши поперёк; с ней верх приходится ровно на ROOF_TOP
    s0, s1, s2 = S_CAB - visor, S_CAB + 0.02, S_CAB_BACK - back_r
    stations = ([s0 + (s1 - s0) * i / 6 for i in range(6)] + [s1 + (s2 - s1) * i / 4 for i in range(4)]
                + [s2 + back_r * i / 8 for i in range(9)])
    loops = []
    for s in stations:
        if s < s1:  # козырёк опускается к верху лобового стекла и тоньше к краю
            u = smoothstep(s0, s1, s)
            zb = top - 0.05 * (1 - u)
            zt = zb + 0.025 + (ROOF_TOP - crown - top - 0.025) * u
        else:
            d = clamp((s - s2) / back_r)
            zb = top
            zt = max(top + 0.012, top + (ROOF_TOP - crown - top) * math.sqrt(1 - d * d))
        th = zt - zb
        rt = min(0.08, th - 0.008)  # плечи крыши скруглены широко, как у натянутого кожзаменителя
        loops.append([t.P(x, s, z) for x, z in rounded_rect(CAB_HALF, zb, zt, rt, 0.004, crown * min(1.0, th / 0.06))])
    t.loft(loops, m["roof"])


def cab(t, m):
    """Щиток расширяется от капота к кабине. Кабина полая: стенки в 3 см с проёмами окон, прозрачные стёкла
    в проёмах, внутри — салон; снаружи — двери, ручки, петли и козырёк крыши."""
    def half(s):
        return 0.40 + (CAB_HALF - 0.40) * smoothstep(S_COWL - 0.04, S_CAB, s)

    def bottom(s):
        return HOOD_BOTTOM + (SILL - HOOD_BOTTOM) * smoothstep(S_COWL, S_CAB, s)

    stations = [S_COWL - 0.04 + (S_CAB - S_COWL + 0.04) * i / 12 for i in range(13)]
    t.loft([[t.P(x, s, z) for x, z in rounded_rect(half(s), bottom(s), BELT, 0.03, 0.02)] for s in stations], m["body"])
    # Пол, передняя и задняя стенки — между боковинами, а не поверх них: иначе их торцы ложатся в плоскость боковины
    # и мерцают снаружи.
    w, top, back = CAB_WALL, ROOF_TOP - 0.10, S_CAB_BACK - CAB_WALL
    inner = CAB_HALF - w
    body = m["body"]
    t.sbox(-inner, S_CAB, SILL, inner, back, FLOOR, m["chassis"])  # пол поверх рамы
    t.both(CAB_HALF - w, S_CAB, SILL, CAB_HALF, S_CAB_BACK, BELT, body)  # двери и боковины ниже окон
    for s0, s1, z0, z1 in ((S_CAB, DOOR_GLASS[0], BELT, top), (DOOR_GLASS[1], S_CAB_BACK, BELT, top),
                           (DOOR_GLASS[0], DOOR_GLASS[1], BELT, GLASS_Z[0]), (DOOR_GLASS[0], DOOR_GLASS[1], GLASS_Z[1], top)):
        t.both(CAB_HALF - w, s0, z0, CAB_HALF, s1, z1, body)  # вокруг окна двери
    t.sbox(-inner, S_CAB, BELT - 0.01, inner, S_CAB + w, GLASS_Z[0], body)  # под лобовым стеклом
    t.both(SCREEN_HALF, S_CAB, GLASS_Z[0], inner, S_CAB + w, GLASS_Z[1], body)  # стойки
    t.sbox(-inner, S_CAB, GLASS_Z[1], inner, S_CAB + w, top, body)
    t.sbox(-inner, back, SILL, inner, S_CAB_BACK, REAR_GLASS_Z[0], body)  # задняя стенка вокруг окошка
    t.sbox(-inner, back, REAR_GLASS_Z[1], inner, S_CAB_BACK, top, body)
    t.both(REAR_GLASS_HALF, back, REAR_GLASS_Z[0], inner, S_CAB_BACK, REAR_GLASS_Z[1], body)
    roof(t, m, top)
    # Стёкла — в глубине проёмов; рамка лобового снаружи, дворник водителя на стекле.
    g0, g1 = GLASS_Z
    s = S_CAB + 0.012
    t.pane([(-SCREEN_HALF, s, g0), (SCREEN_HALF, s, g0), (SCREEN_HALF, s, g1), (-SCREEN_HALF, s, g1)], m["glass"])
    for x0, x1, z0, z1 in ((-SCREEN_HALF - 0.03, SCREEN_HALF + 0.03, GLASS_Z[0] - 0.03, GLASS_Z[0]),
                           (-SCREEN_HALF - 0.03, SCREEN_HALF + 0.03, GLASS_Z[1], GLASS_Z[1] + 0.03),
                           (-SCREEN_HALF - 0.03, -SCREEN_HALF, GLASS_Z[0], GLASS_Z[1]), (SCREEN_HALF, SCREEN_HALF + 0.03, GLASS_Z[0], GLASS_Z[1])):
        t.sbox(x0, S_CAB - 0.008, z0, x1, S_CAB, z1, m["frame"])
    t.sbox(0.05, S_CAB + 0.004, GLASS_Z[1] - 0.025, 0.42, S_CAB + 0.010, GLASS_Z[1] - 0.012, m["black"])
    r0, r1, s = REAR_GLASS_Z[0], REAR_GLASS_Z[1], back + 0.014
    t.pane([(REAR_GLASS_HALF, s, r0), (-REAR_GLASS_HALF, s, r0), (-REAR_GLASS_HALF, s, r1), (REAR_GLASS_HALF, s, r1)], m["glass"])
    for sign in (1, -1):
        x = sign * CAB_HALF
        a, b = (DOOR_GLASS if sign > 0 else DOOR_GLASS[::-1])
        g = x - sign * 0.016
        t.pane([(g, a, g0), (g, b, g0), (g, b, g1), (g, a, g1)], m["glass"])
        d0, d1, dz = DOOR[0], DOOR[1], top - 0.01
        for s0, s1, z0, z1 in ((d0 - 0.005, d0 + 0.005, SILL + 0.02, dz), (d1 - 0.005, d1 + 0.005, SILL + 0.02, dz),
                               (d0 - 0.005, d1 + 0.005, dz - 0.01, dz), (d0 - 0.005, d1 + 0.005, SILL + 0.02, SILL + 0.03)):
            t.sbox(x, s0, z0, x + sign * 0.007, s1, z1, m["gap"])  # щели двери
        t.sbox(x, d1 - 0.16, 1.36, x + sign * 0.03, d1 - 0.06, 1.38, m["chrome"])  # ручка
        for z in (0.95, 1.62):
            t.sbox(x, d0 - 0.01, z, x + sign * 0.022, d0 + 0.03, z + 0.07, m["black"])  # петли


def interior(t, m):
    """Салон за стёклами: коврик, скамья в кожзаменителе, приборная доска с щитком приборов посередине,
    руль водителя на наклонной колонке, рычаг передач на тоннеле коробки."""
    w = CAB_WALL
    t.sbox(-CAB_HALF + w, S_CAB + w, FLOOR, CAB_HALF - w, S_CAB_BACK - w, FLOOR + 0.01, m["rubber"])
    seat = CAB_HALF - w - 0.07
    t.sbox(-seat, 2.18, FLOOR + 0.01, seat, 2.55, 0.98, m["chassis"])  # короб скамьи
    t.sbox(-seat, 2.16, 0.98, seat, 2.55, 1.08, m["roof"])  # подушка
    t.sbox(-seat, 2.49, 1.08, seat, S_CAB_BACK - w, REAR_GLASS_Z[0] - 0.03, m["roof"])  # спинка ниже заднего окошка
    t.sbox(-CAB_HALF + w, S_CAB + w, 1.20, CAB_HALF - w, S_CAB + 0.16, BELT + 0.01, m["body"])  # доска
    face = S_CAB + 0.16
    t.sbox(-0.13, face, 1.25, 0.13, face + 0.008, 1.38, m["chrome_dark"])  # щиток приборов
    for x in (-0.07, 0.0, 0.07):
        t.disc_y(face + 0.008, face + 0.012, x, 1.315, 0.028, m["dial"])
    # Руль: ось колеса поднята на 50° от горизонта — колесо лежит полого, три спицы; колонка проходит под доской
    # к передней стенке.
    c = t.P(0.33, 1.95, 1.40)
    up = Vector((0.0, math.cos(math.radians(50)), math.sin(math.radians(50))))  # к водителю и вверх
    u, v = Vector((1.0, 0.0, 0.0)), up.cross(Vector((1.0, 0.0, 0.0)))
    tube = [(0.012 * math.cos(2 * math.pi * k / 8), 0.012 * math.sin(2 * math.pi * k / 8)) for k in range(8)]
    ring = []
    for k in range(33):
        a = 2 * math.pi * k / 32
        radial = u * math.cos(a) + v * math.sin(a)
        ring.append((c + radial * 0.21, radial, up))
    t.sweep(ring, tube, m["black"], caps=False)
    for a in (math.radians(90), math.radians(210), math.radians(330)):
        d = u * math.cos(a) + v * math.sin(a)
        t.sweep([(c, up, d.cross(up)), (c + d * 0.2, up, d.cross(up))], [(-0.005, -0.012), (0.005, -0.012), (0.005, 0.012), (-0.005, 0.012)],
                m["black"], smooth=False)
    t.sweep([(c, u, v), (c - up * 0.55, u, v)], [(0.022 * x, 0.022 * y) for x, y in ((1, 0), (0.7, 0.7), (0, 1), (-0.7, 0.7), (-1, 0),
                                                                                   (-0.7, -0.7), (0, -1), (0.7, -0.7))], m["black"])
    t.sbox(-0.008, 1.78, HOOD_BOTTOM, 0.008, 1.80, 1.22, m["black"])  # рычаг передач на тоннеле, за доской
    t.blob(0.0, 1.79 - t.length / 2, 1.24, 0.025, 0.025, 0.025, m["black"], 1, 0, jitter=0.0)


def chassis(t, m, rear_axle, frame_end):
    """Рама, бампер, балки мостов, рессоры, мотор снизу и задний фонарь."""
    d = m["chassis"]
    # Бампер — две полосы на рогах рамы, стянутые хомутами.
    for z0, z1 in ((0.435, 0.475), (0.505, 0.545)):
        t.sbox(-0.88, 0.0, z0, 0.88, 0.06, z1, m["black"])
    for x in (-0.80, -0.43, 0.43, 0.80):
        t.sbox(x - 0.03, -0.006, 0.425, x + 0.03, 0.066, 0.555, m["black"])
    t.both(0.40, 0.06, 0.46, 0.46, S_GRILLE + 0.05, 0.60, d)  # рога рамы заходят под концы лонжеронов
    t.both(0.40, S_GRILLE, 0.56, 0.47, frame_end, 0.71, d)  # лонжероны
    for s in (S_GRILLE + 0.1, S_CAB_BACK, rear_axle - 0.6, frame_end - 0.06):
        t.sbox(-0.40, s, 0.58, 0.40, s + 0.06, 0.69, d)
    t.sbox(-0.28, S_HOOD + 0.01, 0.42, 0.28, S_CAB + 0.2, HOOD_BOTTOM, d)  # мотор и коробка под капотом, за передней поперечиной
    # Передняя ось — балка с поперечной рессорой; задний мост с картером; карданный вал.
    t.sbox(-0.66, FRONT_AXLE - 0.035, WHEEL_R - 0.04, 0.66, FRONT_AXLE + 0.035, WHEEL_R + 0.03, d)
    t.sbox(-0.42, FRONT_AXLE - 0.04, WHEEL_R + 0.03, 0.42, FRONT_AXLE + 0.04, WHEEL_R + 0.07, d)
    t.bar_x(rear_axle, WHEEL_R, -0.75, 0.75, 0.06, d)
    t.rounded_box(t.P(0.0, rear_axle, WHEEL_R), (0.16, 0.17, 0.16), 0.11, d)
    t.sbox(-0.05, S_CAB + 0.2, 0.40, 0.05, rear_axle - 0.15, 0.48, d)
    t.both(0.41, rear_axle - 0.55, WHEEL_R + 0.05, 0.47, rear_axle + 0.55, WHEEL_R + 0.12, d)  # рессоры
    # Фонарь и номер сзади слева — у машин тех лет один фонарь.
    t.sbox(0.47, frame_end - 0.03, 0.64, 0.60, frame_end, 0.67, m["black"])  # кронштейн фонаря от лонжерона
    t.sbox(0.60, frame_end - 0.02, 0.62, 0.72, frame_end + 0.04, 0.72, m["black"])
    t.sbox(0.62, frame_end + 0.04, 0.64, 0.70, frame_end + 0.048, 0.70, m["red"])
    t.sbox(0.22, frame_end, 0.58, 0.58, frame_end + 0.012, 0.72, m["plate"])


def wheels(t, m, rear_axle):
    """Шины 6,50-20 с протектором из шашек на чёрных штампованных дисках с пятью овальными окнами и шестью
    гайками; задние спаренные, у внутренних диск не виден."""
    # Каркас шины ниже наружного радиуса на глубину протектора: по земле катятся шашки.
    tyre = [(0.25, -0.070), (0.30, -0.085), (0.35, -0.088), (0.385, -0.080), (0.393, -0.062), (0.398, -0.035), (0.399, 0.0),
            (0.398, 0.035), (0.393, 0.062), (0.385, 0.080), (0.35, 0.088), (0.30, 0.085), (0.25, 0.070)]
    lugs, lug_t = 32, 0.05  # шашек в ряду по окружности; длина шашки по окружности

    def tread(c):
        """Два ряда шашек вразбежку по беговой дорожке, от плеча до середины; углы шашек — ровно на WHEEL_R."""
        r0, r1 = 0.385, math.sqrt(WHEEL_R ** 2 - (lug_t / 2) ** 2)
        for row, (x0, x1) in enumerate(((-0.078, -0.004), (0.004, 0.078))):
            for k in range(lugs):
                a = 2 * math.pi * (k + 0.5 * row) / lugs
                radial, along = Vector((0.0, math.cos(a), math.sin(a))), Vector((0.0, -math.sin(a), math.cos(a)))
                basis = Matrix((Vector((x1 - x0, 0.0, 0.0)), radial * (r1 - r0), along * lug_t)).transposed().to_4x4()
                centre = c + Vector(((x0 + x1) / 2, 0.0, 0.0)) + radial * ((r0 + r1) / 2)
                res = bmesh.ops.create_cube(t.bm, size=1.0, matrix=Matrix.Translation(centre) @ basis)
                t._paint(res["verts"], m["tyre"])
    disc = [(0.262, 0.064), (0.255, 0.074), (0.245, 0.062), (0.235, 0.045), (0.20, 0.040), (0.16, 0.052), (0.12, 0.072),
            (0.085, 0.090), (0.07, 0.098), (0.062, 0.115), (0.05, 0.128), (0.025, 0.134), (0.0, 0.135)]

    def wheel(x, s, side, dish=1.0, nuts=True):
        c = t.P(x, s, WHEEL_R)
        t.revolve(c, tyre, lambda r: m["tyre"], side)
        tread(c)
        t.revolve(c, [(r, w * dish) for r, w in disc], lambda r: m["wheel"] if r > 0.066 else m["hub"], side, closed=False)
        if nuts:
            for k in range(6):
                a = 2 * math.pi * k / 6
                t.cylinder_x(c.x + side * (0.083 * dish + 0.012), c.y + 0.10 * math.cos(a), c.z + 0.10 * math.sin(a), 0.013, 0.03,
                             m["nut"], 6)
            for k in range(5):
                window(c, side, dish, 2 * math.pi * (k + 0.5) / 5)

    def dish_at(r):
        """Выступ диска наружу на радиусе `r` — по его профилю."""
        for (r0, w0), (r1, w1) in zip(disc, disc[1:]):
            if r1 <= r <= r0:
                return w1 + (w0 - w1) * (r - r1) / (r0 - r1)
        return disc[-1][1]

    def window(c, side, dish, a):
        """Овальное окно диска тёмной пластиной по его поверхности, лицом наружу."""
        radial, across = Vector((0.0, math.cos(a), math.sin(a))), Vector((0.0, -math.sin(a), math.cos(a)))
        verts = []
        for i in range(14):
            u = 2 * math.pi * i / 14
            r = 0.178 + 0.032 * math.cos(u)
            p = c + radial * r + across * (0.022 * math.sin(u))
            p.x = c.x + side * (dish_at(r) * dish + 0.002)
            verts.append(t.bm.verts.new(p))
        f = t.face(verts, m["gap"], smooth=False)
        f.normal_update()
        if f.normal.x * side < 0:
            f.normal_flip()

    def hub(s, x0, x1, r):
        """Ступица от конца балки моста до середины диска: колесо держится на оси."""
        t.cylinder_x((x0 + x1) / 2, s - t.length / 2, WHEEL_R, r, abs(x1 - x0), m["hub"], 12)

    for sign in (1, -1):
        front = FRONT_TRACK / 2
        wheel(sign * front, FRONT_AXLE, sign)
        hub(FRONT_AXLE, sign * 0.60, sign * (front + 0.125), 0.06)
        outer, inner = REAR_TRACK / 2 + DUAL_GAP / 2, REAR_TRACK / 2 - DUAL_GAP / 2
        wheel(sign * outer, rear_axle, sign, dish=0.55)
        wheel(sign * inner, rear_axle, sign, dish=0.55, nuts=False)  # диск внутренней шины закрыт наружной
        hub(rear_axle, sign * 0.62, sign * (outer + 0.07), 0.07)


def flatbed(t, m):
    """Деревянная платформа: брусья, пол из досок вдоль, борта из четырёх досок разного тона выше окна кабины,
    на крашеных железных полосах, задний борт откидной. Нижняя доска закрывает торец пола."""
    end = t.length - 0.02
    floor_top = 1.0
    half = 1.02
    side = half - 0.04
    t.both(0.40, S_BED, 0.71, 0.50, end, 0.80, m["chassis"])
    for k in range(6):
        s = S_BED + 0.05 + (end - S_BED - 0.15) * k / 5
        t.sbox(-half, s, 0.80, half, s + 0.08, 0.95, m["chassis"])  # поперечины лежат на продольных брусьях
    n, gap = 10, 0.006  # пол между бортами — десять досок вдоль со щелями, у каждой свой тон
    for k in range(n):
        x0 = -side + 2 * side * k / n + (gap / 2 if k else 0.0)
        x1 = -side + 2 * side * (k + 1) / n - (gap / 2 if k < n - 1 else 0.0)
        t.sbox(x0, S_BED + 0.04, 0.95, x1, end - 0.04, floor_top, m["floor"])
    boards = [(0.95, 1.13), (1.134, 1.264), (1.268, 1.398), (1.402, 1.532)]
    for (z0, z1), key in zip(boards, ("bed", "bed_b", "bed_c", "bed_b")):
        t.both(side, S_BED, z0, half, end, z1, m[key])
        t.sbox(-side, S_BED, z0, side, S_BED + 0.04, z1, m[key])
        t.sbox(-side, end - 0.04, z0, side, end, z1, m[key])
    top = boards[-1][1]
    for k in range(6):  # полосы-стойки бортов на заклёпках, крашеные
        s = S_BED + 0.01 + (end - 0.06 - S_BED - 0.01) * k / 5
        t.both(half, s, 0.95, half + 0.008, s + 0.05, top + 0.01, m["trim"])
    for x in (-half + 0.01, half - 0.06):
        t.sbox(x, end, 0.95, x + 0.05, end + 0.008, top + 0.01, m["trim"])
    for x in (-0.6, 0.6):  # петли заднего борта
        t.sbox(x - 0.05, end, floor_top + 0.02, x + 0.05, end + 0.016, floor_top + 0.06, m["iron"])
    t.both(half, end - 0.16, top - 0.09, half + 0.03, end - 0.12, top - 0.03, m["iron"])  # крюки запоров


def van(t, m, rear_axle):
    """Фургон по снимку хлебного фургона на шасси ГАЗ-АА: будка из крашеных листов почти вплотную к кабине, одной
    стенкой в две краски — светлая сверху, тёмная снизу, стык под рейкой по уровню низа окна двери; угловые стойки,
    рейка под крышей, нащельники на стыках листов у концов арки; юбка ниже пола с аркой над задними колёсами
    и рифлёной окантовкой; скруглённая крыша без свеса; сзади двустворчатая дверь между узкими неподвижными
    листами."""
    s_bed = S_CAB_BACK + 0.035
    end = t.length - 0.02
    half = 1.0
    z0, belt, z1 = 0.90, 1.44, 2.24  # пол, стык красок и верх — по снимку (photos/van_bread_*)
    mid = (s_bed + end) / 2
    half_len = (end - s_bed) / 2
    t.both(0.40, s_bed, 0.71, 0.50, end, 0.80, m["chassis"])
    for s in (s_bed + 0.05, s_bed + 0.60, rear_axle - 0.60, rear_axle + 0.52, end - 0.10):  # поперечины мимо колёс
        t.sbox(-half + 0.02, s, 0.80, half - 0.02, s + 0.08, z0, m["chassis"])
    # Юбка ниже пола — коробом вокруг рамы; над колесом её низ идёт по дуге выреза.
    skirt, arch = 0.70, 0.50
    edge = math.sqrt(arch ** 2 - (skirt - WHEEL_R) ** 2)
    t.both(half - 0.02, s_bed, skirt, half, rear_axle - edge, z0 + 0.01, m["van_low"])
    t.both(half - 0.02, rear_axle + edge, skirt, half, end, z0 + 0.01, m["van_low"])
    n = 64
    for k in range(n):
        d0, d1 = -edge + 2 * edge * k / n, -edge + 2 * edge * (k + 1) / n
        d = min(abs(d0), abs(d1)) if d0 * d1 > 0 else 0.0  # низ куска — по дуге, щель под ним закрыта окантовкой
        t.both(half - 0.02, rear_axle + d0, WHEEL_R + math.sqrt(arch ** 2 - d * d), half, rear_axle + d1, z0 + 0.01, m["van_low"])
    t.sbox(-half + 0.02, s_bed, skirt, half - 0.02, s_bed + 0.02, z0 + 0.01, m["van_low"])  # торцы юбки
    t.sbox(-half + 0.02, end - 0.02, skirt, half - 0.02, end, z0 + 0.01, m["van_low"])
    a0 = math.degrees(math.asin((skirt - WHEEL_R) / arch))
    for sign in (1, -1):  # рифлёная окантовка выреза: полоса по дуге и поперечные рифли на ней
        frames = []
        for i in range(25):
            a = math.radians(a0 + (180 - 2 * a0) * i / 24)
            radial = Vector((0.0, math.cos(a), math.sin(a)))
            frames.append((t.P(sign * (half - 0.015), rear_axle, WHEEL_R) + radial * arch, radial, Vector((1.0, 0.0, 0.0))))
        t.sweep(frames, [(-0.03, -0.03), (0.035, -0.03), (0.035, 0.03), (-0.03, 0.03)], m["black"], smooth=False)
        for i in range(30):
            a = math.radians(a0 + (180 - 2 * a0) * (i + 0.5) / 30)
            radial, along = Vector((0.0, math.cos(a), math.sin(a))), Vector((0.0, -math.sin(a), math.cos(a)))
            basis = Matrix((Vector((0.008, 0.0, 0.0)), radial * 0.055, along * 0.008)).transposed().to_4x4()
            centre = t.P(sign * (half + 0.019), rear_axle, WHEEL_R) + radial * (arch + 0.0025)
            res = bmesh.ops.create_cube(t.bm, size=1.0, matrix=Matrix.Translation(centre) @ basis)
            t._paint(res["verts"], m["black"])
    # Будка — одна стенка: низ и верх одной ширины и с одним скруглением углов, стык закрыт рейкой.
    t.rounded_box(t.P(0.0, mid, (z0 + belt) / 2), (half, half_len, (belt - z0) / 2), 0.03, m["van_low"], m=2, flat=(4, 10, 3))
    t.rounded_box(t.P(0.0, mid, (belt + z1) / 2), (half, half_len, (z1 - belt) / 2), 0.03, m["van_top"], m=2, flat=(4, 10, 3))
    t.rounded_box(t.P(0.0, mid, z1 + 0.025), (half + 0.005, half_len + 0.005, 0.03), 0.03, m["roof"], m=3, flat=(4, 10, 1))
    # Обвязка: рейки по стыку красок и под крышей, угловые стойки, нащельники на стыках листов у концов арки.
    for za, zb in ((belt - 0.05, belt + 0.004), (z1 - 0.05, z1)):
        t.both(half, s_bed, za, half + 0.012, end, zb, m["trim"])
        for s0, s1 in ((s_bed - 0.012, s_bed), (end, end + 0.012)):
            t.sbox(-half - 0.012, s0, za, half + 0.012, s1, zb, m["trim"])
    t.both(half, s_bed, skirt, half + 0.014, s_bed + 0.12, z1, m["trim"])  # передняя стойка
    t.both(half, end - 0.05, skirt, half + 0.014, end, z1, m["trim"])  # задняя стойка
    for s in (rear_axle - edge - 0.02, rear_axle + edge + 0.02):
        t.both(half, s - 0.015, skirt, half + 0.01, s + 0.015, z1 - 0.05, m["trim"])
    # Задние двери на 71% ширины между узкими листами: щели, по две петли-полосы и ручки-скобы у стыка красок.
    door = 0.71
    t.sbox(-0.006, end, z0 + 0.06, 0.006, end + 0.004, z1 - 0.06, m["gap"])
    for x in (-door, door - 0.01):
        t.sbox(x, end, z0 + 0.06, x + 0.01, end + 0.004, z1 - 0.06, m["gap"])
    for a, b in ((z0 + 0.06, z0 + 0.07), (z1 - 0.07, z1 - 0.06)):
        t.sbox(-door, end, a, door, end + 0.004, b, m["gap"])
    for x in (-0.06, 0.04):  # скобы: две стойки и перекладина
        for z in (belt - 0.12, belt + 0.10):
            t.sbox(x, end, z, x + 0.02, end + 0.04, z + 0.02, m["chrome"])
        t.sbox(x, end + 0.03, belt - 0.12, x + 0.02, end + 0.05, belt + 0.12, m["chrome"])
    for z in (z0 + 0.20, z1 - 0.30):
        for sign in (1, -1):
            t.sbox(sign * door, end, z, sign * (door - 0.26), end + 0.012, z + 0.05, m["iron"])
            t.cylinder(sign * (door + 0.012), end + 0.012 - t.length / 2, z - 0.01, 0.012, 0.07, m["iron"], 8)


def dump(t, m, rear_axle):
    """Самосвал ГАЗ-410 по снимкам сохранившейся машины: кузов из тонкого стального листа во всю ширину машины,
    1,96 м по обвязке, борта гладкие и к переднему щиту выше, по верху и по низу обвязка; задний борт в рамке
    на верхних петлях с запорами внизу; кузов опрокидывается назад на шарнире у конца рамы. Рычажный подъёмник
    под кузовом не строится — низ машины подробно не делаем."""
    end = t.length - 0.02
    s0 = S_CAB_BACK + 0.03  # передний щит почти вплотную к кабине
    half, w = 0.94, 0.012  # наружная плоскость бортов и толщина листа; с обвязкой +0,02 — ширина 1,96
    floor = 0.98
    z_front, z_rear = 1.62, 1.48  # верх бортов у щита и у заднего борта

    def top(s):
        return z_front + (z_rear - z_front) * (s - s0) / (end - s0)

    def wedge(x0, x1, sa, sb, za0, za1, zb0, zb1, mi):
        """Брус между отметками `sa` и `sb` с низом za0→zb0 и верхом za1→zb1 вдоль машины: борт, обвязка по
        наклонному верху."""
        x0, x1 = sorted((x0, x1))
        v = [t.bm.verts.new(t.P(x, s, z)) for x, s, z in
             ((x0, sa, za0), (x1, sa, za0), (x1, sb, zb0), (x0, sb, zb0),
              (x0, sa, za1), (x1, sa, za1), (x1, sb, zb1), (x0, sb, zb1))]
        for f in ((0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)):
            t.face([v[i] for i in f], mi, smooth=False)

    t.both(0.40, S_CAB_BACK + 0.06, 0.71, 0.50, end - 0.25, 0.80, m["chassis"])  # надрамник
    for s in (s0 + 0.1, rear_axle - 0.25, end - 0.35):
        t.sbox(-0.62, s, 0.80, 0.62, s + 0.08, floor - w, m["chassis"])
    t.sbox(-half + w, s0 + w, floor - w, half - w, end - w, floor, m["dump_in"])  # дно
    lo = floor - 0.06  # борта, щит и задний борт опущены ниже дна и закрывают его кромку
    for sign in (1, -1):
        wedge(sign * (half - w), sign * half, s0, end, lo, top(s0), lo, top(end), m["body"])  # борт
        wedge(sign * half, sign * (half + 0.02), s0, end, top(s0) - 0.035, top(s0), top(end) - 0.035, top(end), m["body"])
    t.both(half, s0, lo, half + 0.015, end, lo + 0.05, m["body"])  # нижняя обвязка
    t.both(half, s0, lo, half + 0.02, s0 + 0.05, z_front - 0.035, m["body"])  # передняя стойка борта
    t.sbox(-half + w, s0, lo, half - w, s0 + w, z_front + 0.02, m["body"])  # передний щит
    t.sbox(-half - 0.02, s0 - 0.025, z_front + 0.02, half + 0.02, s0 + w, z_front + 0.035, m["body"])  # козырёк щита
    # Задний борт в рамке между бортами, на верхних петлях с осями, запоры внизу.
    t.sbox(-half + w, end - w, lo, half - w, end, z_rear - 0.01, m["body"])
    for z0, z1 in ((lo, lo + 0.05), (z_rear - 0.05, z_rear - 0.01)):
        t.sbox(-half + w, end, z0, half - w, end + 0.012, z1, m["body"])
    for x in (-half + w, half - w - 0.04):
        t.sbox(x, end, lo, x + 0.04, end + 0.012, z_rear - 0.01, m["body"])
    for sign in (1, -1):
        t.cylinder_x(sign * (half - 0.005), end - 0.006 - t.length / 2, z_rear - 0.03, 0.014, 0.05, m["iron"], 8)
    t.both(half, end - 0.10, lo + 0.01, half + 0.02, end, lo + 0.06, m["iron"])  # запоры
    t.both(0.36, end - 0.30, 0.70, 0.52, end - 0.14, floor - w, m["iron"])  # шарнир опрокидывания у конца рамы


def glass(t, key, color, alpha):
    """Прозрачный материал: сквозь стекло видно салон и отражатель фары; выгрузка пишет его режимом BLEND."""
    mi = t.mat(key, color, 0.04)
    mat = t.materials[mi]
    next(n for n in mat.node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled").inputs["Alpha"].default_value = alpha
    mat.surface_render_method = "BLENDED"
    return mi


def materials(t):
    """Крашеное железо — одни карты зелёной краски с потёртостями, 2048 точек на 1,6 м: кузов как есть, крылья,
    рама и колёса — затемнённые почти до чёрного, как чёрная эмаль. Доски — светлое крашеное дерево, подкрашенное
    под цвет кузова; пол — серые выветренные доски; кожзаменитель, резина и хром — свои карты."""
    body = t.tex("body", "green_metal_rust_2k", 1.6)
    # Чёрная эмаль: зелёная основа сама тёмная (около 0,06 в линейном цвете), поэтому подкраска светлая и чуть
    # пурпурная — выходит нейтральный тёмный около 0,025, как у настоящей чёрной краски, и рисунок карты виден.
    black = t.tex("black", "green_metal_rust_2k", 1.6, tint="#c8a0d8")
    rubber = t.tex("rubber", "Rubber004", 0.5, tint="#7a7a7a")  # старая шина темнее спортивного покрытия
    return {
        "body": body,
        "frame": body,
        "trim": t.tex("trim", "green_metal_rust_2k", 1.6, tint="#b4b4b4"),
        "dump_in": t.tex("dump_in", "green_metal_rust_2k", 1.6, tint="#d0d0d0"),  # внутри та же краска, чуть темнее
        "bed": t.tex("bed", "PaintedWood009C_1k", 1.2, tint=WOOD_GREEN),
        "bed_b": t.tex("bed_b", "PaintedWood009C_1k", 1.2, tint="#62806a"),  # соседние доски борта — чуть другого тона
        "bed_c": t.tex("bed_c", "PaintedWood009C_1k", 1.2, tint="#77916f"),
        # будка — гладкие крашеные листы: светлая матовая основа под краски листа видов фургона (van/sheet.png)
        "van_top": t.tex("van_top", "white_stucco", 1.2, tint="#42a37d"),
        "van_low": t.tex("van_low", "white_stucco", 1.2, tint="#335f4a"),
        "floor": t.tex("floor", "Wood033", 1.6, tint="#a39a8c", swap=True),  # серые выветренные доски, волокно вдоль
        "black": black,
        "fender": black,
        "chassis": black,
        "iron": black,
        "wheel": black,
        "hub": black,
        # крыша — матовая тёмно-серая ткань, натянутая на каркас, как брезент или дерматин тех машин
        "roof": t.tex("roof", "Linen", 0.5, tint="#4a4744"),
        "tyre": rubber,
        "rubber": rubber,
        # старый хром отражает размыто: ровный металл, шероховатость 0,28 — у зеркальной карты Metal049A 0,035
        "chrome": t.mat("chrome", "#c4c8cc", 0.28, 1.0),
        "gap": t.mat("gap", "#121314", 0.7),
        "glass": glass(t, "glass", "#2a3634", 0.3),
        "lens": glass(t, "lens", "#d8dfe2", 0.18),
        "bulb": t.mat("bulb", "#f2efe4", 0.2),
        "dial": t.mat("dial", "#e9e3d2", 0.4),
        "reflector": t.mat("reflector", "#8d959c", 0.2, 0.6),
        "chrome_dark": t.mat("chrome_dark", "#55595d", 0.35, 0.5),
        "badge": t.mat("badge", "#2a4f8a", 0.3, 0.3),
        "grille": t.mat("grille", "#141516", 0.65),
        "grille_bar": t.mat("grille_bar", "#2e3032", 0.55, 0.6),
        "nut": t.mat("nut", "#6a6d70", 0.5, 0.4),
        "red": t.mat("red", "#a8202a", 0.3),
        "plate": t.mat("plate", "#e8e6dc", 0.5),
    }


def build(name, kind, out=OUT):
    """Полуторка `kind` в пустом файле с затенением в цвете вершин, её выгрузка в `out` и копия для показа
    в Blender в PREVIEW."""
    lib.reset()
    obj = make(name, kind)
    occlusion(obj, name)
    os.makedirs(out, exist_ok=True)
    lib.export(os.path.join(out, name + ".glb"), vertex_color="ACTIVE")
    preview(obj)
    os.makedirs(PREVIEW, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(PREVIEW, name + ".blend"), copy=True)
    print("written", name)


def occlusion(obj, name):
    """Затенение в цвет вершин, как у гаража: запечённое Cycles над землёй — под крыльями, кузовом, в стыках и
    в кабине темнее, — бока темнеют к земле до GRIME, у каждой детали свой тон из слоя граней `tone`.
    Прозрачное и светящееся (CLEAR) не темнеет."""
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 64
    world = bpy.data.worlds.new("bake")
    world.light_settings.distance = AO_DISTANCE
    scene.world = world
    bpy.ops.mesh.primitive_plane_add(size=40.0, location=(0.0, 0.0, -0.002))
    ground = bpy.context.active_object
    mesh = obj.data
    attr = mesh.color_attributes.new("occlusion", "BYTE_COLOR", "CORNER")
    mesh.color_attributes.active_color = attr
    mesh.color_attributes.render_color_index = mesh.color_attributes.active_color_index
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    scene.render.bake.target = "VERTEX_COLORS"
    bpy.ops.object.bake(type="AO")
    tone = mesh.attributes["tone"].data
    clear = {i for i, m in enumerate(mesh.materials) if m.name[len(name) + 1:] in CLEAR}
    for poly in mesh.polygons:
        k = tone[poly.index].value
        up = poly.normal.z > 0.5
        for li in poly.loop_indices:
            if poly.material_index in clear:
                attr.data[li].color = (1.0, 1.0, 1.0, 1.0)
                continue
            z = mesh.vertices[mesh.loops[li].vertex_index].co.z
            v = 0.5 + 0.5 * attr.data[li].color[0]
            if not up:
                v *= 0.78 + 0.22 * smoothstep(0.0, GRIME, z)
            v *= k
            attr.data[li].color = (v, v, v, 1.0)
    mesh.attributes.remove(mesh.attributes["tone"])
    bpy.data.objects.remove(ground)


def preview(obj):
    """Для показа в Blender: цвет материала умножен на цвет вершин, как это делает игра с COLOR_0."""
    for m in obj.data.materials:
        nodes, links = m.node_tree.nodes, m.node_tree.links
        bsdf = lib.bsdf(m)
        src = bsdf.inputs["Base Color"].links[0].from_socket if bsdf.inputs["Base Color"].links else None
        attr = nodes.new("ShaderNodeVertexColor")
        attr.layer_name = "occlusion"
        mul = nodes.new("ShaderNodeMix")
        mul.data_type = "RGBA"
        mul.blend_type = "MULTIPLY"
        mul.inputs[0].default_value = 1.0
        if src is None:
            mul.inputs[6].default_value = bsdf.inputs["Base Color"].default_value
        else:
            links.new(src, mul.inputs[6])
        links.new(attr.outputs["Color"], mul.inputs[7])
        links.new(mul.outputs[2], bsdf.inputs["Base Color"])


def make(name, kind):
    """Объект полуторки `kind` в текущей коллекции, без сброса файла: "flatbed" — бортовая, "van" — фургон,
    "dump" — самосвал ГАЗ-410. Так её собирает и проверка модели в Blender (art/transport/gaz_aa)."""
    if kind not in ("flatbed", "van", "dump"):
        raise ValueError(f"нет такой полуторки: {kind}")
    rear_axle = FRONT_AXLE + (DUMP_WHEELBASE if kind == "dump" else WHEELBASE)
    length = DUMP_LENGTH if kind == "dump" else LENGTH
    t = Truck(name, length)
    m = materials(t)
    hood(t, m)
    radiator(t, m)
    lamps(t, m)
    fenders(t, m)
    cab(t, m)
    interior(t, m)
    chassis(t, m, rear_axle, length - 0.06)
    wheels(t, m, rear_axle)
    if kind == "flatbed":
        flatbed(t, m)
    elif kind == "van":
        van(t, m, rear_axle)
    else:
        dump(t, m, rear_axle)
    t.project()
    t.refine()
    t.pieces()
    return t.finish()


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = args[0] if args else OUT
    build("gaz_aa", "flatbed", out=out)
    build("gaz_aa_van", "van", out=out)
    build("gaz_aa_dump", "dump", out=out)


if __name__ == "__main__":
    main()
