"""Три валуна светлого известняка для пустоши Орды: приземистые обкатанные глыбы разной величины, с одним-двумя
плоскими сколами, на треть ушедшие в землю.

Каждый — своя модель: rock_a, rock_b, rock_c. Глыба — шар, смятый плавными волнами и срезанный плоскостями сколов.

Запускать в живом Blender (MCP), в своей сцене: exec(open(путь).read()). build() строит объёмы, render_guide()
снимает каждый камерой игры в props/<имя>/guide.png, export_painted() кладёт раскраски paint_raw.png на те же объёмы
и выгружает модели в games/horde/models. Общее — art/horde/3d/painted_lib.py.
"""
import importlib
import math
import random
import sys

import bmesh
from mathutils import Vector

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import Part, build_prop, export_prop, prop_frame  # noqa: E402

# Имя, полуоси шара (x, y, z), какая доля высоты шара ушла в землю, зерно неровности и сколы: направление сверху в
# градусах, наклон нормали скола от земли в градусах и на какой доле от середины до края шар срезан.
ROCKS = [
    ("rock_a", (1.0, 0.78, 0.82), 0.3, 3, [(215, 20, 0.84), (330, 50, 0.86), (95, 10, 0.88)]),
    ("rock_b", (1.32, 1.02, 1.05), 0.32, 11, [(20, 25, 0.83), (140, 55, 0.87), (280, 15, 0.92)]),
    ("rock_c", (0.66, 0.5, 0.5), 0.28, 7, [(310, 30, 0.9), (60, 45, 0.86), (180, 20, 0.9)]),
]
LUMPS = 6  # волн неровности на камень
COLORS = {"stone": (0.80, 0.78, 0.72)}
NAMES = [name for name, *_ in ROCKS]


def boulder(axes, sunk, seed, cuts):
    """Грани валуна: шар, смятый плавными волнами, срезанный плоскими сколами и утопленный в землю на долю `sunk`
    высоты; всё, что ниже земли, ложится на землю — низ плоский."""
    rng = random.Random(seed)
    waves = [(Vector((rng.uniform(-1, 1), rng.uniform(-1, 1), rng.uniform(-1, 1))).normalized() * rng.uniform(1.3, 3.2),
              rng.uniform(0, math.tau), rng.uniform(0.05, 0.11)) for _ in range(LUMPS)]
    planes = []
    for direction, tilt, keep in cuts:
        a, t = math.radians(direction), math.radians(tilt)
        planes.append((Vector((math.cos(a) * math.cos(t), math.sin(a) * math.cos(t), math.sin(t))), keep))
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=3, radius=1.0)
    for vert in bm.verts:
        p = vert.co.normalized()
        p *= 1.0 + sum(size * math.sin(frequency.dot(p) + phase) for frequency, phase, size in waves)
        for normal, keep in planes:
            reach = p.dot(normal) - keep
            if reach > 0:
                p -= normal * reach
        vert.co = Vector((p.x * axes[0], p.y * axes[1], max(0.0, (p.z + 1.0) * axes[2] - 2 * sunk * axes[2])))
    faces = [[tuple(v.co) for v in face.verts] for face in bm.faces]
    bm.free()
    part = Part()
    part.add("stone", faces)
    return part


def build():
    made = {}
    for name, axes, sunk, seed, cuts in ROCKS:
        made.update(build_prop(name, boulder(axes, sunk, seed, cuts), COLORS))
    return made


def render_guide():
    """Снимки объёмов на прозрачном фоне: guide.png каждого — основа раскраски и силуэт картины."""
    for name in NAMES:
        prop_frame(name).render([("guide.png", {name})])


def export_painted():
    for name in NAMES:
        export_prop(prop_frame(name), name)


result = build()
