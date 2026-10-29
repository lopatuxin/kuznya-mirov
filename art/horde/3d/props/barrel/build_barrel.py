"""Ржавая железная бочка на 200 литров: цилиндр с закраинами и двумя обручами.

Запускать в живом Blender (MCP), в своей сцене: exec(open(путь).read()). build() строит объём, render_guide() снимает
его камерой игры в guide.png, export_painted() кладёт раскраску paint_raw.png на тот же объём и выгружает модель в
games/horde/models. Общее — art/horde/3d/painted_lib.py.
"""
import importlib
import sys

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import Part, build_prop, cylinder, export_prop, prop_frame  # noqa: E402

NAME = "barrel"
RADIUS, HEIGHT = 0.29, 0.88
COLORS = {"metal": (0.62, 0.25, 0.15), "rim": (0.45, 0.20, 0.12)}


def build():
    part = Part()
    cylinder(part, "metal", (0, 0, 0), RADIUS, HEIGHT, 20)
    for z in (0.0, HEIGHT / 3, 2 * HEIGHT / 3, HEIGHT - 0.03):
        cylinder(part, "rim", (0, 0, z), RADIUS + 0.015, 0.03, 20)
    return build_prop(NAME, part, COLORS)


def render_guide():
    """Снимок объёма на прозрачном фоне: guide.png — основа раскраски и силуэт картины."""
    prop_frame(NAME).render([("guide.png", {NAME})])


def export_painted():
    export_prop(prop_frame(NAME), NAME)


result = build()
