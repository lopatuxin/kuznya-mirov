"""Три валуна светлого известняка для пустоши Орды: неровные угловатые глыбы разной величины.

Каждый — своя модель: rock_a, rock_b, rock_c. Глыба собрана сечениями по высоте с неровным радиусом.

Запускать в живом Blender (MCP), в своей сцене: exec(open(путь).read()). build() строит объёмы, render_guide()
снимает каждый камерой игры в props/<имя>/guide.png, export_painted() кладёт раскраски paint_raw.png на те же объёмы
и выгружает модели в games/horde/models. Общее — art/horde/3d/painted_lib.py.
"""
import importlib
import sys

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import Part, build_prop, export_prop, loft, prop_frame, ring  # noqa: E402

SIDES = 9
# Имя, радиус у земли, высота, поворот сечений и множители радиуса по точкам — неровность глыбы.
ROCKS = [
    ("rock_a", 1.0, 1.1, 0.3, [1.0, 0.82, 1.08, 0.9, 1.12, 0.78, 0.95, 1.05, 0.86]),
    ("rock_b", 1.4, 1.4, 1.1, [0.92, 1.1, 0.8, 1.02, 0.95, 1.14, 0.84, 0.98, 1.06]),
    ("rock_c", 0.75, 0.8, 2.0, [1.08, 0.9, 1.0, 0.8, 1.1, 0.96, 0.85, 1.12, 0.92]),
]
# Сечения: доля высоты и доля радиуса; верх сколот плоско, как у колотого камня.
SECTIONS = [(0.0, 1.0), (0.45, 1.04), (0.82, 0.78), (1.0, 0.4)]
COLORS = {"stone": (0.80, 0.78, 0.72)}
NAMES = [name for name, *_ in ROCKS]


def build():
    made = {}
    for name, radius, height, turn, jitter in ROCKS:
        part = Part()
        rings = []
        for share, scale in SECTIONS:
            shaken = [j ** (1.0 + share) for j in jitter]  # к верху неровность сильнее
            rings.append(ring((0.0, 0.0), radius * scale, SIDES, height * share, turn + share * 0.4, shaken))
        loft(part, "stone", rings)
        made.update(build_prop(name, part, COLORS))
    return made


def render_guide():
    """Снимки объёмов на прозрачном фоне: guide.png каждого — основа раскраски и силуэт картины."""
    for name in NAMES:
        prop_frame(name).render([("guide.png", {name})])


def export_painted():
    for name in NAMES:
        export_prop(prop_frame(name), name)


result = build()
