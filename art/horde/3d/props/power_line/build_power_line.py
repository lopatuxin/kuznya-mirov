"""Линия электропередачи у дороги в пустоши Орды: два деревянных столба с перекладинами и изоляторами, провода между
ними провисли, у первого столба один провод оборван и свисает; второй столб завален.

Одна модель на оба столба: середина между ними в начале осей. В сцене первый столб стоит в (33,2; 28,2), второй —
в (43,5; 33,5), на обочине дороги. Провода висят выше героя, держат его только столбы.

Запускать в живом Blender (MCP), в своей сцене: exec(open(путь).read()). build() строит объём, render_guide() снимает
его камерой игры в guide.png, export_painted() кладёт раскраску paint_raw.png на тот же объём и выгружает модель в
games/horde/models. Общее — art/horde/3d/painted_lib.py.
"""
import importlib
import math
import sys

from mathutils import Vector

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import Part, beam, build_prop, cylinder, export_prop, prop_frame  # noqa: E402

NAME = "power_line"
FIRST, SECOND = (33.2, 28.2), (43.5, 33.5)  # столбы в клетках сцены: x вправо, y к игроку
MIDDLE = ((FIRST[0] + SECOND[0]) / 2, (FIRST[1] + SECOND[1]) / 2)
HEIGHT, RADIUS = 6.0, 0.13
ARM, ARM_DROP = 1.5, 0.35  # длина перекладины и её высота ниже макушки
LEAN = math.radians(9)  # второй столб завален прочь от линии
WIRE, SAG, PIECES = 0.035, 1.1, 12
COLORS = {"wood": (0.45, 0.34, 0.24), "metal": (0.30, 0.30, 0.32), "glass": (0.55, 0.75, 0.70)}


def local(point):
    """Точка сцены в осях модели Blender: середина линии в начале, ось y обратна оси сцены."""
    return Vector((point[0] - MIDDLE[0], -(point[1] - MIDDLE[1]), 0.0))


def pole(part, base, lean_dir, lean):
    """Столб с перекладиной поперёк линии и тремя изоляторами; концы проводов — точки изоляторов."""
    top = base + lean_dir * math.sin(lean) * HEIGHT + Vector((0, 0, math.cos(lean) * HEIGHT))
    cylinder(part, "wood", tuple(base), RADIUS, 0.01, 8)
    beam(part, "wood", tuple(base), tuple(top), RADIUS * 2)
    line = (local(SECOND) - local(FIRST)).normalized()
    across = Vector((-line.y, line.x, 0.0))
    arm_middle = base + (top - base) * (1 - ARM_DROP / HEIGHT)
    beam(part, "wood", tuple(arm_middle - across * ARM / 2), tuple(arm_middle + across * ARM / 2), 0.12)
    ends = []
    for offset in (-0.6, 0.0, 0.6):
        foot = arm_middle + across * offset + Vector((0, 0, 0.06))
        cylinder(part, "glass", tuple(foot), 0.06, 0.16, 6, 0.04)
        ends.append(foot + Vector((0, 0, 0.16)))
    return ends


def wire(part, a, b):
    """Провисший провод цепочкой коротких брусков."""
    points = [a.lerp(b, k / PIECES) - Vector((0, 0, SAG * 4 * (k / PIECES) * (1 - k / PIECES))) for k in range(PIECES + 1)]
    for p, q in zip(points, points[1:]):
        beam(part, "metal", tuple(p), tuple(q), WIRE)


def build():
    part = Part()
    first, second = local(FIRST), local(SECOND)
    away = (second - first).normalized()
    ends_first = pole(part, first, away, 0.0)
    ends_second = pole(part, second, away, LEAN)
    for a, b in zip(ends_first[1:], ends_second[1:]):
        wire(part, a, b)
    torn = ends_first[0]  # оборванный провод свисает от первого столба выше роста героя
    hang = [torn, torn + Vector((0.5, -0.2, -1.4)), torn + Vector((0.7, -0.4, -2.6)), torn + Vector((0.8, -0.3, -3.4))]
    for p, q in zip(hang, hang[1:]):
        beam(part, "metal", tuple(p), tuple(q), WIRE)
    return build_prop(NAME, part, COLORS)


def render_guide():
    """Снимок объёма на прозрачном фоне: guide.png — основа раскраски и силуэт картины."""
    prop_frame(NAME).render([("guide.png", {NAME})])


def export_painted():
    export_prop(prop_frame(NAME), NAME)


result = build()
