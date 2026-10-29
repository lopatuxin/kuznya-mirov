"""Два лиственных дерева для краёв полосы Орды: ствол с развилкой и округлая крона из нескольких комьев.

Каждое — своя модель: tree_a, tree_b. Крона начинается выше роста героя: держит его только ствол, за кроной он
скрыт, как за балконом.

Запускать в живом Blender (MCP), в своей сцене: exec(open(путь).read()). build() строит объёмы, render_guide()
снимает каждый камерой игры в props/<имя>/guide.png, export_painted() кладёт раскраски paint_raw.png на те же объёмы
и выгружает модели в games/horde/models. Общее — art/horde/3d/painted_lib.py.
"""
import importlib
import math
import sys

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import Part, beam, build_prop, cylinder, export_prop, loft, prop_frame, ring  # noqa: E402

SIDES = 14
# Ствол: радиус у земли и у развилки, высота развилки; ветви — от развилки к середине кома; комья кроны —
# середина по плану, радиус, низ и высота.
TREES = {
    "tree_a": {
        "trunk": (0.22, 0.16, 1.9),
        "branches": [((0.6, 0.3, 3.0), 0.12), ((-0.7, -0.2, 2.9), 0.11), ((0.0, -0.6, 3.2), 0.1)],
        "crown": [((0.0, 0.0), 1.7, 2.1, 2.4), ((0.9, 0.5), 1.1, 2.3, 1.7), ((-1.0, -0.3), 1.0, 2.2, 1.6),
                  ((0.2, -0.9), 0.9, 2.4, 1.5)],
    },
    "tree_b": {
        "trunk": (0.2, 0.14, 2.3),
        "branches": [((0.5, 0.2, 3.6), 0.11), ((-0.6, 0.1, 3.3), 0.1)],
        "crown": [((0.4, 0.2), 1.2, 2.6, 2.3), ((-0.6, 0.0), 1.0, 2.4, 2.0), ((0.0, -0.5), 0.8, 3.1, 1.6)],
    },
}
SECTIONS = [(0.0, 0.55), (0.2, 0.9), (0.45, 1.0), (0.7, 0.9), (0.9, 0.55), (1.0, 0.15)]
COLORS = {"bark": (0.42, 0.33, 0.24), "leaves": (0.33, 0.52, 0.20)}


def build():
    made = {}
    for name, tree in TREES.items():
        part = Part()
        low, high, fork = tree["trunk"]
        cylinder(part, "bark", (0, 0, 0), low, fork, 10, high)
        for end, thick in tree["branches"]:
            beam(part, "bark", (0, 0, fork - 0.1), end, thick)
        for seed, (center, radius, bottom, height) in enumerate(tree["crown"]):
            jitter = [1.0 + 0.1 * math.sin(seed * 1.7 + 2.2 * k) for k in range(SIDES)]
            loft(part, "leaves", [ring(center, radius * scale, SIDES, bottom + height * share, seed, jitter)
                                  for share, scale in SECTIONS])
        made.update(build_prop(name, part, COLORS))
    return made


def render_guide():
    """Снимки объёмов на прозрачном фоне: guide.png каждого — основа раскраски и силуэт картины."""
    for name in TREES:
        prop_frame(name).render([("guide.png", {name})])


def export_painted():
    for name in TREES:
        export_prop(prop_frame(name), name)


result = build()
