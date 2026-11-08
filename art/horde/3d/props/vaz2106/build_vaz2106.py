"""Ржавая «шестёрка» ВАЗ-2106 для Орды: без колёс, лежит на брюхе на краю дороги.

Запускать в живом Blender (MCP), в своей сцене: exec(open(путь).read()). build() строит объём в коллекции
«vaz2106», render_guide() снимает его камерой игры в guide.png; раскраска нейросети paint_raw.png подгоняется в кадр
снимка (paint.png), export_painted() кладёт её на тот же объём и выгружает games/horde/models/vaz2106.glb. Общее — art/horde/3d/painted_lib.py.

Машина собрана в своих осях: перед — +X, левый борт — +Y. Размеры — настоящей «шестёрки»: длина 4,17, ширина 1,61;
без колёс она осела, крыша на 1,15, колёсные арки пустые. Повёрнута передом к дороге вниз-вправо, к камере смотрят
перед и правый борт.
"""
import importlib
import math
import os
import sys

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import ROOT, Frame, Part, box, build_prop, export_prop, loft  # noqa: E402

NAME = "vaz2106"
TURN = math.radians(-40)
FOLDER = os.path.join(ROOT, "art", "horde", "3d", "props", NAME)
FRAME = Frame(6.0, 0.3, 1024, FOLDER)

LENGTH, WIDTH = 4.166, 1.611
REAR, FRONT = -LENGTH / 2, LENGTH / 2
# Ось задних колёс в 0,95 от зада, передних — в 0,79 от переда (база 2,424); без колёс машина осела на 0,28,
# и середины арок почти у земли.
AXLES = (REAR + 0.95, FRONT - 0.79)
ARCH_Z, ARCH_R = 0.02, 0.42
# Профиль сбоку, точки (x, высота): верх капота, линии окон и крышки багажника; низ кузова; верх салона.
DECK = [(REAR, 0.56), (REAR + 0.13, 0.61), (-1.4, 0.645), (-1.27, 0.655), (0.53, 0.67), (1.0, 0.655),
        (FRONT - 0.13, 0.6), (FRONT, 0.57)]
BOTTOM = [(REAR, 0.22), (REAR + 0.38, 0.1), (FRONT - 0.33, 0.1), (FRONT, 0.2)]
REAR_GLASS, ROOF_BACK, ROOF_FRONT, WINDSHIELD = -1.27, -0.97, 0.13, 0.53
ROOF_TOP = 1.15
# Полуширина в плане: углы кузова скруглены.
HALF = [(REAR, 0.74), (REAR + 0.1, 0.79), (REAR + 0.3, 0.805), (FRONT - 0.3, 0.805), (FRONT - 0.08, 0.795),
        (FRONT, 0.77)]
CHAMFER, SILL = 0.08, 0.05  # скос верхнего и нижнего ребра кузова
STEP = 0.04  # шаг сечений вдоль машины

COLORS = {
    "body": (0.62, 0.30, 0.20),
    "glass": (0.12, 0.16, 0.20),
    "metal": (0.55, 0.56, 0.58),
    "dark": (0.08, 0.07, 0.07),
}


def along(points, x):
    """Значение ломаной (x, значение) в точке x."""
    for (x0, v0), (x1, v1) in zip(points, points[1:]):
        if x0 <= x <= x1:
            return v0 + (v1 - v0) * (x - x0) / (x1 - x0)
    return points[0][1] if x < points[0][0] else points[-1][1]


def stations(start, end, extra=()):
    """Места сечений от start до end с шагом STEP и в точках перелома."""
    count = max(1, round((end - start) / STEP))
    marks = {round(start + (end - start) * k / count, 4) for k in range(count + 1)}
    marks |= {round(x, 4) for x in extra if start < x < end}
    return sorted(marks)


def bottom(x):
    """Низ кузова: брюхо, подъём к бамперам и круглые пустые арки."""
    z = along(BOTTOM, x)
    for axle in AXLES:
        if abs(x - axle) < ARCH_R:
            z = max(z, ARCH_Z + math.sqrt(ARCH_R ** 2 - (x - axle) ** 2))
    return z


def lower_body(part):
    """Кузов до линии окон: сечение со скошенными рёбрами, профиль капота и багажника, круглые арки."""
    breaks = [x for x, _ in DECK + BOTTOM + HALF] + [a + d for a in AXLES for d in (-ARCH_R, ARCH_R)]
    rings = []
    for x in stations(REAR, FRONT, breaks):
        h, top, low = along(HALF, x), along(DECK, x), bottom(x)
        rings.append([(x, h, low + SILL), (x, h, top - CHAMFER), (x, h - CHAMFER, top), (x, -h + CHAMFER, top),
                      (x, -h, top - CHAMFER), (x, -h, low + SILL), (x, -h + SILL, low), (x, h - SILL, low)])
    loft(part, "body", rings)
    for axle in AXLES:  # тёмная ниша над пустой аркой, чуть уже кузова: сквозь арку не видно земли под машиной
        box(part, "dark", (axle, 0, 0.29), (2 * ARCH_R - 0.08, 2 * 0.805 - 0.24, 0.38))
    for x, z in ((REAR - 0.05, 0.3), (FRONT + 0.05, 0.27)):  # бамперы
        box(part, "metal", (x, 0, z), (0.1, WIDTH - 0.02, 0.13))


def cabin(part):
    """Салон: стёкла от линии окон до водостока, крыша; лобовое и заднее стекло — наклонные верхние грани."""
    def roof(x):
        if x <= ROOF_BACK:
            return along([(REAR_GLASS, along(DECK, REAR_GLASS) + 0.02), (ROOF_BACK, ROOF_TOP - 0.02)], x)
        if x >= ROOF_FRONT:
            return along([(ROOF_FRONT, ROOF_TOP - 0.02), (WINDSHIELD, along(DECK, WINDSHIELD) + 0.02)], x)
        middle = (ROOF_BACK + ROOF_FRONT) / 2
        return ROOF_TOP - 0.02 * ((x - middle) / (ROOF_FRONT - middle)) ** 2

    xs = stations(REAR_GLASS, WINDSHIELD, (ROOF_BACK, ROOF_FRONT))
    rings = []
    for x in xs:
        belt, top = along(DECK, x), roof(x)
        gutter = max(belt + 0.01, top - 0.04)
        rings.append([(x, 0.7, belt), (x, 0.6, gutter), (x, 0.58, top), (x, -0.58, top), (x, -0.6, gutter),
                      (x, -0.7, belt)])

    def kind(segment, edge):
        if edge in (0, 4) or edge == -1 or edge == 5:
            return "glass"
        sloped = not (ROOF_BACK <= xs[max(segment, 0)] < ROOF_FRONT)
        return "glass" if edge == 2 and sloped else "body"

    loft(part, kind, rings)


def build():
    part = Part()
    lower_body(part)
    cabin(part)
    return build_prop(NAME, part, COLORS, TURN)


def render_guide():
    """Снимок объёма на прозрачном фоне: guide.png — основа раскраски и силуэт картины."""
    FRAME.render([("guide.png", {NAME})])


def export_painted():
    """vaz2106.glb: объём с картиной из раскраски paint_raw.png, подогнанной в кадр, и силуэта guide.png."""
    export_prop(FRAME, NAME)


result = build()
