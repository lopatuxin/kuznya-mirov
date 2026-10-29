"""Деревянный ящик для лагеря Орды: доски, обвязка у низа и у крышки, повёрнут на 20°.

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
from painted_lib import Part, box, build_prop, export_prop, prop_frame  # noqa: E402

NAME = "crate"
TURN = math.radians(20)
SIDE, HEIGHT = 0.9, 0.8
COLORS = {"wood": (0.62, 0.47, 0.29), "wood_dark": (0.40, 0.29, 0.18)}


def build():
    part = Part()
    box(part, "wood", (0, 0, HEIGHT / 2), (SIDE, SIDE, HEIGHT))
    for z in (0.04, HEIGHT - 0.04):
        box(part, "wood_dark", (0, 0, z), (SIDE + 0.03, SIDE + 0.03, 0.08))
    return build_prop(NAME, part, COLORS, TURN)


def render_guide():
    """Снимок объёма на прозрачном фоне: guide.png — основа раскраски и силуэт картины."""
    prop_frame(NAME).render([("guide.png", {NAME})])


def export_painted():
    export_prop(prop_frame(NAME), NAME)


result = build()
