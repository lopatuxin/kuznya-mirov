"""Хилый забор вокруг вышки Орды: пролёты из жердей и редких досок на кривых столбиках, проход в пустошь справа.

Забор идёт двумя ломаными наброска (клетки сцены: x вправо, y к игроку). Каждый участок ломаной — отдельный кадр и
отдельная раскраска нейросети (props/fence/e<номер>/); участок режется на пролёты около PANEL клетки, каждый пролёт —
своя модель fence_<NN> со своим куском картины участка. Пролёты встык делят столбики: щели
между ними нет. Пролёт можно сдвинуть, но не повернуть: картина нарисована под камеру игры.

Запускать в живом Blender (MCP), в своей сцене: exec(open(путь).read()). build() строит пролёты, render_guide()
снимает каждый участок камерой игры в e<номер>/guide.png, export_painted() подгоняет раскраски paint_raw.png в кадр,
кладёт их на пролёты и выгружает модели в games/horde/models. Расстановка пролётов — PLACES после build().
"""
import importlib
import math
import os
import sys

import bpy
import numpy
from mathutils import Vector

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import PROPS, Frame, Part, beam, export, fresh_collection, make_object  # noqa: E402

RUNS = [
    [(0.5, 6.0), (8.0, 5.5), (15.0, 7.0), (18.5, 11.0), (19.0, 14.0)],
    [(19.0, 17.5), (18.0, 22.0), (14.0, 26.0), (7.0, 27.5), (0.5, 27.0)],
]
PANEL = 2.2
POST, POST_HEIGHT = 0.13, 1.25
RAILS = [(0.45, 0.07), (0.95, 0.07)]  # высота и толщина жердей
SLATS = 3  # досок на пролёт, прибитых к жердям
FOLDER = os.path.join(PROPS, "fence")
COLORS = {"post": (0.45, 0.33, 0.22), "rail": (0.60, 0.47, 0.31), "slat": (0.68, 0.58, 0.42)}

PLACES = {}  # имя пролёта → середина в клетках сцены


def blender(point):
    return Vector((point[0], -point[1], 0.0))


def wobble(seed):
    """Небольшой постоянный наклон столбика: забор хилый, столбики стоят вкривь."""
    return Vector((0.06 * math.sin(seed * 2.7), 0.06 * math.cos(seed * 1.9), 0.0))


def span(a, b, seed, start_post, end_post):
    """Пролёт от a до b (точки Blender на земле): столбики, две жерди, редкие доски."""
    part = Part()
    along = (b - a).normalized()
    across = Vector((-along.y, along.x, 0.0))
    posts = [a] if start_post else []
    posts += [b] if end_post else []
    for k, foot in enumerate(posts):
        beam(part, "post", tuple(foot), tuple(foot + wobble(seed + k) + Vector((0, 0, POST_HEIGHT))), POST)
    for z, thick in RAILS:
        sag = 0.04 * math.sin(seed)
        beam(part, "rail", tuple(a + across * 0.08 + Vector((0, 0, z))), tuple(b + across * 0.08 + Vector((0, 0, z + sag))),
             thick)
    for k in range(SLATS):
        t = (k + 1) / (SLATS + 1) + 0.05 * math.sin(seed * 3 + k)
        foot = a.lerp(b, t) + across * 0.13
        lean = along * 0.08 * math.sin(seed + 5 * k)
        beam(part, "slat", tuple(foot + Vector((0, 0, 0.05))), tuple(foot + lean + Vector((0, 0, 1.05 + 0.1 * math.sin(k + seed)))),
             0.07)
    return part


def edges():
    """Участки ломаных: номер участка, начало, конец, есть ли столбик в начале участка и в конце."""
    number = 0
    for points in RUNS:
        for k, (a, b) in enumerate(zip(points, points[1:])):
            number += 1
            yield number, a, b, k == len(points) - 2


def build():
    PLACES.clear()
    made = {}
    index = 0
    for number, a, b, last in edges():
        collection = fresh_collection(f"fence_e{number}")
        middle = blender(((a[0] + b[0]) / 2, (a[1] + b[1]) / 2))
        start, end = blender(a) - middle, blender(b) - middle
        count = max(1, round((end - start).length / PANEL))
        for k in range(count):
            index += 1
            name = f"fence_{index:02d}"
            p, q = start.lerp(end, k / count), start.lerp(end, (k + 1) / count)
            obj = make_object(name, span(p, q, index, True, last and k == count - 1), collection, COLORS, 0.0, "fence")
            xs = [v.co.x + middle.x for v in obj.data.vertices]
            ys = [v.co.y + middle.y for v in obj.data.vertices]
            # Движок ставит модель серединой её коробки по плану в середину места объекта.
            PLACES[name] = (round((min(xs) + max(xs)) / 2, 3), round(-(min(ys) + max(ys)) / 2, 3))
            made[name] = len(obj.data.polygons)
    return made


def spans_of(number):
    return [obj.name for obj in bpy.data.collections[f"fence_e{number}"].objects]


def frame(number):
    folder = os.path.join(FOLDER, f"e{number}")
    os.makedirs(folder, exist_ok=True)
    return Frame.around(spans_of(number), folder)


def render_guide():
    """Снимки участков на прозрачном фоне: e<номер>/guide.png — основа раскраски и силуэт картины."""
    for number, *_ in edges():
        frame(number).render([("guide.png", set(spans_of(number)))])


def cropped(of, obj, rgba):
    """Кусок картины участка под пролёт: UV пролёта переводятся в его кусок, чтобы модель не несла весь участок."""
    layer = obj.data.uv_layers.active.data
    uvs = numpy.array([loop.uv[:] for loop in layer])
    low = numpy.clip(numpy.floor(uvs.min(axis=0) * of.px) - 2, 0, of.px - 1).astype(int)
    high = numpy.clip(numpy.ceil(uvs.max(axis=0) * of.px) + 2, 1, of.px).astype(int)
    piece = rgba[low[1]:high[1], low[0]:high[0]].copy()
    size = high - low
    for loop in layer:
        loop.uv = ((loop.uv[0] * of.px - low[0]) / size[0], (loop.uv[1] * of.px - low[1]) / size[1])
    image = bpy.data.images.new("picture_" + obj.name, int(size[0]), int(size[1]), alpha=True)
    image.pixels = piece.ravel()
    image.pack()
    texture = next(n for n in obj.data.materials[0].node_tree.nodes if n.bl_idname == "ShaderNodeTexImage")
    texture.image = image


def export_painted():
    """fence_<NN>.glb: пролёты с кусками картин участков."""
    for number, *_ in edges():
        of = frame(number)
        of.fit("paint_raw.png", "guide.png", "paint.png")
        whole = of.picture(f"fence_e{number}", of.load("paint.png"), of.load("guide.png"), painted_lib.EDGE)
        rgba = numpy.array(whole.pixels[:], dtype=numpy.float32).reshape(of.px, of.px, 4)
        for name in spans_of(number):
            obj = of.projected_copy(bpy.data.objects[name], name + "_game", whole)
            cropped(of, obj, rgba)
            export(obj, [], name + ".glb")
            bpy.data.objects.remove(obj, do_unlink=True)


result = build()
