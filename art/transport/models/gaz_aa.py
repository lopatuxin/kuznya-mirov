# ГАЗ-АА «полуторка» довоенного выпуска и машины на её шасси: бортовая, фургон и самосвал ГАЗ-410.
# Размеры по паспорту: длина 5,335 м, база 3,44, ширина 2,04, высота по кабине 1,97; колея передних колёс
# 1,405, задних 1,6; шины 6,50-20, задние спаренные. Форма — по фотографиям сохранившихся машин: узкий капот
# с жалюзи сбоку, высокая облицовка радиатора, фары на поперечине, круглые чёрные крылья, переходящие
# в подножки, плоская кабина с козырьком крыши над стеклом. У самосвала база короче — 2,98, снята с фотографий.
# Материалы — карты Poly Haven и ambientCG из art/transport/textures в настоящем размере, как у хлебозавода:
# крашеное зелёное и чёрное железо с потёртостями, крашеные и некрашеные доски, кожзаменитель крыши, резина.
# Перед — −Y Blender, левый борт (сторона водителя) — +X, основание — в нуле, середина длины — в нуле. Единица — метр.
# Запуск только полуторок: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/gaz_aa.py [-- <папка>]
# Без папки модели ложатся в games/transport/models.
import math
import os
import sys

import bmesh
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402
from bakery import Plant  # noqa: E402
from gazel import Car, catmull, clamp, smoothstep  # noqa: E402

OUT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "models"))

LENGTH = 5.335
FRONT_AXLE = 0.62  # от переднего края бампера
WHEELBASE = 3.44
DUMP_WHEELBASE = 2.98
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
HOOD_TOP = 1.40
HOOD_CROWN = 0.03
BELT = 1.43
ROOF_TOP = 1.97
CAB_HALF = 0.72
SILL = 0.64
FENDER_X = 0.735  # середина крыла поперёк
CAB_WALL = 0.03
FLOOR = 0.73  # пол кабины поверх лонжеронов рамы
SCREEN_HALF = 0.63  # полуширина лобового стекла
GLASS_Z = (1.48, 1.86)  # низ и верх лобового стекла и окон дверей
DOOR_GLASS = (1.70, 2.27)  # окно двери вдоль машины
REAR_GLASS_HALF = 0.28
REAR_GLASS_Z = (1.55, 1.78)
WOOD_GREEN = "#6b896b"  # подкраска светлых досок под зелень кузова
WOOD_GREEN_DARK = "#5a755a"


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
    return 0.30 + 0.10 * clamp((s - S_HOOD) / (S_COWL - S_HOOD))


def hood_top(s):
    return HOOD_TOP + 0.03 * clamp((s - S_HOOD) / (S_COWL - S_HOOD))


def fender_path():
    """Средняя линия крыла (s, z): спереди опущена к оси, дуга над колесом, сзади стекает в подножку."""
    rho = 0.50
    arc = [(FRONT_AXLE + rho * math.cos(math.radians(a)), WHEEL_R + rho * math.sin(math.radians(a)))
           for a in (168, 150, 130, 110, 90, 70, 52, 38)]
    return catmull(arc + [(FRONT_AXLE + 0.58, 0.66), (FRONT_AXLE + 0.76, 0.61), (S_CAB - 0.06, 0.59)], 5)


def hood(t, m):
    """Капот от облицовки до щитка: чуть шире и выше к кабине, выпуклый верх, шов петли посередине, жалюзи сбоку."""
    stations = [S_HOOD + (S_COWL + 0.02 - S_HOOD) * i / 24 for i in range(25)]
    t.loft([[t.P(x, s, z) for x, z in rounded_rect(hood_half(s), HOOD_BOTTOM, hood_top(s), 0.10, 0.01, HOOD_CROWN)]
            for s in stations], m["body"])
    for k in range(30):
        s0 = S_HOOD + 0.03 + (S_COWL - S_HOOD - 0.03) * k / 30
        s1 = s0 + (S_COWL - S_HOOD - 0.03) / 30
        z = hood_top((s0 + s1) / 2) + HOOD_CROWN
        t.sbox(-0.008, s0, z - 0.004, 0.008, s1, z + 0.007, m["chrome_dark"])  # петля капота
        x = hood_half(s1)
        t.both(x - 0.006, s0, 1.255, x + 0.003, s1, 1.262, m["gap"])  # шов боковины
    for k in range(18):
        s = 0.96 + k * 0.026
        x = hood_half(s) + 0.002
        t.both(x - 0.004, s, 0.93, x + 0.004, s + 0.011, 1.16, m["gap"])
    t.cylinder(0.0, 1.52 - t.length / 2, BELT - 0.01, 0.035, 0.05, m["chrome"], 12)  # пробка бака на щитке


def radiator(t, m):
    """Высокая чёрная облицовка, сетка в планках, знак ГАЗ сверху и крышка горловины."""
    t.rounded_box(t.P(0.0, (S_GRILLE + S_HOOD + 0.02) / 2, 1.0), (0.31, (S_HOOD + 0.02 - S_GRILLE) / 2, 0.41), 0.06,
                  m["black"], m=3, flat=(3, 2, 6))
    # Сердцевина радиатора и сетка перед ней — прутки через 2,4 см, посередине планка облицовки.
    t.sbox(-0.255, S_GRILLE - 0.006, 0.66, 0.255, S_GRILLE + 0.02, 1.33, m["grille"])
    for k in range(22):
        x = -0.252 + k * 0.024
        t.sbox(x, S_GRILLE - 0.010, 0.66, x + 0.005, S_GRILLE - 0.006, 1.33, m["grille_bar"])
    for k in range(28):
        z = 0.665 + k * 0.024
        t.sbox(-0.255, S_GRILLE - 0.009, z, 0.255, S_GRILLE - 0.006, z + 0.005, m["grille_bar"])
    t.sbox(-0.014, S_GRILLE - 0.014, 0.66, 0.014, S_GRILLE - 0.004, 1.33, m["black"])
    badge = t.P(0.0, S_GRILLE - 0.003, 1.365)
    t.revolve_y(badge, [(0.0, 0.012), (0.05, 0.012), (0.055, 0.0)], lambda i: m["chrome"], 16)
    t.revolve_y(badge, [(0.0, 0.016), (0.038, 0.016), (0.04, 0.012)], lambda i: m["badge"], 16)
    t.cylinder(0.0, 0.47 - t.length / 2, 1.405, 0.04, 0.06, m["chrome"], 12)


def lamps(t, m):
    """Две круглые фары на поперечине между крыльями, перед облицовкой: хромовый корпус, за прозрачным стеклом —
    зеркальная чаша отражателя и лампа."""
    t.bar_x(S_GRILLE + 0.02, 1.02, -0.62, 0.62, 0.018, m["black"])
    shell = [(0.02, -0.10), (0.06, -0.09), (0.088, -0.065), (0.102, -0.03), (0.105, 0.0), (0.1, 0.011), (0.088, 0.013),
             (0.045, 0.026), (0.0, 0.028)]
    bowl = [(0.0, -0.075), (0.03, -0.07), (0.06, -0.05), (0.085, -0.02), (0.095, 0.004)]
    for sign in (1, -1):
        c = t.P(sign * 0.50, S_GRILLE - 0.03, 1.12)
        t.revolve_y(c, shell, lambda i: m["chrome"] if i < 5 else m["lens"], 24)
        t.revolve_y(c, bowl, lambda i: m["reflector"], 24)
        t.blob(c.x, c.y + 0.03, c.z, 0.016, 0.022, 0.016, m["bulb"], 1, 0, jitter=0.0)


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
    # Щиток от низа капота к внутреннему краю крыла — сбоку не видно колеса над капотом насквозь.
    inner = FENDER_X - 0.17
    pairs = [(s, (hood_half(s) - 0.01, HOOD_BOTTOM), (inner, z - 0.01)) for s, z in path if S_GRILLE + 0.02 <= s <= S_COWL]
    t.strip(pairs, m["black"])


def cab(t, m):
    """Щиток расширяется от капота к кабине. Кабина полая: стенки в 3 см с проёмами окон, прозрачные стёкла
    в проёмах, внутри — салон; снаружи — двери, ручки, петли и козырёк крыши."""
    def half(s):
        return 0.40 + (CAB_HALF - 0.40) * smoothstep(S_COWL - 0.04, S_CAB, s)

    def bottom(s):
        return HOOD_BOTTOM + (SILL - HOOD_BOTTOM) * smoothstep(S_COWL, S_CAB, s)

    stations = [S_COWL - 0.04 + (S_CAB - S_COWL + 0.04) * i / 12 for i in range(13)]
    t.loft([[t.P(x, s, z) for x, z in rounded_rect(half(s), bottom(s), BELT, 0.03, 0.02)] for s in stations], m["body"])
    w, top, back = CAB_WALL, ROOF_TOP - 0.06, S_CAB_BACK - CAB_WALL
    body = m["body"]
    t.sbox(-CAB_HALF, S_CAB, SILL, CAB_HALF, S_CAB_BACK, FLOOR, m["chassis"])  # пол поверх рамы
    t.both(CAB_HALF - w, S_CAB, SILL, CAB_HALF, S_CAB_BACK, BELT, body)  # двери и боковины ниже окон
    for s0, s1, z0, z1 in ((S_CAB, DOOR_GLASS[0], BELT, top), (DOOR_GLASS[1], S_CAB_BACK, BELT, top),
                           (DOOR_GLASS[0], DOOR_GLASS[1], BELT, GLASS_Z[0]), (DOOR_GLASS[0], DOOR_GLASS[1], GLASS_Z[1], top)):
        t.both(CAB_HALF - w, s0, z0, CAB_HALF, s1, z1, body)  # вокруг окна двери
    t.sbox(-CAB_HALF, S_CAB, BELT - 0.01, CAB_HALF, S_CAB + w, GLASS_Z[0], body)  # под лобовым стеклом
    t.both(SCREEN_HALF, S_CAB, GLASS_Z[0], CAB_HALF, S_CAB + w, GLASS_Z[1], body)  # стойки
    t.sbox(-CAB_HALF, S_CAB, GLASS_Z[1], CAB_HALF, S_CAB + w, top, body)
    t.sbox(-CAB_HALF, back, SILL, CAB_HALF, S_CAB_BACK, REAR_GLASS_Z[0], body)  # задняя стенка вокруг окошка
    t.sbox(-CAB_HALF, back, REAR_GLASS_Z[1], CAB_HALF, S_CAB_BACK, top, body)
    t.both(REAR_GLASS_HALF, back, REAR_GLASS_Z[0], CAB_HALF, S_CAB_BACK, REAR_GLASS_Z[1], body)
    t.rounded_box(t.P(0.0, (S_CAB - 0.11 + S_CAB_BACK + 0.04) / 2, ROOF_TOP - 0.035),
                  (CAB_HALF + 0.03, (S_CAB_BACK + 0.04 - S_CAB + 0.11) / 2, 0.035), 0.03, m["roof"], m=2, flat=(4, 6, 1))
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
        for s0, s1, z0, z1 in ((1.655, 1.665, SILL + 0.02, 1.90), (2.325, 2.335, SILL + 0.02, 1.90),
                               (1.655, 2.335, 1.895, 1.905), (1.655, 2.335, SILL + 0.02, SILL + 0.03)):
            t.sbox(x, s0, z0, x + sign * 0.007, s1, z1, m["gap"])  # щели двери
        t.sbox(x, 2.18, 1.36, x + sign * 0.03, 2.28, 1.38, m["chrome"])  # ручка
        for z in (0.95, 1.62):
            t.sbox(x, 1.645, z, x + sign * 0.022, 1.685, z + 0.07, m["black"])  # петли


def interior(t, m):
    """Салон за стёклами: коврик, скамья в кожзаменителе, приборная доска с щитком приборов посередине,
    руль водителя на наклонной колонке, рычаг передач на тоннеле коробки."""
    w = CAB_WALL
    t.sbox(-CAB_HALF + w, S_CAB + w, FLOOR, CAB_HALF - w, S_CAB_BACK - w, FLOOR + 0.01, m["rubber"])
    t.sbox(-0.62, 2.18, FLOOR + 0.01, 0.62, 2.55, 0.98, m["chassis"])  # короб скамьи
    t.sbox(-0.62, 2.16, 0.98, 0.62, 2.55, 1.08, m["roof"])  # подушка
    t.sbox(-0.62, 2.49, 1.08, 0.62, S_CAB_BACK - w, REAR_GLASS_Z[0] - 0.03, m["roof"])  # спинка ниже заднего окошка
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
    t.sbox(-0.88, 0.0, 0.43, 0.88, 0.06, 0.55, m["black"])  # бампер — швеллер на рогах рамы
    t.both(0.40, 0.06, 0.46, 0.46, S_GRILLE + 0.05, 0.54, d)
    t.both(0.40, S_GRILLE, 0.56, 0.47, frame_end, 0.71, d)  # лонжероны
    for s in (S_GRILLE + 0.1, S_CAB_BACK, rear_axle - 0.6, frame_end - 0.06):
        t.sbox(-0.40, s, 0.58, 0.40, s + 0.06, 0.69, d)
    t.sbox(-0.28, S_HOOD, 0.42, 0.28, S_CAB + 0.2, HOOD_BOTTOM, d)  # мотор и коробка под капотом
    # Передняя ось — балка с поперечной рессорой; задний мост с картером; карданный вал.
    t.sbox(-0.66, FRONT_AXLE - 0.035, WHEEL_R - 0.04, 0.66, FRONT_AXLE + 0.035, WHEEL_R + 0.03, d)
    t.sbox(-0.42, FRONT_AXLE - 0.04, WHEEL_R + 0.03, 0.42, FRONT_AXLE + 0.04, WHEEL_R + 0.07, d)
    t.bar_x(rear_axle, WHEEL_R, -0.75, 0.75, 0.06, d)
    t.rounded_box(t.P(0.0, rear_axle, WHEEL_R), (0.16, 0.17, 0.16), 0.11, d)
    t.sbox(-0.05, S_CAB + 0.2, 0.40, 0.05, rear_axle - 0.15, 0.48, d)
    t.both(0.41, rear_axle - 0.55, WHEEL_R + 0.05, 0.47, rear_axle + 0.55, WHEEL_R + 0.12, d)  # рессоры
    # Фонарь и номер сзади слева — у машин тех лет один фонарь.
    t.sbox(0.60, frame_end - 0.02, 0.62, 0.72, frame_end + 0.04, 0.72, m["black"])
    t.sbox(0.62, frame_end + 0.04, 0.64, 0.70, frame_end + 0.048, 0.70, m["red"])
    t.sbox(0.22, frame_end, 0.58, 0.58, frame_end + 0.012, 0.72, m["plate"])


def wheels(t, m, rear_axle):
    """Шины 6,50-20 на чёрных дисках с шестью гайками; задние спаренные, у внутренних диск не виден."""
    tyre = [(0.25, -0.070), (0.30, -0.085), (0.35, -0.088), (0.385, -0.080), (0.402, -0.062), (0.410, -0.035), (0.411, 0.0),
            (0.410, 0.035), (0.402, 0.062), (0.385, 0.080), (0.35, 0.088), (0.30, 0.085), (0.25, 0.070)]
    disc = [(0.262, 0.064), (0.255, 0.074), (0.245, 0.062), (0.235, 0.045), (0.20, 0.040), (0.16, 0.052), (0.12, 0.072),
            (0.085, 0.090), (0.07, 0.098), (0.062, 0.115), (0.05, 0.128), (0.025, 0.134), (0.0, 0.135)]

    def wheel(x, s, side, with_disc=True, dish=1.0):
        c = t.P(x, s, WHEEL_R)
        t.revolve(c, tyre, lambda r: m["tyre"], side)
        if with_disc:
            t.revolve(c, [(r, w * dish) for r, w in disc], lambda r: m["wheel"] if r > 0.066 else m["hub"], side, closed=False)
            for k in range(6):  # гайки
                a = 2 * math.pi * k / 6
                t.cylinder_x(c.x + side * (0.083 * dish + 0.012), c.y + 0.10 * math.cos(a), c.z + 0.10 * math.sin(a), 0.013, 0.03,
                             m["nut"], 6)

    for sign in (1, -1):
        wheel(sign * FRONT_TRACK / 2, FRONT_AXLE, sign)
        wheel(sign * (REAR_TRACK / 2 + DUAL_GAP / 2), rear_axle, sign, dish=0.55)
        wheel(sign * (REAR_TRACK / 2 - DUAL_GAP / 2), rear_axle, sign, with_disc=False)


def flatbed(t, m):
    """Деревянная платформа: брусья, пол из досок вдоль, борта из трёх досок на железных стойках, задний откидной."""
    end = t.length - 0.02
    floor_top = 1.0
    half = 1.02
    t.both(0.40, S_BED, 0.71, 0.50, end, 0.80, m["chassis"])
    for k in range(6):
        s = S_BED + 0.05 + (end - S_BED - 0.15) * k / 5
        t.sbox(-half, s, 0.84, half, s + 0.08, 0.95, m["chassis"])
    t.sbox(-half, S_BED, 0.95, half, end, floor_top, m["floor"])
    boards = [(floor_top, floor_top + 0.14), (floor_top + 0.155, floor_top + 0.295), (floor_top + 0.31, floor_top + 0.45)]
    for z0, z1 in boards:
        t.both(half - 0.04, S_BED, z0, half, end, z1, m["bed"])
        t.sbox(-half, S_BED, z0, half, S_BED + 0.04, z1, m["bed"])
        t.sbox(-half, end - 0.04, z0, half, end, z1, m["bed"])
    top = boards[-1][1]
    for s in (S_BED + 0.01, S_BED + 0.9, S_BED + 1.75, end - 0.05):  # стойки и оковка бортов
        t.both(half, s, floor_top - 0.04, half + 0.012, s + 0.05, top + 0.01, m["iron"])
    for x in (-half + 0.01, half - 0.06):
        t.sbox(x, end, floor_top - 0.04, x + 0.05, end + 0.012, top + 0.01, m["iron"])
    for x in (-0.6, 0.6):  # петли заднего борта
        t.sbox(x - 0.05, end, floor_top + 0.02, x + 0.05, end + 0.016, floor_top + 0.06, m["iron"])
    t.both(half + 0.012, end - 0.16, top - 0.09, half + 0.03, end - 0.12, top - 0.03, m["iron"])  # крюки запоров


def van(t, m, rear_axle):
    """Деревянный фургон: две краски по поясу, крыша со свесом, двустворчатая дверь сзади, борта опущены
    ниже пола с вырезом над задним колесом."""
    end = t.length - 0.02
    half = 1.0
    z0, belt, z1 = 0.90, 1.58, 2.25
    mid = (S_BED + end) / 2
    half_len = (end - S_BED) / 2
    t.both(0.40, S_BED, 0.71, 0.50, end, 0.80, m["chassis"])
    for k in range(5):
        s = S_BED + 0.05 + (end - S_BED - 0.15) * k / 4
        t.sbox(-half, s, 0.84, half, s + 0.08, z0, m["chassis"])
    # Юбка ниже пола: над колесом её низ идёт по дуге выреза, по краю выреза — тёмная накладка.
    skirt, arch = 0.70, 0.50
    edge = math.sqrt(arch ** 2 - (skirt - WHEEL_R) ** 2)
    t.both(half - 0.02, S_BED, skirt, half, rear_axle - edge, z0 + 0.01, m["van_low"])
    t.both(half - 0.02, rear_axle + edge, skirt, half, end, z0 + 0.01, m["van_low"])
    for k in range(32):
        d0, d1 = -edge + 2 * edge * k / 32, -edge + 2 * edge * (k + 1) / 32
        d = max(abs(d0), abs(d1))
        t.both(half - 0.02, rear_axle + d0, WHEEL_R + math.sqrt(arch ** 2 - d * d), half, rear_axle + d1, z0 + 0.01, m["van_low"])
    a0 = math.degrees(math.asin((skirt - WHEEL_R) / arch))
    for sign in (1, -1):
        frames = []
        for i in range(19):
            a = math.radians(a0 + (180 - 2 * a0) * i / 18)
            radial = Vector((0.0, math.cos(a), math.sin(a)))
            frames.append((t.P(sign * (half - 0.015), rear_axle, WHEEL_R) + radial * arch, radial, Vector((1.0, 0.0, 0.0))))
        t.sweep(frames, [(0.0, -0.03), (0.035, -0.03), (0.035, 0.03), (0.0, 0.03)], m["black"], smooth=False)
    t.rounded_box(t.P(0.0, mid, (z0 + belt) / 2), (half, half_len, (belt - z0) / 2), 0.02, m["van_low"], m=2, flat=(4, 10, 3))
    t.rounded_box(t.P(0.0, mid, (belt + z1) / 2), (half, half_len, (z1 - belt) / 2), 0.05, m["van_top"], m=3, flat=(4, 10, 3))
    t.rounded_box(t.P(0.0, mid, z1 + 0.02), (half + 0.03, half_len + 0.03, 0.025), 0.02, m["roof"], m=2, flat=(4, 10, 1))
    # Пояс-нащельник вокруг кузова, рёбра обшивки по бокам, низ кузова.
    t.both(half, S_BED - 0.005, belt - 0.025, half + 0.018, end + 0.005, belt + 0.025, m["trim"])
    t.sbox(-half - 0.018, end, belt - 0.025, half + 0.018, end + 0.018, belt + 0.025, m["trim"])
    t.sbox(-half - 0.018, S_BED - 0.018, belt - 0.025, half + 0.018, S_BED, belt + 0.025, m["trim"])
    for k in range(1, 6):
        s = S_BED + (end - S_BED) * k / 6
        t.both(half, s - 0.012, z0 + 0.04, half + 0.01, s + 0.012, z1 - 0.04, m["trim"])
    t.both(half, S_BED, z0, half + 0.02, end, z0 + 0.05, m["trim"])
    # Задние двери: щели посередине и по краям, ручки, петли.
    t.sbox(-0.006, end, z0 + 0.06, 0.006, end + 0.004, z1 - 0.06, m["gap"])
    for a, b in ((z0 + 0.06, z0 + 0.07), (z1 - 0.07, z1 - 0.06)):
        t.sbox(-0.92, end, a, 0.92, end + 0.004, b, m["gap"])
    for x in (-0.92, 0.91):
        t.sbox(x, end, z0 + 0.06, x + 0.01, end + 0.004, z1 - 0.06, m["gap"])
    for x in (-0.05, 0.03):
        t.sbox(x, end, 1.45, x + 0.02, end + 0.03, 1.65, m["chrome"])
    for z in (1.05, 1.55, 2.05):
        for x in (-0.95, 0.85):
            t.sbox(x, end, z, x + 0.10, end + 0.015, z + 0.05, m["iron"])


def dump(t, m, rear_axle):
    """Самосвал ГАЗ-410: железный кузов без подъёмника — опрокидывается на шарнире у задней оси."""
    end = t.length - 0.02
    s0 = S_BED + 0.04
    half = 0.86
    floor, top = 0.98, 1.44
    t.both(0.40, S_CAB_BACK + 0.06, 0.71, 0.50, end - 0.25, 0.80, m["chassis"])
    for s in (s0 + 0.1, rear_axle - 0.25, end - 0.35):
        t.sbox(-0.62, s, 0.80, 0.62, s + 0.08, floor - 0.04, m["chassis"])
    t.sbox(-half, s0, floor - 0.04, half, end, floor, m["body"])  # дно
    t.sbox(-half + 0.03, s0 + 0.03, floor, half - 0.03, end - 0.03, floor + 0.004, m["dump_in"])
    t.both(half - 0.03, s0, floor - 0.04, half, end, top, m["body"])  # борта, изнутри темнее
    t.both(half - 0.034, s0 + 0.03, floor, half - 0.03, end - 0.03, top - 0.01, m["dump_in"])
    t.sbox(-half, s0, floor - 0.04, half, s0 + 0.03, top + 0.10, m["body"])  # передний борт выше — щит
    t.sbox(-half - 0.02, s0 - 0.12, top + 0.07, half + 0.02, s0 + 0.03, top + 0.10, m["body"])  # козырёк к кабине
    t.sbox(-half + 0.03, s0 + 0.03, floor, half - 0.03, s0 + 0.034, top + 0.08, m["dump_in"])
    t.sbox(-half, end - 0.03, floor - 0.04, half, end, top - 0.02, m["body"])  # задний борт на верхних петлях
    t.sbox(-half + 0.03, end - 0.034, floor, half - 0.03, end - 0.03, top - 0.03, m["dump_in"])
    # Обвязка по верху и рёбра бортов, запоры заднего борта, шарнир опрокидывания.
    t.both(half, s0, top - 0.04, half + 0.035, end, top + 0.005, m["body"])
    for s in (s0 + 0.45, s0 + 0.95, end - 0.45):
        t.both(half, s, floor - 0.04, half + 0.03, s + 0.05, top - 0.04, m["body"])
    t.both(half - 0.08, end, top - 0.06, half - 0.02, end + 0.04, top, m["iron"])
    t.both(half, end - 0.10, floor + 0.04, half + 0.05, end + 0.02, floor + 0.10, m["iron"])
    t.both(0.36, rear_axle + 0.10, 0.72, 0.52, rear_axle + 0.26, floor - 0.04, m["iron"])


def glass(t, key, color, alpha):
    """Прозрачный материал: сквозь стекло видно салон и отражатель фары; выгрузка пишет его режимом BLEND."""
    mi = t.mat(key, color, 0.04)
    mat = t.materials[mi]
    next(n for n in mat.node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled").inputs["Alpha"].default_value = alpha
    mat.surface_render_method = "BLENDED"
    return mi


def materials(t):
    """Крашеное железо — одни карты зелёной краски с потёртостями: кузов как есть, крылья, рама и колёса —
    затемнённые почти до чёрного, как чёрная эмаль. Доски — светлое крашеное дерево, подкрашенное под цвет
    кузова; кожзаменитель, резина и хром — свои карты."""
    body = t.tex("body", "green_metal_rust", 1.6)
    black = t.tex("black", "green_metal_rust", 1.6, tint="#3a3a3a")
    rubber = t.tex("rubber", "Rubber004", 0.5, tint="#7a7a7a")  # старая шина темнее спортивного покрытия
    return {
        "body": body,
        "frame": body,
        "trim": t.tex("trim", "green_metal_rust", 1.6, tint="#b4b4b4"),
        "dump_in": t.tex("dump_in", "green_metal_rust", 1.6, tint="#9a9a9a"),
        "bed": t.tex("bed", "PaintedWood009C", 1.2, tint=WOOD_GREEN),
        "van_top": t.tex("van_top", "PaintedWood009C", 1.2, tint=WOOD_GREEN),
        "van_low": t.tex("van_low", "PaintedWood009C", 1.2, tint=WOOD_GREEN_DARK),
        "floor": t.tex("floor", "Planks012", 2.1, swap=True),
        "black": black,
        "fender": black,
        "chassis": black,
        "iron": black,
        "wheel": black,
        "hub": black,
        "roof": t.tex("roof", "Leather031", 0.6),
        "tyre": rubber,
        "rubber": rubber,
        "chrome": t.tex("chrome", "Metal049A", 0.4),
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
    """Полуторка `kind`: "flatbed" — бортовая, "van" — фургон, "dump" — самосвал ГАЗ-410."""
    if kind not in ("flatbed", "van", "dump"):
        raise ValueError(f"нет такой полуторки: {kind}")
    lib.reset()
    rear_axle = FRONT_AXLE + (DUMP_WHEELBASE if kind == "dump" else WHEELBASE)
    length = rear_axle + 1.02 if kind == "dump" else LENGTH
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
    t.finish()
    os.makedirs(out, exist_ok=True)
    lib.export(os.path.join(out, name + ".glb"))
    print("written", name)


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = args[0] if args else OUT
    build("gaz_aa", "flatbed", out=out)
    build("gaz_aa_van", "van", out=out)
    build("gaz_aa_dump", "dump", out=out)


if __name__ == "__main__":
    main()
