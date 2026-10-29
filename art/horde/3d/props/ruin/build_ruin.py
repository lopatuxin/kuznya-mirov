"""Полуразрушенный кирпичный дом в пустоши Орды: стены разной высоты без крыши, пустые окна, пролом со стороны лагеря,
обломки внутри по углам и снаружи у юго-восточной стены.

Три модели: «ruin» — дом целиком, твёрдый; «ruin_cut» — он же без двух передних стен (от них остаются низкие обрубки),
его показывают, пока герой внутри; «ruin_floor» — плоский пол комнаты, не твёрдый: пол дома — земля. Дом собран в
своих осях (стены вдоль осей), потом повёрнут на 45°, как вышка: к камере смотрят стена с проломом (слева, -X) и
стена с окном (справа, -Y).

Запускать в живом Blender (MCP), в своей сцене: exec(open(путь).read()). build() строит объёмы, render_guide()
снимает их камерой игры (guide_outside.png, guide_cut.png), export_painted() подгоняет раскраски нейросети
paint_outside_raw.png и paint_cut_raw.png в кадр, кладёт их на те же объёмы и выгружает три модели в
games/horde/models. Общее — art/horde/3d/painted_lib.py.
"""
import importlib
import math
import os
import sys

import bmesh
import bpy
import numpy
from mathutils import Matrix, Vector

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import EDGE, PROPS, Frame, Part, box, box_faces, export, fresh_collection, loft, make_object, \
    merge, merge_cut, ring, turned  # noqa: E402

TURN = math.radians(45)
SIDE, THICK = 4.95, 0.3
HALF = SIDE / 2
SLAB = 0.25  # ширина кусков стены по верху: сколотый край ступеньками, как у битого кирпича
STUB = 0.4  # высота обрубков передних стен в разрезе
BREACH = (-0.9, 1.1)  # пролом в стене -X, от и до вдоль неё
# Стена: ось, смещение, высота у начала, у конца, размах сколов, проёмы окон (середина, ширина, низ, верх).
WALLS = {
    "back_x": ("y", HALF, 3.3, 2.2, 0.6, [(-0.9, 0.9, 1.0, 1.8), (1.0, 0.9, 1.0, 1.8)]),
    "back_y": ("x", HALF, 2.8, 3.4, 0.6, [(0.0, 1.0, 1.0, 1.8)]),
    "front_x": ("y", -HALF, 1.6, 2.3, 0.7, []),
    "front_y": ("x", -HALF, 2.4, 1.0, 0.8, [(0.3, 0.9, 1.0, 1.8)]),
}
FRONT = ("front_x", "front_y")
CUT_MARGIN = 4  # запас вокруг отличий разреза, в точках картины
FOLDER = os.path.join(PROPS, "ruin")
COLORS = {"brick": (0.66, 0.42, 0.33), "plaster": (0.80, 0.74, 0.62), "rubble": (0.60, 0.50, 0.42),
          "wood": (0.40, 0.30, 0.20), "floor": (0.55, 0.50, 0.42)}


def height_at(name, t):
    """Высота стены в доле t её длины: от начала к концу, со сколами."""
    _, _, start, end, chip, _ = WALLS[name]
    seed = sum(map(ord, name))
    return max(0.3, start + (end - start) * t - chip * abs(math.sin(seed + 9.7 * t) * math.sin(3.3 * seed + 23.0 * t)))


def wall(part, name, cut):
    axis, offset, *_, holes = WALLS[name]
    count = round(SIDE / SLAB)
    for k in range(count):
        a = -HALF + SIDE * k / count
        b = -HALF + SIDE * (k + 1) / count
        middle = (a + b) / 2
        if name == "front_x" and BREACH[0] < middle < BREACH[1]:
            continue
        top = STUB if cut and name in FRONT else height_at(name, (middle + HALF) / SIDE)
        pieces = [(0.0, top)]
        for centre, width, low, high in holes:
            if abs(middle - centre) < width / 2 and not (cut and name in FRONT):
                pieces = [(0.0, min(top, low))] + ([(high, top)] if top > high else [])
        for z0, z1 in pieces:
            if z1 <= z0:
                continue
            size = (b - a, THICK, z1 - z0) if axis == "x" else (THICK, b - a, z1 - z0)
            center = (middle, offset, (z0 + z1) / 2) if axis == "x" else (offset, middle, (z0 + z1) / 2)
            box(part, "brick", center, size)


def rubble(part, center, radius, height, seed):
    jitter = [1.0 + 0.2 * math.sin(seed * 1.3 + 2.1 * k) for k in range(8)]
    loft(part, "rubble", [ring(center, radius * scale, 8, height * share, seed, jitter)
                          for share, scale in ((0.0, 1.0), (0.5, 0.75), (1.0, 0.25))])


def build_part(cut):
    part = Part()
    for name in WALLS:
        wall(part, name, cut)
    inner = HALF - THICK
    rubble(part, (inner - 0.5, inner - 0.5), 0.7, 0.6, 1)  # дальний угол
    rubble(part, (inner - 0.4, -inner + 0.9), 0.5, 0.45, 2)
    merge(part, fallen_beam())
    for k, (x, y) in enumerate(((0.6, -HALF - 0.6), (1.5, -HALF - 0.5), (-0.4, -HALF - 0.7))):
        rubble(part, (x, y), 0.45 - 0.08 * k, 0.35, 5 + k)  # обломки снаружи у стены -Y
    return part


def fallen_beam():
    """Упавшая балка перекрытия: одним концом на обломках в дальнем углу, другим на полу."""
    beam = Part()
    box(beam, "wood", (0.0, 0.0, 0.0), (2.2, 0.18, 0.18))
    tilted = turned(beam, math.radians(-16), "Y")
    placed = Part()
    for kind, faces in turned(tilted, math.radians(35)).faces.items():
        placed.add(kind, [[(x + 0.9, y + 0.8, z + 0.45) for x, y, z in face] for face in faces])
    return placed


def floor_object(collection):
    """Пол комнаты: плоскость на земле внутри стен, не твёрдая."""
    inner = HALF - THICK / 2
    part = Part()
    part.add("floor", [[(-inner, -inner, 0.0), (inner, -inner, 0.0), (inner, inner, 0.0), (-inner, inner, 0.0)]])
    return make_object("ruin_floor", part, collection, COLORS, TURN, "ruin")


def inside_box(collection):
    """Коробка комнаты: узел `inside` для движка, не рисуется; лежит по осям дома, поворот — поворот самого узла."""
    inner = HALF - THICK
    mesh = bpy.data.meshes.new("inside_room")
    bm = bmesh.new()
    for points in box_faces((-inner, -inner, 0.0), (inner, inner, 2.5)):
        bm.faces.new([bm.verts.new(p) for p in points])
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new("inside_room", mesh)
    obj.rotation_euler = (0.0, 0.0, TURN)
    obj.display_type = "WIRE"
    obj.hide_render = True
    collection.objects.link(obj)


def build():
    whole = fresh_collection("ruin")
    cut = fresh_collection("ruin_cut")
    make_object("ruin", build_part(False), whole, COLORS, TURN, "ruin")
    inside_box(whole)
    floor_object(whole)
    make_object("ruin_cut", build_part(True), cut, COLORS, TURN, "ruin")
    return {name: len(bpy.data.objects[name].data.polygons) for name in ("ruin", "ruin_cut", "ruin_floor")}


def frame():
    """Кадр на дом целиком с обломками — общий для снимков снаружи и в разрезе."""
    return Frame.around(["ruin"], FOLDER)


def render_guide():
    """Снимки снаружи и в разрезе, оба с полом комнаты: guide_outside.png, guide_cut.png."""
    os.makedirs(FOLDER, exist_ok=True)
    frame().render([("guide_outside.png", {"ruin", "ruin_floor"}), ("guide_cut.png", {"ruin_cut", "ruin_floor"})])


def front_walls_mask(of):
    """Точки картины, которые снаружи заслоняют передние стены целиком, по их полной высоте: за ними разрез берёт
    свою раскраску."""
    turn = Matrix.Rotation(TURN, 3, "Z")
    top = max(max(WALLS[name][2:4]) for name in FRONT) + 0.2
    walls = [[(-HALF, -HALF, 0), (HALF, -HALF, 0), (HALF, -HALF, top), (-HALF, -HALF, top)],
             [(-HALF, -HALF, 0), (-HALF, HALF, 0), (-HALF, HALF, top), (-HALF, -HALF, top)]]
    return of.quads_mask([[turn @ Vector(p) for p in wall] for wall in walls])


def export_painted():
    """ruin.glb (с узлом inside_room), ruin_cut.glb и ruin_floor.glb. Разрез и пол берут из своей раскраски только то,
    чем разрез отличается от дома снаружи, остальное — из раскраски снаружи: при смене вида рисунок не прыгает."""
    of = frame()
    of.fit("paint_outside_raw.png", "guide_outside.png", "paint_outside.png")
    of.fit("paint_cut_raw.png", "guide_cut.png", "paint_cut.png")
    outside, cut = of.load("paint_outside.png"), of.load("paint_cut.png")
    mask_outside, mask_cut = of.load("guide_outside.png"), of.load("guide_cut.png")
    merged = merge_cut(outside, cut, mask_outside, mask_cut, front_walls_mask(of), CUT_MARGIN)
    made = []
    for name, rgba, mask, extra in (("ruin", outside, mask_outside, ["inside_room"]),
                                    ("ruin_cut", merged, mask_cut, []),
                                    ("ruin_floor", merged, None, [])):
        if mask is None:
            mask = numpy.ones_like(rgba)
        picture = of.picture(name, rgba, mask, EDGE if name != "ruin_floor" else 0)
        obj = of.projected_copy(bpy.data.objects[name], name + "_game", picture)
        made.append(obj)
        export(obj, extra, name + ".glb")
    for obj in made:
        bpy.data.objects.remove(obj, do_unlink=True)


result = build()
