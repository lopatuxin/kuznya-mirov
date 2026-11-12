"""Полуразрушенный кирпичный дом в пустоши Орды: стены разной высоты без крыши, пустые окна, пролом со стороны лагеря,
осыпи битого кирпича внутри по углам и снаружи у юго-восточной стены.

Три модели: «ruin» — дом целиком, твёрдый; «ruin_cut» — он же без двух передних стен (от них остаются низкие обрубки),
его показывают, пока герой внутри; «ruin_floor» — плоский пол комнаты, не твёрдый: пол дома — земля. Дом собран в
своих осях (стены вдоль осей), потом повёрнут на 45°, как вышка: к камере смотрят стена с проломом (слева, -X) и
стена с окном (справа, -Y).

Запускать в живом Blender (MCP), в своей сцене: exec(open(путь).read()). build() строит объёмы, render_guide()
снимает их камерой игры (guide_outside.png, guide_cut.png), prepare_planting() подгоняет раскраску нейросети
paint_outside_raw.png в кадр и кладёт её на землю карты для посадки, export_planted() кладёт посаженную раскраску и
раскраску разреза paint_cut_raw.png на те же объёмы и выгружает три модели и заплатку земли в games/horde/models.
Общее — art/horde/3d/painted_lib.py.
"""
import importlib
import math
import os
import random
import sys

import bmesh
import bpy
import numpy
from mathutils import Matrix, Vector

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import PROPS, Frame, Part, box, box_faces, export, fresh_collection, loft, make_object, \
    merge, merge_cut, ring, turned  # noqa: E402

TURN = math.radians(45)
SIDE, THICK = 4.95, 0.3
HALF = SIDE / 2
SLAB = 0.25  # ширина кусков стены по верху
COURSE = 0.13  # высота ступеньки скола — два ряда кирпича: верх стены осыпается рядами, а не зубцами
STUB = 0.4  # высота обрубков передних стен в разрезе
BREACH = (-0.9, 1.1)  # пролом в стене -X, от и до вдоль неё
BREACH_SLOPE = 0.9  # на таком отрезке от пролома стена ступеньками опускается к нему
# Высота углов: углы держатся дольше стен, и две стены в углу сходятся одной высотой.
CORNERS = {(-1, -1): 1.9, (1, -1): 2.6, (-1, 1): 2.7, (1, 1): 3.4}
# Стена: ось, вдоль которой она идёт, сторона, размах осыпи посередине, проёмы окон (середина, ширина, низ, верх).
WALLS = {
    "back_x": ("y", 1, 0.7, [(-0.9, 0.9, 1.0, 1.8), (1.0, 0.9, 1.0, 1.8)]),
    "back_y": ("x", 1, 0.5, [(0.0, 1.0, 1.0, 1.8)]),
    "front_x": ("y", -1, 0.4, []),
    "front_y": ("x", -1, 0.9, [(0.3, 0.9, 1.0, 1.8)]),
}
FRONT = ("front_x", "front_y")
CUT_MARGIN = 4  # запас вокруг отличий разреза, в точках картины
FOLDER = os.path.join(PROPS, "ruin")
RUIN_AT = (47.0, 11.0)  # середина дома в клетках сцены
COLORS = {"brick": (0.66, 0.42, 0.33), "plaster": (0.80, 0.74, 0.62), "rubble": (0.60, 0.50, 0.42),
          "wood": (0.40, 0.30, 0.20), "floor": (0.55, 0.50, 0.42)}


def height_at(name, along):
    """Высота стены в точке `along` вдоль неё: от высоты одного угла к высоте другого, посередине осыпь широкими
    провалами, у пролома ступеньками вниз; всё кратно ступеньке скола, у самих углов осыпи нет."""
    axis, side, chip, _ = WALLS[name]
    start = CORNERS[(-1, side) if axis == "x" else (side, -1)]
    end = CORNERS[(1, side) if axis == "x" else (side, 1)]
    t = min(1.0, max(0.0, (along + HALF) / SIDE))
    seed = sum(map(ord, name))
    dip = math.sin(math.pi * t) ** 1.5 * (0.65 + 0.35 * math.sin(seed + 2 * math.pi * 1.4 * t))
    height = start + (end - start) * t - chip * dip
    # Неровность осыпи: ряды выпадают кусками разной длины, ступени не выходят лестницей.
    height += 1.6 * COURSE * math.sin(math.pi * t) * math.sin(seed * 0.7 + 2 * math.pi * 4.3 * t) * math.sin(seed + 2 * math.pi * 1.7 * t)
    if name == "front_x":
        gap = max(BREACH[0] - along, along - BREACH[1], 0.0)
        height *= min(1.0, 0.4 + 0.6 * gap / BREACH_SLOPE)
    return max(COURSE * 2, round(height / COURSE) * COURSE)


def wall(part, name, cut):
    """Стена кусками по SLAB. Стены вдоль x идут до внешних углов, стены вдоль y — между ними: углы сложены
    без выемки и без двойных граней."""
    axis, side, _, holes = WALLS[name]
    offset = side * HALF
    reach = HALF + THICK / 2 if axis == "x" else HALF - THICK / 2
    count = round(2 * reach / SLAB)
    for k in range(count):
        a = -reach + 2 * reach * k / count
        b = -reach + 2 * reach * (k + 1) / count
        middle = (a + b) / 2
        if name == "front_x" and BREACH[0] < middle < BREACH[1]:
            continue
        top = STUB if cut and name in FRONT else height_at(name, middle)
        pieces = [(0.0, top)]
        for centre, width, low, high in holes:
            if abs(middle - centre) < width / 2 and not (cut and name in FRONT):
                pieces = [(0.0, min(top, low))] + ([(high, top)] if top > high + COURSE else [])
        for z0, z1 in pieces:
            if z1 <= z0:
                continue
            size = (b - a, THICK, z1 - z0) if axis == "x" else (THICK, b - a, z1 - z0)
            center = (middle, offset, (z0 + z1) / 2) if axis == "x" else (offset, middle, (z0 + z1) / 2)
            box(part, "brick", center, size)


BRICK = (0.25, 0.12, 0.07)


def brick_pile(part, center, radius, height, seed, bricks):
    """Осыпь: низкий бугор битого кирпича и пыли, по нему и вокруг — россыпь целых и битых кирпичей, у большой
    осыпи ещё кусок стены в несколько рядов, упавший набок."""
    rng = random.Random(seed)
    jitter = [rng.uniform(0.72, 1.2) for _ in range(10)]
    loft(part, "rubble", [ring(center, radius * scale, 10, height * share, seed, jitter)
                          for share, scale in ((0.0, 1.0), (0.3, 0.88), (0.7, 0.55), (1.0, 0.25))])
    for _ in range(bricks):
        distance = radius * math.sqrt(rng.uniform(0.0, 1.0)) * 1.15
        angle = rng.uniform(0.0, math.tau)
        x, y = center[0] + distance * math.cos(angle), center[1] + distance * math.sin(angle)
        on = max(0.0, height * (1.0 - (distance / radius) ** 1.6))  # высота бугра под кирпичом
        length = BRICK[0] * rng.choice((1.0, 1.0, 0.6, 0.5))  # половинки и четвертушки — битые
        brick = Part()
        box(brick, "brick", (0.0, 0.0, BRICK[2] / 2), (length, BRICK[1], BRICK[2]))
        brick = turned(turned(brick, rng.uniform(-0.35, 0.35), "X"), rng.uniform(0.0, math.pi))
        for kind, faces in brick.faces.items():
            part.add(kind, [[(px + x, py + y, pz + on - 0.02) for px, py, pz in face] for face in faces])
    if radius >= 0.6:
        chunk = Part()
        box(chunk, "brick", (0.0, 0.0, 0.13), (0.62, THICK, 0.26))
        chunk = turned(turned(chunk, math.radians(14), "X"), rng.uniform(0.0, math.pi))
        dx, dy = rng.uniform(-0.3, 0.3) * radius, rng.uniform(-0.3, 0.3) * radius
        for kind, faces in chunk.faces.items():
            part.add(kind, [[(px + center[0] + dx, py + center[1] + dy, pz + height * 0.3) for px, py, pz in face]
                            for face in faces])


def build_part(cut):
    part = Part()
    for name in WALLS:
        wall(part, name, cut)
    inner = HALF - THICK
    brick_pile(part, (inner - 0.55, inner - 0.55), 0.75, 0.45, 1, 40)  # дальний угол, на нём балка
    brick_pile(part, (inner - 0.4, -inner + 0.8), 0.5, 0.22, 2, 22)
    merge(part, fallen_beam())
    for k, (x, y) in enumerate(((0.6, -HALF - 0.65), (1.6, -HALF - 0.55), (-0.5, -HALF - 0.6))):
        brick_pile(part, (x, y), (0.62, 0.48, 0.4)[k], (0.2, 0.16, 0.12)[k], 5 + k, (30, 22, 14)[k])  # снаружи у -Y
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
    # Движок ставит модель на землю по нижнему обломку, который лежит ниже нуля.
    for points in box_faces((-inner, -inner, -0.05), (inner, inner, 2.5)):
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
    top = max(CORNERS.values()) + 0.2
    walls = [[(-HALF, -HALF, 0), (HALF, -HALF, 0), (HALF, -HALF, top), (-HALF, -HALF, top)],
             [(-HALF, -HALF, 0), (-HALF, HALF, 0), (-HALF, HALF, top), (-HALF, -HALF, top)]]
    return of.quads_mask([[turn @ Vector(p) for p in wall] for wall in walls])


def prepare_planting():
    """Подкладка для посадки дома в землю: раскраска снаружи paint_outside_raw.png, подогнанная в кадр снимка, поверх
    земли карты с его места — plant_in.png."""
    of = frame()
    of.fit("paint_outside_raw.png", "guide_outside.png", "paint_outside.png")
    origin = painted_lib.scene_origin(["ruin"], RUIN_AT)
    painted_lib.prepare_planting(["ruin", "ruin_floor"], FOLDER, of.load("paint_outside.png"), of, origin)


def export_planted():
    """ruin.glb (с узлом inside_room), ruin_cut.glb, ruin_floor.glb и заплатка земли ruin_soil.glb из посаженной
    раскраски plant_raw.png. Разрез и пол берут из раскраски разреза paint_cut_raw.png только то, чем разрез отличается
    от дома снаружи, остальное — из посаженной раскраски: при смене вида рисунок не прыгает."""
    of, outside, mask_outside, _ = painted_lib.finish_planting(["ruin", "ruin_floor"], FOLDER)
    source = frame()
    source.fit("paint_cut_raw.png", "guide_cut.png", "paint_cut.png")
    cut = of.resample(source.load("paint_cut.png"), source)
    of.render([("plant_guide_cut.png", {"ruin_cut", "ruin_floor"})])
    mask_cut = of.load("plant_guide_cut.png")
    merged = merge_cut(outside, cut, mask_outside, mask_cut, front_walls_mask(of), CUT_MARGIN)
    made = []
    for name, rgba, mask, extra in (("ruin", outside, mask_outside, ["inside_room"]),
                                    ("ruin_cut", merged, mask_cut, []),
                                    ("ruin_floor", merged, None, [])):
        if mask is None:
            mask = numpy.ones_like(rgba)
        picture = of.picture(name, rgba, mask)
        obj = of.projected_copy(bpy.data.objects[name], name + "_game", picture)
        made.append(obj)
        export(obj, extra, name + ".glb")
    for obj in made:
        bpy.data.objects.remove(obj, do_unlink=True)
    painted_lib.export_soil(of, outside, ["ruin"], "ruin_soil")


result = build()
