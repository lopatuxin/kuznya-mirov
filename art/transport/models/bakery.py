# Кинешемский хлебокомбинат (Кинешма, 2-я Львовская ул., 25) со всей территорией по спутниковому снимку и
# фотографии: трёхэтажный корпус из силикатного кирпича с жёлтой пристройкой, цеха с вытяжками на крыше,
# кирпичная труба с котельной, экспедиция с навесом над рампой на северном дворе, арочный ангар, склад,
# сараи, бетонный забор с главными воротами со шлагбаумом под аркой газовой трубы, магазин «Каравай» со
# стоянкой за забором и вторые ворота рядом с ним.
# Стены — с проёмами: окна стоят в глубине стены, с рамами, отливами и перемычками. Материалы — карты
# Poly Haven из art/transport/textures в настоящем размере; затенение углов запечено в цвет вершин.
# Оси модели повёрнуты вдоль корпусов: X — на восток-юго-восток (28° по часовой от востока), Y — на
# северо-северо-восток. Начало — точка 57,447795 с. ш., 42,126127 в. д.; единица — метр. Где модель
# стоит на карте, куда заезжает машина и где встаёт под погрузку, записано в art/transport/city.json.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/bakery.py [-- <папка>]
import math
import os
import random
import sys

import bmesh
import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402
from lib import Model  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "..", "..", "..", "games", "transport", "models"))
TEXTURES = os.path.normpath(os.path.join(HERE, "..", "textures"))

GROUND = 0.06
FENCE_H = 2.4
PIPE = 0.18
WALL = 0.38  # толщина наружной стены
INSET = 0.14  # глубина рамы окна от плоскости стены
PLINTH = 0.55  # высота цоколя
GRID = 2.0  # шаг сетки по земле и крышам, по которой запекается затенение
GRIME = 1.4  # до этой высоты стены темнеют к земле

# Забор по кругу: от северо-восточного угла магазина на север к пристройке, вдоль корпуса — стены цехов,
# дальше на восток, вокруг склада, ангара, сараев и вдоль проезда на западе до северного угла магазина.
FENCE = [
    [(-27.5, -30.5), (-27.5, -28.2)],
    [(-27.5, -21.4), (-27.5, -19.2), (-26.0, -19.2)],
    [(23.0, -20.0), (56.0, -20.0), (56.0, 15.0), (47.0, 35.0)],
    [(47.0, 75.0), (47.0, 84.0), (-17.0, 84.0), (-17.0, 54.0), (-44.0, 54.0), (-44.0, 34.0)],
    [(-44.0, 26.0), (-44.0, -30.5), (-40.0, -30.5)],
]
MAIN_GATE = (-44.0, 26.0, 34.0)  # x забора и края проёма по y
SIDE_GATE = (-27.5, -28.2, -21.4)


def gltf_output():
    """Группа узлов, из которой выгрузка glTF берёт карту затенения материала."""
    group = bpy.data.node_groups.get("glTF Material Output")
    if group is None:
        group = bpy.data.node_groups.new("glTF Material Output", "ShaderNodeTree")
        group.interface.new_socket("Occlusion", in_out="INPUT", socket_type="NodeSocketFloat")
    return group


class Plant(Model):
    """Сетка территории: к коробкам библиотеки — материалы с картами, развёртка в метрах, косые стенки,
    арка ангара, круглая труба."""

    def __init__(self, name):
        super().__init__(name)
        self.uv = self.bm.loops.layers.uv.verify()
        self.tiles = {}
        self.manual = set()

    def tex(self, key, folder, meters, tint="#ffffff", swap=False):
        """Материал из карт папки `folder`: цвет (умноженный на `tint`), рельеф, шероховатость, металл и
        затенение; одна картинка покрывает `meters` × `meters`; `swap` — развёртка повёрнута на четверть."""
        if key in self.index:
            return self.index[key]
        m = bpy.data.materials.new(f"{self.name}_{key}")
        m.use_nodes = True
        nodes, links = m.node_tree.nodes, m.node_tree.links
        bsdf = nodes["Principled BSDF"]

        def image(file, data):
            node = nodes.new("ShaderNodeTexImage")
            node.image = bpy.data.images.load(os.path.join(TEXTURES, folder, file), check_existing=True)
            if data:
                node.image.colorspace_settings.name = "Non-Color"
            return node

        color = image("color.jpg", False)
        if tint.lower() == "#ffffff":
            links.new(color.outputs["Color"], bsdf.inputs["Base Color"])
        else:
            mix = nodes.new("ShaderNodeMix")
            mix.data_type = "RGBA"
            mix.blend_type = "MULTIPLY"
            mix.inputs[0].default_value = 1.0
            links.new(color.outputs["Color"], mix.inputs[6])
            mix.inputs[7].default_value = (*lib.hex_color(tint), 1.0)
            links.new(mix.outputs[2], bsdf.inputs["Base Color"])
        arm = image("arm.jpg", True)
        channels = nodes.new("ShaderNodeSeparateColor")
        links.new(arm.outputs["Color"], channels.inputs["Color"])
        links.new(channels.outputs["Green"], bsdf.inputs["Roughness"])
        links.new(channels.outputs["Blue"], bsdf.inputs["Metallic"])
        relief = image("normal.jpg", True)
        normal_map = nodes.new("ShaderNodeNormalMap")
        links.new(relief.outputs["Color"], normal_map.inputs["Color"])
        links.new(normal_map.outputs["Normal"], bsdf.inputs["Normal"])
        out = nodes.new("ShaderNodeGroup")
        out.node_tree = gltf_output()
        links.new(channels.outputs["Red"], out.inputs["Occlusion"])
        self.materials.append(m)
        self.index[key] = len(self.materials) - 1
        self.tiles[self.index[key]] = (meters, swap)
        return self.index[key]

    def slab(self, a, b, z0, z1, thick, mi, meters):
        """Стенка толщиной `thick` по отрезку a–b на плане: плиты забора; развёртка — вдоль отрезка."""
        ax, ay = a
        bx, by = b
        length = math.hypot(bx - ax, by - ay)
        dx, dy = (bx - ax) / length, (by - ay) / length
        nx, ny = -dy * thick / 2, dx * thick / 2
        base = [(ax + nx, ay + ny), (bx + nx, by + ny), (bx - nx, by - ny), (ax - nx, ay - ny)]
        bottom = [self.bm.verts.new((x, y, z0)) for x, y in base]
        top = [self.bm.verts.new((x, y, z1)) for x, y in base]
        faces = [self.bm.faces.new([bottom[3], bottom[2], bottom[1], bottom[0]]), self.bm.faces.new(top)]
        for i in range(4):
            j = (i + 1) % 4
            faces.append(self.bm.faces.new([bottom[i], bottom[j], top[j], top[i]]))
        for f in faces:
            f.material_index = mi
            f.normal_update()
            self.manual.add(f)
            for loop in f.loops:
                x, y, z = loop.vert.co
                along = (x - ax) * dx + (y - ay) * dy
                across = (x - ax) * -dy + (y - ay) * dx
                loop[self.uv].uv = (along / meters, (z if abs(f.normal.z) < 0.5 else across) / meters)

    def arch(self, x0, x1, y0, y1, height, mi_sheet, mi_end, meters, segments=24):
        """Арочный ангар вдоль Y: полуэллипс шириной x1−x0 и высотой `height`; листы `mi_sheet` развёрнуты
        вдоль дуги, торцы — стены `mi_end`."""
        cx, half = (x0 + x1) / 2, (x1 - x0) / 2
        profile = [(cx - half * math.cos(math.pi * i / segments), height * math.sin(math.pi * i / segments)) for i in range(segments + 1)]
        arc = [0.0]
        for (xa, za), (xb, zb) in zip(profile, profile[1:]):
            arc.append(arc[-1] + math.hypot(xb - xa, zb - za))
        south = [self.bm.verts.new((x, y0, z)) for x, z in profile]
        north = [self.bm.verts.new((x, y1, z)) for x, z in profile]
        for i in range(segments):
            f = self.bm.faces.new([south[i], south[i + 1], north[i + 1], north[i]])
            f.material_index = mi_sheet
            self.manual.add(f)
            for loop, (y, s) in zip(f.loops, ((y0, arc[i]), (y0, arc[i + 1]), (y1, arc[i + 1]), (y1, arc[i]))):
                loop[self.uv].uv = (y / meters, s / meters)
        self.bm.faces.new(list(reversed(south))).material_index = mi_end
        self.bm.faces.new(north).material_index = mi_end
        self.bm.faces.new([south[0], north[0], north[-1], south[-1]]).material_index = mi_end

    def tube(self, cx, cy, z0, r0, r1, h, mi, meters, segments=24):
        """Круглая кирпичная труба: развёртка по окружности, кирпич не растянут."""
        ring = [(math.cos(2 * math.pi * i / segments), math.sin(2 * math.pi * i / segments)) for i in range(segments + 1)]
        bottom = [self.bm.verts.new((cx + r0 * c, cy + r0 * s, z0)) for c, s in ring]
        top = [self.bm.verts.new((cx + r1 * c, cy + r1 * s, z0 + h)) for c, s in ring]
        for i in range(segments):
            f = self.bm.faces.new([bottom[i], bottom[i + 1], top[i + 1], top[i]])
            f.material_index = mi
            self.manual.add(f)
            for loop, (k, z, r) in zip(f.loops, ((i, z0, r0), (i + 1, z0, r0), (i + 1, z0 + h, r1), (i, z0 + h, r1))):
                loop[self.uv].uv = (2 * math.pi * r * k / segments / meters, z / meters)
        self.bm.faces.new(top[:-1]).material_index = mi

    def blot(self, cx, cy, rx, ry, mi, seed, z=GROUND + 0.02, n=20):
        """Пятно на земле с плавным неровным краем — лужа, нанос грязи, масло: тонкая призма, чтобы верх
        смотрел вверх при пересчёте нормалей."""
        rnd = random.Random(seed)
        f1, f2 = rnd.uniform(0, 6.3), rnd.uniform(0, 6.3)
        ring = []
        for i in range(n):
            a = 2 * math.pi * i / n
            k = 0.82 + 0.18 * math.sin(2 * a + f1) + 0.1 * math.sin(3 * a + f2) + 0.06 * rnd.random()
            ring.append((cx + rx * k * math.cos(a), cy + ry * k * math.sin(a)))
        top = [self.bm.verts.new((x, y, z)) for x, y in ring]
        bottom = [self.bm.verts.new((x, y, z - 0.012)) for x, y in ring]
        self.bm.faces.new(top).material_index = mi
        self.bm.faces.new(list(reversed(bottom))).material_index = mi
        for i in range(n):
            j = (i + 1) % n
            self.bm.faces.new([bottom[i], bottom[j], top[j], top[i]]).material_index = mi

    def pipe(self, points, mi):
        """Газовая труба квадратным сечением по ломаной из точек (x, y, z)."""
        h = PIPE / 2
        for (ax, ay, az), (bx, by, bz) in zip(points, points[1:]):
            self.box(min(ax, bx) - h, min(ay, by) - h, min(az, bz) - h, max(ax, bx) + h, max(ay, by) + h, max(az, bz) + h, mi)

    def project(self):
        """Развёртка граней без своей: по главной оси нормали, в метрах размера картинки материала."""
        for f in self.bm.faces:
            if f in self.manual:
                continue
            meters, swap = self.tiles.get(f.material_index, (2.0, False))
            f.normal_update()
            axis = max(range(3), key=lambda i: abs(f.normal[i]))
            for loop in f.loops:
                x, y, z = loop.vert.co
                u, v = (x, y) if axis == 2 else (y, z) if axis == 0 else (x, z)
                if swap:
                    u, v = v, u
                loop[self.uv].uv = (u / meters, v / meters)

    def grid(self):
        """Режет землю и крыши сеткой — затенение запекается в вершины, ему нужны вершины чаще, — а стены
        по высоте `GRIME`, до которой они темнеют к земле."""
        lo = [min(v.co[i] for v in self.bm.verts) for i in range(2)]
        hi = [max(v.co[i] for v in self.bm.verts) for i in range(2)]
        planes = []
        for axis in (0, 1):
            c = math.floor(lo[axis] / GRID) * GRID
            while c < hi[axis]:
                planes.append((axis, c + 0.013, True))
                c += GRID
        planes.append((2, GRIME, False))
        for axis, c, flat in planes:
            co, no = [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]
            co[axis], no[axis] = c, 1.0
            faces = []
            for f in self.bm.faces:
                f.normal_update()
                if (abs(f.normal.z) > 0.5) == flat:
                    faces.append(f)
            edges = list({e for f in faces for e in f.edges})
            verts = list({v for e in edges for v in e.verts})
            bmesh.ops.bisect_plane(self.bm, geom=verts + edges + faces, plane_co=co, plane_no=no)


# Стороны стены: коробка в осях стены — a вдоль стены, d вглубь от наружной плоскости (минус — наружу).
def sbox(m, side, coord, a0, a1, d0, d1, z0, z1, mi):
    if side == "front":
        m.box(a0, coord + d0, z0, a1, coord + d1, z1, mi)
    elif side == "back":
        m.box(a0, coord - d1, z0, a1, coord - d0, z1, mi)
    elif side == "left":
        m.box(coord + d0, a0, z0, coord + d1, a1, z1, mi)
    else:
        m.box(coord - d1, a0, z0, coord - d0, a1, z1, mi)


def span(start, stop, step):
    """Места проёмов от `start` до `stop` включительно через `step`."""
    n = int(round((stop - start) / step))
    return [start + step * i for i in range(n + 1)]


def row(places, z, w, h, kind):
    """Ряд проёмов: (середина вдоль стены, низ, ширина, высота, вид)."""
    return [(a, z, w, h, kind) for a in places]


class Kit:
    """Материалы фасадов и общий счётчик случайности окон."""

    def __init__(self, m):
        self.m = m
        self.rnd = random.Random(25)
        self.frame = m.mat("pvc", "#eeece6", 0.45)
        self.steel = m.mat("steel", "#3a4044", 0.5, 0.6)
        self.tin = m.mat("tin", "#a3a8ab", 0.4, 0.7)
        self.glass = [m.mat("glass", "#4f6070", 0.06, 0.65), m.mat("glass_dark", "#2c3842", 0.08, 0.5),
                      m.mat("curtain", "#c9bea6", 0.9), m.mat("blinds", "#a9adab", 0.7)]
        self.dirty_glass = m.mat("glass_factory", "#56605f", 0.25, 0.35)
        self.door = m.mat("door", "#5b3e2e", 0.55, 0.3)
        self.concrete = m.tex("concrete", "garage_floor", 1.89)
        self.shutter = m.tex("shutter", "painted_metal_shutter", 2.0)
        self.grille = m.mat("grille", "#2b2b2d", 0.5, 0.6)

    def pick_glass(self):
        return self.rnd.choices(self.glass, weights=(5, 3, 2, 1))[0]


def facade(kit, side, coord, a0, a1, z0, z1, wall, holes, plinth=None, corner=0.0):
    """Стена с проёмами: куски стены вокруг проёмов, ниже `PLINTH` — цоколь чуть наружу и на `corner` за края
    стены, чтобы закрыть угол здания; в проёмы — окна."""
    m = kit.m
    cuts = sorted({a0, a1, *(min(a1, max(a0, a + s * w / 2)) for a, _, w, _, _ in holes for s in (-1, 1))})
    strips = []
    for s0, s1 in zip(cuts, cuts[1:]):
        if s1 - s0 < 1e-6:
            continue
        cover = sorted((z, z + h) for a, z, w, h, _ in holes if a - w / 2 < s1 - 1e-6 and a + w / 2 > s0 + 1e-6)
        if strips and strips[-1][2] == cover:
            strips[-1][1] = s1
        else:
            strips.append([s0, s1, cover])
    for s0, s1, cover in strips:
        z = z0
        for h0, h1 in cover + [(z1, z1)]:
            if h0 > z + 1e-6:
                lo, hi = z, min(h0, z1)
                if plinth is not None and lo < PLINTH:
                    p0, p1 = s0 - (corner if s0 == a0 else 0.0), s1 + (corner if s1 == a1 else 0.0)
                    sbox(m, side, coord, p0, p1, -0.03, WALL, lo, min(hi, PLINTH), plinth)
                    lo = PLINTH
                if hi > lo:
                    sbox(m, side, coord, s0, s1, 0.0, WALL, lo, hi, wall)
            z = max(z, h1)
    for hole in holes:
        opening(kit, side, coord, hole)


def opening(kit, side, coord, hole):
    """Заполнение проёма: окно ПВХ, заводской переплёт, окно магазина с решёткой, дверь или рольставни."""
    m = kit.m
    a, zb, w, h, kind = hole
    a0, a1, zt = a - w / 2, a + w / 2, zb + h
    d, f = INSET, 0.07

    def bar(b0, b1, c0, c1, mi, depth=(d, d + f)):
        sbox(m, side, coord, b0, b1, depth[0], depth[1], c0, c1, mi)

    if kind in ("gate", "dock"):
        bar(a0, a1, zb, zt, kit.shutter, depth=(0.18, 0.24))
        bar(a0 - 0.1, a1 + 0.1, zt, zt + 0.4, kit.tin, depth=(-0.3, 0.0))
        return
    if kind == "door":
        bar(a0, a1, zb, zt, kit.door, depth=(0.12, 0.17))
        bar(a0, a0 + f, zb, zt, kit.steel, depth=(0.05, 0.12))
        bar(a1 - f, a1, zb, zt, kit.steel, depth=(0.05, 0.12))
        bar(a0, a1, zt - f, zt, kit.steel, depth=(0.05, 0.12))
        sbox(m, side, coord, a0 - 0.4, a1 + 0.4, -1.1, 0.0, zt + 0.25, zt + 0.33, kit.tin)
        return
    glass = kit.dirty_glass if kind == "factory" else kit.pick_glass()
    frame = kit.steel if kind == "factory" else kit.frame
    bar(a0, a1, zb, zt, glass, depth=(d + 0.03, d + 0.05))
    bar(a0, a0 + f, zb, zt, frame)
    bar(a1 - f, a1, zb, zt, frame)
    bar(a0, a1, zb, zb + f, frame)
    bar(a0, a1, zt - f, zt, frame)
    if kind == "factory":
        cols, rows = max(2, round(w / 0.75)), max(2, round(h / 0.65))
        for i in range(1, cols):
            u = a0 + w * i / cols
            bar(u - 0.025, u + 0.025, zb, zt, frame)
        for j in range(1, rows):
            z = zb + h * j / rows
            bar(a0, a1, z - 0.025, z + 0.025, frame)
    else:
        mid = a0 + w * 0.5
        bar(mid - f / 2, mid + f / 2, zb, zt, frame)
        tz = zb + h * 0.7
        bar(a0, mid, tz - f / 2, tz + f / 2, frame)
    if kind == "shop":
        for i in range(1, 5):
            u = a0 + w * i / 5
            bar(u - 0.015, u + 0.015, zb + 0.05, zt - 0.05, kit.grille, depth=(-0.04, -0.01))
        for z in (zb + h * 0.33, zb + h * 0.66, zt - 0.08):
            bar(a0 + 0.05, a1 - 0.05, z - 0.015, z + 0.015, kit.grille, depth=(-0.04, -0.01))
    sbox(m, side, coord, a0 - 0.05, a1 + 0.05, -0.08, d, zb - 0.03, zb + 0.025, kit.tin)
    if kind != "shop":
        sbox(m, side, coord, a0 - 0.2, a1 + 0.2, -0.015, 0.0, zt, zt + 0.22, kit.concrete)


def block(kit, x0, y0, x1, y1, h, wall, roof, sides, parapet=0.5, plinth=None, walls=None):
    """Здание с плоской крышей: стены с проёмами по сторонам `sides` (сторона — список проёмов; нет стороны —
    стену здесь ставит соседнее здание, и кровля доходит до неё), кровля внутри стен, парапет с крышкой; без
    парапета (`parapet=0`) — без крышки: такую стену накрывает двускатная крыша."""
    m = kit.m
    walls = walls or {}
    top = h + parapet
    ya = y0 + (WALL if "front" in sides else 0.0)
    yb = y1 - (WALL if "back" in sides else 0.0)
    spans = {"front": (y0, x0, x1), "back": (y1, x0, x1), "left": (x0, ya, yb), "right": (x1, ya, yb)}
    for side, holes in sides.items():
        coord, a0, a1 = spans[side]
        ends = side in ("front", "back")
        facade(kit, side, coord, a0, a1, GROUND, top, walls.get(side, wall), holes, plinth, corner=0.03 if ends else 0.0)
        if parapet > 0:
            sbox(m, side, coord, a0 - (0.04 if ends else 0.0), a1 + (0.04 if ends else 0.0), -0.04, WALL, top, top + 0.06, kit.tin)
    rx0, rx1 = x0 + (WALL if "left" in sides else 0.0), x1 - (WALL if "right" in sides else 0.0)
    m.box(rx0, ya, GROUND, rx1, yb, h, roof)


def ground_wear(m, patch, drift, puddle, oil):
    """Следы жизни на земле, чтобы двор и стоянка не были ровным листом: заплаты свежего асфальта, наносы
    земли у забора, стен и ворот ангара, лужи, масло у рампы. Колеи — потемнением `WEAR` в цвете вершин."""
    lift = GROUND + 0.015
    for x0, y0, x1, y1 in ((-30.0, 30.0, -26.2, 33.1), (-12.0, 34.2, -7.4, 36.6), (1.5, 27.0, 5.8, 29.6), (-40.6, 9.0, -37.4, 14.2),
                           (30.4, 22.0, 33.6, 26.3), (-20.5, 44.0, -16.8, 46.4), (8.0, 40.2, 12.9, 42.0), (30.2, -15.4, 33.0, -11.1)):
        m.box(x0, y0, GROUND, x1, y1, lift, patch)
    drifts = [(-43.0, y, 0.9, 4.5) for y in (-24.0, -12.0, 2.0, 40.0, 48.0)] + [(x, 53.2, 3.5, 0.7) for x in (-38.0, -26.0)]
    drifts += [(-7.6, 50.6, 4.2, 1.3), (24.8, 46.0, 0.8, 3.5), (24.8, 58.0, 0.8, 3.0), (40.0, -19.0, 5.0, 0.8),
               (-27.0, -46.0, 0.9, 6.0), (-26.6, -66.0, 0.8, 2.5), (10.0, -20.6, 4.0, 0.7)]
    for i, (x, y, rx, ry) in enumerate(drifts):
        m.blot(x, y, rx, ry, drift, 300 + i)
    for i, (x, y, rx, ry) in enumerate(((-5.0, 38.0, 2.2, 1.3), (-35.0, 30.0, 1.5, 1.0), (15.0, 45.0, 1.8, 1.1), (36.0, 10.0, 1.4, 0.9),
                                        (-12.0, -40.0, 2.6, 1.6), (-3.0, -58.0, 1.9, 1.2), (5.0, -30.0, 1.4, 0.9), (-20.0, -54.0, 1.2, 0.8))):
        m.blot(x, y, rx, ry, puddle, 400 + i, z=GROUND + 0.03)
    for i, x in enumerate((8.0, 11.6, 15.8)):
        m.blot(x, 27.2, 0.7, 0.45, oil, 500 + i, z=GROUND + 0.025)


def downpipe(kit, side, coord, a, top):
    """Водосточная труба по фасаду с воронкой наверху и коленом внизу."""
    sbox(kit.m, side, coord, a - 0.06, a + 0.06, -0.16, -0.04, 0.25, top - 0.3, kit.tin)
    sbox(kit.m, side, coord, a - 0.14, a + 0.14, -0.26, -0.02, top - 0.45, top - 0.15, kit.tin)
    sbox(kit.m, side, coord, a - 0.06, a + 0.06, -0.38, -0.04, 0.12, 0.25, kit.tin)


def cowl(m, x, y, z, mi):
    """Вытяжка на крыше цеха: труба с колпаком."""
    m.cylinder(x, y, z, 0.32, 1.1, mi, 10)
    m.cylinder(x, y, z + 1.1, 0.55, 0.12, mi, 10, r_top=0.2)


def mast(m, x, y, z, h, mi):
    """Антенная мачта на крыше: стойка, перекладины, тарелка."""
    m.cylinder(x, y, z, 0.05, h, mi, 6)
    for k in (0.55, 0.8):
        m.box(x - 0.6, y - 0.02, z + h * k, x + 0.6, y + 0.02, z + h * k + 0.04, mi)
    m.cylinder(x + 0.3, y + 0.25, z + 1.2, 0.45, 0.08, mi, 12)


def fence(m, panel, post):
    for line in FENCE:
        for k, (a, b) in enumerate(zip(line, line[1:])):
            m.slab(a, b, GROUND, FENCE_H, 0.16, panel, 8.0)
            length = math.hypot(b[0] - a[0], b[1] - a[1])
            n = max(1, round(length / 3.0))
            for i in range(0 if k == 0 else 1, n + 1):
                t = i / n
                x, y = a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t
                m.cbox(x, y, GROUND, 0.26, 0.26, FENCE_H + 0.15, post)


def gate(m, u, v0, v1, pipe, pier, boom_red, boom_white, post, raised, before, after):
    """Ворота в заборе вдоль Y: кирпичные столбы, шлагбаум (поднят или опущен) и арка газовой трубы; труба
    приходит по точкам `before` и уходит по точкам `after` — от стены до стены."""
    for v in (v0, v1):
        m.cbox(u, v, GROUND, 0.6, 0.6, 2.8, pier)
    m.pipe(before + [(u - 0.5, v0 - 0.5, 2.7), (u - 0.5, v0 - 0.5, 5.6), (u - 0.5, v1 + 0.5, 5.6), (u - 0.5, v1 + 0.5, 2.7)] + after, pipe)
    m.cbox(u + 0.7, v1 - 0.3, GROUND, 0.3, 0.3, 1.1, post)
    seg = (v1 - v0 - 0.6) / 8
    for i in range(8):
        mi = boom_red if i % 2 == 0 else boom_white
        if raised:
            m.box(u + 0.65, v1 - 0.35, 1.1 + seg * i, u + 0.75, v1 - 0.25, 1.1 + seg * (i + 1), mi)
        else:
            m.box(u + 0.65, v1 - 0.3 - seg * (i + 1), 0.95, u + 0.75, v1 - 0.3 - seg * i, 1.05, mi)


def build(out=OUT):
    lib.reset()
    m = Plant("bakery")
    kit = Kit(m)
    silicate = m.tex("silicate", "white_sandstone_bricks_03", 1.5, tint="#d8dde6")
    red_brick = m.tex("red_brick", "brick_wall_001", 3.0)
    plaster = m.tex("plaster", "white_stucco", 2.0, tint="#ffdd80")
    grey_plaster = m.tex("grey_plaster", "white_stucco", 2.0, tint="#d6d6d4")
    roofing = m.tex("roofing", "bitumen", 20.0)
    sheet_wall = m.tex("sheet_wall", "corrugated_iron_02", 2.7)
    sheet_roof = m.tex("sheet_roof", "corrugated_iron_02", 2.7, tint="#b8c4d4", swap=True)
    green_roof = m.tex("green_roof", "corrugated_iron_02", 2.7, tint="#8fc48a")
    red_roof = m.tex("red_roof", "corrugated_iron_02", 2.7, tint="#e08a78")
    rust = m.tex("rust", "rusty_corrugated_iron", 2.0)
    fence_panel = m.tex("fence", "concrete_panels", 8.0)
    asphalt = m.tex("asphalt", "Asphalt023S", 1.25, tint="#f4eee4")
    patch = m.tex("patch", "Asphalt024A", 2.0)
    dirt = m.tex("dirt", "Ground103", 2.5, tint="#dcdad6")
    drift = m.tex("drift", "Ground103", 2.5, tint="#c9c4bc")
    puddle = m.mat("puddle", "#4a4c48", 0.05, 0.45)
    oil = m.mat("oil", "#1f2124", 0.3, 0.3)
    concrete = kit.concrete
    tiles = m.mat("tiles", "#3d6a58", 0.35)
    vent = m.mat("vent", "#8d9194", 0.45, 0.6)
    soot = m.mat("soot", "#2f2925", 0.95)
    pier = red_brick
    gas = m.mat("gas", "#d6bf6a", 0.5, 0.2)
    boom_red = m.mat("boom_red", "#c8322b", 0.6)
    boom_white = m.mat("boom_white", "#f2f0ea", 0.6)
    sign = m.mat("sign", "#ecc3cf", 0.6)
    container = m.mat("container", "#3a6fb0", 0.6, 0.3)
    ac = m.mat("ac", "#e8e8e4", 0.5)

    # Земля: асфальт двора, прохода у корпуса и двора у трубы, грунт стоянки у магазина.
    m.box(-44.0, 20.3, 0, 24.0, 52.0, GROUND, asphalt)
    m.box(-44.0, 52.0, 0, -16.0, 54.0, GROUND, asphalt)
    m.box(-44.0, -30.5, 0, -32.6, 20.3, GROUND, asphalt)
    m.box(-32.6, -30.5, 0, -27.5, -14.5, GROUND, asphalt)
    m.box(-27.5, -19.2, 0, -26.0, -14.5, GROUND, asphalt)
    m.box(23.0, -20.0, 0, 47.0, 20.3, GROUND, asphalt)
    m.box(24.0, 20.3, 0, 47.0, 35.0, GROUND, asphalt)
    m.box(-27.5, -48.0, 0, 16.0, -19.2, GROUND, dirt)
    m.box(-27.5, -68.0, 0, -2.0, -48.0, GROUND, dirt)

    # Главный корпус: три этажа силикатного кирпича, окна ПВХ, плоская крыша с парапетом.
    pvc = (1.0, 4.6, 8.2)
    yard = span(-30.8, -13.8, 3.4)
    block(kit, -32.6, -14.5, -12.0, 21.0, 11.4, silicate, roofing, {
        "back": row(yard, 4.6, 1.5, 1.6, "pvc") + row(yard, 8.2, 1.5, 1.6, "pvc") + row(yard[:2] + yard[3:], 1.0, 1.5, 1.6, "pvc")
                + [(yard[2], GROUND, 1.3, 2.2, "door")],
        "left": [hole for z in pvc for hole in row(span(-12.0, 18.6, 3.4), z, 1.5, 1.6, "pvc")],
        "front": [hole for z in pvc for hole in row((-30.8, -28.2), z, 1.5, 1.6, "pvc")] + row(span(-24.2, -14.0, 3.4), 8.2, 1.5, 1.6, "pvc"),
        "right": row(span(-12.6, -2.4, 3.4), 8.2, 1.5, 1.6, "pvc"),
    }, parapet=0.6, plinth=concrete)
    for side, coord, a in (("back", 21.0, -32.2), ("back", 21.0, -22.3), ("left", -32.6, -14.1), ("left", -32.6, 3.2), ("left", -32.6, 20.6)):
        downpipe(kit, side, coord, a, 12.0)
    for x, z in ((-27.4, 3.75), (-17.2, 7.35), (-30.8, 3.75)):
        m.box(x - 0.45, 21.0, z, x + 0.45, 21.3, z + 0.6, ac)
    mast(m, -16.0, 16.0, 11.4, 6.0, vent)
    mast(m, -28.0, -10.0, 11.4, 4.5, vent)
    m.box(-30.0, 14.0, 11.4, -27.6, 16.6, 13.6, silicate)
    m.box(-30.1, 13.9, 13.6, -27.5, 16.7, 13.7, kit.tin)
    for x, y in ((-20.0, 4.0), (-25.0, -6.0), (-17.0, -12.0)):
        m.cbox(x, y, 11.4, 0.6, 0.6, 0.8, vent)

    # Жёлтая двухэтажная пристройка перед корпусом — её видно за воротами у магазина.
    block(kit, -26.0, -19.2, -12.0, -14.5, 7.0, plaster, roofing, {
        "front": row((-24.4, -21.8, -16.2, -13.6), 1.0, 1.3, 1.5, "pvc") + row((-24.4, -21.8, -19.0, -16.2, -13.6), 4.2, 1.3, 1.5, "pvc")
                 + [(-19.0, GROUND, 1.4, 2.3, "door")],
        "left": row((-16.85,), 1.0, 1.2, 1.5, "pvc") + row((-16.85,), 4.2, 1.2, 1.5, "pvc"),
    }, parapet=0.4, plinth=concrete)
    downpipe(kit, "front", -19.2, -25.7, 7.4)

    # Цеха: северный двухэтажный и южный пониже — заводские переплёты, ворота экспедиции, вытяжки.
    block(kit, -12.0, 0.0, 24.5, 20.3, 8.0, silicate, roofing, {
        "back": row(span(-9.5, 22.0, 3.5), 5.4, 2.6, 2.0, "factory") + row((7.0, 11.5, 16.0), 1.1, 2.6, 2.8, "dock"),
        "right": row(span(2.5, 17.5, 3.75), 5.2, 2.4, 2.0, "factory") + row((11.5, 15.5), 1.4, 2.4, 2.4, "factory"),
        "front": [],
    }, parapet=0.5, plinth=concrete)
    block(kit, -12.0, -20.0, 23.0, 0.0, 6.5, silicate, roofing, {
        "front": row(span(1.0, 21.0, 4.0), 1.5, 2.6, 2.8, "factory") + row((-9.5, -3.5), 3.8, 1.4, 1.4, "pvc")
                 + [(-6.5, 1.0, 3.0, 2.6, "dock")],
        "right": row((-17.5, -13.5, -9.5, -2.5), 1.5, 2.4, 2.8, "factory"),
        "left": [],
    }, parapet=0.5, plinth=concrete)
    for side, coord, a, top in (("front", -20.0, -0.5, 7.0), ("front", -20.0, 11.0, 7.0), ("front", -20.0, 22.6, 7.0), ("right", 24.5, 19.8, 8.5)):
        downpipe(kit, side, coord, a, top)
    for x, y in ((-6.0, 12.0), (1.0, 8.0), (8.0, 13.0), (15.0, 9.0), (20.0, 15.0), (-2.0, 4.0)):
        cowl(m, x, y, 8.0, vent)
    for x, y in ((-4.0, -9.0), (3.0, -14.0), (10.0, -6.0), (16.0, -12.0), (20.0, -4.0)):
        cowl(m, x, y, 6.5, vent)
    m.box(-11.0, -21.2, GROUND, -2.0, -20.0, 1.0, concrete)
    m.box(-11.2, -21.6, 3.6, -1.8, -20.0, 3.72, sheet_roof)
    for x in (-10.9, -2.1):
        m.cbox(x, -21.45, GROUND, 0.14, 0.14, 3.6, kit.steel)

    # Экспедиция: пристройка и рампа под навесом на северном дворе, отбойники у рампы.
    block(kit, -12.0, 20.3, 4.5, 24.0, 4.6, silicate, roofing, {
        "back": row((-10.0, -7.0, -4.0, 2.4), 1.6, 1.4, 1.2, "pvc") + [(-1.0, GROUND, 1.2, 2.2, "door")],
        "left": [],
        "right": [],
    }, parapet=0.3, plinth=concrete)
    m.box(4.5, 20.3, GROUND, 18.5, 23.8, 1.1, concrete)
    m.box(18.5, 20.8, GROUND, 20.0, 23.8, 0.55, concrete)
    m.box(4.4, 23.8, 0.95, 18.6, 23.86, 1.12, kit.tin)
    for x in (6.0, 9.0, 13.0, 17.0):
        m.box(x - 0.2, 23.8, 0.35, x + 0.2, 24.0, 0.95, soot)
    m.box(4.3, 20.3, 4.3, 18.7, 24.6, 4.42, sheet_roof)
    m.box(4.3, 24.5, 4.0, 18.7, 24.6, 4.3, kit.steel)
    for x in (4.6, 11.5, 18.4):
        m.cbox(x, 24.4, GROUND, 0.16, 0.16, 4.3, kit.steel)

    # Котельная из красного кирпича и труба с поясами; сажа у оголовка.
    block(kit, 24.5, 1.0, 31.0, 9.0, 4.2, red_brick, roofing, {
        "front": [(27.6, GROUND, 1.2, 2.2, "door")],
        "back": row((27.7,), 1.6, 1.2, 1.2, "factory"),
        "right": row((3.0, 7.0), 1.6, 1.2, 1.2, "factory"),
    }, parapet=0.3, plinth=concrete)
    m.box(25.5, -6.5, GROUND, 28.5, -3.5, 1.2, red_brick)
    m.tube(27.0, -5.0, 1.2, 1.2, 0.78, 24.0, red_brick, 3.0)
    m.cylinder(27.0, -5.0, 25.2, 0.8, 0.8, soot, 24, r_top=0.76)
    for z in (6.0, 11.0, 16.0, 21.0):
        r = 1.2 - 0.42 * (z - 1.2) / 24.0 + 0.04
        m.cylinder(27.0, -5.0, z, r, 0.18, kit.steel, 24)

    # Арочный ангар за двором: ржавый профлист по дуге, торцы — серый профлист, ворота к двору.
    m.arch(-16.0, 0.7, 52.0, 82.0, 6.5, rust, sheet_wall, 2.0)
    m.box(-10.6, 51.9, GROUND, -4.6, 52.0, 4.6, kit.shutter)

    # Сараи у северо-западного угла двора.
    block(kit, -41.0, 42.0, -33.0, 50.0, 2.9, grey_plaster, sheet_wall, {
        "front": [(-37.0, GROUND, 1.2, 2.1, "door")], "back": [], "left": [], "right": [],
    }, parapet=0.15)
    block(kit, -31.0, 44.0, -25.0, 50.0, 2.7, grey_plaster, red_roof, {
        "front": [(-28.0, GROUND, 2.6, 2.4, "gate")], "back": [], "left": [], "right": [],
    }, parapet=0.15)

    # Склад к востоку от двора: ворота с рольставнями на двор, северо-восточный угол пониже.
    block(kit, 24.0, 35.0, 47.0, 68.0, 6.5, silicate, roofing, {
        "left": row((42.0, 52.0, 62.0), GROUND, 3.6, 3.9, "gate") + row((38.5, 47.0, 57.0, 65.5), 4.6, 2.4, 1.2, "factory"),
        "front": row((28.0, 33.0, 38.0, 43.0), 3.8, 2.4, 1.6, "factory"),
        "right": row((40.0, 46.0, 52.0, 58.0, 64.0), 3.8, 2.4, 1.6, "factory"),
        "back": [],
    }, parapet=0.4, plinth=concrete)
    block(kit, 24.0, 68.0, 37.0, 75.0, 6.5, silicate, roofing, {
        "left": row((71.5,), 3.8, 2.4, 1.6, "factory"), "back": row((27.0, 33.0), 3.8, 2.4, 1.6, "factory"), "right": [],
    }, parapet=0.4, plinth=concrete)
    block(kit, 37.0, 68.0, 47.0, 75.0, 5.0, silicate, sheet_wall, {
        "right": row((71.5,), 2.8, 2.4, 1.4, "factory"), "back": row((42.0,), 2.8, 2.4, 1.4, "factory"),
    }, parapet=0.3, plinth=concrete)
    for side, coord, a, top in (("left", 24.0, 35.6, 6.9), ("left", 24.0, 44.8, 6.9), ("left", 24.0, 74.6, 6.9)):
        downpipe(kit, side, coord, a, top)
    for x, y in ((30.0, 45.0), (40.0, 58.0)):
        m.cbox(x, y, 6.5, 0.9, 0.9, 0.9, vent)

    # Сарай с зелёной крышей за трубой.
    block(kit, 34.0, 0.0, 53.0, 13.0, 4.2, grey_plaster, roofing, {
        "front": [(43.0, GROUND, 3.2, 3.0, "gate")], "back": [], "left": [], "right": row((6.5,), 1.6, 1.2, 1.0, "factory"),
    }, parapet=0.0)
    m.gable(34.0, 0.0, 53.0, 13.0, 4.28, 5.8, green_roof, grey_plaster, over=0.3, along="x")
    m.box(34.0, 0.0, 4.2, 34.0 + WALL, 13.0, 4.28, grey_plaster)
    m.box(53.0 - WALL, 0.0, 4.2, 53.0, 13.0, 4.28, grey_plaster)

    # Магазин «Каравай» за забором: жёлтая штукатурка на зелёном кафельном цоколе, окна с решётками, задняя
    # стена — серый кирпич; южная часть с плоской крышей и вывеской, северная — под двускатной железной крышей.
    shop_south = row((-72.0, -69.0, -66.0, -58.5, -55.5, -52.5, -49.5), 0.9, 1.3, 1.5, "shop") + [(-62.2, GROUND, 1.6, 2.3, "door")]
    block(kit, -40.0, -74.5, -27.5, -45.5, 4.4, plaster, roofing, {
        "right": shop_south, "left": [], "front": [],
    }, parapet=0.4, plinth=tiles, walls={"left": silicate})
    block(kit, -40.0, -45.5, -27.5, -30.5, 3.8, plaster, plaster, {
        "right": row((-43.0, -39.5, -36.0), 0.9, 1.2, 1.4, "shop") + [(-33.0, GROUND, 1.1, 2.2, "door")],
        "left": [], "back": [],
    }, parapet=0.0, plinth=tiles, walls={"left": silicate})
    m.gable(-40.0, -45.5, -27.5, -30.5, 3.88, 6.0, sheet_roof, plaster, over=0.4, along="y")
    m.box(-40.0, -30.5 - WALL, 3.8, -27.5, -30.5, 3.88, plaster)
    m.box(-27.48, -67.0, 3.3, -27.3, -57.0, 5.6, sign)
    m.box(-27.45, -67.15, 3.15, -27.32, -56.85, 5.75, ac)
    for y, top in ((-74.2, 4.8), (-46.4, 4.8), (-30.8, 3.75)):
        downpipe(kit, "right", -27.5, y, top)

    # Ворота у магазина, как на фотографии: опущенный шлагбаум под аркой трубы; главные — с проезда на
    # северный двор, шлагбаум поднят.
    gate(m, SIDE_GATE[0], SIDE_GATE[1], SIDE_GATE[2], gas, pier, boom_red, boom_white, concrete, raised=False,
         before=[(-28.0, -30.5, 2.7)], after=[(-28.0, -14.5, 2.7)])
    gate(m, MAIN_GATE[0], MAIN_GATE[1], MAIN_GATE[2], gas, pier, boom_red, boom_white, concrete, raised=True,
         before=[(-32.6, 18.0, 2.7), (-44.5, 18.0, 2.7)], after=[(-44.5, 46.0, 2.7), (-41.0, 46.0, 2.7)])
    for x in (-36.0, -40.0):
        m.cbox(x, 18.0, GROUND, 0.1, 0.1, 2.6, kit.steel)
    fence(m, fence_panel, concrete)

    # Синий контейнер на стоянке.
    m.box(1.4, -39.8, GROUND, 3.2, -38.2, 1.4, container)
    ground_wear(m, patch, drift, puddle, oil)

    m.project()
    m.grid()
    obj = m.finish()
    bake_occlusion(obj)
    lib.text(obj, "КАРАВАЙ", 1.05, (-27.2, -62.0, 4.45), math.pi / 2, "#9c2445", depth=0.06)
    report(obj)
    os.makedirs(out, exist_ok=True)
    lib.export(os.path.join(out, "bakery.glb"), vertex_color="ACTIVE")
    print("written bakery")


def lattice(ix, iy, seed):
    """Случайное число узла сетки шума от 0 до 1 — одно и то же для одного узла."""
    h = (ix * 374761393 + iy * 668265263 + seed * 2147483647) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    return (h ^ (h >> 16)) / 0xFFFFFFFF


def noise(x, y, seed):
    """Плавный шум от 0 до 1: узлы сетки с шагом 1, между ними — гладко."""
    ix, iy = math.floor(x), math.floor(y)
    fx, fy = x - ix, y - iy
    sx, sy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    top = lattice(ix, iy, seed) * (1 - sx) + lattice(ix + 1, iy, seed) * sx
    bottom = lattice(ix, iy + 1, seed) * (1 - sx) + lattice(ix + 1, iy + 1, seed) * sx
    return top * (1 - sy) + bottom * sy


# Где ездят: от въезда со стороны Менделеева к магазину и к рампе южного цеха, во дворе — от ворот
# к экспедиции, складу и ангару. Вдоль этих линий земля темнее, как протёртая колёсами.
WEAR = [
    [(-1.0, -67.0), (-6.0, -58.0), (-14.0, -52.0), (-21.0, -50.0), (-23.0, -42.0), (-21.0, -33.0)],
    [(-14.0, -52.0), (-12.0, -40.0), (-7.0, -28.0), (-6.5, -21.5)],
    [(-46.0, 30.0), (-30.0, 30.0), (0.0, 29.0), (13.0, 28.6)],
    [(5.0, 30.0), (15.0, 38.0), (22.0, 43.0)],
    [(-10.0, 30.5), (-8.5, 43.0), (-7.6, 51.5)],
]


def worn(x, y):
    """Насколько точка протёрта колёсами: 1 на линии `WEAR`, к 2–3 м в сторону — 0."""
    best = 1e9
    for line in WEAR:
        for (ax, ay), (bx, by) in zip(line, line[1:]):
            dx, dy = bx - ax, by - ay
            t = max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)))
            best = min(best, math.hypot(x - ax - dx * t, y - ay - dy * t))
    return math.exp(-(best / 1.6) ** 2)


def macro(x, y):
    """Крупные светлые и тёмные пятна земли 4–15 м — гасят повтор картинки асфальта и грунта — и колеи."""
    spots = 0.84 + 0.2 * (0.65 * noise(x / 10.0, y / 10.0, 1) + 0.35 * noise(x / 4.0, y / 4.0, 2))
    return spots * (1.0 - 0.22 * worn(x, y))


def bake_occlusion(obj):
    """Затенение в цвет вершин. Земле, крышам и потолкам навесов — запечённое Cycles: у стен, в углах, под
    навесами темнее; земле сверх того — крупные пятна `macro`. Стенам — ровная грязь к земле до `GRIME`:
    запечённое легло бы пятнами от углов откосов."""
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 96
    world = bpy.data.worlds.new("bake")
    world.light_settings.distance = 3.0
    scene.world = world
    stains = split_off(obj, [m for m in obj.data.materials if m.name.split("_", 1)[1] in STAINS])
    stains.hide_render = True
    bpy.ops.mesh.primitive_plane_add(size=600.0, location=(0.0, 0.0, -0.002))
    ground = bpy.context.active_object
    attr = obj.data.color_attributes.new("occlusion", "BYTE_COLOR", "CORNER")
    obj.data.color_attributes.active_color = attr
    obj.data.color_attributes.render_color_index = obj.data.color_attributes.active_color_index
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    scene.render.bake.target = "VERTEX_COLORS"
    bpy.ops.object.bake(type="AO")
    mesh = obj.data
    for poly in mesh.polygons:
        flat = abs(poly.normal.z) > 0.5
        for li in poly.loop_indices:
            co = mesh.vertices[mesh.loops[li].vertex_index].co
            if flat:
                v = 0.45 + 0.55 * attr.data[li].color[0]
                if co.z < 0.2:
                    v *= macro(co.x, co.y)
            else:
                t = min(1.0, max(0.0, co.z / GRIME))
                v = 0.72 + 0.28 * t * t * (3 - 2 * t)
            attr.data[li].color = (v, v, v, 1.0)
    bpy.data.objects.remove(ground)
    paint = stains.data.color_attributes.new("occlusion", "BYTE_COLOR", "CORNER")
    for li, loop in enumerate(stains.data.loops):
        co = stains.data.vertices[loop.vertex_index].co
        v = 0.95 * macro(co.x, co.y)
        paint.data[li].color = (v, v, v, 1.0)
    stains.hide_render = False
    bpy.ops.object.select_all(action="DESELECT")
    stains.select_set(True)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.join()


STAINS = ("patch", "drift", "puddle", "oil")


def split_off(obj, materials):
    """Отделяет от модели грани этих материалов в свой объект: пятна на земле не должны бросать тень при
    запекании затенения — они лежат на сантиметр выше земли, и тень от этой ступеньки легла бы ореолом."""
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="DESELECT")
    for mat in materials:
        obj.active_material_index = obj.data.materials.find(mat.name)
        bpy.ops.object.material_slot_select()
    bpy.ops.mesh.separate(type="SELECTED")
    bpy.ops.object.mode_set(mode="OBJECT")
    return next(o for o in bpy.context.selected_objects if o is not obj)


def report(obj):
    """Коробка модели в её осях: середина и размеры нужны art/transport/city.json."""
    pts = [obj.matrix_world @ Vector(c) for c in obj.bound_box]
    for child in obj.children:
        pts += [child.matrix_world @ Vector(c) for c in child.bound_box]
    lo = [min(p[i] for p in pts) for i in range(3)]
    hi = [max(p[i] for p in pts) for i in range(3)]
    print("bakery center", round((lo[0] + hi[0]) / 2, 3), round((lo[1] + hi[1]) / 2, 3),
          "size", round(hi[0] - lo[0], 3), round(hi[1] - lo[1], 3), "height", round(hi[2] - lo[2], 3))


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    build(args[0] if args else OUT)


if __name__ == "__main__":
    main()
