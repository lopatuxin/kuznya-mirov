"""Прямоугольный забор стартовой сцены. В живом Blender вызвать build(), prepare_painting() и export_painted()."""

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
from painted_lib import (  # noqa: E402
    PROPS,
    ROOT,
    Frame,
    Part,
    beam,
    bilinear,
    crop_projected_texture,
    export,
    fresh_collection,
    load_any,
    loft,
    make_object,
    plan_box,
    world_points,
)

FOLDER = os.path.join(PROPS, "fence")
COLLECTION = "start_fence"
PAINT = "start_fence_painted.png"
MASK = "start_fence_mask.png"
DETAIL_PAINT = "start_fence_detail.png"
DETAIL_PIXELS = 2304
ORIGIN = (9.75, 16.5)
PATH = ((19.0, 14.0), (19.0, 5.5), (0.5, 5.5), (0.5, 27.5), (19.0, 27.5), (19.0, 17.5))
PANEL = 4.2
COLORS = {
    "post": (0.34, 0.28, 0.22),
    "rail": (0.43, 0.37, 0.31),
    "repair": (0.51, 0.41, 0.31),
}
SPLIT_TOP = {3, 6, 9, 12, 15, 17}
REPAIRS = {1, 5, 8, 13, 16}
BRACES = {3, 8, 13, 15}


def panel_points() -> list[Vector]:
    corners = [Vector((x - ORIGIN[0], ORIGIN[1] - y, 0.0)) for x, y in PATH]
    points = [corners[0]]
    for start, end in zip(corners, corners[1:]):
        count = max(1, round((end - start).length / PANEL))
        points.extend(start.lerp(end, step / count) for step in range(1, count + 1))
    return points


def post_lean(number: int) -> Vector:
    return Vector((0.15 * math.sin(2.7 * number), 0.13 * math.cos(1.9 * number), 0.0))


def add_post(part: Part, foot: Vector, number: int) -> None:
    lean = post_lean(number)
    height = 1.16 + 0.12 * math.sin(number * 2.5)
    half = 0.14 + 0.022 * math.cos(number * 1.3)
    corners = ((-1, -1), (1, -1), (1, 1), (-1, 1))
    rings = []
    for fraction, taper in ((0.0, 1.0), (0.43, 0.98), (1.0, 0.83)):
        center = foot + lean * fraction
        rings.append([
            tuple(center + Vector((
                x * half * taper,
                y * half * taper,
                height * fraction + (0.038 * math.sin(number + index * 2.2) if fraction == 1.0 else 0.0),
            )))
            for index, (x, y) in enumerate(corners)
        ])
    loft(part, "post", rings)


def add_board(
    part: Part, kind: str, centers: tuple[Vector, ...], thickness: float, height: float, splinter: float = 0.0
) -> None:
    axis = (centers[-1] - centers[0]).normalized()
    side = axis.cross(Vector((0, 0, 1))).normalized()
    across = side * thickness / 2
    up = side.cross(axis).normalized() * height / 2
    corners = (-across - up, -across + up, across + up, across - up)
    rings = [[tuple(center + corner) for corner in corners] for center in centers]
    rings[-1] = [
        tuple(Vector(point) + axis * splinter * amount)
        for point, amount in zip(rings[-1], (-0.32, 0.80, 0.19, -0.76))
    ]
    loft(part, kind, rings)


def span(start: Vector, end: Vector, number: int, final_post: bool) -> Part:
    part = Part()
    add_post(part, start, number)
    if final_post:
        add_post(part, end, number + 1)
    along = (end - start).normalized()
    face = along.cross(Vector((0, 0, 1)))
    if face.y > 0.0:
        face = -face
    for height, board_height in ((0.34, 0.15), (0.88, 0.17)):
        low = height < 0.5
        if not low and number == 10:
            continue
        first = start + post_lean(number) * height + Vector((0, 0, height + 0.065 * math.sin(number)))
        last = end + post_lean(number + 1) * height + Vector((0, 0, height + 0.065 * math.sin(number + 1)))
        middle = first.lerp(last, 0.52) - Vector((0, 0, 0.047 + 0.031 * math.cos(number)))
        if not low and number in SPLIT_TOP:
            left = first.lerp(last, 0.43) - Vector((0, 0, 0.085))
            right = first.lerp(last, 0.60) - Vector((0, 0, 0.045))
            add_board(part, "rail", (first, left), 0.095, board_height, 0.23)
            add_board(part, "rail", (last, right), 0.095, board_height, 0.19)
        else:
            add_board(part, "rail", (first, middle, last), 0.095, board_height)
        if not low and number in REPAIRS:
            patch = middle + face * 0.058
            add_board(
                part, "repair", (patch - along * 0.52, patch + along * 0.51 + Vector((0, 0, 0.09))),
                0.045, 0.15, 0.07,
            )
    if number in BRACES:
        bottom = start + along * 0.74 + face * 0.12 + Vector((0, 0, 0.082))
        top = start + post_lean(number) * 0.62 + face * 0.08 + Vector((0, 0, 0.68))
        beam(part, "repair", bottom, top, 0.14)
    return part


def scene_center(names: list[str]) -> tuple[float, float]:
    (x0, y0), (x1, y1) = plan_box(world_points(names))
    return round(ORIGIN[0] + (x0 + x1) / 2, 5), round(ORIGIN[1] - (y0 + y1) / 2, 5)


def build() -> dict[str, tuple[float, float]]:
    collection = fresh_collection(COLLECTION)
    points = panel_points()
    places = {}
    for number, (start, end) in enumerate(zip(points, points[1:]), 1):
        name = f"start_fence_{number:02d}"
        part = span(start, end, number, number == len(points) - 1)
        make_object(name, part, collection, COLORS, prefix=COLLECTION)
        places[name] = scene_center([name])
    places["start_fence_soil"] = scene_center(list(places))
    return places


def fence_frame(folder: str, pixels: int = 1024) -> Frame:
    return Frame(21.4, 0.60, pixels, folder)


def prepare_painting(folder: str) -> None:
    frame = fence_frame(folder)
    names = {obj.name for obj in bpy.data.collections[COLLECTION].objects}
    frame.render([(MASK, names)])
    solid = frame.load(MASK)
    terrain = load_any(os.path.join(ROOT, "art", "horde", "painted", "start_ground.png"))
    height, width = terrain.shape[:2]
    x, y = frame.ground_points()
    scene_x, scene_y = numpy.abs(ORIGIN[0] + x), numpy.abs(ORIGIN[1] - y)
    scene_x = numpy.where(scene_x > 18.0, 36.0 - scene_x, scene_x)
    scene_y = numpy.where(scene_y > 36.0, 72.0 - scene_y, scene_y)
    col = numpy.clip(scene_x / 18.0 * width - 0.5, 0, width - 1)
    row = numpy.clip((1.0 - scene_y / 36.0) * height - 0.5, 0, height - 1)
    ground = bilinear(terrain, col, row)
    cover = solid[:, :, 3:4]
    pixels = ground * (1 - cover) + solid * cover
    pixels[:, :, 3] = 1.0
    frame.save(pixels, "start_fence_guide.png")


def export_spans() -> None:
    frame = fence_frame(FOLDER, DETAIL_PIXELS)
    detail = frame.load(DETAIL_PAINT)
    image = frame.picture(COLLECTION, detail, detail)
    rgba = numpy.array(image.pixels[:], dtype=numpy.float32).reshape(frame.px, frame.px, 4)
    for source in bpy.data.collections[COLLECTION].objects:
        model = frame.projected_copy(source, source.name + "_game", image)
        crop_projected_texture(frame, model, rgba)
        export(model, [], source.name + ".glb")
        bpy.data.objects.remove(model, do_unlink=True)


def export_painted(folder: str) -> None:
    export_spans()
    frame = fence_frame(folder)
    names = [obj.name for obj in bpy.data.collections[COLLECTION].objects]
    mask = frame.load(MASK)
    paint = fence_frame(FOLDER).load(PAINT)
    painted_lib.export_soil(frame, paint, names, "start_fence_soil", object_mask=mask)
