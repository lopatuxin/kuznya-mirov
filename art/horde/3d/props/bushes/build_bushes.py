"""Три куста для пустоши и лагеря Орды: густые, из нескольких сросшихся округлых комьев листвы.

Каждый — своя модель: bush_a, bush_b, bush_c. Ком собран сечениями по высоте с неровным радиусом.

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
from painted_lib import Part, build_prop, export_prop, loft, prop_frame, ring  # noqa: E402

SIDES = 12
# Ком: середина по плану, радиус, низ и высота. Комья куста перекрываются, низ у земли — куст растёт из неё.
BUSHES = {
    "bush_a": [((0.0, 0.0), 0.9, 0.0, 1.2), ((0.55, 0.35), 0.6, 0.0, 0.9), ((-0.5, -0.3), 0.55, 0.0, 0.8)],
    "bush_b": [((0.0, 0.0), 1.05, 0.0, 1.45), ((-0.7, 0.2), 0.7, 0.0, 1.0), ((0.6, -0.45), 0.65, 0.0, 1.05),
               ((0.2, 0.7), 0.5, 0.0, 0.85)],
    "bush_c": [((0.0, 0.0), 0.75, 0.0, 1.05), ((0.5, -0.2), 0.5, 0.0, 0.75)],
}
# Сечения кома: доля высоты и доля радиуса — округлая шапка над узким основанием.
SECTIONS = [(0.0, 0.7), (0.25, 0.95), (0.5, 1.0), (0.75, 0.85), (0.92, 0.5), (1.0, 0.15)]
COLORS = {"leaves": (0.36, 0.55, 0.22)}


def lump(part, center, radius, low, height, seed):
    jitter = [1.0 + 0.12 * math.sin(seed * 2.3 + 1.9 * k) + 0.06 * math.sin(seed + 4.1 * k) for k in range(SIDES)]
    loft(part, "leaves", [ring(center, radius * scale, SIDES, low + height * share, seed, jitter)
                          for share, scale in SECTIONS])


def build():
    made = {}
    for name, lumps in BUSHES.items():
        part = Part()
        for seed, (center, radius, low, height) in enumerate(lumps):
            lump(part, center, radius, low, height, seed + len(name) + ord(name[-1]))
        made.update(build_prop(name, part, COLORS))
    return made


def render_guide():
    """Снимки объёмов на прозрачном фоне: guide.png каждого — основа раскраски и силуэт картины."""
    for name in BUSHES:
        prop_frame(name).render([("guide.png", {name})])


def export_painted():
    for name in BUSHES:
        export_prop(prop_frame(name), name)


result = build()
