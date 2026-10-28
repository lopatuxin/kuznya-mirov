"""Объём сторожевой вышки Орды под рост героя: из него снимают картину и на него же её проецируют.

Запускать в живом Blender (MCP): exec(open(путь).read()). Строит в текущей пустой сцене две коллекции:
«tower» — вышка целиком и «tower_cut» — она же в разрезе: без передних стен будки и без крыши, от стен
остаются низкие обрубки. Единица — клетка игры (герой 1,73 клетки). Вышка собрана в своих осях
(стены будки вдоль осей), потом повёрнута на 45° вокруг вертикали: к камере игры смотрят стена с дверью
(справа) и стена с окном (слева), как на прежней картине.

Цвет частей — плоские материалы по роду (дерево, жесть, камень, ткань): снимок с ними — подмалёвок для
раскраски нейросетью (render_guides). Раскрашенные картины ложатся обратно на тот же объём проекцией с той же
камеры, а прозрачность берут из силуэта самого снимка (export_painted): рисунок и объём совпадают по построению.

Камера игры ортографическая, наклон 30°: точка вышки видна на снимке по её проекциям на ось «вправо» и ось
«вверх» камеры; кадр GUIDE_SIZE клеток со стороной GUIDE_PX точек, его середина на высоте GUIDE_UP по оси «вверх».
"""
import os

import numpy

import math

import bmesh
import bpy
from mathutils import Matrix, Vector

TURN = math.radians(45)

DECK = 7.0  # сторона настила
DECK_TOP, DECK_BEAM = 4.5, 0.35
POST_AT, POST = 2.95, 0.36  # середины стоек от середины и сечение
FOOTING_LOW, FOOTING_HIGH, FOOTING = 1.0, 0.66, 0.8  # низ, верх тумбы и её высота
HUT, WALL, WALL_THICK = 3.2, 2.5, 0.15  # будка: сторона, высота стен, толщина
RIDGE_RISE, EAVE = 1.15, 0.35
DOOR_WIDTH, DOOR_HEIGHT = 1.1, 2.15
WINDOW_WIDTH, WINDOW_LOW, WINDOW_HIGH = 1.0, 1.0, 1.9
RAIL_HEIGHT, RAIL_POST = 1.0, 0.12
LADDER_WIDTH, LADDER_RUN = 0.8, 1.7  # ширина и вынос низа лестницы от края настила
STUB = 0.3  # высота обрубков передних стен в разрезе
STOVE = (1.1, 1.0)  # печка и её труба сквозь крышу

PITCH = math.radians(30)
GUIDE_SIZE, GUIDE_UP, GUIDE_PX = 12.0, 3.25, 1024
CUT_MARGIN = 4  # запас вокруг отличий разреза, в точках картины
RIGHT = Vector((1.0, 0.0, 0.0))
UP = Vector((0.0, math.sin(PITCH), math.cos(PITCH)))
FORWARD = Vector((0.0, math.cos(PITCH), -math.sin(PITCH)))
ROOT = r"C:\projects\kuznya-mirov"
PAINT = os.path.join(ROOT, "art", "horde", "3d", "tower", "paint")
MODELS = os.path.join(ROOT, "games", "horde", "models")
# Где вышка стоит в Орде (середина в клетках сцены) и плиты нарисованной земли под ней: картина и левый
# край плиты; картина плиты — 32 клетки вширь и 36 вглубь (art/horde/3d/painted/build_ground.py).
TOWER_AT = (9.0, 16.0)
GROUND_PLATES = [(os.path.join(ROOT, "art", "horde", "painted", "ground_01.png"), 0.0),
                 (os.path.join(ROOT, "art", "horde", "painted", "ground_02.png"), 21.2823)]
GROUND_PICTURE = (32.0, 36.0)
# Заплатка земли под вышкой — плоскость на земле, полуширина по X и полуглубина по Y в клетках; дорисованная
# земля видна целиком ближе SOIL_FADE[0] клетки от опор и низа лестницы и растворяется к SOIL_FADE[1].
SOIL_HALF = (6.0, 5.5)
SOIL_FADE = (0.4, 1.8)

COLORS = {
    "wood": (0.55, 0.42, 0.26),
    "wood_dark": (0.36, 0.27, 0.17),
    "plank": (0.70, 0.60, 0.40),
    "wall": (0.20, 0.55, 0.60),
    "roof": (0.80, 0.33, 0.18),
    "stone": (0.78, 0.76, 0.70),
    "metal": (0.30, 0.30, 0.32),
    "cloth": (0.45, 0.48, 0.35),
    "glass": (0.12, 0.16, 0.20),
}


def material(kind):
    name = "tower_" + kind
    found = bpy.data.materials.get(name)
    if found:
        return found
    made = bpy.data.materials.new(name)
    made.diffuse_color = (*COLORS[kind], 1.0)
    return made


class Part:
    """Набор граней одного рода в осях вышки."""

    def __init__(self):
        self.faces = {}

    def add(self, kind, faces):
        self.faces.setdefault(kind, []).extend(faces)


def box_faces(low, high):
    (x0, y0, z0), (x1, y1, z1) = low, high
    v = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
         (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    return [[v[0], v[3], v[2], v[1]], [v[4], v[5], v[6], v[7]], [v[0], v[1], v[5], v[4]],
            [v[1], v[2], v[6], v[5]], [v[2], v[3], v[7], v[6]], [v[3], v[0], v[4], v[7]]]


def box(part, kind, center, size):
    c, s = Vector(center), Vector(size) / 2
    part.add(kind, box_faces(tuple(c - s), tuple(c + s)))


def beam(part, kind, a, b, thick):
    """Брус квадратного сечения от точки a до точки b."""
    a, b = Vector(a), Vector(b)
    axis = (b - a).normalized()
    helper = Vector((0, 0, 1)) if abs(axis.z) < 0.9 else Vector((1, 0, 0))
    side = axis.cross(helper).normalized() * thick / 2
    up = side.cross(axis).normalized() * thick / 2
    ring = [-side - up, -side + up, side + up, side - up]  # обход, при котором грани смотрят наружу
    low = [tuple(a + r) for r in ring]
    high = [tuple(b + r) for r in ring]
    faces = [list(reversed(low)), high]
    for i in range(4):
        j = (i + 1) % 4
        faces.append([low[i], low[j], high[j], high[i]])
    part.add(kind, faces)


def frustum(part, kind, center, low_half, high_half, height):
    x, y, z = center
    low = [(x - low_half, y - low_half, z), (x + low_half, y - low_half, z),
           (x + low_half, y + low_half, z), (x - low_half, y + low_half, z)]
    high = [(x - high_half, y - high_half, z + height), (x + high_half, y - high_half, z + height),
            (x + high_half, y + high_half, z + height), (x - high_half, y + high_half, z + height)]
    faces = [list(reversed(low)), high]
    for i in range(4):
        j = (i + 1) % 4
        faces.append([low[i], low[j], high[j], high[i]])
    part.add(kind, faces)


def substructure(part):
    """Тумбы, стойки, связи крест-накрест между ними и настил с обвязкой."""
    corners = [(sx * POST_AT, sy * POST_AT) for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
    for x, y in corners:
        frustum(part, "stone", (x, y, 0.0), FOOTING_LOW / 2, FOOTING_HIGH / 2, FOOTING)
        box(part, "wood", (x, y, (FOOTING + DECK_TOP - DECK_BEAM) / 2),
            (POST, POST, DECK_TOP - DECK_BEAM - FOOTING))
    low_z, high_z = FOOTING + 0.25, DECK_TOP - DECK_BEAM - 0.15
    for i in range(4):
        (ax, ay), (bx, by) = corners[i], corners[(i + 1) % 4]
        beam(part, "wood_dark", (ax, ay, low_z), (bx, by, high_z), 0.2)
        beam(part, "wood_dark", (bx, by, low_z), (ax, ay, high_z), 0.2)
        beam(part, "wood_dark", (ax, ay, FOOTING + 0.4), (bx, by, FOOTING + 0.4), 0.18)
    half = DECK / 2
    box(part, "wood_dark", (0, 0, DECK_TOP - DECK_BEAM / 2 - 0.05), (DECK, DECK, DECK_BEAM - 0.1))
    box(part, "plank", (0, 0, DECK_TOP - 0.05), (DECK - 0.02, DECK - 0.02, 0.1))
    for sx in (-1, 1):
        box(part, "wood", (sx * (half - 0.1), 0, DECK_TOP - DECK_BEAM / 2), (0.2, DECK, DECK_BEAM))
        box(part, "wood", (0, sx * (half - 0.1), DECK_TOP - DECK_BEAM / 2), (DECK, 0.2, DECK_BEAM))


def railing(part):
    """Перила по краю настила; на передней правой стороне (-Y) — проход к лестнице."""
    half = DECK / 2 - 0.08
    z = DECK_TOP
    gap = (0.6, 0.6 + LADDER_WIDTH + 0.2)  # проход по оси X на стороне -Y
    sides = [
        ((-half, -half), (gap[0], -half)),
        ((gap[1], -half), (half, -half)),
        ((half, -half), (half, half)),
        ((half, half), (-half, half)),
        ((-half, half), (-half, -half)),
    ]
    for (ax, ay), (bx, by) in sides:
        length = math.hypot(bx - ax, by - ay)
        posts = max(1, round(length / 1.5))
        for k in range(posts + 1):
            t = k / posts
            x, y = ax + (bx - ax) * t, ay + (by - ay) * t
            box(part, "wood", (x, y, z + RAIL_HEIGHT / 2), (RAIL_POST, RAIL_POST, RAIL_HEIGHT))
        for height in (0.5, RAIL_HEIGHT - 0.04):
            beam(part, "wood", (ax, ay, z + height), (bx, by, z + height), 0.09)


def ladder(part):
    """Лестница от земли к проходу в перилах на стороне -Y."""
    x0 = 0.6 + 0.1 + LADDER_WIDTH / 2
    edge = -DECK / 2
    top = (x0, edge - 0.05, DECK_TOP)
    foot = (x0, edge - LADDER_RUN, 0.06)  # тетивы толщиной 0,1 не уходят под землю
    for side in (-1, 1):
        dx = side * LADDER_WIDTH / 2
        beam(part, "wood", (foot[0] + dx, foot[1], foot[2]), (top[0] + dx, top[1], top[2] + 0.2), 0.1)
    rungs = 12
    for k in range(1, rungs):
        t = k / rungs
        y = foot[1] + (top[1] - foot[1]) * t
        z = foot[2] + (top[2] - foot[2]) * t
        beam(part, "wood", (x0 - LADDER_WIDTH / 2, y, z), (x0 + LADDER_WIDTH / 2, y, z), 0.07)


def wall_with_hole(part, axis, offset, along, hole, height, low_cut=None):
    """Стена вдоль оси `axis` ('x' или 'y') на смещении `offset`, от -along до along, с проёмом.

    hole — (середина, ширина, низ, верх) проёма или None; low_cut — высота обрубка вместо всей стены.
    """
    top = DECK_TOP + (low_cut if low_cut is not None else height)
    pieces = []
    if hole is None:
        pieces.append((-along, along, DECK_TOP, top))
    else:
        middle, width, low, high = hole
        left, right = middle - width / 2, middle + width / 2
        pieces.append((-along, left, DECK_TOP, top))
        pieces.append((right, along, DECK_TOP, top))
        if low > 0:
            pieces.append((left, right, DECK_TOP, min(top, DECK_TOP + low)))
        if DECK_TOP + high < top:
            pieces.append((left, right, DECK_TOP + high, top))
    for a, b, z0, z1 in pieces:
        if z1 <= z0 or b <= a:
            continue
        mid, size = (a + b) / 2, b - a
        if axis == "x":
            box(part, "wall", (mid, offset, (z0 + z1) / 2), (size, WALL_THICK, z1 - z0))
        else:
            box(part, "wall", (offset, mid, (z0 + z1) / 2), (WALL_THICK, size, z1 - z0))


def hut(part, cut):
    """Будка: передние стены -Y (дверь) и -X (окно), задние +Y и +X; в разрезе передние — обрубки."""
    half = HUT / 2
    along = half + WALL_THICK / 2
    door = (0.3, DOOR_WIDTH, 0.0, DOOR_HEIGHT)
    window = (0.0, WINDOW_WIDTH, WINDOW_LOW, WINDOW_HIGH)
    wall_with_hole(part, "x", -half, along, door, WALL, STUB if cut else None)
    wall_with_hole(part, "y", -half, along, window, WALL, STUB if cut else None)
    wall_with_hole(part, "x", half, along, (0.0, 0.9, 1.1, 1.8), WALL)
    wall_with_hole(part, "y", half, along, None, WALL)
    if cut:
        return
    roof(part)


def roof(part):
    """Двускатная крыша, конёк вдоль оси Y, фронтоны на сторонах -Y и +Y; труба печки."""
    half = HUT / 2 + WALL_THICK / 2
    eave_z = DECK_TOP + WALL
    ridge_z = eave_z + RIDGE_RISE
    y0, y1 = -half - EAVE, half + EAVE
    for side in (-1, 1):
        x_eave = side * (half + EAVE)
        slope = [(x_eave, y0, eave_z - 0.1), (0.0, y0, ridge_z), (0.0, y1, ridge_z), (x_eave, y1, eave_z - 0.1)]
        under = [(x, y, z - 0.12) for x, y, z in slope]
        faces = [slope if side < 0 else list(reversed(slope)), list(reversed(under)) if side < 0 else under]
        for i in range(4):
            j = (i + 1) % 4
            faces.append([under[i], under[j], slope[j], slope[i]])
        part.add("roof", faces)
    for y in (-half, half):
        gable = [(-half, y, eave_z), (half, y, eave_z), (0.0, y, ridge_z)]
        part.add("wall", [gable, list(reversed(gable))])
    box(part, "metal", (STOVE[0], STOVE[1], ridge_z + 0.1), (0.28, 0.28, 2.2))


def furniture(part):
    """Обстановка комнаты: нары у задней стены, стол со стулом, печка, ящики, рация."""
    half = HUT / 2
    z = DECK_TOP
    box(part, "wood", (-0.35, half - 0.5, z + 0.25), (2.0, 0.85, 0.5))
    box(part, "cloth", (-0.35, half - 0.5, z + 0.56), (1.9, 0.8, 0.12))
    box(part, "wood", (-0.35, half - 0.5, z + 1.35), (2.0, 0.85, 0.12))
    box(part, "cloth", (-0.35, half - 0.5, z + 1.47), (1.9, 0.8, 0.12))
    for x in (-1.3, 0.6):
        box(part, "wood", (x, half - 0.12, z + 0.8), (0.1, 0.1, 1.6))
    box(part, "wood", (half - 0.5, -0.4, z + 0.78), (0.8, 1.2, 0.07))
    for dx in (-0.32, 0.32):
        for dy in (-0.5, 0.5):
            box(part, "wood", (half - 0.5 + dx, -0.4 + dy, z + 0.38), (0.06, 0.06, 0.76))
    box(part, "metal", (half - 0.45, -0.6, z + 0.95), (0.4, 0.3, 0.3))
    box(part, "wood", (half - 0.5, 0.45, z + 0.25), (0.45, 0.45, 0.5))
    box(part, "metal", (STOVE[0], STOVE[1], z + 0.45), (0.6, 0.6, 0.9))
    box(part, "metal", (STOVE[0], STOVE[1], z + 1.7), (0.28, 0.28, 1.6))
    box(part, "wood_dark", (-half + 0.45, -half + 0.45, z + 0.3), (0.6, 0.6, 0.6))
    box(part, "wood_dark", (-half + 0.45, -half + 0.45, z + 0.8), (0.45, 0.45, 0.4))


def deck_props(part):
    """Бочка в дальнем углу балкона: обход будки кругом остаётся шире героя."""
    half = DECK / 2
    box(part, "metal", (half - 0.4, half - 0.4, DECK_TOP + 0.45), (0.55, 0.55, 0.9))


def build_part(cut):
    part = Part()
    substructure(part)
    railing(part)
    ladder(part)
    hut(part, cut)
    furniture(part)
    deck_props(part)
    return part


def make_object(name, part, collection):
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    turn = Matrix.Rotation(TURN, 3, "Z")
    kinds = list(part.faces)
    for index, kind in enumerate(kinds):
        for points in part.faces[kind]:
            face = bm.faces.new([bm.verts.new(turn @ Vector(p)) for p in points])
            face.material_index = index
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bm.to_mesh(mesh)
    bm.free()
    for kind in kinds:
        mesh.materials.append(material(kind))
    obj = bpy.data.objects.new(name, mesh)
    collection.objects.link(obj)
    return obj


def inside_box(collection):
    """Коробка внутренности будки: узел `inside` для движка, не рисуется. Коробка лежит по осям будки, а поворот
    вышки — поворот самого узла: движок берёт коробку в осях узла, иначе её углы вышли бы на балкон."""
    half = HUT / 2 - WALL_THICK / 2
    mesh = bpy.data.meshes.new("inside_hut")
    bm = bmesh.new()
    for points in box_faces((-half, -half, DECK_TOP), (half, half, DECK_TOP + WALL)):
        bm.faces.new([bm.verts.new(p) for p in points])
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new("inside_hut", mesh)
    obj.rotation_euler = (0.0, 0.0, TURN)
    obj.display_type = "WIRE"
    obj.hide_render = True
    collection.objects.link(obj)
    return obj


def build():
    scene = bpy.context.scene
    for name in ("tower", "tower_cut"):
        old = bpy.data.collections.get(name)
        if old:
            for obj in list(old.objects):
                bpy.data.objects.remove(obj, do_unlink=True)
            bpy.data.collections.remove(old)
    whole = bpy.data.collections.new("tower")
    cut = bpy.data.collections.new("tower_cut")
    scene.collection.children.link(whole)
    scene.collection.children.link(cut)
    make_object("tower", build_part(False), whole)
    inside_box(whole)
    make_object("tower_cut", build_part(True), cut)
    return {"tower": len(bpy.data.objects["tower"].data.polygons),
            "tower_cut": len(bpy.data.objects["tower_cut"].data.polygons)}


def guide_camera():
    """Камера игры над вышкой: кадр GUIDE_SIZE клеток, середина кадра на высоте GUIDE_UP."""
    data = bpy.data.cameras.get("guide_cam") or bpy.data.cameras.new("guide_cam")
    data.type, data.sensor_fit, data.ortho_scale = "ORTHO", "VERTICAL", GUIDE_SIZE
    cam = bpy.data.objects.get("guide_cam") or bpy.data.objects.new("guide_cam", data)
    if cam.name not in bpy.context.scene.collection.objects:
        bpy.context.scene.collection.objects.link(cam)
    cam.location = UP * GUIDE_UP - FORWARD * 60
    cam.rotation_euler = (math.pi / 2 - PITCH, 0.0, 0.0)
    return cam


def render_guides():
    """Подмалёвки снаружи и в разрезе на прозрачном фоне: guide_outside.png, guide_cut.png в PAINT."""
    scene = bpy.context.scene
    scene.camera = guide_camera()
    scene.render.engine = "BLENDER_WORKBENCH"
    scene.render.resolution_x = scene.render.resolution_y = GUIDE_PX
    scene.render.film_transparent = True
    shading = scene.display.shading
    shading.light, shading.color_type = "STUDIO", "MATERIAL"
    shading.show_cavity = shading.show_object_outline = True
    shown = {obj: obj.hide_render for obj in scene.objects}
    for obj in scene.objects:
        obj.hide_render = obj.name not in ("tower", "tower_cut")
    for name, other in (("outside", "tower_cut"), ("cut", "tower")):
        bpy.data.objects[other].hide_render = True
        scene.render.filepath = os.path.join(PAINT, f"mask_{name}.png")
        bpy.ops.render.render(write_still=True)
        bpy.data.objects[other].hide_render = False
    for obj, hidden in shown.items():
        obj.hide_render = hidden


def guide_uv(point):
    """Точка картины, куда видна точка вышки: доли кадра по ширине и снизу вверх."""
    return (point.dot(RIGHT) / GUIDE_SIZE + 0.5, (point.dot(UP) - GUIDE_UP) / GUIDE_SIZE + 0.5)


def frame_ground():
    """Точки кадра камеры игры на земле (z = 0): для каждой точки картины — клетки сцены Орды."""
    rows, cols = numpy.mgrid[0:GUIDE_PX, 0:GUIDE_PX]
    across = ((cols + 0.5) / GUIDE_PX - 0.5) * GUIDE_SIZE
    deep = (((rows + 0.5) / GUIDE_PX - 0.5) * GUIDE_SIZE + GUIDE_UP) / math.sin(PITCH)
    return TOWER_AT[0] + across, TOWER_AT[1] - deep


def ground_under():
    """Земля карты Орды так, как камера игры видит её за вышкой: ground_under.png в PAINT."""
    scene_x, scene_y = frame_ground()
    out = numpy.zeros((GUIDE_PX, GUIDE_PX, 4), dtype=numpy.float32)
    for path, left in GROUND_PLATES:
        image = bpy.data.images.load(path)
        width, height = image.size
        pixels = numpy.array(image.pixels[:], dtype=numpy.float32).reshape(height, width, 4)
        bpy.data.images.remove(image)
        here = scene_x >= left
        col = numpy.clip(((scene_x - left) / GROUND_PICTURE[0] * width).astype(int), 0, width - 1)
        # За дальним краем сцены земли нет: кадр берёт её зеркально, без полос.
        row = numpy.clip(((1 - numpy.abs(scene_y) / GROUND_PICTURE[1]) * height).astype(int), 0, height - 1)
        out[here] = pixels[row[here], col[here]]
    out[:, :, 3] = 1.0
    save_pixels(out, "ground_under.png")
    tower = load_pixels("paint_outside_before_soil.png")
    mask = load_pixels("mask_outside.png")[:, :, 3] > 0.5
    out[mask] = tower[mask]
    save_pixels(out, "tower_on_ground.png")


def soil_image():
    """Земля под вышкой: раскраска снаружи вместе с дорисованной присыпкой у тумб, тенью и тропой, видимая
    целиком у опор и у низа лестницы и растворяющаяся в земле карты дальше от них."""
    pixels = load_pixels("paint_outside.png")
    scene_x, scene_y = frame_ground()
    turn = -TURN
    dx, dy = scene_x - TOWER_AT[0], -(scene_y - TOWER_AT[1])
    local_x = dx * math.cos(turn) - dy * math.sin(turn)
    local_y = dx * math.sin(turn) + dy * math.cos(turn)
    reach = POST_AT + FOOTING_LOW / 2
    outside_x = numpy.maximum(numpy.abs(local_x) - reach, 0.0)
    outside_y = numpy.maximum(numpy.abs(local_y) - reach, 0.0)
    distance = numpy.hypot(outside_x, outside_y)
    ladder = (0.6 + 0.1 + LADDER_WIDTH / 2, -DECK / 2 - LADDER_RUN)
    distance = numpy.minimum(distance, numpy.maximum(numpy.hypot(local_x - ladder[0], local_y - ladder[1]) - 0.6, 0.0))
    fade = numpy.clip((SOIL_FADE[1] - distance) / (SOIL_FADE[1] - SOIL_FADE[0]), 0.0, 1.0)
    pixels[:, :, 3] = fade * fade * (3 - 2 * fade)
    image = bpy.data.images.new("picture_soil", GUIDE_PX, GUIDE_PX, alpha=True)
    image.pixels = pixels.ravel()
    image.pack()
    return image


def export_soil():
    """Заплатка земли soil.glb: плоскость на земле под кадром вышки, картина смешивается с землёй карты."""
    mesh = bpy.data.meshes.new("tower_soil")
    bm = bmesh.new()
    half_x, half_y = SOIL_HALF
    corners = [(-half_x, -half_y, 0.0), (half_x, -half_y, 0.0), (half_x, half_y, 0.0), (-half_x, half_y, 0.0)]
    bm.faces.new([bm.verts.new(c) for c in corners])
    bm.to_mesh(mesh)
    bm.free()
    layer = mesh.uv_layers.new(name="UVMap")
    for loop in mesh.loops:
        layer.data[loop.index].uv = guide_uv(mesh.vertices[loop.vertex_index].co)
    made = bpy.data.materials.new("tower_soil")
    made.use_nodes = True
    nodes, links = made.node_tree.nodes, made.node_tree.links
    bsdf = next(n for n in nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
    texture = nodes.new("ShaderNodeTexImage")
    texture.image = soil_image()
    texture.extension = "EXTEND"
    links.new(texture.outputs["Color"], bsdf.inputs["Base Color"])
    links.new(texture.outputs["Alpha"], bsdf.inputs["Alpha"])
    bsdf.inputs["Roughness"].default_value = 1.0
    made.surface_render_method = "BLENDED"
    mesh.materials.append(made)
    obj = bpy.data.objects.new("tower_soil", mesh)
    bpy.context.scene.collection.objects.link(obj)
    for other in bpy.context.scene.objects:
        other.select_set(other is obj)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.export_scene.gltf(filepath=os.path.join(MODELS, "soil.glb"), use_selection=True, export_format="GLB",
                              export_yup=True, export_apply=True, export_animations=False, use_active_scene=True)
    bpy.data.objects.remove(obj, do_unlink=True)


def save_pixels(pixels, file):
    image = bpy.data.images.new(file, GUIDE_PX, GUIDE_PX, alpha=True)
    image.pixels = pixels.ravel()
    image.filepath_raw = os.path.join(PAINT, file)
    image.file_format = "PNG"
    image.save()
    bpy.data.images.remove(image)


def load_pixels(file):
    image = bpy.data.images.load(os.path.join(PAINT, file))
    assert tuple(image.size) == (GUIDE_PX, GUIDE_PX), f"{file}: не {GUIDE_PX} точек"
    pixels = numpy.array(image.pixels[:], dtype=numpy.float32).reshape(GUIDE_PX, GUIDE_PX, 4)
    bpy.data.images.remove(image)
    return pixels


def grow(mask, steps):
    """Маска, раздутая на `steps` точек во все четыре стороны."""
    for _ in range(steps):
        wider = mask.copy()
        wider[1:, :] |= mask[:-1, :]
        wider[:-1, :] |= mask[1:, :]
        wider[:, 1:] |= mask[:, :-1]
        wider[:, :-1] |= mask[:, 1:]
        mask = wider
    return mask


def behind_front_walls():
    """Точки картины, которые снаружи заслоняют передние стены будки целиком, с окном и дверью: сквозь их
    проёмы снаружи видна та же комната, что в разрезе, но нарисована она там как окно и тёмный проём."""
    half = HUT / 2 + WALL_THICK / 2
    low, high = DECK_TOP, DECK_TOP + WALL
    turn = Matrix.Rotation(TURN, 3, "Z")
    walls = [[(-half, -half, low), (half, -half, low), (half, -half, high), (-half, -half, high)],
             [(-half, -half, low), (-half, half, low), (-half, half, high), (-half, -half, high)]]
    rows, cols = numpy.mgrid[0:GUIDE_PX, 0:GUIDE_PX]
    points = numpy.stack([cols + 0.5, rows + 0.5], axis=-1)
    inside = numpy.zeros((GUIDE_PX, GUIDE_PX), dtype=bool)
    for wall in walls:
        corners = [numpy.array(guide_uv(turn @ Vector(p))) * GUIDE_PX for p in wall]
        sides = []
        for i in range(4):
            a, b = corners[i], corners[(i + 1) % 4]
            edge, to_point = b - a, points - a
            sides.append(edge[0] * to_point[..., 1] - edge[1] * to_point[..., 0])
        sides = numpy.stack(sides)
        inside |= (sides >= 0).all(axis=0) | (sides <= 0).all(axis=0)
    return inside


def picture_image(name):
    """Картина для игры: цвет — раскраска paint_<name>.png, прозрачность — силуэт объёма mask_<name>.png,
    раздутый на точку, чтобы край объёма не брал фон раскраски.

    Разрез берёт из своей раскраски только то, чем он отличается от вышки снаружи (комната и срезанные стены
    по снимкам объёма, с запасом CUT_MARGIN точек), остальное — из раскраски снаружи: при смене вида опоры,
    настил и перила не меняют рисунок."""
    rgba = load_pixels(f"paint_{name}.png")
    mask = load_pixels(f"mask_{name}.png")
    if name == "cut":
        outside = load_pixels("paint_outside.png")
        changed = numpy.abs(mask - load_pixels("mask_outside.png")).max(axis=2) > 0.02
        own = grow(changed | behind_front_walls(), CUT_MARGIN)
        rgba[~own] = outside[~own]
    rgba[:, :, 3] = grow(mask[:, :, 3] > 0.5, 1)
    image = bpy.data.images.new("picture_" + name, GUIDE_PX, GUIDE_PX, alpha=True)
    image.pixels = rgba.ravel()
    image.pack()
    return image


def picture_material(name, image):
    made = bpy.data.materials.new(name)
    made.use_nodes = True
    nodes, links = made.node_tree.nodes, made.node_tree.links
    bsdf = next(n for n in nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
    texture = nodes.new("ShaderNodeTexImage")
    texture.image = image
    texture.extension = "EXTEND"
    clip = nodes.new("ShaderNodeMath")
    clip.operation = "ROUND"
    links.new(texture.outputs["Color"], bsdf.inputs["Base Color"])
    links.new(texture.outputs["Alpha"], clip.inputs[0])
    links.new(clip.outputs[0], bsdf.inputs["Alpha"])
    bsdf.inputs["Roughness"].default_value = 1.0
    made.surface_render_method = "DITHERED"
    return made


def projected_copy(source, name, picture):
    """Копия объёма с UV проекцией с камеры игры и картиной вместо плоских цветов."""
    mesh = source.data.copy()
    mesh.name = name
    mesh.materials.clear()
    mesh.materials.append(picture_material(name, picture))
    for poly in mesh.polygons:
        poly.material_index = 0
    layer = mesh.uv_layers.new(name="UVMap")
    for loop in mesh.loops:
        layer.data[loop.index].uv = guide_uv(source.matrix_world @ mesh.vertices[loop.vertex_index].co)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def export_painted():
    """Модели игры: tower.glb (с узлом inside_hut) и tower_cut.glb с картинами из раскрасок и силуэтов."""
    made = []
    for name, picture, extra in (("tower", "outside", ["inside_hut"]), ("tower_cut", "cut", [])):
        obj = projected_copy(bpy.data.objects[name], name + "_game", picture_image(picture))
        made.append(obj)
        for other in bpy.context.scene.objects:
            other.select_set(other is obj or other.name in extra)
        bpy.context.view_layer.objects.active = obj
        bpy.ops.export_scene.gltf(filepath=os.path.join(MODELS, name + ".glb"), use_selection=True,
                                  export_format="GLB", export_yup=True, export_apply=True,
                                  export_animations=False, export_materials="EXPORT", use_active_scene=True)
    for obj in made:
        bpy.data.objects.remove(obj, do_unlink=True)


result = build()
