# Деревья средней полосы для Транспортной компании: берёза, липа, яблоня, рябина, сосна, ель — каждое свой .glb в
# games/transport/models. Ствол и сучья — сеткой с корой, листва — карточками, как у деревьев SpeedTree: плоскость с
# картинкой веточки и прозрачностью вырезом (alphaMode MASK). Картинку веточки скрипт снимает сам: собирает две
# веточки на тонких стеблях — из листьев библиотеки, из сложных листьев рябины, из иголок хвои, с ягодами и яблоками —
# и снимает их сверху Cycles цветом, нормалями и шероховатостью в атлас art/transport/textures/<дерево>_twig: левая
# половина атласа одна веточка, правая другая. Крона освещается одним объёмом: нормали карточек смотрят от середины
# кроны, а глубже в кроне и ниже карточки и сучья темнее — затенение в цвете вершин `occlusion`.
# Листья — ambientCG (CC0): берёза LeafSet004, липа LeafSet023, яблоня LeafSet022, листочки рябины LeafSet001. Кора —
# ambientCG Bark002, Bark012, Bark014, Bark015; берёзовая — бесшовные плитки из фотографии W.carter «Part of birch
# trunk in Norrkila» (Wikimedia Commons, CC0): белая с чечевичками — art/transport/textures/birch_bark, тёмный
# растрескавшийся низ ствола — birch_bark_base. Иголки, ягоды и яблоки — сетка своего цвета.
# Основание — в нуле, ствол уходит на `SINK` ниже: земля карты неровная. Единица — метр, размер настоящий.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/trees.py [-- <папка> [дерево ...]]
import math
import os
import random
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402
from gazel import smoothstep  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
TEXTURES = os.path.normpath(os.path.join(HERE, "..", "textures"))
OUT = os.path.normpath(os.path.join(HERE, "..", "..", "..", "games", "transport", "models"))

SINK = 0.3
ATLAS = 1024  # точек на сторону атласа веточек
CUTOFF = 0.4  # порог выреза листвы
UP = Vector((0.0, 0.0, 1.0))
DOWN = Vector((0.0, 0.0, -1.0))

# Листья набора: где на картинке черешок — внизу рамки листа ("bottom") или слева ("left"), — и рамка каждого листа
# (x0, y0, x1, y1 в долях картинки, y сверху). У LeafSet004 первые три листа светлее.
LEAVES = {
    "LeafSet004": ("bottom", ((0.1094, 0.042, 0.3408, 0.4854), (0.374, 0.0439, 0.6211, 0.4941), (0.6455, 0.0615, 0.9072, 0.4678),
                              (0.125, 0.5303, 0.3545, 0.958), (0.4141, 0.541, 0.627, 0.9561), (0.6738, 0.5488, 0.9023, 0.9531))),
    "LeafSet001": ("bottom", ((0.0645, 0.0361, 0.2383, 0.4551), (0.6904, 0.0947, 0.8525, 0.458), (0.3857, 0.1387, 0.5195, 0.3691),
                              (0.4014, 0.5537, 0.5752, 0.9482), (0.0732, 0.5605, 0.2344, 0.9229), (0.7305, 0.5674, 0.8818, 0.9355))),
    "LeafSet022": ("bottom", ((0.1035, 0.0332, 0.2842, 0.4551), (0.4072, 0.0381, 0.5898, 0.4639), (0.7441, 0.0547, 0.9063, 0.4531),
                              (0.7158, 0.5244, 0.9023, 0.9512), (0.0684, 0.5342, 0.2529, 0.9639), (0.4131, 0.5381, 0.5879, 0.9629))),
    "LeafSet023": ("left", ((0.5244, 0.0205, 0.9932, 0.3223), (0.0234, 0.0361, 0.4707, 0.2891), (0.0361, 0.3633, 0.4736, 0.6426),
                            (0.5156, 0.374, 0.9922, 0.6377), (0.0146, 0.6895, 0.4873, 0.9736), (0.5547, 0.7021, 0.9746, 0.9629))),
}


def along(points, t):
    """Точка ломаной на доле длины `t` и направление ломаной там."""
    lengths = [(b - a).length for a, b in zip(points, points[1:])]
    goal = t * sum(lengths)
    for a, b, length in zip(points, points[1:], lengths):
        if goal <= length or b is points[-1]:
            k = min(1.0, goal / length) if length else 0.0
            return a.lerp(b, k), (b - a).normalized()
        goal -= length
    return points[-1].copy(), (points[-1] - points[-2]).normalized()


def grow(start, direction, length, rnd, steps, lift, jitter):
    """Ветка ломаной из `steps` отрезков: на каждом метре направление поворачивает к небу на `lift(t)` (меньше нуля —
    вниз, ветка никнет) и чуть в сторону на `jitter`."""
    d = Vector(direction).normalized()
    p = Vector(start)
    points = [p.copy()]
    step = length / steps
    for i in range(steps):
        t = (i + 0.5) / steps
        wobble = Vector((rnd.uniform(-1, 1), rnd.uniform(-1, 1), rnd.uniform(-1, 1))) * jitter
        d = (d + (UP * lift(t) + wobble) * step).normalized()
        p = p + d * step
        points.append(p.copy())
    return points


def turn(axis, angle_from, spin, side=None):
    """Направление под углом `angle_from` к оси `axis`, повёрнутое вокруг неё на `spin` от `side` (по умолчанию —
    от горизонтали сбоку от оси)."""
    axis = axis.normalized()
    if side is None:
        side = axis.cross(UP)
        side = side.normalized() if side.length > 1e-4 else axis.orthogonal().normalized()
    other = axis.cross(side).normalized()
    perp = side * math.cos(spin) + other * math.sin(spin)
    return (axis * math.cos(angle_from) + perp * math.sin(angle_from)).normalized()


def edge_noise(rnd, low, high, count=7):
    """Рваная граница по окружности ствола: функция доли окружности → высота от `low` до `high`."""
    bumps = [rnd.uniform(low, high) for _ in range(count)]

    def at(a):
        i = a * count
        k = i - int(i)
        return bumps[int(i) % count] * (1 - k) + bumps[(int(i) + 1) % count] * k

    return at


class Tree(lib.Model):
    """Сетка дерева: трубы сучьев с развёрткой в метрах коры и карточки листвы."""

    def __init__(self, name):
        super().__init__(name)
        self.uv = self.bm.loops.layers.uv.verify()
        self.tiles = {}
        self.cards = []

    def orient(self):
        """Грани заданы обходом сразу наружу; пересчёт развернул бы одиночные карточки как попало."""

    def bark(self, key, folder, meters, rough=0.85, tint="#ffffff"):
        """Кора из папки `folder`: цвет, умноженный на `tint`; одна картинка — `meters` × `meters`. Есть в папке
        normal.jpg и arm.jpg — рельеф и шероховатость из них, нет — шероховатость `rough`."""
        m = bpy.data.materials.new(f"{self.name}_{key}")
        m.use_nodes = True
        nodes, links = m.node_tree.nodes, m.node_tree.links
        node = lib.bsdf(m)

        def image(file, data):
            tex = nodes.new("ShaderNodeTexImage")
            tex.image = bpy.data.images.load(os.path.join(TEXTURES, folder, file), check_existing=True)
            if data:
                tex.image.colorspace_settings.name = "Non-Color"
            return tex

        color = image("color.jpg", False)
        mix = nodes.new("ShaderNodeMix")
        mix.data_type = "RGBA"
        mix.blend_type = "MULTIPLY"
        mix.inputs[0].default_value = 1.0
        links.new(color.outputs["Color"], mix.inputs[6])
        mix.inputs[7].default_value = (*lib.hex_color(tint), 1.0)
        links.new(mix.outputs[2], node.inputs["Base Color"])
        node.inputs["Metallic"].default_value = 0.0
        if os.path.exists(os.path.join(TEXTURES, folder, "arm.jpg")):
            channels = nodes.new("ShaderNodeSeparateColor")
            links.new(image("arm.jpg", True).outputs["Color"], channels.inputs["Color"])
            links.new(channels.outputs["Green"], node.inputs["Roughness"])
        else:
            node.inputs["Roughness"].default_value = rough
        if os.path.exists(os.path.join(TEXTURES, folder, "normal.jpg")):
            normal_map = nodes.new("ShaderNodeNormalMap")
            links.new(image("normal.jpg", True).outputs["Color"], normal_map.inputs["Color"])
            links.new(normal_map.outputs["Normal"], node.inputs["Normal"])
        shade(m)
        self.materials.append(m)
        self.index[key] = len(self.materials) - 1
        self.tiles[self.index[key]] = meters
        return self.index[key]

    def plain(self, key, color, rough=0.85):
        mi = self.mat(key, color, rough)
        shade(self.materials[mi])
        self.tiles[mi] = 1.0
        return mi

    def foliage(self, key, folder, cutoff):
        """Листва атласа веточек `folder`: цвет с прозрачностью вырезом по `cutoff`, нормали и шероховатость атласа."""
        m = bpy.data.materials.new(f"{self.name}_{key}")
        m.use_nodes = True
        nodes, links = m.node_tree.nodes, m.node_tree.links
        node = lib.bsdf(m)

        def image(file, data):
            tex = nodes.new("ShaderNodeTexImage")
            tex.image = bpy.data.images.load(os.path.join(TEXTURES, folder, file), check_existing=True)
            tex.image.reload()
            if data:
                tex.image.colorspace_settings.name = "Non-Color"
            return tex

        color = image("color.png", False)
        links.new(color.outputs["Color"], node.inputs["Base Color"])
        # Порог: выгрузка glTF видит «больше чем» после альфы и пишет alphaMode MASK с alphaCutoff.
        cut = nodes.new("ShaderNodeMath")
        cut.operation = "GREATER_THAN"
        cut.inputs[1].default_value = cutoff
        links.new(color.outputs["Alpha"], cut.inputs[0])
        links.new(cut.outputs["Value"], node.inputs["Alpha"])
        arm = image("arm.png", True)
        channels = nodes.new("ShaderNodeSeparateColor")
        links.new(arm.outputs["Color"], channels.inputs["Color"])
        links.new(channels.outputs["Green"], node.inputs["Roughness"])
        node.inputs["Metallic"].default_value = 0.0
        relief = image("normal.png", True)
        normal_map = nodes.new("ShaderNodeNormalMap")
        links.new(relief.outputs["Color"], normal_map.inputs["Color"])
        links.new(normal_map.outputs["Normal"], node.inputs["Normal"])
        m.surface_render_method = "DITHERED"
        m.use_backface_culling = False
        shade(m)
        self.materials.append(m)
        self.index[key] = len(self.materials) - 1
        return self.index[key]

    def tube(self, points, radii, sides, material, shift=(0.0, 0.0)):
        """Труба по ломаной `points` с радиусом `radii` в каждой точке; кольца поворачиваются вслед за ломаной без
        закрутки. `material` — номер материала или функция (высота, доля окружности, доля длины) → номер. Развёртка —
        по окружности и вдоль трубы в метрах коры, сдвинутая на `shift`."""
        pick = material if callable(material) else (lambda z, a, s: material)
        d0 = (points[1] - points[0]).normalized()
        e1 = d0.orthogonal().normalized()
        rings = []
        length = 0.0
        for i, p in enumerate(points):
            d = (points[min(i + 1, len(points) - 1)] - points[max(i - 1, 0)]).normalized()
            e1 = (e1 - d * e1.dot(d)).normalized()
            e2 = d.cross(e1)
            if i:
                length += (p - points[i - 1]).length
            rings.append(([self.bm.verts.new(p + (e1 * math.cos(2 * math.pi * k / sides) + e2 * math.sin(2 * math.pi * k / sides)) * radii[i])
                           for k in range(sides)], length))
        for i in range(len(points) - 1):
            (r0, l0), (r1, l1) = rings[i], rings[i + 1]
            for k in range(sides):
                corners = ((r0, k, l0, radii[i]), (r0, k + 1, l0, radii[i]), (r1, k + 1, l1, radii[i + 1]), (r1, k, l1, radii[i + 1]))
                f = self.bm.faces.new([ring[kk % sides] for ring, kk, _, _ in corners])
                mi = pick(points[i].z, k / sides, i / (len(points) - 1))
                f.material_index = mi
                f.smooth = True
                meters = self.tiles.get(mi, 1.0)
                for loop, (_, kk, ll, rr) in zip(f.loops, corners):
                    loop[self.uv].uv = (2 * math.pi * rr * kk / sides / meters + shift[0], ll / meters + shift[1])

    def card(self, base, axis, normal, length, variant, mi, curl=0.22):
        """Карточка веточки длиной `length` и шириной в половину: черешок — в `base`, веточка идёт по `axis`, лицо
        смотрит по `normal`. Карточка не плоская: поперёк сломана по стеблю, края опущены на `curl` ширины — как у
        настоящей веточки, листья которой смотрят в разные стороны. `variant` — половина атласа: 0 левая, 1 правая."""
        v = axis.normalized()
        n = (normal - v * normal.dot(v)).normalized()
        u = v.cross(n)
        width = length / 2
        rows = []
        for t in (0.0, 1.0):
            c = base + v * (length * t)
            edge = n * (-curl * width * t)
            rows.append(([self.bm.verts.new(c - u * width / 2 + edge), self.bm.verts.new(c), self.bm.verts.new(c + u * width / 2 + edge)], t))
        u0 = 0.5 * variant
        for (a, ta), (b, tb) in zip(rows, rows[1:]):
            for k in range(2):
                f = self.bm.faces.new([a[k], a[k + 1], b[k + 1], b[k]])
                f.material_index = mi
                f.smooth = True
                uk, uk1 = u0 + 0.25 * k, u0 + 0.25 * (k + 1)
                for loop, uv in zip(f.loops, ((uk, ta), (uk1, ta), (uk1, tb), (uk, tb))):
                    loop[self.uv].uv = uv
                self.cards.append(f)

    def finish(self, crown_light=0.8):
        """Сетка в объект: у карточек нормали кроны — от её середины, смешанные с лицом карточки на `crown_light`;
        затенение в цвет вершин `occlusion` — глубже в кроне и ниже темнее, ствол темнеет к земле."""
        self.bm.faces.index_update()
        card_faces = {f.index for f in self.cards}
        pts = [v.co for f in self.cards for v in f.verts]
        lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
        hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
        center, axes = (lo + hi) / 2, (hi - lo) / 2
        me = bpy.data.meshes.new(self.name)
        self.bm.to_mesh(me)
        self.bm.free()
        for m in self.materials:
            me.materials.append(m)
        normals = []
        attr = me.color_attributes.new("occlusion", "BYTE_COLOR", "CORNER")
        for poly in me.polygons:
            face_n = Vector(poly.normal)
            for li in poly.loop_indices:
                co = me.vertices[me.loops[li].vertex_index].co
                rel = Vector(((co.x - center.x) / axes.x, (co.y - center.y) / axes.y, (co.z - center.z) / axes.z))
                depth = rel.length
                height = (co.z - lo.z) / (hi.z - lo.z)
                if poly.index in card_faces:
                    crown_n = Vector((rel.x / axes.x, rel.y / axes.y, rel.z / axes.z)).normalized()
                    side = face_n if face_n.dot(crown_n) >= 0 else -face_n
                    # Нормали чуть подняты к небу: свет сквозь листья движок не считает, без подъёма низ кроны чёрный.
                    normals.append(tuple((crown_n * crown_light + side * (1 - crown_light) + UP * 0.45).normalized()))
                else:
                    normals.append((0.0, 0.0, 0.0))
                inner = 0.65 + 0.35 * smoothstep(0.1, 1.0, depth) if depth < 1.0 else 1.0
                below = 0.85 + 0.15 * smoothstep(0.0, 0.7, height)
                ground = 0.7 + 0.3 * smoothstep(0.0, 0.8, co.z)
                v = inner * below * ground
                attr.data[li].color = (v, v, v, 1.0)
        me.normals_split_custom_set(normals)
        me.color_attributes.active_color = attr
        me.color_attributes.render_color_index = me.color_attributes.active_color_index
        obj = bpy.data.objects.new(self.name, me)
        bpy.context.collection.objects.link(obj)
        return obj


def shade(material):
    """Цвет материала умножается на цвет вершин `occlusion`: так затенение видно в Blender и так его понимает glTF."""
    nodes, links = material.node_tree.nodes, material.node_tree.links
    node = lib.bsdf(material)
    attr = nodes.new("ShaderNodeVertexColor")
    attr.layer_name = "occlusion"
    mix = nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs[0].default_value = 1.0
    source = node.inputs["Base Color"]
    if source.links:
        links.new(source.links[0].from_socket, mix.inputs[6])
    else:
        mix.inputs[6].default_value = source.default_value
    links.new(attr.outputs["Color"], mix.inputs[7])
    links.new(mix.outputs[2], source)


def twig_card(m, base, direction, mi, rnd, twig, weep, face_up=0.9, outward=0.5, roll=30):
    """Карточка веточки у точки сучка `base`: идёт вдоль сучка `direction`, никнет вниз на долю `weep` (меньше нуля —
    тянется вверх) и чуть наружу от ствола на `outward`; лицом наружу и к небу на `face_up` — сверху, откуда смотрит
    камера игры, видны листья, а не ребро карточки; поворот вокруг своей оси до `roll`°, длина около веточки атласа,
    половина атласа случайная."""
    radial = Vector((base.x, base.y, 0.0))
    radial = radial.normalized() if radial.length > 0.05 else Vector((rnd.uniform(-1, 1), rnd.uniform(-1, 1), 0)).normalized()
    flat = Vector((direction.x, direction.y, 0.0))
    flat = (flat.normalized() if flat.length > 0.05 else radial) + radial * outward
    rise = max(0.0, direction.z)
    if weep >= 0:
        axis = flat.normalized() * (1 - weep) + DOWN * weep + UP * rise * (1 - weep)
    else:
        axis = flat.normalized() * (1 + weep) + UP * (rise - weep)
    axis = axis.normalized()
    up = (radial * 0.6 + UP * face_up).normalized()
    face = Matrix.Rotation(math.radians(rnd.uniform(-roll, roll)), 3, axis) @ (up - axis * up.dot(axis))
    if face.length < 1e-3:
        face = axis.orthogonal()
    m.card(base, axis, face, twig.length * rnd.uniform(0.7, 1.0), rnd.randint(0, 1), mi)


# --- Атлас веточек ---------------------------------------------------------------------------------------------------


class Needles:
    """Хвоя побегов: длина иголки, сколько иголок на метр побега, угол от побега, цвета и с какой доли длины побега
    они начинаются (у сосны — пучком на конце)."""

    def __init__(self, length, density, angle, colors, start, width=0.0013):
        self.length, self.density, self.angle, self.colors, self.start, self.width = length, density, angle, colors, start, width


class Fruit:
    """Плоды: радиус, сколько в грозди, разброс грозди, цвета, доля побегов с гроздью и где она — на конце побега
    ("tip", ягоды рябины) или вдоль него ("along", яблоки)."""

    def __init__(self, radius, count, spread, colors, chance, where):
        self.radius, self.count, self.spread, self.colors, self.chance, self.where = radius, count, spread, colors, chance, where


class Twig:
    """Как устроена веточка дерева: длина (атлас — квадрат со стороной в длину, на нём две веточки шириной в половину),
    цвет стебля, сколько боковых побегов и под каким углом, листья набора `leaves` длиной `leaf` через `spacing` по
    побегу, доля узлов с коротким побегом пары листьев `spur`, доля светлых листьев `light`; `compound` — сложный лист
    рябины (пар листочков от, до, длина черешка от, до), `needles` — хвоя вместо листьев, `fruit` — плоды; `tint` —
    подкраска листьев умножением: набор светлее и желтее листьев породы."""

    def __init__(self, length, stem, shoots, spread, leaves=None, leaf=(0.05, 0.07), spacing=(0.02, 0.03), spur=0.5,
                 light=None, compound=None, needles=None, fruit=None, tint="#ffffff"):
        self.length, self.stem, self.shoots, self.spread = length, stem, shoots, spread
        self.leaves, self.leaf, self.spacing, self.spur, self.light = leaves, leaf, spacing, spur, light
        self.compound, self.needles, self.fruit, self.tint = compound, needles, fruit, tint


BIRCH_TWIG = Twig(1.2, "#4a3a32", (6, 8), math.radians(34), "LeafSet004", (0.05, 0.072), (0.02, 0.027), light=0.65)
LINDEN_TWIG = Twig(1.1, "#5a4232", (5, 7), math.radians(40), "LeafSet023", (0.065, 0.09), (0.03, 0.04), spur=0.25,
                   tint="#8ca6bc")
APPLE_TWIG = Twig(0.9, "#4f3a2e", (4, 6), math.radians(42), "LeafSet022", (0.055, 0.075), (0.024, 0.032), spur=0.6,
                  fruit=Fruit(0.032, (1, 2), 0.03, ("#c8452a", "#b93a26", "#d39a3a", "#c56a2c"), 0.55, "along"), tint="#a8bcb8")
ROWAN_TWIG = Twig(1.0, "#5b4a3e", (3, 4), math.radians(36), "LeafSet001", (0.04, 0.055), (0.05, 0.07), spur=0.0,
                  compound=(5, 7, 0.13, 0.18),
                  fruit=Fruit(0.0048, (22, 36), 0.05, ("#d2401c", "#c9361a", "#e0581e", "#b8301a"), 0.6, "tip"))
PINE_TWIG = Twig(0.8, "#7a5236", (5, 7), math.radians(42), spur=0.0,
                 needles=Needles((0.05, 0.08), 2800, (20, 55), ("#56704c", "#4c6646", "#62785a", "#6b7f52"), 0.25, width=0.0017))
SPRUCE_TWIG = Twig(1.0, "#5e4532", (10, 13), math.radians(55), spur=0.0,
                   needles=Needles((0.02, 0.03), 2600, (40, 80), ("#2f4a33", "#36533a", "#2a432f", "#3f5b3d"), 0.03, width=0.0019))


def twig_geometry(bm, uv, tint, twig, x0, rnd):
    """Одна веточка в полосе x0…x0 + длина/2 плоскости XY, черешком к y = 0: стебель с боковыми побегами, на побегах —
    листья поочерёдно по сторонам (у части узлов — короткий побег с парой листьев), сложные листья рябины или иголки;
    гроздья ягод на концах побегов, яблоки вдоль них. Что вылезло бы за полосу, не ставится. Материалы: 0 — лист,
    1 — стебель, 2 — иголка, 3 — плод; цвет иголок и плодов — в слое вершин `tint`."""
    half = twig.length / 2
    side_of_stem, rects = LEAVES[twig.leaves] if twig.leaves else ("bottom", ())
    z_axis = Vector((0.0, 0.0, 1.0))

    def inside(p, margin=0.004):
        return x0 + margin < p.x < x0 + half - margin and 0.002 < p.y < twig.length - margin

    def paint(face, color):
        rgb = lib.hex_color(color)
        k = rnd.uniform(0.85, 1.12)
        for loop in face.loops:
            loop[tint] = (min(1.0, rgb[0] * k), min(1.0, rgb[1] * k), min(1.0, rgb[2] * k), 1.0)

    def stem(points, r0, r1):
        n = len(points)
        radii = [r0 + (r1 - r0) * i / (n - 1) for i in range(n)]
        rings = []
        for i, p in enumerate(points):
            d = (points[min(i + 1, n - 1)] - points[max(i - 1, 0)]).normalized()
            side = z_axis.cross(d).normalized()
            rings.append([bm.verts.new(p + (side * math.cos(a) + z_axis * math.sin(a)) * radii[i])
                          for a in (2 * math.pi * k / 6 for k in range(6))])
        for i in range(n - 1):
            for k in range(6):
                f = bm.faces.new([rings[i][k], rings[i][(k + 1) % 6], rings[i + 1][(k + 1) % 6], rings[i + 1][k]])
                f.material_index = 1
                f.smooth = True

    def leaf(anchor, angle, size, z):
        if twig.light is not None:
            rect = rects[rnd.randrange(3) if rnd.random() < twig.light else rnd.randrange(3, len(rects))]
        else:
            rect = rnd.choice(rects)
        x_0, y_0, x_1, y_1 = rect
        lengthwise, across = (y_1 - y_0, x_1 - x_0) if side_of_stem == "bottom" else (x_1 - x_0, y_1 - y_0)
        d = Vector((math.cos(angle), math.sin(angle), 0.0))
        twist = math.radians(rnd.uniform(-45, 45))
        s = Vector((-d.y, d.x, 0.0)) * math.cos(twist) + z_axis * math.sin(twist)
        w = size * across / lengthwise
        corners = [(w / 2, 0.0), (-w / 2, 0.0), (-w / 2, size), (w / 2, size)]  # лицом к камере съёмки, +Z
        pts = [anchor + d * y + s * x + Vector((0, 0, z + 0.25 * y * rnd.uniform(-0.3, 0.3))) for x, y in corners]
        if not all(inside(p) for p in pts):
            return
        f = bm.faces.new([bm.verts.new(p) for p in pts])
        f.material_index = 0
        for loop, (x, y) in zip(f.loops, corners):
            if side_of_stem == "bottom":
                loop[uv].uv = (x_0 + (x / w + 0.5) * (x_1 - x_0), (1 - y_1) + (y / size) * (y_1 - y_0))
            else:
                loop[uv].uv = (x_0 + (y / size) * (x_1 - x_0), (1 - y_1) + (x / w + 0.5) * (y_1 - y_0))

    def compound(anchor, angle, size, z):
        """Сложный лист рябины: черешок с парами листочков и листочком на конце."""
        pairs_lo, pairs_hi, rachis_lo, rachis_hi = twig.compound
        pairs, rachis = rnd.randint(pairs_lo, pairs_hi), rnd.uniform(rachis_lo, rachis_hi)
        d = Vector((math.cos(angle), math.sin(angle), 0.0))
        end = anchor + d * rachis
        if not inside(end):
            return
        stem([anchor + Vector((0, 0, z - 0.003)), end + Vector((0, 0, z - 0.003))], 0.0013, 0.0008)
        for i in range(pairs):
            t = 0.12 + 0.8 * i / max(1, pairs - 1)
            for sign in (-1, 1):
                leaf(anchor + d * (rachis * t), angle + sign * math.radians(rnd.uniform(58, 80)),
                     size * rnd.uniform(0.85, 1.05) * (1 - 0.2 * t), z + rnd.uniform(0.0, 0.006))
        leaf(end - d * 0.004, angle + math.radians(rnd.uniform(-10, 10)), size * 0.95, z + 0.006)

    def needles(points, length):
        nd = twig.needles
        for _ in range(int(length * (1 - nd.start) * nd.density)):
            t = rnd.uniform(nd.start, 1.0)
            p, d = along(points, t)
            side = Vector((-d.y, d.x, 0.0))
            phi = rnd.uniform(0, 2 * math.pi)
            a = math.radians(rnd.uniform(*nd.angle))
            out = (d * math.cos(a) + (side * math.cos(phi) + z_axis * math.sin(phi)) * math.sin(a)).normalized()
            size = rnd.uniform(*nd.length)
            perp = out.cross(z_axis)
            perp = perp.normalized() if perp.length > 1e-3 else side
            tip = p + out * size
            quad = [p - perp * nd.width / 2, p + perp * nd.width / 2, tip + perp * nd.width * 0.15, tip - perp * nd.width * 0.15]
            if not all(inside(q) for q in quad):
                continue
            f = bm.faces.new([bm.verts.new(q) for q in quad])
            f.material_index = 2
            paint(f, rnd.choice(nd.colors))

    def fruits(center, count):
        fr = twig.fruit
        for _ in range(count):
            a, r = rnd.uniform(0, 2 * math.pi), fr.spread * math.sqrt(rnd.random())
            c = center + Vector((math.cos(a) * r, math.sin(a) * r, rnd.uniform(0.0, 0.012)))
            radius = fr.radius * rnd.uniform(0.85, 1.15)
            if not inside(c, radius + 0.004):
                continue
            res = bmesh.ops.create_icosphere(bm, subdivisions=2, radius=radius, matrix=Matrix.Translation(c))
            color = rnd.choice(fr.colors)
            for f in {f for v in res["verts"] for f in v.link_faces}:
                f.material_index = 3
                f.smooth = True
                paint(f, color)

    def shoot(start, direction, length, r0, depth):
        """Побег: стебель обрывается у края полосы, чтобы не залезть на соседнюю веточку атласа."""
        points = [Vector(start)]
        d = Vector(direction).normalized()
        steps = max(4, int(length / 0.05))
        for _ in range(steps):
            a = math.radians(rnd.uniform(-5, 5))
            d = Vector((d.x * math.cos(a) - d.y * math.sin(a), d.x * math.sin(a) + d.y * math.cos(a), 0.0)).normalized()
            p = points[-1] + d * (length / steps)
            if not (x0 + 0.02 < p.x < x0 + half - 0.02 and p.y < twig.length - 0.02):
                break
            points.append(p)
        if len(points) < 3:
            return
        length = sum((b - a).length for a, b in zip(points, points[1:]))
        stem([p + Vector((0, 0, -0.004)) for p in points], r0, r0 * 0.35)
        if twig.needles:
            needles(points, length)
        else:
            place = compound if twig.compound else leaf
            t, side = rnd.uniform(0.05, 0.1), rnd.choice((-1, 1))
            while t < 0.97:
                p, dir_ = along(points, t)
                base_angle = math.atan2(dir_.y, dir_.x)
                size = rnd.uniform(*twig.leaf) * (0.65 + 0.35 * smoothstep(1.0, 0.55, t))
                place(p, base_angle + side * math.radians(rnd.uniform(35, 70)), size, rnd.uniform(0.0, 0.02))
                if rnd.random() < twig.spur:
                    spur = base_angle - side * math.radians(rnd.uniform(30, 60))
                    q = p + Vector((math.cos(spur), math.sin(spur), 0.0)) * rnd.uniform(0.008, 0.018)
                    for _ in range(2):
                        leaf(q, spur + math.radians(rnd.uniform(-35, 35)), size * rnd.uniform(0.8, 1.0), rnd.uniform(0.0, 0.02))
                    if twig.fruit and twig.fruit.where == "along" and rnd.random() < twig.fruit.chance * 0.25:
                        fruits(q + Vector((0, 0, 0.03)), rnd.randint(*twig.fruit.count))
                side = -side
                t += rnd.uniform(*twig.spacing) / length
            tip, tip_dir = along(points, 1.0)
            place(tip, math.atan2(tip_dir.y, tip_dir.x), rnd.uniform(*twig.leaf) * 0.6, 0.02)
        if twig.fruit and twig.fruit.where == "tip" and rnd.random() < twig.fruit.chance:
            tip, tip_dir = along(points, 0.92)
            fruits(tip + tip_dir * 0.03 + Vector((0, 0, 0.035)), rnd.randint(*twig.fruit.count))
        if depth < 2:
            count = rnd.randint(*twig.shoots) if depth == 0 else rnd.randint(1, 2)
            for i in range(count):
                ts = 0.12 + 0.6 * (i + rnd.uniform(0.0, 0.6)) / count
                p, dir_ = along(points, ts)
                sign = 1 if i % 2 else -1
                a = math.atan2(dir_.y, dir_.x) + sign * twig.spread * rnd.uniform(0.7, 1.2)
                shoot(p, Vector((math.cos(a), math.sin(a), 0.0)), length * (1 - ts) * rnd.uniform(0.45, 0.7), r0 * 0.6, depth + 1)

    lean = math.radians(rnd.uniform(-6, 6))
    shoot(Vector((x0 + half / 2, 0.004, 0.0)), Vector((math.sin(lean), math.cos(lean), 0.0)), twig.length * 0.93, 0.0035, 0)


def bake_twig(name, twig, seed):
    """Атлас веточек `name`_twig: две веточки рядом, снятые сверху без света — цвет с прозрачностью, нормали в осях
    карточки (x — вправо, y — к кончику веточки, z — на зрителя) и затенение-шероховатость-металл. Цвет и нормали
    растекаются в прозрачные точки, чтобы уменьшенные копии атласа не темнели по краям листьев."""
    folder = os.path.join(TEXTURES, f"{name}_twig")
    os.makedirs(folder, exist_ok=True)
    scene = bpy.data.scenes.new(f"{name}_twig")
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 24
    scene.cycles.use_denoising = False
    scene.cycles.transparent_max_bounces = 32
    scene.render.film_transparent = True
    scene.render.resolution_x = scene.render.resolution_y = ATLAS
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "8"
    scene.display_settings.display_device = "sRGB"
    scene.view_settings.look = "None"
    scene.view_settings.exposure = 0.0
    scene.view_settings.gamma = 1.0

    leaf_mat, leaf_emit = twig_material(f"{name}_twig_leaf", os.path.join(TEXTURES, twig.leaves), twig.tint) if twig.leaves else (None, None)
    stem_mat, stem_emit = solid_material(f"{name}_twig_stem", (*lib.hex_color(twig.stem), 1.0), 0.8)
    needle_mat, needle_emit = solid_material(f"{name}_twig_needle", None, 0.6)
    fruit_mat, fruit_emit = solid_material(f"{name}_twig_fruit", None, 0.45)
    me = bpy.data.meshes.new(f"{name}_twig")
    bm = bmesh.new()
    uv = bm.loops.layers.uv.verify()
    tint = bm.loops.layers.float_color.new("tint")
    rnd = random.Random(seed)
    for variant in (0, 1):
        twig_geometry(bm, uv, tint, twig, variant * twig.length / 2, rnd)
    bm.to_mesh(me)
    bm.free()
    for mat in (leaf_mat or stem_mat, stem_mat, needle_mat, fruit_mat):
        me.materials.append(mat)
    obj = bpy.data.objects.new(f"{name}_twig", me)
    scene.collection.objects.link(obj)
    cam = bpy.data.cameras.new(f"{name}_twig")
    cam.type = "ORTHO"
    cam.ortho_scale = twig.length
    cam_obj = bpy.data.objects.new(f"{name}_twig_cam", cam)
    cam_obj.location = (twig.length / 2, twig.length / 2, 2.0)
    scene.collection.objects.link(cam_obj)
    scene.camera = cam_obj
    world = bpy.data.worlds.new(f"{name}_twig")
    world.use_nodes = True
    scene.world = world

    raw = {}
    for kind in ("color", "normal", "arm"):
        scene.view_settings.view_transform = "Standard" if kind == "color" else "Raw"
        for emit in (leaf_emit, stem_emit, needle_emit, fruit_emit):
            if emit:
                emit(kind)
        raw[kind] = os.path.join(folder, f"_{kind}.png")
        scene.render.filepath = raw[kind]
        bpy.ops.render.render(write_still=True, scene=scene.name)

    alpha = None
    for kind in ("color", "normal", "arm"):
        img = bpy.data.images.load(raw[kind])
        img.colorspace_settings.name = "Non-Color"
        px = np.empty(ATLAS * ATLAS * 4, np.float32)
        img.pixels.foreach_get(px)
        px = px.reshape(ATLAS, ATLAS, 4)
        bpy.data.images.remove(img)
        os.remove(raw[kind])
        if alpha is None:
            alpha = px[:, :, 3].copy()
        rgb = bleed(px[:, :, :3], alpha)
        out = np.concatenate([rgb, (alpha if kind == "color" else np.ones_like(alpha))[:, :, None]], axis=2)
        save = bpy.data.images.new(f"{name}_{kind}", ATLAS, ATLAS, alpha=kind == "color")
        save.colorspace_settings.name = "Non-Color"
        save.pixels.foreach_set(out.ravel())
        save.filepath_raw = os.path.join(folder, f"{kind}.png")
        save.file_format = "PNG"
        save.save()
        bpy.data.images.remove(save)
    bpy.data.objects.remove(obj)
    bpy.data.objects.remove(cam_obj)
    bpy.data.scenes.remove(scene)
    return f"{name}_twig"


def bleed(rgb, alpha, steps=40):
    """Цвет непрозрачных точек растекается в прозрачные соседние по шагу за раз; что не дотекло — средний цвет."""
    known = (alpha > 0.02).astype(np.float32)
    out = rgb * known[:, :, None]
    for _ in range(steps):
        acc = np.zeros_like(out)
        cnt = np.zeros_like(known)
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            acc += np.roll(out * known[:, :, None], (dy, dx), (0, 1))
            cnt += np.roll(known, (dy, dx), (0, 1))
        grow_mask = (cnt > 0) & (known == 0)
        out[grow_mask] = acc[grow_mask] / cnt[grow_mask][:, None]
        known[grow_mask] = 1.0
    if (known == 0).any():
        out[known == 0] = out[known > 0].mean(axis=0)
    return out


def twig_material(key, folder, tint):
    """Материал листьев для съёмки атласа: светится цветом, умноженным на `tint`, нормалью или шероховатостью листа
    (выбирает `emit`), прозрачен по маске листа."""
    m = bpy.data.materials.new(key)
    m.use_nodes = True
    nodes, links = m.node_tree.nodes, m.node_tree.links
    nodes.clear()
    out = nodes.new("ShaderNodeOutputMaterial")

    def image(file, data):
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = bpy.data.images.load(os.path.join(folder, file), check_existing=True)
        if data:
            tex.image.colorspace_settings.name = "Non-Color"
        return tex

    color = image("color.png", False)
    arm = image("arm.png", True)
    relief = image("normal.png", True)
    channels = nodes.new("ShaderNodeSeparateColor")
    links.new(arm.outputs["Color"], channels.inputs["Color"])
    shaded = nodes.new("ShaderNodeMix")
    shaded.data_type = "RGBA"
    shaded.blend_type = "MULTIPLY"
    shaded.inputs[0].default_value = 1.0
    links.new(color.outputs["Color"], shaded.inputs[6])
    links.new(channels.outputs["Red"], shaded.inputs[7])
    tinted = nodes.new("ShaderNodeMix")
    tinted.data_type = "RGBA"
    tinted.blend_type = "MULTIPLY"
    tinted.inputs[0].default_value = 1.0
    links.new(shaded.outputs[2], tinted.inputs[6])
    tinted.inputs[7].default_value = (*lib.hex_color(tint), 1.0)
    normal_map = nodes.new("ShaderNodeNormalMap")
    links.new(relief.outputs["Color"], normal_map.inputs["Color"])
    packed = nodes.new("ShaderNodeVectorMath")
    packed.operation = "MULTIPLY_ADD"
    links.new(normal_map.outputs["Normal"], packed.inputs[0])
    packed.inputs[1].default_value = (0.5, 0.5, 0.5)
    packed.inputs[2].default_value = (0.5, 0.5, 0.5)
    combined = nodes.new("ShaderNodeCombineColor")
    links.new(channels.outputs["Red"], combined.inputs["Red"])
    links.new(channels.outputs["Green"], combined.inputs["Green"])
    combined.inputs["Blue"].default_value = 0.0
    emission = nodes.new("ShaderNodeEmission")
    clear = nodes.new("ShaderNodeBsdfTransparent")
    mix = nodes.new("ShaderNodeMixShader")
    links.new(color.outputs["Alpha"], mix.inputs[0])
    links.new(clear.outputs[0], mix.inputs[1])
    links.new(emission.outputs[0], mix.inputs[2])
    links.new(mix.outputs[0], out.inputs["Surface"])
    sources = {"color": tinted.outputs[2], "normal": packed.outputs[0], "arm": combined.outputs[0]}

    def emit(kind):
        links.new(sources[kind], emission.inputs["Color"])

    return m, emit


def solid_material(key, color, rough):
    """Материал стебля, иголки или плода для съёмки атласа: светится цветом (`color` или слой вершин `tint`), своей
    нормалью или шероховатостью `rough`."""
    m = bpy.data.materials.new(key)
    m.use_nodes = True
    nodes, links = m.node_tree.nodes, m.node_tree.links
    nodes.clear()
    out = nodes.new("ShaderNodeOutputMaterial")
    emission = nodes.new("ShaderNodeEmission")
    links.new(emission.outputs[0], out.inputs["Surface"])
    geometry = nodes.new("ShaderNodeNewGeometry")
    packed = nodes.new("ShaderNodeVectorMath")
    packed.operation = "MULTIPLY_ADD"
    links.new(geometry.outputs["Normal"], packed.inputs[0])
    packed.inputs[1].default_value = (0.5, 0.5, 0.5)
    packed.inputs[2].default_value = (0.5, 0.5, 0.5)
    painted = nodes.new("ShaderNodeVertexColor")
    painted.layer_name = "tint"

    def emit(kind):
        for link in list(emission.inputs["Color"].links):
            links.remove(link)
        if kind == "normal":
            links.new(packed.outputs[0], emission.inputs["Color"])
        elif kind == "arm":
            emission.inputs["Color"].default_value = (1.0, rough, 0.0, 1.0)
        elif color is None:
            links.new(painted.outputs["Color"], emission.inputs["Color"])
        else:
            emission.inputs["Color"].default_value = color

    return m, emit


# --- Лиственные --------------------------------------------------------------------------------------------------------


class Broadleaf:
    """Числа лиственного дерева для `broadleaf`. Ствол — `trunk` высоты дерева (у берёзы до макушки, у яблони — до
    развилки), радиус `radius` у земли без комля, к концу тоньше на долю `taper`, кривизна `wobble`. Сучья — `limbs`
    штук с доли ствола `crown` (от, до), от отвесного угол `rise` (у нижних, у верхних), длина до `reach` м,
    провисание `droop` (у нижних, у верхних), сучья длиннее `fork_from` м с развилкой. Веточки-карточки — `sprig_rate`
    мест на метр сучка, никнут на `weep` (основа, добавка от провисания). `bark(m, rnd)` даёт материалы: ствола (номер
    или функция высоты, доли окружности и длины), сучка по его радиусу и тонких ветвей."""

    def __init__(self, name, twig, seed, height, trunk, radius, taper, crown, limbs, rise, reach, droop, fork_from,
                 sprig_rate, weep, bark, wobble=0.025, lean=0.04, limb_jitter=0.12, profile=0.75):
        self.name, self.twig, self.seed, self.height, self.trunk, self.radius, self.taper = name, twig, seed, height, trunk, radius, taper
        self.crown, self.limbs, self.rise, self.reach, self.droop, self.fork_from = crown, limbs, rise, reach, droop, fork_from
        self.sprig_rate, self.weep, self.bark, self.wobble, self.lean = sprig_rate, weep, bark, wobble, lean
        self.limb_jitter, self.profile = limb_jitter, profile


def birch_bark(m, rnd):
    """Белая кора с тёмным растрескавшимся низом: граница рваная, от 0,7 до 1,7 м. Сучок бел у ствола, дальше кора
    темнеет: тонкие ветви берёзы бурые."""
    white = m.bark("bark", "birch_bark", 0.45, rough=0.8)
    base = m.bark("bark_base", "birch_bark_base", 0.6, rough=0.9)
    rods = m.plain("rods", "#4b3d36")
    edge = edge_noise(rnd, 0.7, 1.7)
    return (lambda z, a, s: base if z < edge(a) else white), (lambda r0: lambda z, a, s: white if s < 0.45 and r0 > 0.03 else rods), rods


def plain_bark(folder, meters, tint, rods_color):
    """Кора одной картинки на стволе и толстых сучьях, тонкие ветви — ровным цветом `rods_color`."""

    def make(m, rnd):
        bark = m.bark("bark", folder, meters, tint=tint)
        rods = m.plain("rods", rods_color)
        return bark, (lambda r0: bark if r0 > 0.025 else rods), rods

    return make


def broadleaf(spec, out=OUT):
    """Лиственное дерево по числам `spec`: ствол, сучья с развилками, на сучьях — веточки-карточки, чаще к концам, по
    две-три веером из одного места: крона сверху сплошная, с просветами."""
    lib.reset()
    twig_folder = bake_twig(spec.name, spec.twig, spec.seed)
    rnd = random.Random(spec.seed)
    m = Tree(spec.name)
    trunk_mat, limb_mat, rods = spec.bark(m, rnd)
    leaves = m.foliage("leaves", twig_folder, CUTOFF)

    height = spec.height * rnd.uniform(0.95, 1.05)
    trunk_len = height * spec.trunk
    trunk = grow(Vector((0.0, 0.0, -SINK)), Vector((rnd.uniform(-spec.lean, spec.lean), rnd.uniform(-spec.lean, spec.lean), 1.0)),
                 trunk_len + SINK, rnd, max(6, int(trunk_len / 0.6)), lambda t: 0.0, spec.wobble)

    def trunk_r(t):
        """Радиус ствола на доле длины `t`; у земли — комель шире."""
        z = t * (trunk_len + SINK) - SINK
        return spec.radius * (1 - spec.taper * t ** 1.2) + spec.radius * 0.35 * math.exp(-max(0.0, z) / 0.35)

    m.tube(trunk, [trunk_r(i / (len(trunk) - 1)) for i in range(len(trunk))], 10, trunk_mat)

    def sprigs(branch, length, droop, start=0.25):
        n = int(length * spec.sprig_rate) + 1
        for j in range(n):
            s = start + (0.98 - start) * math.sqrt((j + rnd.uniform(0.0, 0.8)) / n)
            p, d = along(branch, s)
            side = (1 if j % 2 else -1) * math.radians(rnd.uniform(50, 110))
            sprig_dir = turn(d, math.radians(rnd.uniform(40, 65)), side)
            for _ in range(3 if rnd.random() < 0.3 else 2):
                fan = turn(sprig_dir, math.radians(rnd.uniform(10, 35)), rnd.uniform(0, 2 * math.pi))
                twig_card(m, p, fan, leaves, rnd, spec.twig, spec.weep[0] + spec.weep[1] * droop)

    low, high = spec.crown
    for i in range(spec.limbs):
        rel = i / (spec.limbs - 1)
        t = low + (high - low) * rel + rnd.uniform(-0.01, 0.01)
        start, _ = along(trunk, t)
        spin = i * 2.39996 + rnd.uniform(-0.35, 0.35)
        outward = Vector((math.cos(spin), math.sin(spin), 0.0))
        rise = math.radians(spec.rise[0] + (spec.rise[1] - spec.rise[0]) * rel + rnd.uniform(-7, 7))
        direction = UP * math.cos(rise) + outward * math.sin(rise)
        length = spec.reach * math.sin(math.pi * (0.16 + 0.84 * rel)) ** spec.profile * rnd.uniform(0.85, 1.1) + 0.5
        droop = spec.droop[0] + (spec.droop[1] - spec.droop[0]) * rel
        limb = grow(start, direction, length, rnd, 6, lambda s, d=droop: 0.35 - 0.9 * d * s * s, spec.limb_jitter)
        r0 = min(spec.radius * 0.5, 0.5 * trunk_r(t))
        m.tube(limb, [r0 + (0.012 - r0) * j / 6 for j in range(7)], 6, limb_mat(r0), shift=(rnd.random(), rnd.random()))
        sprigs(limb, length, droop)
        for _ in range(2):
            p, d = along(limb, rnd.uniform(0.85, 1.0))
            twig_card(m, p, d, leaves, rnd, spec.twig, 0.25 * droop)
        if length > spec.fork_from:
            s = rnd.uniform(0.35, 0.55)
            p, d = along(limb, s)
            fork_len = length * (1 - s) * rnd.uniform(0.7, 0.9)
            fork = grow(p, turn(d, math.radians(rnd.uniform(28, 45)), rnd.uniform(0, 2 * math.pi)), fork_len, rnd, 4,
                        lambda u, d=droop: 0.2 - 1.0 * d * u * u, spec.limb_jitter)
            fr = max(0.012, r0 * 0.5)
            m.tube(fork, [fr + (0.006 - fr) * j / 4 for j in range(5)], 5, limb_mat(fr))
            sprigs(fork, fork_len, droop, 0.25)
            p, d = along(fork, 1.0)
            twig_card(m, p, d, leaves, rnd, spec.twig, 0.25 * droop)
    return save(m, out)


BIRCH = Broadleaf("birch", BIRCH_TWIG, 11, height=15.0, trunk=1.0, radius=0.155, taper=0.82, crown=(0.36, 0.95), limbs=24,
                  rise=(62, 22), reach=5.2, droop=(1.0, 0.3), fork_from=2.4, sprig_rate=2.2, weep=(0.3, 0.4), bark=birch_bark)
LINDEN = Broadleaf("linden", LINDEN_TWIG, 12, height=14.0, trunk=0.82, radius=0.21, taper=0.75, crown=(0.24, 1.0), limbs=28,
                   rise=(70, 18), reach=5.6, droop=(0.45, 0.1), fork_from=2.2, sprig_rate=2.6, weep=(0.12, 0.25),
                   bark=plain_bark("Bark012", 1.0, "#b8b3ab", "#4a3e36"), profile=0.55)
APPLE = Broadleaf("apple", APPLE_TWIG, 13, height=5.5, trunk=0.32, radius=0.13, taper=0.3, crown=(0.55, 1.0), limbs=12,
                  rise=(66, 28), reach=3.0, droop=(0.6, 0.3), fork_from=1.2, sprig_rate=5.0, weep=(0.1, 0.25),
                  bark=plain_bark("Bark012", 0.8, "#a59c92", "#4f4038"), wobble=0.12, lean=0.12, limb_jitter=0.3, profile=0.3)
ROWAN = Broadleaf("rowan", ROWAN_TWIG, 14, height=7.0, trunk=0.85, radius=0.1, taper=0.75, crown=(0.32, 1.0), limbs=15,
                  rise=(55, 18), reach=2.6, droop=(0.5, 0.15), fork_from=1.6, sprig_rate=3.0, weep=(0.15, 0.25),
                  bark=plain_bark("Bark002", 0.6, "#a6a39c", "#55493f"), wobble=0.05, limb_jitter=0.18)


# --- Хвойные ---------------------------------------------------------------------------------------------------------


def spruce(name="spruce", seed=21, out=OUT):
    """Ель европейская, 18 м: прямой ствол до макушки, мутовки сучьев от метра над землёй, книзу длиннее — конус;
    сучья отходят почти горизонтально, никнут и поднимают концы, с них по обе стороны свисают лапы — карточки
    хвойных веточек лицом к небу."""
    lib.reset()
    twig_folder = bake_twig(name, SPRUCE_TWIG, seed)
    rnd = random.Random(seed)
    m = Tree(name)
    bark = m.bark("bark", "Bark012", 0.9, tint="#9f978e")
    rods = m.plain("rods", "#3d332c")
    leaves = m.foliage("leaves", twig_folder, CUTOFF)
    height = 18.0 * rnd.uniform(0.95, 1.05)
    trunk = grow(Vector((0.0, 0.0, -SINK)), Vector((rnd.uniform(-0.02, 0.02), rnd.uniform(-0.02, 0.02), 1.0)), height + SINK, rnd,
                 24, lambda t: 0.0, 0.01)
    m.tube(trunk, [0.03 + 0.17 * (1 - i / 24) ** 1.1 + 0.05 * math.exp(-max(0.0, i / 24 * (height + SINK) - SINK) / 0.35)
                   for i in range(25)], 10, bark)
    z = 1.0
    whorl = 0
    while z < height - 0.6:
        rel = z / height
        reach = 3.4 * (1 - rel) ** 0.95 + 0.35
        for k in range(rnd.randint(4, 6)):
            spin = whorl * 0.7 + k * 2 * math.pi / 5 + rnd.uniform(-0.35, 0.35)
            outward = Vector((math.cos(spin), math.sin(spin), 0.0))
            start, _ = along(trunk, (z + SINK) / (height + SINK))
            tilt = math.radians(95 - 40 * rel + rnd.uniform(-8, 8))
            direction = UP * math.cos(tilt) + outward * math.sin(tilt)
            length = reach * rnd.uniform(0.85, 1.1)
            droop = 0.6 * (1 - rel)
            branch = grow(start, direction, length, rnd, 4, lambda s, d=droop: -0.5 * d + 0.9 * s * s, 0.08)
            r0 = 0.012 + 0.04 * (1 - rel)
            m.tube([branch[0], branch[2], branch[4]], [r0, (r0 + 0.006) / 2, 0.006], 3, rods)
            steps = max(1, int(length / 0.42))
            for j in range(steps):
                s = 0.2 + 0.8 * (j + rnd.uniform(0.0, 0.5)) / steps
                p, d = along(branch, s)
                for sign in (-1, 1):
                    lap = turn(d, math.radians(rnd.uniform(40, 70)), sign * math.radians(90) + rnd.uniform(-0.3, 0.3))
                    twig_card(m, p, lap, leaves, rnd, SPRUCE_TWIG, 0.3 + 0.2 * (1 - rel), face_up=1.4, outward=0.2, roll=20)
            p, d = along(branch, 1.0)
            twig_card(m, p, d, leaves, rnd, SPRUCE_TWIG, -0.1, face_up=1.4, outward=0.2, roll=20)
        z += rnd.uniform(0.55, 0.75)
        whorl += 1
    for _ in range(3):
        p, _ = along(trunk, 1.0)
        twig_card(m, p - UP * 0.3, Vector((rnd.uniform(-0.3, 0.3), rnd.uniform(-0.3, 0.3), 1.0)), leaves, rnd, SPRUCE_TWIG, -0.8,
                  face_up=0.6, outward=0.0, roll=60)
    return save(m, out)


def pine(name="pine", seed=22, out=OUT):
    """Сосна обыкновенная, 20 м: высокий голый ствол — внизу тёмная толстая кора бороздами, выше рыжая тонкая, —
    крона с верхней трети, сучья раскидистые и кривые, хвоя пучками-облаками на концах лицом к небу; на стволе —
    сухие обломки нижних сучьев."""
    lib.reset()
    twig_folder = bake_twig(name, PINE_TWIG, seed)
    rnd = random.Random(seed)
    m = Tree(name)
    dark = m.bark("bark", "Bark014", 1.0, tint="#8c7c70")
    ginger = m.bark("bark_top", "Bark015", 0.7, tint="#f0a26a")
    rods = m.plain("rods", "#5a4334")
    dead = m.plain("dead", "#6d6359")
    leaves = m.foliage("leaves", twig_folder, CUTOFF)
    height = 20.0 * rnd.uniform(0.95, 1.05)
    trunk_len = height * 0.9
    trunk = grow(Vector((0.0, 0.0, -SINK)), Vector((rnd.uniform(-0.04, 0.04), rnd.uniform(-0.04, 0.04), 1.0)), trunk_len + SINK, rnd,
                 26, lambda t: 0.0, 0.03)
    edge = edge_noise(rnd, height * 0.3, height * 0.45)

    def trunk_r(t):
        z = t * (trunk_len + SINK) - SINK
        return 0.03 + 0.17 * (1 - t) ** 1.1 + 0.06 * math.exp(-max(0.0, z) / 0.35)

    m.tube(trunk, [trunk_r(i / 26) for i in range(27)], 10, lambda z, a, s: dark if z < edge(a) else ginger)
    for _ in range(7):
        t = rnd.uniform(0.25, 0.55)
        p, _ = along(trunk, t)
        stub = grow(p, turn(UP, math.radians(rnd.uniform(70, 100)), rnd.uniform(0, 2 * math.pi)), rnd.uniform(0.2, 0.6), rnd, 2,
                    lambda s: -0.3, 0.2)
        m.tube(stub, [0.03, 0.02, 0.008], 4, dead)

    def tufts(branch, start, count, rel):
        for _ in range(count):
            p, d = along(branch, rnd.uniform(start, 1.0))
            for _ in range(2):
                spray = turn(d, math.radians(rnd.uniform(15, 45)), rnd.uniform(0, 2 * math.pi))
                twig_card(m, p, spray, leaves, rnd, PINE_TWIG, -0.25 - 0.25 * rel, face_up=1.6, outward=0.3, roll=35)

    z = height * 0.58
    while z < trunk_len - 0.3:
        rel = (z - height * 0.58) / (trunk_len - height * 0.58)
        if rnd.random() < 0.9:
            for _ in range(rnd.randint(3, 5)):
                spin = rnd.uniform(0, 2 * math.pi)
                outward = Vector((math.cos(spin), math.sin(spin), 0.0))
                start, _ = along(trunk, (z + SINK) / (trunk_len + SINK))
                tilt = math.radians(80 - 40 * rel + rnd.uniform(-10, 10))
                direction = UP * math.cos(tilt) + outward * math.sin(tilt)
                length = (3.4 * math.sin(math.pi * (0.25 + 0.6 * rel)) + 0.5) * rnd.uniform(0.75, 1.15)
                branch = grow(start, direction, length, rnd, 5, lambda s: 0.25 - 0.4 * s, 0.3)
                r0 = 0.035 + 0.04 * (1 - rel)
                m.tube(branch, [r0 + (0.012 - r0) * j / 5 for j in range(6)], 5, lambda zz, a, s: ginger if s < 0.3 else rods)
                tufts(branch, 0.4, int(length * 4) + 3, rel)
                for _ in range(rnd.randint(1, 2)):
                    s = rnd.uniform(0.35, 0.7)
                    p, d = along(branch, s)
                    fork_len = length * (1 - s) * rnd.uniform(0.6, 0.9)
                    fork = grow(p, turn(d, math.radians(rnd.uniform(30, 55)), rnd.uniform(0, 2 * math.pi)), fork_len, rnd, 3,
                                lambda u: 0.3, 0.3)
                    m.tube(fork, [0.018, 0.013, 0.009, 0.006], 4, rods)
                    tufts(fork, 0.3, int(fork_len * 4) + 2, rel)
        z += rnd.uniform(0.45, 0.75)
    tufts(trunk[-4:], 0.0, 4, 1.0)
    return save(m, out)


def save(m, out):
    """Сетка дерева в объект и в .glb `out`/<имя>.glb с затенением в цвете вершин; возвращает объект и числа."""
    obj = m.finish()
    pts = [obj.matrix_world @ Vector(c) for c in obj.bound_box]
    size = [round(max(p[i] for p in pts) - min(p[i] for p in pts), 2) for i in range(3)]
    tris = sum(len(p.vertices) - 2 for p in obj.data.polygons)
    os.makedirs(out, exist_ok=True)
    lib.export(os.path.join(out, m.name + ".glb"), vertex_color="ACTIVE")
    print("written", m.name, "size", size, "triangles", tris, "cards", len(m.cards) // 2)
    return obj, {"size": size, "triangles": tris, "cards": len(m.cards) // 2}


TREES = {
    "birch": lambda out: broadleaf(BIRCH, out),
    "linden": lambda out: broadleaf(LINDEN, out),
    "apple": lambda out: broadleaf(APPLE, out),
    "rowan": lambda out: broadleaf(ROWAN, out),
    "pine": lambda out: pine(out=out),
    "spruce": lambda out: spruce(out=out),
}


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = args[0] if args else OUT
    for name in args[1:] or TREES:
        TREES[name](out)


if __name__ == "__main__":
    main()
