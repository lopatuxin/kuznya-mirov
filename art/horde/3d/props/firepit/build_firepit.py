"""Кострище лагеря Орды: кольцо камней, зола и обгоревшие поленья шалашом посередине.

Пламени здесь нет: рисованный огонь придёт кадрами (фаза Кузни 64) вместе со своим объёмом.

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
from painted_lib import Part, box, build_prop, cylinder, export_prop, loft, merge, prop_frame, ring, turned  # noqa: E402

NAME = "firepit"
STONES, RING = 11, 0.82
LOGS = [(0.4, 0.05), (2.5, 0.12), (4.4, 0.08)]  # поворот вокруг середины и подъём над золой
COLORS = {"stone": (0.70, 0.68, 0.64), "ash": (0.30, 0.28, 0.27), "wood": (0.25, 0.17, 0.11)}


def build():
    part = Part()
    for k in range(STONES):  # окатанные камни: низкие неровные глыбки
        stone = Part()
        radius, height = 0.15 + 0.03 * (k % 3), 0.17 + 0.04 * ((k * 2) % 3)
        jitter = [1.0 + 0.18 * math.sin(3.1 * k + 1.7 * i) for i in range(7)]
        loft(stone, "stone", [ring((RING, 0.0), radius * scale, 7, height * share, k, jitter)
                              for share, scale in ((0.0, 0.9), (0.55, 1.0), (1.0, 0.55))])
        merge(part, turned(stone, math.tau * k / STONES))
    cylinder(part, "ash", (0, 0, 0), RING - 0.12, 0.03, 16)
    for angle, lift in LOGS:
        log = Part()
        box(log, "wood", (0.0, 0.0, lift + 0.1), (0.75, 0.09, 0.09))
        merge(part, turned(turned(log, math.radians(-14), "Y"), angle))
    return build_prop(NAME, part, COLORS)


def render_guide():
    """Снимок объёма на прозрачном фоне: guide.png — основа раскраски и силуэт картины."""
    prop_frame(NAME).render([("guide.png", {NAME})])


def export_painted():
    export_prop(prop_frame(NAME), NAME)


result = build()
