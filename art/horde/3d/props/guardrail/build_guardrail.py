"""Кусок дорожного ограждения вдоль края дороги в пустоши Орды: железная волнистая полоса на столбиках, дальний конец
сорван и смят к земле.

Собрано вдоль +X серединой в начале осей и повёрнуто вдоль края дороги: в сцене от (33,7; 12,9) к (39,5; 18,2),
смятый конец — дальний. Полоса со стороны дороги (-Y до поворота).

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
from painted_lib import Part, beam, box, build_prop, export_prop, prop_frame  # noqa: E402

NAME = "guardrail"
START, END = (33.7, 12.9), (39.5, 18.2)  # концы в клетках сцены: x вправо, y к игроку
LENGTH = math.hypot(END[0] - START[0], END[1] - START[1])
TURN = -math.atan2(END[1] - START[1], END[0] - START[0])  # в Blender ось y обратна оси сцены
POSTS = 6
POST, POST_HEIGHT = 0.11, 0.75
RAIL_LOW, RAIL_HIGH, RAIL_THICK, RAIL_OUT = 0.42, 0.74, 0.06, -0.12
CRUMPLE = [(0.45, -0.05, 0.5), (0.8, 0.12, 0.25), (1.05, 0.3, 0.06)]  # смятый конец за последним столбиком
COLORS = {"metal": (0.66, 0.68, 0.70), "post": (0.42, 0.42, 0.44)}


def build():
    part = Part()
    half = LENGTH / 2
    for k in range(POSTS + 1):
        x = -half + LENGTH * k / POSTS
        tilt = 0.25 if k == POSTS else 0.0  # последний столбик согнут вместе с полосой
        beam(part, "post", (x, 0.0, 0.0), (x + tilt, 0.0, POST_HEIGHT - tilt * 0.6), POST)
    middle = (RAIL_LOW + RAIL_HIGH) / 2
    box(part, "metal", (0.0, RAIL_OUT, middle), (LENGTH, RAIL_THICK, RAIL_HIGH - RAIL_LOW))
    previous = (half, RAIL_OUT, middle)
    for along, aside, z in CRUMPLE:
        point = (half + along, RAIL_OUT + aside, z)
        beam(part, "metal", previous, point, RAIL_HIGH - RAIL_LOW)
        previous = point
    return build_prop(NAME, part, COLORS, TURN)


def render_guide():
    """Снимок объёма на прозрачном фоне: guide.png — основа раскраски и силуэт картины."""
    prop_frame(NAME).render([("guide.png", {NAME})])


def export_painted():
    export_prop(prop_frame(NAME), NAME)


result = build()
