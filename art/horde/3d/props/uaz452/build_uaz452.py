"""Ржавая «буханка» УАЗ-452 для пустоши Орды: без колёс, осела на ступицы.

Машина собрана в своих осях: перед — +X, левый борт — +Y. Размеры — настоящей «буханки»: длина 4,36, ширина 1,94;
без колёс она осела, крыша на 1,76. Кузов — «буханкой»: скруглённые рёбра крыши, короткий нос с заваленным лобовым
стеклом, полоса окон по бортам. Повёрнута передом вниз-влево, к камере смотрят перед и левый борт.

Запускать в живом Blender (MCP), в своей сцене: exec(open(путь).read()). build() строит объём, render_guide() снимает
его камерой игры в guide.png, export_painted() кладёт раскраску paint_raw.png на тот же объём и выгружает модель в
games/horde/models. Общее — art/horde/3d/painted_lib.py.
"""
import importlib
import math
import sys

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import Part, box, build_prop, export_prop, loft, prop_frame  # noqa: E402

NAME = "uaz452"
TURN = math.radians(-130)
LENGTH, HALF = 4.36, 0.97
REAR, FRONT = -LENGTH / 2, LENGTH / 2
AXLES = (REAR + 1.08, FRONT - 0.98)  # база 2,3
ARCH_Z, ARCH_R = 0.05, 0.48
ROOF, ROUND = 1.76, 0.35  # верх крыши и радиус скругления её рёбер
NOSE, HOOD = FRONT - 0.42, 1.0  # где начинается заваленное лобовое стекло и высота нижнего края лобового
WINDOWS = (1.02, 1.38)  # полоса окон по бортам
BOTTOM = 0.12
STEP = 0.04
COLORS = {"body": (0.40, 0.45, 0.30), "glass": (0.12, 0.16, 0.20), "metal": (0.55, 0.56, 0.58),
          "dark": (0.08, 0.07, 0.07)}


def top(x):
    """Верх кузова: крыша, к носу — лобовое стекло до его нижнего края."""
    if x <= NOSE:
        return ROOF
    return ROOF - (ROOF - HOOD) * (x - NOSE) / (FRONT - NOSE)


def half(x):
    """Полуширина в плане: углы «буханки» скруглены."""
    edge = min(x - REAR, FRONT - x)
    return HALF - 0.12 * max(0.0, 1 - edge / 0.3) ** 2


def bottom(x):
    z = BOTTOM
    for axle in AXLES:
        if abs(x - axle) < ARCH_R:
            z = max(z, ARCH_Z + math.sqrt(ARCH_R ** 2 - (x - axle) ** 2))
    return z


def section(x):
    """Сечение: борт, полоса окон, скруглённое ребро крыши, крыша — по обе стороны."""
    h, zt, zb = half(x), top(x), bottom(x)
    r = min(ROUND, (zt - zb) / 3)
    w1 = min(WINDOWS[1], zt - r - 0.02)
    w0 = min(WINDOWS[0], w1 - 0.02)
    c = math.cos(math.radians(45))
    right = [(h, zb + 0.05), (h, w0), (h, w1), (h, zt - r), (h - r + r * c, zt - r + r * c), (h - r, zt)]
    left = [(-y, z) for y, z in reversed(right)]
    return [(x, y, z) for y, z in right + left + [(-(h - 0.05), zb), (h - 0.05, zb)]]


def build():
    xs = sorted({round(REAR + k * STEP, 4) for k in range(int(LENGTH / STEP) + 1)} | {FRONT, NOSE}
                | {round(a + d, 4) for a in AXLES for d in (-ARCH_R, ARCH_R)})
    xs = [x for x in xs if REAR <= x <= FRONT]

    def kind(segment, edge):
        if edge == -1:
            return "body"
        x = xs[max(segment, 0)]
        if edge in (1, 9) and REAR + 0.25 < x < NOSE:  # окна бортов
            return "glass"
        if edge in (3, 4, 5, 6, 7) and x >= NOSE:  # заваленное лобовое стекло
            return "glass"
        return "body"

    part = Part()
    loft(part, kind, [section(x) for x in xs])
    for axle in AXLES:  # тёмная ниша над пустой аркой: сквозь арку не видно земли под машиной
        box(part, "dark", (axle, 0, 0.32), (2 * ARCH_R - 0.1, 2 * HALF - 0.3, 0.42))
    for x in (REAR - 0.05, FRONT + 0.05):  # бамперы
        box(part, "metal", (x, 0, 0.36), (0.1, 2 * HALF, 0.14))
    return build_prop(NAME, part, COLORS, TURN)


def render_guide():
    """Снимок объёма на прозрачном фоне: guide.png — основа раскраски и силуэт картины."""
    prop_frame(NAME).render([("guide.png", {NAME})])


def export_painted():
    export_prop(prop_frame(NAME), NAME)


result = build()
