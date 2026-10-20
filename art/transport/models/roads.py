# Куски грунтовых дорог Транспортной компании («Фаза-43-вид-дороги»): прямой кусок каждого вида в двух вариантах
# и перекрёстки («Фаза-46-перекрёстки-готовыми-моделями»). Кусок — плоский прямоугольник вдоль −Y Blender (после
# выгрузки — вдоль +Z glTF, как ждёт движок): ширина по X, длина по Y, середина в нуле. Перекрёсток — плоская
# мелкая сетка по контуру из shape.json, середина в нуле, рукава вдоль ±X и ±Y. Карты — из
# art/transport/textures/road_*, их рисует tools/art/roads.mjs: цвет с прозрачностью (край уходит в землю —
# alphaMode BLEND), рельеф, шероховатость.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/roads.py [-- <папка>]
import json
import math
import os
import sys

import bmesh
import bpy
from mathutils import Vector
from mathutils.geometry import delaunay_2d_cdt

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "..", "..", "..", "games", "transport", "models"))
TEXTURES = os.path.normpath(os.path.join(HERE, "..", "textures"))

# Ширина и длина куска в метрах — как в tools/art/roads.mjs.
KINDS = {"main": (12.0, 16.0), "street": (7.0, 14.0), "drive": (4.5, 9.0)}
VARIANTS = ("a", "b")
JUNCTION_STEP = 1.0  # метров между точками сетки перекрёстка — по контуру и внутри


def material(name, folder):
    """Материал из карт папки: цвет и прозрачность из color.png, рельеф, шероховатость и металл из arm.jpg."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nodes, links = m.node_tree.nodes, m.node_tree.links
    bsdf = nodes["Principled BSDF"]

    def image(file, data):
        node = nodes.new("ShaderNodeTexImage")
        node.image = bpy.data.images.load(os.path.join(TEXTURES, folder, file), check_existing=True)
        if data:
            node.image.colorspace_settings.name = "Non-Color"
        return node

    color = image("color.png", False)
    links.new(color.outputs["Color"], bsdf.inputs["Base Color"])
    links.new(color.outputs["Alpha"], bsdf.inputs["Alpha"])
    arm = image("arm.jpg", True)
    channels = nodes.new("ShaderNodeSeparateColor")
    links.new(arm.outputs["Color"], channels.inputs["Color"])
    links.new(channels.outputs["Green"], bsdf.inputs["Roughness"])
    links.new(channels.outputs["Blue"], bsdf.inputs["Metallic"])
    relief = image("normal.jpg", True)
    normal_map = nodes.new("ShaderNodeNormalMap")
    links.new(relief.outputs["Color"], normal_map.inputs["Color"])
    links.new(normal_map.outputs["Normal"], bsdf.inputs["Normal"])
    m.surface_render_method = "BLENDED"
    return m


def flat(name, width, length, folder):
    """Плоский прямоугольник `width` × `length` с развёрткой во всю картинку: u — поперёк, v — вдоль."""
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    uv = bm.loops.layers.uv.verify()
    w, l = width / 2, length / 2
    # Картинка сверху вниз — от начала куска к концу; начало — со стороны +Y Blender (−Z glTF).
    corners = [((-w, l, 0), (0, 1)), ((w, l, 0), (1, 1)), ((w, -l, 0), (1, 0)), ((-w, -l, 0), (0, 0))]
    face = bm.faces.new([bm.verts.new(p) for p, _ in corners])
    for loop, (_, t) in zip(face.loops, corners):
        loop[uv].uv = t
    face.normal_update()
    if face.normal.z < 0:
        face.normal_flip()
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(material(name, folder))
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def inside(p, ring):
    """Точка внутри многоугольника (чётность пересечений)."""
    x, y = p
    result = False
    for (xi, yi), (xj, yj) in zip(ring, ring[-1:] + ring[:-1]):
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            result = not result
    return result


def ring_distance(p, ring):
    """Расстояние от точки до контура."""
    best = math.inf
    for (ax, ay), (bx, by) in zip(ring, ring[1:] + ring[:1]):
        dx, dy = bx - ax, by - ay
        t = max(0.0, min(1.0, ((p[0] - ax) * dx + (p[1] - ay) * dy) / (dx * dx + dy * dy or 1e-12)))
        best = min(best, math.hypot(p[0] - ax - dx * t, p[1] - ay - dy * t))
    return best


def junctions():
    """Имена перекрёстков, чьи картинки и shape.json нарисовал tools/art/roads.mjs."""
    return sorted(d for d in os.listdir(TEXTURES) if os.path.isfile(os.path.join(TEXTURES, d, "shape.json")))


def junction(name):
    """Перекрёсток по shape.json рядом с его картинками: плоская сетка по контуру, рукава вдоль осей; развёртка —
    рамка картинки, верх картинки — со стороны +Y."""
    with open(os.path.join(TEXTURES, name, "shape.json"), encoding="utf-8") as f:
        shape = json.load(f)
    x0, y0, x1, y1 = shape["bounds"]
    ring = []
    for x, y in shape["outline"]:
        if not ring or math.hypot(x - ring[-1][0], y - ring[-1][1]) > 1e-6:
            ring.append((x, y))
    if math.hypot(ring[0][0] - ring[-1][0], ring[0][1] - ring[-1][1]) <= 1e-6:
        ring.pop()
    if sum(ax * by - bx * ay for (ax, ay), (bx, by) in zip(ring, ring[1:] + ring[:1])) < 0:
        ring.reverse()  # триангуляции нужен обход против часовой
    # Точки мелких дуг гуще шага сетки дают острые треугольники: из них остаются те, что не ближе полушага к
    # предыдущей, и все изломы контура (углы устьев).
    thinned = []
    for i, p in enumerate(ring):
        a, b = ring[i - 1], ring[(i + 1) % len(ring)]
        turn = abs(math.atan2((p[0] - a[0]) * (b[1] - p[1]) - (p[1] - a[1]) * (b[0] - p[0]),
                              (p[0] - a[0]) * (b[0] - p[0]) + (p[1] - a[1]) * (b[1] - p[1])))
        if not thinned or turn > math.radians(30) or math.dist(p, thinned[-1]) >= 0.5 * JUNCTION_STEP:
            thinned.append(p)
    ring = thinned
    # Движок изгибает рукава под настоящий угол улиц: сетка ровная и мелкая — контур поделён на отрезки не длиннее
    # шага, внутри — треугольная решётка с тем же шагом, всё вместе — триангуляция Делоне без тонких треугольников.
    points = []
    for i, (x, y) in enumerate(ring):
        nx, ny = ring[(i + 1) % len(ring)]
        n = max(1, math.ceil(math.hypot(nx - x, ny - y) / JUNCTION_STEP))
        points += [(x + (nx - x) * k / n, y + (ny - y) * k / n) for k in range(n)]
    boundary = len(points)
    row = JUNCTION_STEP * math.sqrt(3) / 2
    for j in range(int((y1 - y0) / row) + 2):
        for i in range(int((x1 - x0) / JUNCTION_STEP) + 2):
            p = (x0 + (i + 0.5 * (j % 2)) * JUNCTION_STEP, y0 + j * row)
            if inside(p, ring) and ring_distance(p, ring) > 0.5 * JUNCTION_STEP:
                points.append(p)
    coords, _, faces, *_ = delaunay_2d_cdt([Vector(p) for p in points], [], [list(range(boundary))], 1, 1e-6)
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    uv = bm.loops.layers.uv.verify()
    verts = {}
    for f in faces:
        vs = [verts.setdefault(i, bm.verts.new((coords[i].x, coords[i].y, 0))) for i in f]
        face = bm.faces.new(vs)
        face.normal_update()
        if face.normal.z < 0:
            face.normal_flip()
        for loop in face.loops:
            x, y, _ = loop.vert.co
            loop[uv].uv = ((x - x0) / (x1 - x0), (y - y0) / (y1 - y0))
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(material(name, name))
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def build(out=OUT):
    os.makedirs(out, exist_ok=True)
    parts = [(f"road_{kind}_{v}", w, l, f"road_{kind}_{v}") for kind, (w, l) in KINDS.items() for v in VARIANTS]
    for name, w, l, folder in parts:
        lib.reset()
        flat(name, w, l, folder)
        lib.export(os.path.join(out, name + ".glb"))
        print(name, os.path.getsize(os.path.join(out, name + ".glb")))
    for name in junctions():
        lib.reset()
        junction(name)
        lib.export(os.path.join(out, name + ".glb"))
        print(name, os.path.getsize(os.path.join(out, name + ".glb")))


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    build(args[0] if args else OUT)


if __name__ == "__main__":
    main()
