"""Общее для нарисованных моделей Орды: объём из граней, снимок камерой игры, проекция картины на объём, выгрузка.

Модель строится скриптом в своих осях вокруг начала координат, единица — клетка игры (герой 1,73 клетки). Цвет
частей — плоские материалы по роду (дерево, жесть, камень…): снимок с ними — основа для раскраски нейросетью.
Раскрашенная картина ложится обратно на тот же объём проекцией с той же камеры, а прозрачность берёт из силуэта
самого снимка: рисунок и объём совпадают по построению.

Камера игры ортографическая, наклон 30°: точка модели видна на снимке по её проекциям на ось «вправо» и ось
«вверх» камеры. Кадр — квадрат `size` клеток со стороной `px` точек, его середина на высоте `up` по оси «вверх».
Сдвиг модели по сцене картину не меняет, поворот меняет: другой поворот — другая картина.
"""
import math
import os

import bmesh
import bpy
import numpy
from mathutils import Matrix, Vector

PITCH = math.radians(30)
RIGHT = Vector((1.0, 0.0, 0.0))
UP = Vector((0.0, math.sin(PITCH), math.cos(PITCH)))
FORWARD = Vector((0.0, math.cos(PITCH), -math.sin(PITCH)))
ROOT = r"C:\projects\kuznya-mirov"
MODELS = os.path.join(ROOT, "games", "horde", "models")


class Part:
    """Набор граней по роду материала, в осях модели."""

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


def loft(part, kind, rings):
    """Тело по сечениям: кольца точек одинаковой длины, обход против часовой стрелки, если смотреть от последнего
    кольца к первому; торцы закрыты.

    `kind` — род всех граней или функция (промежуток между кольцами, ребро кольца) → род; торцы — промежутки -1 и
    len(rings) - 1, ребро -1."""
    of = kind if callable(kind) else (lambda segment, edge: kind)
    count = len(rings[0])
    part.add(of(-1, -1), [list(reversed(rings[0]))])
    part.add(of(len(rings) - 1, -1), [list(rings[-1])])
    for segment, (a, b) in enumerate(zip(rings, rings[1:])):
        for i in range(count):
            j = (i + 1) % count
            part.add(of(segment, i), [[a[i], a[j], b[j], b[i]]])


def ring(center, radius, sides, z, turn=0.0, jitter=None):
    """Кольцо точек вокруг вертикали на высоте z, против часовой стрелки сверху; `jitter` — множители радиуса по
    точкам, для неровных тел."""
    x, y = center
    return [(x + radius * (jitter[k] if jitter else 1.0) * math.cos(turn + math.tau * k / sides),
             y + radius * (jitter[k] if jitter else 1.0) * math.sin(turn + math.tau * k / sides), z)
            for k in range(sides)]


def cylinder(part, kind, center, radius, height, sides=16, top_radius=None):
    """Стоячий цилиндр или усечённый конус: низ в center, верх на height выше."""
    x, y, z = center
    loft(part, kind, [ring((x, y), radius, sides, z), ring((x, y), top_radius or radius, sides, z + height)])


def turned(part, angle, axis="Z", pivot=(0.0, 0.0, 0.0)):
    """Те же грани, повёрнутые на `angle` вокруг оси через точку `pivot`: часть, собранная прямо, ложится наклонно."""
    rotation = Matrix.Rotation(angle, 3, axis)
    origin = Vector(pivot)
    out = Part()
    for kind, faces in part.faces.items():
        out.add(kind, [[tuple(origin + rotation @ (Vector(p) - origin)) for p in face] for face in faces])
    return out


def merge(part, other):
    for kind, faces in other.faces.items():
        part.add(kind, faces)


def flat_material(prefix, kind, color):
    name = f"{prefix}_{kind}"
    made = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    made.diffuse_color = (*color, 1.0)
    return made


def make_object(name, part, collection, colors, turn=0.0, prefix=None):
    """Объект из граней части, повёрнутый на `turn` вокруг вертикали, с плоскими цветами `colors` по роду."""
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    rotation = Matrix.Rotation(turn, 3, "Z")
    kinds = list(part.faces)
    for index, kind in enumerate(kinds):
        for points in part.faces[kind]:
            face = bm.faces.new([bm.verts.new(rotation @ Vector(p)) for p in points])
            face.material_index = index
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bm.to_mesh(mesh)
    bm.free()
    for kind in kinds:
        mesh.materials.append(flat_material(prefix or name, kind, colors[kind]))
    obj = bpy.data.objects.new(name, mesh)
    collection.objects.link(obj)
    return obj


def fresh_collection(name):
    """Пустая коллекция `name` в текущей сцене: прежняя с её объектами удаляется."""
    old = bpy.data.collections.get(name)
    if old:
        for obj in list(old.objects):
            bpy.data.objects.remove(obj, do_unlink=True)
        bpy.data.collections.remove(old)
    made = bpy.data.collections.new(name)
    bpy.context.scene.collection.children.link(made)
    return made


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


def merge_cut(outside, cut, mask_outside, mask_cut, covered, margin):
    """Раскраска разреза здания: из своей — только то, чем разрез отличается от здания снаружи (по снимкам объёма) и
    что снаружи заслоняли срезанные стены (`covered`), с запасом `margin` точек; остальное — из раскраски снаружи,
    чтобы при смене вида опоры и стены, оставшиеся на месте, не меняли рисунок."""
    changed = numpy.abs(mask_cut - mask_outside).max(axis=2) > 0.02
    own = grow(changed | covered, margin)
    merged = outside.copy()
    merged[own] = cut[own]
    return merged


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


class Frame:
    """Кадр камеры игры над моделью и папка его снимков, раскрасок и масок."""

    def __init__(self, size, up, px, folder):
        self.size, self.up, self.px, self.folder = size, up, px, folder

    @staticmethod
    def around(names, folder, px=1024, margin=1.15):
        """Кадр с серединой в начале осей по ширине, который вмещает объекты `names` с запасом `margin`."""
        points = [obj.matrix_world @ v.co for name in names for obj in [bpy.data.objects[name]] for v in obj.data.vertices]
        across = max(abs(p.dot(RIGHT)) for p in points)
        ups = [p.dot(UP) for p in points]
        size = max(2 * across, max(ups) - min(ups)) * margin
        return Frame(size, (max(ups) + min(ups)) / 2, px, folder)

    def uv(self, point):
        """Точка картины, куда видна точка модели: доли кадра по ширине и снизу вверх."""
        return (point.dot(RIGHT) / self.size + 0.5, (point.dot(UP) - self.up) / self.size + 0.5)

    def camera(self):
        name = "guide_cam_" + os.path.basename(self.folder)
        data = bpy.data.cameras.get(name) or bpy.data.cameras.new(name)
        data.type, data.sensor_fit, data.ortho_scale = "ORTHO", "VERTICAL", self.size
        cam = bpy.data.objects.get(name) or bpy.data.objects.new(name, data)
        if cam.name not in bpy.context.scene.collection.objects:
            bpy.context.scene.collection.objects.link(cam)
        cam.location = UP * self.up - FORWARD * 60
        cam.rotation_euler = (math.pi / 2 - PITCH, 0.0, 0.0)
        return cam

    def render(self, shots):
        """Снимки объёма на прозрачном фоне: для каждой пары (файл, имена объектов) видны только эти объекты."""
        scene = bpy.context.scene
        scene.camera = self.camera()
        scene.render.engine = "BLENDER_WORKBENCH"
        scene.render.resolution_x = scene.render.resolution_y = self.px
        scene.render.film_transparent = True
        shading = scene.display.shading
        shading.light, shading.color_type = "STUDIO", "MATERIAL"
        shading.show_cavity = shading.show_object_outline = True
        shown = {obj: obj.hide_render for obj in scene.objects}
        for file, names in shots:
            for obj in scene.objects:
                obj.hide_render = obj.name not in names
            scene.render.filepath = os.path.join(self.folder, file)
            bpy.ops.render.render(write_still=True)
        for obj, hidden in shown.items():
            obj.hide_render = hidden

    def load(self, file):
        image = bpy.data.images.load(os.path.join(self.folder, file))
        assert tuple(image.size) == (self.px, self.px), f"{file}: не {self.px} точек"
        pixels = numpy.array(image.pixels[:], dtype=numpy.float32).reshape(self.px, self.px, 4)
        bpy.data.images.remove(image)
        return pixels

    def save(self, pixels, file):
        image = bpy.data.images.new(file, self.px, self.px, alpha=True)
        image.pixels = pixels.ravel()
        image.filepath_raw = os.path.join(self.folder, file)
        image.file_format = "PNG"
        image.save()
        bpy.data.images.remove(image)

    def fit(self, raw, guide, out):
        """Раскраска нейросети в кадр снимка: нейросеть отдаёт картинку своего размера и чуть в стороне, поэтому
        рамка нарисованного — всё, что не цвета фона в углу картинки, — растягивается в рамку силуэта снимка."""
        image = bpy.data.images.load(os.path.join(self.folder, raw))
        width, height = image.size
        pixels = numpy.array(image.pixels[:], dtype=numpy.float32).reshape(height, width, 4)
        bpy.data.images.remove(image)
        border = numpy.concatenate([pixels[0], pixels[-1], pixels[:, 0], pixels[:, -1]])[:, :3]
        background = numpy.median(border, axis=0)
        drawn = numpy.abs(pixels[:, :, :3] - background).sum(axis=2) > 0.15
        # Фон нейросети не совсем ровный: строка или столбец считаются нарисованными, когда в них не одна точка.
        rows = numpy.nonzero(drawn.sum(axis=1) > 3)[0]
        cols = numpy.nonzero(drawn.sum(axis=0) > 3)[0]
        crop = pixels[rows.min():rows.max() + 1, cols.min():cols.max() + 1].copy()
        rows, cols = numpy.nonzero(self.load(guide)[:, :, 3] > 0.5)
        size = (cols.max() - cols.min() + 1, rows.max() - rows.min() + 1)
        piece = bpy.data.images.new("fit_piece", crop.shape[1], crop.shape[0], alpha=True)
        piece.pixels = crop.ravel()
        piece.scale(*size)
        scaled = numpy.array(piece.pixels[:], dtype=numpy.float32).reshape(size[1], size[0], 4)
        bpy.data.images.remove(piece)
        result = numpy.empty((self.px, self.px, 4), dtype=numpy.float32)
        result[:, :] = (*background, 1.0)
        result[rows.min():rows.max() + 1, cols.min():cols.max() + 1] = scaled
        self.save(result, out)

    def picture(self, name, rgba, mask, edge=0):
        """Картина для игры: цвет раскраски, прозрачность — силуэт объёма, раздутый на точку, чтобы край объёма
        не брал фон раскраски. С `edge` точки цвета фона в полосе такой ширины у края силуэта тоже прозрачны: там,
        где нейросеть нарисовала край чуть внутри объёма (листва), не остаётся каймы фона."""
        rgba = rgba.copy()
        solid = mask[:, :, 3] > 0.5
        alpha = grow(solid, 1)
        if edge:
            border = numpy.concatenate([rgba[0], rgba[-1], rgba[:, 0], rgba[:, -1]])[:, :3]
            background = numpy.abs(rgba[:, :, :3] - numpy.median(border, axis=0)).sum(axis=2) < 0.08
            band = alpha & grow(~solid, edge)
            alpha &= ~(band & background)
        rgba[:, :, 3] = alpha
        image = bpy.data.images.new("picture_" + name, self.px, self.px, alpha=True)
        image.pixels = rgba.ravel()
        image.pack()
        return image

    def quads_mask(self, quads):
        """Точки картины внутри четырёхугольников, заданных точками модели: где их видит камера игры."""
        rows, cols = numpy.mgrid[0:self.px, 0:self.px]
        points = numpy.stack([cols + 0.5, rows + 0.5], axis=-1)
        inside = numpy.zeros((self.px, self.px), dtype=bool)
        for quad in quads:
            corners = [numpy.array(self.uv(Vector(p))) * self.px for p in quad]
            sides = []
            for i in range(4):
                a, b = corners[i], corners[(i + 1) % 4]
                edge, to_point = b - a, points - a
                sides.append(edge[0] * to_point[..., 1] - edge[1] * to_point[..., 0])
            sides = numpy.stack(sides)
            inside |= (sides >= 0).all(axis=0) | (sides <= 0).all(axis=0)
        return inside

    def projected_copy(self, source, name, image):
        """Копия объёма с UV проекцией с камеры игры и картиной вместо плоских цветов."""
        mesh = source.data.copy()
        mesh.name = name
        mesh.materials.clear()
        mesh.materials.append(picture_material(name, image))
        for poly in mesh.polygons:
            poly.material_index = 0
        layer = mesh.uv_layers.new(name="UVMap")
        for loop in mesh.loops:
            layer.data[loop.index].uv = self.uv(source.matrix_world @ mesh.vertices[loop.vertex_index].co)
        obj = bpy.data.objects.new(name, mesh)
        bpy.context.scene.collection.objects.link(obj)
        return obj


PROPS = os.path.join(ROOT, "art", "horde", "3d", "props")


def prop_frame(name):
    """Кадр предмета по его объёму; снимки и раскраски — в art/horde/3d/props/<name>."""
    folder = os.path.join(PROPS, name)
    os.makedirs(folder, exist_ok=True)
    return Frame.around([name], folder)


def build_prop(name, part, colors, turn=0.0):
    """Объём предмета в своей коллекции `name` из граней части, повёрнутый на `turn`."""
    obj = make_object(name, part, fresh_collection(name), colors, turn)
    return {name: len(obj.data.polygons)}


EDGE = 14  # полоса у края силуэта, где цвет фона раскраски становится прозрачным, в точках картины


def export_prop(frame, name, extra=()):
    """<name>.glb: объём с картиной из раскраски нейросети paint_raw.png, подогнанной в кадр снимка guide.png."""
    frame.fit("paint_raw.png", "guide.png", "paint.png")
    picture = frame.picture(name, frame.load("paint.png"), frame.load("guide.png"), EDGE)
    obj = frame.projected_copy(bpy.data.objects[name], name + "_game", picture)
    export(obj, extra, name + ".glb")
    bpy.data.objects.remove(obj, do_unlink=True)


def export(obj, extra, file):
    """Выгрузка объекта и узлов `extra` из текущей сцены в games/horde/models/<file>."""
    for other in bpy.context.scene.objects:
        other.select_set(other is obj or other.name in extra)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.export_scene.gltf(filepath=os.path.join(MODELS, file), use_selection=True,
                              export_format="GLB", export_yup=True, export_apply=True,
                              export_animations=False, export_materials="EXPORT", use_active_scene=True)
