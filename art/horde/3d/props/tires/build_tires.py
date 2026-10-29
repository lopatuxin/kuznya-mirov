"""Две старые шины: одна лежит, вторая привалилась к ней краем.

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
from painted_lib import Part, build_prop, cylinder, export_prop, merge, prop_frame, turned  # noqa: E402

NAME = "tires"
OUTER, INNER, WIDTH = 0.33, 0.19, 0.21
SECOND = (0.62, 0.05)  # середина второй шины
COLORS = {"rubber": (0.16, 0.16, 0.17), "hole": (0.05, 0.05, 0.05)}


def tire(x, y):
    """Лежащая шина: резина цилиндром, дыра посередине — тёмный круг на верхней грани."""
    part = Part()
    cylinder(part, "rubber", (x, y, 0.0), OUTER, WIDTH, 20)
    cylinder(part, "hole", (x, y, WIDTH), INNER, 0.005, 20)
    return part


def build():
    part = tire(0.0, 0.0)
    merge(part, turned(tire(*SECOND), math.radians(-28), "Y", (SECOND[0] - OUTER, SECOND[1], 0.0)))
    return build_prop(NAME, part, COLORS)


def render_guide():
    """Снимок объёма на прозрачном фоне: guide.png — основа раскраски и силуэт картины."""
    prop_frame(NAME).render([("guide.png", {NAME})])


def export_painted():
    export_prop(prop_frame(NAME), NAME)


result = build()
