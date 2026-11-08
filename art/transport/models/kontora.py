# Дом купеческой усадьбы начальной деревушки, 1890 год; в 1930-м в нём строительная контора артели игрока. По образцу
# art/transport/estate/concept.png, но кирпич голый, без побелки. Двухэтажный дом 13 × 11 м у самой улицы: низ из
# старого красного кирпича на цоколе потемнее, лучковые окна под перемычками из кирпича на ребро, с распахнутыми
# железными ставнями; верх — сруб «в обло» из круглых выветренных брёвен с выпусками на углах, окна с белыми резными
# наличниками, как у изб деревни; между этажами кирпичный поясок под жестяным отливом. Вальмовая крыша из старого
# железа с фальцами, выцветшая красно-коричневая, как у каретника-гаража, свес подшит досками, под краем кровли —
# желоб вокруг всего дома; две кирпичные трубы, водосточные трубы с воронками по углам. К улице посередине — крыльцо
# на точёных столбах под железным козырьком, двустворчатая филёнчатая дверь со стеклом в своде; на щипце козырька
# синяя эмалевая табличка «КОНТОРА». Сзади — дверь во двор с каменными ступенями. За стёклами — занавески и тёмные
# комнаты.
# Справа от дома, за проходом, — навес с бочками гаража (`garage.py`), сам каретник-гараж — в глубине участка;
# дощатый забор — слева от дома и сзади, до забора гаража.
# Земли, травы и деревьев в модели нет: их рисует движок; тропы с прозрачным краем — от улицы к крыльцу и от задней
# двери к навесу гаража. Улица — со стороны −Y Blender, начало — середина дома по ширине на краю участка у улицы
# (край — в 9,3 м от оси главной улицы, как у дворов), единица — метр; верх земли — ноль; размер настоящий.
# Материалы — карты ambientCG и Poly Haven из art/transport/textures; затенение запечено в цвет вершин, поэтому
# длинные и широкие части — стены, скаты, фальцы, желоба — нарезаны частыми вершинами.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/kontora.py [-- <папка>]
import math
import os
import random
import sys

import bmesh
import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402
from bakery import TEXTURES, split_off  # noqa: E402
from garage import chimney, faded, plank_fence  # noqa: E402
from yard import Face, Homestead, bake_occlusion, cabin, tones, window  # noqa: E402

OUT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "models"))

GROUND = 0.0  # земля участка — земля карты движка; ниже неё в модели ничего нет

# Кирпичный низ: наружные грани стен, толщина стены в два с половиной кирпича, верх цоколя — он же пол первого
# этажа, верх кирпича — под срубом.
X0, X1, Y0, Y1 = -6.5, 6.5, 0.5, 11.5
WALL = 0.64
PLINTH = 0.6
TOP = 3.6
WALL_STEP = 0.25  # шаг сетки вершин по кирпичным стенам
# Окна низа — середины по сторонам, ширина, низ проёма, пята и подъём лучкового свода; двери — середина, ширина,
# пята и подъём свода.
LOW = {"front": [-5.0, -3.0, 3.0, 5.0], "back": [-4.6, -1.6, 1.2], "left": [3.2, 8.2], "right": [3.2, 8.2]}
WIN, SILL, SPRING, RISE = 0.9, 1.25, 2.45, 0.26
DOOR, DOOR_W, DOOR_SPRING, DOOR_RISE = 0.0, 1.4, 2.6, 0.3
BACK_DOOR, BACK_W, BACK_SPRING = 3.6, 1.1, 2.45
VOUSSOIR = 0.24  # длина кирпича перемычки по радиусу
SHUTTER = 0.035  # толщина ставни
# Ложки кирпичей на картинке Bricks085 — доли картинки (u0, u1, v0, v1) внутри швов: на каждый кирпич перемычки
# ложится один настоящий кирпич фотографии.
BRICKS = [(0.0557, 0.1553, 0.0811, 0.1045), (0.1689, 0.2686, 0.0811, 0.1045), (0.2822, 0.3789, 0.0811, 0.1045),
          (0.3926, 0.4932, 0.0811, 0.1045), (0.6201, 0.7178, 0.0811, 0.1045), (0.7314, 0.8311, 0.0811, 0.1045),
          (0.8447, 0.9404, 0.0811, 0.1045), (0.1084, 0.2041, 0.1543, 0.1777), (0.2178, 0.3154, 0.1543, 0.1777),
          (0.4453, 0.541, 0.1543, 0.1777), (0.5547, 0.6523, 0.1543, 0.1777), (0.666, 0.7666, 0.1543, 0.1777),
          (0.1562, 0.2598, 0.2266, 0.249), (0.4961, 0.5996, 0.2266, 0.249), (0.6133, 0.71, 0.2266, 0.249),
          (0.7236, 0.8242, 0.2266, 0.249), (0.8379, 0.9365, 0.2266, 0.249), (0.0977, 0.1982, 0.2959, 0.3193),
          (0.2119, 0.3125, 0.2959, 0.3193), (0.4385, 0.5381, 0.2959, 0.3193), (0.5518, 0.6494, 0.2959, 0.3193),
          (0.6631, 0.7637, 0.2959, 0.3193), (0.4922, 0.5967, 0.3643, 0.3867), (0.6104, 0.707, 0.3643, 0.3867),
          (0.835, 0.9365, 0.3643, 0.3867), (0.4316, 0.5332, 0.4336, 0.458), (0.5469, 0.6465, 0.4336, 0.458),
          (0.7324, 0.8311, 0.4336, 0.458), (0.8447, 0.9375, 0.4336, 0.458)]

# Сруб верха: оси стен — на 0,2 м внутри от граней кирпича, радиус бревна, шаг венца, выпуск за угол, число венцов
# боковых стен, граней по окружности бревна; окна — середины по сторонам, ширина, низ и верх проёма.
LX0, LX1, LY0, LY1 = X0 + 0.2, X1 - 0.2, Y0 + 0.2, Y1 - 0.2
LOG_R, STEP, OUTLET, COURSES = 0.14, 0.235, 0.3, 13
LOG_SIDES = 20
END_DISC = (0.501, 0.509, 0.42)  # середина и радиус среза на картинке TreeEnd005, доли стороны — как у конторы порта
GLASS_SHADE = 0.85  # затенение стёкол: их запекание идёт без них самих
UP = {"front": [-4.4, -2.2, 0.0, 2.2, 4.4], "back": [-3.6, 0.0, 3.6], "left": [2.4, 6.0, 9.6], "right": [2.4, 6.0, 9.6]}
UP_W, UP_Z0, UP_Z1 = 0.8, TOP + 0.85, TOP + 2.05

# Крыша: свес от граней кирпича, подъём ската на метр, толщина кровли; фальцы — шаг (две трети метра: сетка
# затенения через 2 м ложится посередине между фальцами), ширина, высота, длина загиба к свесу и недоход до рёбер
# и конька — там фальцы уходят под накладки.
OVER, SLOPE, ROOF = 0.75, 0.58, 0.06
PITCH, SEAM_W, SEAM_H, SEAM_LEAD, SEAM_TRIM = 2.0 / 3.0, 0.01, 0.025, 0.08, 0.1
# Железо кровли — как у крыш порта и гаража: картинка Metal025 на 4 м, подкраска, шероховатость arm_roof.
ROOF_METERS, ROOF_TINT = 4.0, "#ffa47e"
# Карниз: лобовая доска — на `FASCIA` внутрь от края кровли, подшивка — на `SOFFIT` ниже края, доски по 14 см
# толщиной `BOARD`; желоб радиусом `GUTTER` — осью на 3 см внутрь от края кровли и на 7,5 см ниже его.
FASCIA, SOFFIT, BOARD, GUTTER = 0.1, 0.14, 0.025, 0.065
# Водосточные трубы: радиус, от угла дома вдоль стены, от кирпичной стены до оси трубы.
PIPE_R, PIPE_U, PIPE_OFF = 0.055, 0.45, 0.17

# Крыльцо: половина ширины площадки, её вылет от стены, места столбов; козырёк — свес за столбы, высота свеса и конька.
PORCH_HALF, PORCH_D, POST_U, POST_D = 1.45, 1.9, 1.3, 1.75
CANOPY_D, CANOPY_EAVE, CANOPY_RIDGE = 2.0, 3.1, 3.55
HEADER = POST_D + 0.06  # передняя грань бруса на столбах: на нём стоит зашитый щипец с табличкой

# Забор: слева от дома и сзади — до забора гаража (его задний забор в этой же линии).
FENCE_X, FENCE_Y0, FENCE_BACK_Y, FENCE_BACK_X1 = X0 - 1.5, 1.0, 26.8, 9.0

# Тропы: ломаная, полуширина — от улицы к ступеням крыльца, от задней двери к навесу с бочками гаража.
STREET = -5.6
PATHS = [
    ([(DOOR, STREET), (DOOR + 0.1, -2.6), (DOOR, Y0 - PORCH_D - 0.4)], 0.75),
    ([(BACK_DOOR, Y1 + 0.7), (5.2, 12.9), (7.4, 13.5), (9.6, 13.6)], 0.45),
]


class Estate(Homestead):
    """Сетка дома: брёвна сруба — по `LOG_SIDES` граней, вблизи без многогранника."""

    def log(self, a, b, r0, r1, mi, end, segments=12, ring=0.7, shift=(0.0, 0.0)):
        super().log(a, b, r0, r1, mi, end, max(segments, LOG_SIDES), ring, shift)


def photo(m, mi, color=None, arm=None):
    """Своя картинка папки материала вместо color.jpg или arm.jpg: состаренный срез, шероховатость кровли и жести."""
    for node in m.materials[mi].node_tree.nodes:
        if node.bl_idname != "ShaderNodeTexImage":
            continue
        path = bpy.path.abspath(node.image.filepath)
        new = {"color.jpg": color, "arm.jpg": arm}.get(os.path.basename(path))
        if new:
            data = node.image.colorspace_settings.name == "Non-Color"
            node.image = bpy.data.images.load(os.path.join(os.path.dirname(path), new), check_existing=True)
            if data:
                node.image.colorspace_settings.name = "Non-Color"
    return mi


def glass(m):
    """Стекло с пылью, как у конторы порта: картинка WindowGlass, прозрачность из её альфы (у налёта плотнее),
    почти гладкое и без металла — отражает небо слабо; смешивание — как у прозрачных частей игры."""
    mi = m.mat("glass", "#ffffff", 0.1)
    mat = m.materials[mi]
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    node = lib.bsdf(mat)
    folder = os.path.join(TEXTURES, "WindowGlass")

    def image(file, data):
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = bpy.data.images.load(os.path.join(folder, file), check_existing=True)
        if data:
            tex.image.colorspace_settings.name = "Non-Color"
        return tex

    color = image("color.png", False)
    links.new(color.outputs["Color"], node.inputs["Base Color"])
    links.new(color.outputs["Alpha"], node.inputs["Alpha"])
    channels = nodes.new("ShaderNodeSeparateColor")
    links.new(image("arm.jpg", True).outputs["Color"], channels.inputs["Color"])
    links.new(channels.outputs["Green"], node.inputs["Roughness"])
    relief = nodes.new("ShaderNodeNormalMap")
    links.new(image("normal.jpg", True).outputs["Color"], relief.inputs["Color"])
    links.new(relief.outputs["Normal"], node.inputs["Normal"])
    mat.surface_render_method = "BLENDED"
    m.tiles[mi] = (1.0, False)
    return mi


def materials(m):
    return {
        # Светлота частей после карты и подкраски, примерно: кровля 0,45, кирпич 0,3, брёвна 0,33, наличники 0,85.
        # Кирпич и его подкраски — те же, что у каретника-гаража усадьбы (`garage.py`).
        "wall": m.tex("wall", "Bricks085", 2.0, tint="#ffd0c0"),
        "brick": m.tex("brick", "Bricks085", 2.0, tint="#e0b4a6"),
        # Кирпичи перемычек — та же фотография, что стена, по кирпичу на клин; швы между ними — раствор.
        "voussoir": m.tex("voussoir", "Bricks085", 2.0, tint="#ffd0c0"),
        "voussoir_dark": m.tex("voussoir_dark", "Bricks085", 2.0, tint="#e8b8a8"),
        "mortar": m.mat("mortar", "#a08672", 0.95),  # раствор швов стены на фотографии после подкраски, в тени шва
        "logs": tones(m, "log", "Wood033", 1.6, ("#8f7c66", "#83715d", "#9a8771", "#7a6a59")),
        # Срез бревна с кольцами и трещинами, выветренный в серый тон стен, матовый.
        "log_end": photo(m, m.tex("log_end", "TreeEnd005", 1.0, tint="#857f7a"), color="color_aged.jpg", arm="arm_matte.jpg"),
        "boards": tones(m, "board", "Wood033", 1.6, ("#b8b0a4", "#a9a195", "#c3bbaf", "#a19a8f")),
        "fence": tones(m, "fence", "Wood033", 1.6, ("#b8b0a4", "#a9a195", "#c3bbaf", "#a19a8f")),
        "post": m.tex("post", "Wood059", 1.4, tint="#8b8079"),
        "plank": m.tex("plank", "Planks012", 2.1, tint="#c8c2ba"),
        # Белая краска — картинка без подкраски: светлее её в игре не сделать, светлоту добирает частая сетка
        # вершин наличников и рам (`fine`) — затенение не темнит их целиком от углов.
        "paint": m.tex("paint", "PaintedWood009C_1k", 1.2),
        "door": m.tex("door", "PaintedWood009C_1k", 1.2, tint="#8c6a4c"),
        "shutter": m.tex("shutter", "green_metal_rust", 1.6, tint="#8a9a8e"),
        "roof": photo(m, m.tex("roof", "Metal025", ROOF_METERS, tint=ROOF_TINT), arm="arm_roof.jpg"),
        # Оцинкованная жесть с ржавчиной: отливы, желоба, водосточные трубы.
        "tin": photo(m, m.tex("tin", "Metal025", 1.0, tint="#b8b8b2"), arm="arm_iron.jpg"),
        "stone": m.tex("stone", "white_sandstone_bricks_03", 2.0, tint="#a39e95"),
        "path": faded(m, "path", "Ground103", 2.5, "#e6dccd"),
        "glass": glass(m),
        "curtain": m.tex("curtain", "Linen", 0.6, tint="#f4efe6"),
        "room": m.mat("room", "#1d1916", 0.95),
        "soot": m.mat("soot", "#0e0d0c", 0.95),
        "iron": m.mat("iron", "#2c2d2f", 0.55, 0.5),
        "enamel": m.mat("enamel", "#1c3c8c", 0.25),
        "enamel_white": m.mat("enamel_white", "#f2efe6", 0.3),
    }


def wooden(mt):
    """Материалы досок, брусьев и столбов: им развёртка — волокнами вдоль каждого. Стёс брёвен у окон остаётся
    развёрткой `project` — волокнами вдоль стены, как у самих брёвен."""
    return {mt["paint"], mt["door"], mt["plank"], mt["post"], *mt["boards"], *mt["fence"]}


def fresh(m, start):
    """Грани, построенные после того, как в сетке было `start` граней."""
    m.bm.faces.ensure_lookup_table()
    return [m.bm.faces[i] for i in range(start, len(m.bm.faces))]


def grain(m, faces, axis, shift=(0.0, 0.0)):
    """Развёртка вдоль волокон: на картинках дерева волокна идут вдоль u — u ложится вдоль `axis`, v — поперёк по
    грани; торцы — поперёк оси. В метрах материала, со сдвигом `shift`; `project` эти грани уже не трогает."""
    a = Vector(axis).normalized()
    e1 = a.orthogonal().normalized()
    e2 = a.cross(e1)
    for f in faces:
        meters = m.tiles.get(f.material_index, (2.0, False))[0]
        f.normal_update()
        across = f.normal.cross(a)
        uu, vv = (e1, e2) if across.length < 0.3 else (a, across.normalized())
        for loop in f.loops:
            loop[m.uv].uv = (loop.vert.co.dot(uu) / meters + shift[0], loop.vert.co.dot(vv) / meters + shift[1])
        m.manual.add(f)


def grain_along(m, faces, wood, rnd):
    """Каждая доска, брус и столб из граней `faces` (тела — грани с общими вершинами) из дерева `wood` — волокнами
    вдоль своего самого длинного ребра, со своим сдвигом картинки."""
    faces = [f for f in faces if f.material_index in wood]
    owners = {}
    for f in faces:
        for v in f.verts:
            owners.setdefault(v, []).append(f)
    left = set(faces)
    while left:
        group = [left.pop()]
        stack = list(group)
        while stack:
            for v in stack.pop().verts:
                for g in owners[v]:
                    if g in left:
                        left.remove(g)
                        group.append(g)
                        stack.append(g)
        edge = max((e for f in group for e in f.edges), key=lambda e: e.calc_length())
        grain(m, group, edge.verts[1].co - edge.verts[0].co, (rnd.random(), rnd.random()))


def turn_uv(m, faces):
    """Развёртка тела вращения повёрнута на четверть: волокна точёного столба — вдоль него, а не по окружности."""
    for f in faces:
        for loop in f.loops:
            u, v = loop[m.uv].uv
            loop[m.uv].uv = (v, u)


def cut(m, faces, planes):
    """Режет грани `faces` плоскостями (точка, нормаль) — только их — и возвращает получившиеся грани."""
    for co, no in planes:
        edges = list({e for f in faces for e in f.edges})
        verts = list({v for e in edges for v in e.verts})
        res = bmesh.ops.bisect_plane(m.bm, geom=verts + edges + faces, plane_co=co, plane_no=no)
        faces = [g for g in res["geom"] if isinstance(g, bmesh.types.BMFace)]
    return faces


def fine(m, faces, mis, face, u0, u1, z0, z1, step=0.1):
    """Лицевые грани `faces` материалов `mis` у проёма от `u0` до `u1` и от `z0` до `z1` по стене `face` режутся
    сеткой через `step`: у узких досок наличника и рамы вершины иначе только по углам, в щелях у соседних досок, и
    затенение темнит доску целиком."""
    o = face.at(0.0, 0.0, 0.0)
    outward = face.at(0.0, 0.0, 1.0) - o
    along = face.at(1.0, 0.0, 0.0) - o
    picked = []
    for f in faces:
        f.normal_update()
        us = [v.co.dot(along) for v in f.verts]
        zs = [v.co.z for v in f.verts]
        # Планкам уже `step` частая сетка не даст вершин посередине — их не режем.
        if f.material_index in mis and f.normal.dot(outward) > 0.9 and min(max(us) - min(us), max(zs) - min(zs)) >= step:
            picked.append(f)
    faces = picked
    planes = [(o + along * (u0 + step * (i + 0.5)), along) for i in range(int((u1 - u0) / step))]
    planes += [(Vector((0.0, 0.0, z0 + step * (i + 0.5))), Vector((0.0, 0.0, 1.0))) for i in range(int((z1 - z0) / step))]
    cut(m, faces, planes)


def rail(m, a, b, w, h, mi, up, step=0.4, lead=0.0):
    """Брус от `a` до `b` шириной `w`, низом на линии a–b, высотой `h` в сторону `up`; кольца вершин через `step` —
    затенению в вершинах нужны вершины вдоль длинной детали. На длине `lead` от `a` брус сходит к низу на нет:
    фальц у свеса загнут вниз, его конец лежит заподлицо с краем кровли."""
    a, b = Vector(a), Vector(b)
    length = (b - a).length
    d = (b - a) / length
    n = Vector(up)
    n = (n - d * n.dot(d)).normalized()
    side = d.cross(n)
    stations = {0.0, length} | {step * i for i in range(1, int(length / step) + 1) if step * i < length - 0.05}
    if 0.0 < lead < length:
        stations.add(lead)
    rings = []
    for t in sorted(stations):
        hh = max(0.002, h * min(1.0, t / lead)) if lead > 0 else h
        p = a + d * t
        rings.append([m.bm.verts.new(p + side * sw * w / 2 + n * sh * hh) for sw, sh in ((-1, 0), (1, 0), (1, 1), (-1, 1))])
    m.bm.faces.new(list(reversed(rings[0]))).material_index = mi
    m.bm.faces.new(rings[-1]).material_index = mi
    for r0, r1 in zip(rings, rings[1:]):
        for k in range(4):
            j = (k + 1) % 4
            m.bm.faces.new((r0[k], r0[j], r1[j], r1[k])).material_index = mi


def bent(points, radius=0.12, steps=5):
    """Ломаная со скруглёнными изломами: каждый излом — дугой радиусом около `radius`."""
    pts = [Vector(p) for p in points]
    out = [pts[0]]
    for p0, p1, p2 in zip(pts, pts[1:], pts[2:]):
        d0, d1 = (p1 - p0).normalized(), (p2 - p1).normalized()
        turn = d0.angle(d1)
        if turn < 1e-3:
            out.append(p1)
            continue
        t = min(radius * math.tan(turn / 2), (p1 - p0).length / 2, (p2 - p1).length / 2)
        a, c = p1 - d0 * t, p1 + d1 * t
        out += [a * (1 - s) ** 2 + p1 * 2 * (1 - s) * s + c * s * s for s in (i / steps for i in range(steps + 1))]
    out.append(pts[-1])
    return out


def tube(m, points, r, mi, mouth, segments=16):
    """Круглая труба по ломаной `points` (изломы скруглить заранее — `bent`): кольца вершин в точках поперёк
    касательной, развёртка вдоль трубы и по окружности; начало закрыто донышком, конец — тёмным устьем `mouth`."""
    pts = [Vector(p) for p in points]
    dirs = [(b - a).normalized() for a, b in zip(pts, pts[1:])]
    tangents = [dirs[0]] + [(a + b).normalized() for a, b in zip(dirs, dirs[1:])] + [dirs[-1]]
    e1 = tangents[0].orthogonal().normalized()
    meters = m.tiles.get(mi, (2.0, False))[0]
    rings, along, prev = [], 0.0, tangents[0]
    for i, (p, t) in enumerate(zip(pts, tangents)):
        e1 = (prev.rotation_difference(t) @ e1).normalized()
        prev = t
        e2 = t.cross(e1)
        if i:
            along += (p - pts[i - 1]).length
        rings.append(([m.bm.verts.new(p + (e1 * math.cos(2 * math.pi * k / segments) + e2 * math.sin(2 * math.pi * k / segments)) * r)
                       for k in range(segments)], along))
    for (r0, s0), (r1, s1) in zip(rings, rings[1:]):
        for k in range(segments):
            corners = ((r0, s0, k), (r0, s0, k + 1), (r1, s1, k + 1), (r1, s1, k))
            f = m.bm.faces.new([ring[kk % segments] for ring, _, kk in corners])
            f.material_index = mi
            f.smooth = True
            m.manual.add(f)
            for loop, (_, s, kk) in zip(f.loops, corners):
                loop[m.uv].uv = (s / meters, 2 * math.pi * r * kk / segments / meters)
    m.bm.faces.new(list(reversed(rings[0][0]))).material_index = mi
    m.bm.faces.new(rings[-1][0]).material_index = mouth


def faces():
    """Наружные плоскости кирпичных стен и пролёт каждой вдоль себя: торцевые стены — между уличной и задней."""
    return {
        "front": (Face("front", Y0), X0, X1),
        "back": (Face("back", Y1), X0, X1),
        "left": (Face("left", X0), Y0 + WALL, Y1 - WALL),
        "right": (Face("right", X1), Y0 + WALL, Y1 - WALL),
    }


def arch(u0, u1, zs, rise, n=12):
    """Лучковый свод проёма от `u0` до `u1` с пятой на `zs` и подъёмом `rise`: точки (u, z) слева направо, середина,
    радиус и высота середины круга."""
    half, um = (u1 - u0) / 2, (u0 + u1) / 2
    r = (half * half + rise * rise) / (2 * rise)
    zc = zs + rise - r
    a0 = math.atan2(zs - zc, half)
    pts = [(um + r * math.cos(a), zc + r * math.sin(a)) for a in (math.pi - a0 - (math.pi - 2 * a0) * i / n for i in range(n + 1))]
    return pts, um, r, zc


def holes():
    """Проёмы кирпичного низа по сторонам: (u0, u1, низ, пята, подъём, вид)."""
    out = {side: [(u - WIN / 2, u + WIN / 2, SILL, SPRING, RISE, "window") for u in places] for side, places in LOW.items()}
    out["front"].append((DOOR - DOOR_W / 2, DOOR + DOOR_W / 2, PLINTH, DOOR_SPRING, DOOR_RISE, "door"))
    out["back"].append((BACK_DOOR - BACK_W / 2, BACK_DOOR + BACK_W / 2, PLINTH, BACK_SPRING, RISE, "back_door"))
    return out


def spandrel(m, face, u0, u1, zs, rise, ztop, mi):
    """Кирпич над сводом проёма: от свода до `ztop`, во всю толщину стены."""
    pts, _, _, _ = arch(u0, u1, zs, rise)
    rows = [[m.bm.verts.new(face.at(u, z if low else ztop, d)) for u, z in pts] for d in (0.0, -WALL) for low in (True, False)]
    front_low, front_top, back_low, back_top = rows
    for i in range(len(pts) - 1):
        j = i + 1
        for quad in ((front_low[i], front_low[j], front_top[j], front_top[i]), (back_low[i], back_top[i], back_top[j], back_low[j]),
                     (front_low[i], back_low[i], back_low[j], front_low[j]), (front_top[i], front_top[j], back_top[j], back_top[i])):
            m.bm.faces.new(quad).material_index = mi
    for k in (0, -1):
        m.bm.faces.new((front_low[k], front_top[k], back_top[k], back_low[k])).material_index = mi


def voussoirs(m, mt, face, u0, u1, zs, rise, rnd, bricks):
    """Перемычка над сводом — кирпичи на ребро торцами по радиусу, выступает из стены на 2 см; тон каждого кирпича
    берётся из `rnd`, а сам он — свой кирпич фотографии стены из `bricks`, вдоль радиуса; в швах между клиньями —
    полоса раствора под ними."""
    pts, um, r, zc = arch(u0, u1, zs, rise)
    left = math.atan2(pts[0][1] - zc, pts[0][0] - um)
    right = math.atan2(pts[-1][1] - zc, pts[-1][0] - um)
    band, _, _, _ = arch(u0, u1, zs, rise, 40)
    outer = (r + VOUSSOIR - 0.002) / r  # раствор только под клиньями: снаружи перемычки шов — уже шов стены
    rows = [[m.bm.verts.new(face.at(um + (u - um) * k, zc + (z - zc) * k, d)) for u, z in band] for k in (1.0, outer) for d in (0.0, 0.01)]
    inner_back, inner_front, outer_back, outer_front = rows
    for i in range(len(band) - 1):
        j = i + 1
        for quad in ((inner_front[i], inner_front[j], outer_front[j], outer_front[i]), (inner_back[i], outer_back[i], outer_back[j], inner_back[j]),
                     (inner_front[i], inner_back[i], inner_back[j], inner_front[j]), (outer_front[i], outer_front[j], outer_back[j], outer_back[i])):
            m.bm.faces.new(quad).material_index = mt["mortar"]
    for k in (0, -1):
        m.bm.faces.new((inner_front[k], outer_front[k], outer_back[k], inner_back[k])).material_index = mt["mortar"]
    count = max(3, round(r * (left - right) / 0.078))
    step, joint = (left - right) / count, 0.011 / r
    centre = face.at(um, zc, 0.0)
    for i in range(count):
        a0, a1 = left - i * step - joint / 2, left - (i + 1) * step + joint / 2
        outline = [(um + rr * math.cos(a), zc + rr * math.sin(a)) for rr, a in ((r + 0.004, a0), (r + 0.004, a1), (r + VOUSSOIR, a1), (r + VOUSSOIR, a0))]
        start = len(m.bm.faces)
        face.shape(m, outline, 0.0, 0.02, rnd.choice((mt["voussoir"], mt["voussoir_dark"])))
        mid = (a0 + a1) / 2
        radial = face.at(math.cos(mid), math.sin(mid), 0.0) - face.at(0.0, 0.0, 0.0)
        across = face.at(-math.sin(mid), math.cos(mid), 0.0) - face.at(0.0, 0.0, 0.0)
        bu0, bu1, bv0, bv1 = bricks.choice(BRICKS)
        if bricks.random() < 0.5:
            bu0, bu1 = bu1, bu0
        width = (r + VOUSSOIR / 2) * (a0 - a1)
        for f in fresh(m, start):
            for loop in f.loops:
                q = loop.vert.co - centre
                s = (q.dot(radial) - r - 0.004) / (VOUSSOIR - 0.004)
                loop[m.uv].uv = (bu0 + (bu1 - bu0) * s, (bv0 + bv1) / 2 + (bv1 - bv0) * q.dot(across) / width)
            m.manual.add(f)


def room(m, mt, face, u0, u1, z0, z1, d):
    """Тёмная комната за окном: коробка от плоскости `d` вглубь дома, шире проёма — за стеклом и занавесками видна
    темнота, а не пустой дом."""
    face.box(m, u0 - 0.1, u1 + 0.1, z0 - 0.1, z1 + 0.1, d - 0.3, d, mt["room"])


def curtains(m, mt, face, u0, u1, z0, z1, d):
    """Белые занавески за стеклом: две половинки, раздвинутые к краям, и подзор поверху."""
    w = (u1 - u0) * 0.27
    for a0, a1, b0 in ((u0, u0 + w, z0 + 0.03), (u1 - w, u1, z0 + 0.03), (u0, u1, z1 - 0.14)):
        face.box(m, a0, a1, b0, z1, d - 0.006, d, mt["curtain"])


def window_low(m, mt, face, u0, u1, zb, zs, rise):
    """Окно низа в глубине проёма: стекло под сводом, белая рама с переплётом и дугой по своду, жестяной отлив;
    за стеклом занавески и тёмная комната; по сторонам — распахнутые железные ставни с двумя полосами."""
    pts, um, r, zc = arch(u0, u1, zs, rise)
    depth = -0.16
    face.shape(m, [(u0, zb), (u1, zb)] + list(reversed(pts)), depth - 0.01, depth, mt["glass"])
    curtains(m, mt, face, u0, u1, zb, zs, depth - 0.04)
    room(m, mt, face, u0, u1, zb, zs + rise, -WALL)
    paint = mt["paint"]
    for a0, a1, b0, b1 in ((u0, u0 + 0.07, zb, zs), (u1 - 0.07, u1, zb, zs), (u0, u1, zb, zb + 0.07), (u0, u1, zs - 0.035, zs + 0.035),
                           (um - 0.03, um + 0.03, zb, zs + rise - 0.05)):
        face.box(m, a0, a1, b0, b1, depth, depth + 0.05, paint)
    normal = tuple(face.at(0.0, 0.0, 1.0) - face.at(0.0, 0.0, 0.0))
    inner = r - 0.035
    angles = [math.atan2(z - zc, u - um) for u, z in pts]
    for a, b in zip(angles, angles[1:]):
        m.beam(face.at(um + inner * math.cos(a), zc + inner * math.sin(a), depth + 0.025),
               face.at(um + inner * math.cos(b), zc + inner * math.sin(b), depth + 0.025), 0.07, 0.05, paint, up=normal)
    face.box(m, u0 - 0.06, u1 + 0.06, zb - 0.05, zb, depth, 0.09, mt["tin"])
    half = (u1 - u0) / 2
    for s0, s1 in ((u0 - 0.03 - half, u0 - 0.03), (u1 + 0.03, u1 + 0.03 + half)):
        face.box(m, s0, s1, zb - 0.02, zs, 0.005, 0.005 + SHUTTER, mt["shutter"])
        for z in (zb + 0.2, zs - 0.26):
            face.box(m, s0 + 0.02, s1 - 0.02, z, z + 0.06, 0.005 + SHUTTER, 0.015 + SHUTTER, mt["iron"])


def door_leaves(m, mt, face, u0, u1, zb, zs, rise, double):
    """Филёнчатая дверь в глубине проёма — в две створки или одна — с рамой и стеклом в своде над поперечиной; за
    стеклом свода — тёмные сени."""
    pts, um, _, _ = arch(u0, u1, zs, rise)
    depth = -0.2
    face.shape(m, pts, depth - 0.01, depth, mt["glass"])
    room(m, mt, face, u0, u1, zb, zs + rise, -WALL)
    paint, door = mt["paint"], mt["door"]
    face.box(m, u0, u1, zs - 0.05, zs + 0.03, depth, depth + 0.06, paint)
    face.box(m, um - 0.025, um + 0.025, zs, zs + rise - 0.04, depth, depth + 0.04, paint)
    leaves = ((u0 + 0.02, um - 0.005), (um + 0.005, u1 - 0.02)) if double else ((u0 + 0.02, u1 - 0.02),)
    for a, b in leaves:
        face.box(m, a, b, zb, zs - 0.05, depth, depth + 0.05, door)
        for c0, c1 in ((zb + 0.15, zb + 0.8), (zb + 0.95, zs - 0.2)):
            face.box(m, a + 0.09, b - 0.09, c0, c1, depth + 0.05, depth + 0.07, door)
    handles = (um - 0.09, um + 0.07) if double else (u1 - 0.17,)
    for u in handles:
        face.box(m, u, u + 0.02, zb + 0.95, zb + 1.1, depth + 0.05, depth + 0.1, mt["iron"])


def lower(m, mt, rnd):
    """Кирпичный низ: стены с проёмами — куски по сторонам проёмов, под окнами и над сводами, — цоколь потемнее на
    3,5 см наружу, поясок под срубом с жестяным отливом; в проёмах окна и двери, над сводами перемычки."""
    bricks = random.Random(85)
    wood = random.Random(86)
    for side, (face, a0, a1) in faces().items():
        openings = holes()[side]
        cuts = sorted({a0, a1} | {u for h in openings for u in h[:2]})
        for s0, s1 in zip(cuts, cuts[1:]):
            hole = next((h for h in openings if h[0] - 1e-6 <= s0 and s1 <= h[1] + 1e-6), None)
            if hole is None:
                face.box(m, s0, s1, GROUND, TOP, -WALL, 0.0, mt["wall"])
                continue
            u0, u1, zb, zs, rise, _ = hole
            if zb > GROUND:
                face.box(m, s0, s1, GROUND, zb, -WALL, 0.0, mt["wall"])
            spandrel(m, face, u0, u1, zs, rise, TOP, mt["wall"])
        ends = 0.035 if side in ("front", "back") else 0.0
        b0, b1 = (X0, X1) if side in ("front", "back") else (Y0, Y1)
        face.box(m, b0 - ends, b1 + ends, GROUND, PLINTH, 0.0, 0.035, mt["brick"])
        face.box(m, b0 - 0.07 * (ends > 0), b1 + 0.07 * (ends > 0), TOP - 0.34, TOP - 0.1, 0.0, 0.07, mt["brick"])
        face.box(m, b0 - 0.1 * (ends > 0), b1 + 0.1 * (ends > 0), TOP - 0.1, TOP - 0.06, -0.06, 0.1, mt["tin"])
        for u0, u1, zb, zs, rise, kind in openings:
            voussoirs(m, mt, face, u0, u1, zs, rise, rnd, bricks)
            start = len(m.bm.faces)
            if kind == "window":
                window_low(m, mt, face, u0, u1, zb, zs, rise)
            else:
                door_leaves(m, mt, face, u0, u1, zb, zs, rise, kind == "door")
            fine(m, fresh(m, start), {mt["paint"]}, face, u0, u1, zb, zs + rise)
            grain_along(m, fresh(m, start), wooden(mt), wood)


def dense(m, mis):
    """Отвесные грани кирпичного низа материалов `mis` режутся сеткой через `WALL_STEP`: без частых вершин затенение
    под окнами и у откосов ложится прямоугольниками с резкими краями на стыках кусков стены."""
    walls = []
    for f in m.bm.faces:
        f.normal_update()
        c = f.calc_center_median()
        inner = X0 + WALL - 0.005 <= c.x <= X1 - WALL + 0.005 and Y0 + WALL - 0.005 <= c.y <= Y1 - WALL + 0.005  # изнутри дома не видно
        if f.material_index in mis and abs(f.normal.z) < 0.5 and not inner:
            walls.append(f)
    planes = []
    for axis, lo, hi in ((0, X0 - 0.2, X1 + 0.2), (1, Y0 - 0.2, Y1 + 0.2), (2, GROUND, TOP)):
        n = Vector((0.0, 0.0, 0.0))
        n[axis] = 1.0
        c = lo + WALL_STEP
        while c < hi:
            planes.append((n * (c + 0.007), n))
            c += WALL_STEP
    cut(m, walls, planes)


def upper(m, mt, rnd):
    """Сруб второго этажа на кирпичном низе с окнами в белых резных наличниках; за стёклами — занавески и тёмные
    комнаты. Возвращает высоту верха сруба."""
    openings = {side: [(u - UP_W / 2, u + UP_W / 2, UP_Z0, UP_Z1) for u in places] for side, places in UP.items()}
    top = cabin(m, mt, (LX0, LX1, LY0, LY1), LOG_R, STEP, COURSES, TOP, OUTLET, openings, mt["logs"], rnd)
    walls = {"front": Face("front", LY0), "back": Face("back", LY1), "left": Face("left", LX0), "right": Face("right", LX1)}
    wood = random.Random(87)
    for side, places in openings.items():
        for u0, u1, z0, z1 in places:
            start = len(m.bm.faces)
            a0, a1, b0, b1, deep = window(m, mt, walls[side], LOG_R, u0, u1, z0, z1)
            fine(m, fresh(m, start), {mt["paint"]}, walls[side], u0 - 0.3, u1 + 0.3, z0 - 0.4, z1 + 0.45)
            grain_along(m, fresh(m, start), wooden(mt), wood)
            curtains(m, mt, walls[side], a0, a1, b0, b1, -0.04)
            room(m, mt, walls[side], a0, a1, b0, b1, -deep)
    return top


def roof(m, mt, top):
    """Вальмовая крыша из железа над срубом: скаты подняты над верхним венцом, чтобы под ними прошли выпуски брёвен
    на углах; фальцы через `PITCH` от свеса к коньку и к рёбрам вальм — у свеса загнуты вниз заподлицо с краем, вверху
    уходят под накладки рёбер и конька, у труб кончаются; две кирпичные трубы у конька — от потолка верхнего этажа.
    Листы скатов нарезаны вершинами у фальцев, рёбер, конька и труб: затенение в вершинах не растекается от них
    тёмными пятнами. Возвращает высоту свеса."""
    ex0, ey0, ex1, ey1 = X0 - OVER, Y0 - OVER, X1 + OVER, Y1 + OVER
    ze = top + 0.25 - (OVER + 0.2) * SLOPE
    inset = (ey1 - ey0) / 2
    ym, rx0, rx1 = (ey0 + ey1) / 2, ex0 + inset, ex1 - inset
    zr = ze + inset * SLOPE
    mi = mt["roof"]
    chimneys = (("back", -1.7, ym + 0.9), ("front", 1.7, ym - 0.9))
    half = 0.33  # половина трубы на плане
    # Скат: начало отсчёта на свесе и направления на плане — от свеса внутрь и вдоль свеса; углы и выпуклый контур.
    corners = {"fl": Vector((ex0, ey0)), "fr": Vector((ex1, ey0)), "bl": Vector((ex0, ey1)), "br": Vector((ex1, ey1))}
    ends = {"fl": Vector((rx0, ym)), "fr": Vector((rx1, ym)), "bl": Vector((rx0, ym)), "br": Vector((rx1, ym))}
    planes = {
        "front": (Vector((0.0, ey0)), Vector((0.0, 1.0)), Vector((1.0, 0.0)), ("fl", "fr"), [(ex0, ey0, ze), (ex1, ey0, ze), (rx1, ym, zr), (rx0, ym, zr)]),
        "back": (Vector((0.0, ey1)), Vector((0.0, -1.0)), Vector((1.0, 0.0)), ("bl", "br"), [(ex1, ey1, ze), (ex0, ey1, ze), (rx0, ym, zr), (rx1, ym, zr)]),
        "left": (Vector((ex0, 0.0)), Vector((1.0, 0.0)), Vector((0.0, 1.0)), ("fl", "bl"), [(ex0, ey1, ze), (ex0, ey0, ze), (rx0, ym, zr)]),
        "right": (Vector((ex1, 0.0)), Vector((-1.0, 0.0)), Vector((0.0, 1.0)), ("fr", "br"), [(ex1, ey0, ze), (ex1, ey1, ze), (rx1, ym, zr)]),
    }
    for side, (base, w, a, hips, outline) in planes.items():
        normal = Vector((-w.x * SLOPE, -w.y * SLOPE, 1.0)).normalized()
        along = "x" if a.x else "y"
        lo, hi = (ex0, ex1) if along == "x" else (ey0, ey1)
        mid = 0.0 if along == "x" else ym
        seams = [mid + PITCH * (j + 0.5) for j in range(-30, 30) if lo + 0.15 < mid + PITCH * (j + 0.5) < hi - 0.15]

        def at(s, dist):
            p = base + a * s + w * dist
            return Vector((p.x, p.y, ze + dist * SLOPE))

        def plan(s, dist):
            p = base + a * s + w * dist
            return Vector((p.x, p.y, 0.0))

        def flat(v):
            return Vector((v.x, v.y, 0.0))

        start = len(m.bm.faces)
        m.plate(outline, ROOF, mi)
        cuts = []
        for s in seams:
            for off in (-0.06, -0.012, 0.012, 0.06):
                cuts.append((plan(s + off, 0.0), flat(a)))
        dist = 0.5
        while dist < inset - 0.25:
            cuts.append((plan(0.0, dist), flat(w)))
            dist += 0.5
        if side in ("front", "back"):
            cuts += [(plan(0.0, inset - off), flat(w)) for off in (0.13, 0.2)]
        for key in hips:
            h = (ends[key] - corners[key]).normalized()
            n = -(w - h * w.dot(h)).normalized()
            cuts += [(Vector((corners[key].x + n.x * off, corners[key].y + n.y * off, 0.0)), Vector((n.x, n.y, 0.0))) for off in (0.1, 0.16)]
        mine = [(cx, cy) for where, cx, cy in chimneys if where == side]
        for cx, cy in mine:
            cuts += [(Vector((cx + s * (half + 0.015), 0.0, 0.0)), Vector((1.0, 0.0, 0.0))) for s in (-1, 1)]
            cuts += [(Vector((0.0, cy + s * (half + 0.015), 0.0)), Vector((0.0, 1.0, 0.0))) for s in (-1, 1)]
        sheet = []
        for f in fresh(m, start):
            f.normal_update()
            if f.normal.z > 0.5:  # низ листа закрыт подшивкой и чердаком — режется только верх
                sheet.append(f)
        cut(m, sheet, cuts)
        for s in seams:
            reach = min(s - lo, hi - s, inset) - SEAM_TRIM
            if reach < 0.3:
                continue
            pieces = [(0.0, reach)]
            for cx, cy in mine:
                cs, cd = (cx, cy - base.y) if along == "x" else (cy, cx - base.x)
                cd = abs(cd)
                if abs(s - cs) < half + SEAM_W:
                    pieces = [(0.0, cd - half), (cd + half, reach)]
            for d0, d1 in pieces:
                rail(m, at(s, d0), at(s, d1), SEAM_W, SEAM_H, mi, normal, lead=SEAM_LEAD if d0 == 0.0 else 0.0)
    # Накладки рёбер и конька: низом на линии ребра, кольца вершин через полметра.
    for key in corners:
        c, e = corners[key], ends[key]
        rail(m, (c.x, c.y, ze), (e.x, e.y, zr), 0.18, 0.06, mi, (0.0, 0.0, 1.0), step=0.5)
    rail(m, (rx0, ym, zr), (rx1, ym, zr), 0.22, 0.07, mi, (0.0, 0.0, 1.0), step=0.5)
    for where, cx, cy in chimneys:
        chimney(m, cx - half, cy - half, cx + half, cy + half, top - 0.5, zr + 0.85, mt["brick"], mt["soot"], wall=0.15, cap=0.12, over=0.07)
    return ze


def gutter(m, a, b, out, mi, z):
    """Полукруглый желоб из жести на высоте `z` по оси от угла `a` до угла `b` на плане (`out` — наружу от дома):
    кольца вершин через полметра, оба конца срезаны в ус по диагонали угла — желоба соседних сторон сходятся без
    щели."""
    a, b = Vector((a[0], a[1], z)), Vector((b[0], b[1], z))
    out = Vector((out[0], out[1], 0.0))
    length = (b - a).length
    d = (b - a) / length
    section = [(-math.cos(math.pi * k / 10), -math.sin(math.pi * k / 10)) for k in range(11)]
    shell = [(o * GUTTER, h * GUTTER) for o, h in section] + [(o * (GUTTER - 0.0025), h * (GUTTER - 0.0025)) for o, h in reversed(section)]
    stations = [0.0] + [length * i / max(1, round(length / 0.5)) for i in range(1, max(1, round(length / 0.5)))] + [length]
    rings = []
    for t in stations:
        miter = -1.0 if t == 0.0 else 1.0 if t == length else 0.0
        rings.append([m.bm.verts.new(a + d * (t + miter * o) + out * o + Vector((0.0, 0.0, h))) for o, h in shell])
    count = len(shell)
    for r0, r1 in zip(rings, rings[1:]):
        for k in range(count):
            j = (k + 1) % count
            m.bm.faces.new((r0[k], r0[j], r1[j], r1[k])).material_index = mi
    m.bm.faces.new(list(reversed(rings[0]))).material_index = mi
    m.bm.faces.new(rings[-1]).material_index = mi
    return a, d, length


def hooks(m, a, d, length, out, mi):
    """Крюки желоба через 0,9 м: полоса по низу желоба от переднего края до заднего и вверх по лобовой доске."""
    out = Vector((out[0], out[1], 0.0))
    up = Vector((0.0, 0.0, 1.0))
    n = max(1, round(length / 0.9))
    for i in range(n):
        p = a + d * (length * (i + 0.5) / n)
        arc = [p + (out * -math.cos(math.pi * k / 4) - up * math.sin(math.pi * k / 4)) * (GUTTER + 0.003) for k in range(5)]
        for q0, q1 in zip(arc, arc[1:]):
            m.beam(q0, q1, 0.004, 0.025, mi, up=tuple(d))
        back = p - out * (GUTTER + 0.005)
        m.beam(back, back + up * 0.045, 0.004, 0.025, mi, up=tuple(d))


def cornice(m, mt, ze):
    """Карниз: свес кровли подшит снизу досками вдоль стен — от лобовой доски под краем кровли до сруба; верхние
    венцы и их выпуски на углах уходят в подшивку, щели между скатами и срубом больше нет. Под краем кровли на
    крюках — желоб вокруг всего дома, на углах сведённый в ус."""
    ex0, ey0, ex1, ey1 = X0 - OVER, Y0 - OVER, X1 + OVER, Y1 + OVER
    zs = ze - SOFFIT
    fx0, fy0, fx1, fy1 = ex0 + FASCIA, ey0 + FASCIA, ex1 - FASCIA, ey1 - FASCIA
    boards = mt["boards"]
    rnd = random.Random(31)
    start = len(m.bm.faces)
    # Лобовые доски: уличная и задняя во всю длину, торцевые — между ними; верх — под нижней гранью кровли.
    top = ze - 0.014
    m.box(fx0, fy0, zs - 0.02, fx1, fy0 + BOARD, top, boards[0])
    m.box(fx0, fy1 - BOARD, zs - 0.02, fx1, fy1, top, boards[1])
    m.box(fx0, fy0 + BOARD, zs - 0.02, fx0 + BOARD, fy1 - BOARD, top, boards[2])
    m.box(fx1 - BOARD, fy0 + BOARD, zs - 0.02, fx1, fy1 - BOARD, top, boards[3])
    # Подшивка: доски по 14 см с щелью в 4 мм вдоль стен; уличная и задняя — во всю длину, торцевые — между ними.
    for y0, y1, x0, x1 in ((fy0 + BOARD, LY0, fx0 + BOARD, fx1 - BOARD), (LY1, fy1 - BOARD, fx0 + BOARD, fx1 - BOARD)):
        n = max(1, round((y1 - y0) / 0.14))
        for i in range(n):
            m.box(x0, y0 + (y1 - y0) * i / n + 0.002, zs, x1, y0 + (y1 - y0) * (i + 1) / n - 0.002, zs + BOARD, rnd.choice(boards))
    for x0, x1 in ((fx0 + BOARD, LX0), (LX1, fx1 - BOARD)):
        n = max(1, round((x1 - x0) / 0.14))
        for i in range(n):
            m.box(x0 + (x1 - x0) * i / n + 0.002, LY0, zs, x0 + (x1 - x0) * (i + 1) / n - 0.002, LY1, zs + BOARD, rnd.choice(boards))
    grain_along(m, fresh(m, start), wooden(mt), rnd)
    # Желоба: ось на 3 см внутрь от края кровли, верх — на 7,5 см ниже его; углы — точки пересечения осей.
    z = ze - 0.075
    gx0, gy0, gx1, gy1 = ex0 + 0.03, ey0 + 0.03, ex1 - 0.03, ey1 - 0.03
    for a, b, out in (((gx0, gy0), (gx1, gy0), (0.0, -1.0)), ((gx1, gy1), (gx0, gy1), (0.0, 1.0)),
                      ((gx0, gy1), (gx0, gy0), (-1.0, 0.0)), ((gx1, gy0), (gx1, gy1), (1.0, 0.0))):
        p, d, length = gutter(m, a, b, out, mt["tin"], z)
        hooks(m, p, d, length, out, mt["iron"])


def downpipes(m, mt, ze):
    """Водосточные трубы по углам уличной и задней стен: под желобом воронка, от неё колено к стене — отвод вниз и
    наискось под свесом, — прямая труба вниз на костылях с хомутами, внизу колено от стены с открытым устьем."""
    for side in ("front", "back"):
        face = faces()[side][0]
        gutter_d = OVER - 0.03
        for u in (X0 + PIPE_U, X1 - PIPE_U):
            # Выпуск из дна желоба в воронку.
            m.lathe(face.at(u, ze - 0.075 - GUTTER + 0.01, gutter_d), (0.0, 0.0, -1.0), [(0.0, 0.0), (0.0, 0.04), (0.08, 0.04), (0.08, 0.0)],
                    mt["tin"], 16)
            funnel = ze - 0.075 - GUTTER - 0.012
            m.lathe(face.at(u, funnel, gutter_d), (0.0, 0.0, -1.0),
                    [(0.03, 0.0), (0.03, 0.085), (0.0, 0.095), (0.0, 0.108), (0.025, 0.108), (0.03, 0.098), (0.17, PIPE_R + 0.008),
                     (0.2, PIPE_R + 0.008), (0.2, 0.0)], mt["tin"], 20)
            neck = funnel - 0.2
            elbow = neck - 0.12
            low = 0.42
            route = [face.at(u, neck + 0.01, gutter_d), face.at(u, elbow, gutter_d), face.at(u, elbow - (gutter_d - PIPE_OFF), PIPE_OFF),
                     face.at(u, low, PIPE_OFF), face.at(u, low - 0.24, PIPE_OFF + 0.24)]
            tube(m, bent(route), PIPE_R, mt["tin"], mt["soot"])
            # Хомуты с костылями: на кирпиче — в стену, на срубе — в бревно (его грань на 6 см глубже кирпичной).
            for z in (0.9, 2.0, 3.0, 4.2, 4.9):
                wall = -0.03 if z < TOP else -0.1
                m.lathe(face.at(u, z, PIPE_OFF), (0.0, 0.0, 1.0), [(-0.015, PIPE_R + 0.001), (0.015, PIPE_R + 0.001), (0.015, PIPE_R + 0.006),
                                                                   (-0.015, PIPE_R + 0.006)], mt["iron"], 16, closed=True)
                m.beam(face.at(u, z, PIPE_OFF - PIPE_R - 0.004), face.at(u, z, wall), 0.006, 0.03, mt["iron"])
            # Пояски стыков звеньев по прямой трубе.
            for z in (1.45, 2.55, 3.95):
                m.lathe(face.at(u, z, PIPE_OFF), (0.0, 0.0, 1.0), [(-0.012, PIPE_R), (0.012, PIPE_R), (0.012, PIPE_R + 0.003),
                                                                   (-0.012, PIPE_R + 0.003)], mt["tin"], 16, closed=True)


def porch(m, mt):
    """Крыльцо к улице: дощатая площадка на уровне пола, обшитая снизу, три ступени, два точёных столба, перила с
    балясинами по бокам; козырёк из железа щипцом к улице на брусе по столбам: щипец зашит досками и стоит на брусе,
    вверху — под самым железом, по краям белые причелины, сведённые у конька в ус; на щипце синяя эмалевая
    табличка — буквы ставит `build`. Волокна столбов, перил и бруса — вдоль них."""
    face, _, _ = faces()["front"]
    boards = mt["boards"]
    rnd = random.Random(29)
    face.box(m, -PORCH_HALF, PORCH_HALF, PLINTH - 0.07, PLINTH, 0.0, PORCH_D, boards[0])
    face.box(m, -PORCH_HALF + 0.03, PORCH_HALF - 0.03, GROUND, PLINTH - 0.07, 0.0, PORCH_D - 0.04, mt["plank"])
    for z, d0, d1 in ((0.4, PORCH_D, PORCH_D + 0.3), (0.2, PORCH_D + 0.3, PORCH_D + 0.6)):
        face.box(m, -0.95, 0.95, GROUND, z, d0, d1, boards[1])
    profile = [(0.0, 0.0), (0.0, 0.085), (0.16, 0.085), (0.2, 0.06), (0.55, 0.055), (0.6, 0.075), (0.68, 0.055), (1.55, 0.05),
               (1.62, 0.072), (1.7, 0.05), (2.3, 0.05), (2.36, 0.08), (CANOPY_EAVE - 0.1 - PLINTH, 0.08), (CANOPY_EAVE - 0.1 - PLINTH, 0.0)]
    along = face.at(1.0, 0.0, 0.0) - face.at(0.0, 0.0, 0.0)
    out = face.at(0.0, 0.0, 1.0) - face.at(0.0, 0.0, 0.0)
    for u in (-POST_U, POST_U):
        start = len(m.bm.faces)
        m.lathe(face.at(u, PLINTH, POST_D), (0.0, 0.0, 1.0), profile, boards[2], 16)
        turn_uv(m, fresh(m, start))
        start = len(m.bm.faces)
        for z in (PLINTH + 0.1, PLINTH + 0.88):
            m.beam(face.at(u, z, POST_D), face.at(u, z, 0.0), 0.06, 0.06, boards[3])
        grain(m, fresh(m, start), out, (rnd.random(), rnd.random()))
        d = 0.15
        while d < POST_D - 0.08:
            start = len(m.bm.faces)
            face.box(m, u - 0.02, u + 0.02, PLINTH + 0.1, PLINTH + 0.88, d - 0.02, d + 0.02, boards[3])
            grain(m, fresh(m, start), (0.0, 0.0, 1.0), (rnd.random(), rnd.random()))
            d += 0.16
        rail_end = 0.95 if u > 0 else -0.95
        start = len(m.bm.faces)
        m.beam(face.at(u, PLINTH + 0.88, POST_D), face.at(rail_end, PLINTH + 0.88, POST_D), 0.06, 0.06, boards[3])
        grain(m, fresh(m, start), along, (rnd.random(), rnd.random()))
    start = len(m.bm.faces)
    face.box(m, -PORCH_HALF - 0.05, PORCH_HALF + 0.05, CANOPY_EAVE - 0.1, CANOPY_EAVE, POST_D - 0.06, HEADER, boards[2])
    grain(m, fresh(m, start), along, (rnd.random(), rnd.random()))
    half = PORCH_HALF + 0.2
    rise = (CANOPY_RIDGE - CANOPY_EAVE) / half

    def under(u):
        """Низ железа козырька над точкой `u` — без 2 мм."""
        return CANOPY_EAVE + (half - abs(u)) * rise - 0.04 * math.sqrt(1.0 + rise * rise) - 0.002

    for sign in (-1, 1):
        m.plate([face.at(sign * half, CANOPY_EAVE, CANOPY_D), face.at(sign * half, CANOPY_EAVE, 0.0),
                 face.at(0.0, CANOPY_RIDGE, 0.0), face.at(0.0, CANOPY_RIDGE, CANOPY_D)], 0.04, mt["roof"])
        start = len(m.bm.faces)
        face.shape(m, [(sign * half, under(half)), (0.0, under(0.0)), (0.0, under(0.0) - 0.12), (sign * half, under(half) - 0.12)],
                   CANOPY_D - 0.04, CANOPY_D - 0.01, mt["paint"])
        grain(m, fresh(m, start), face.at(-sign * half, CANOPY_RIDGE - CANOPY_EAVE, 0.0) - face.at(0.0, 0.0, 0.0), (rnd.random(), rnd.random()))
    start = len(m.bm.faces)
    face.shape(m, [(-PORCH_HALF, CANOPY_EAVE), (PORCH_HALF, CANOPY_EAVE), (PORCH_HALF, under(PORCH_HALF)), (0.0, under(0.0)),
                   (-PORCH_HALF, under(PORCH_HALF))], HEADER - 0.05, HEADER, boards[0])
    grain(m, fresh(m, start), (0.0, 0.0, 1.0), (rnd.random(), rnd.random()))
    face.box(m, -0.35, 0.35, CANOPY_EAVE + 0.02, CANOPY_EAVE + 0.22, HEADER, HEADER + 0.02, mt["enamel"])
    for a0, a1, b0, b1 in ((-0.32, 0.32, 0.04, 0.055), (-0.32, 0.32, 0.185, 0.2), (-0.32, -0.305, 0.04, 0.2), (0.305, 0.32, 0.04, 0.2)):
        face.box(m, a0, a1, CANOPY_EAVE + b0, CANOPY_EAVE + b1, HEADER + 0.02, HEADER + 0.025, mt["enamel_white"])


def back_steps(m, mt):
    """Две каменные ступени у задней двери."""
    face, _, _ = faces()["back"]
    for z, d1 in ((0.3, 0.9), (PLINTH, 0.5)):
        face.box(m, BACK_DOOR - 0.75, BACK_DOOR + 0.75, GROUND, z, 0.0, d1, mt["stone"])


def fences(m, mt):
    """Дощатый забор слева от дома и сзади; доски, столбы и слеги — волокнами вдоль себя."""
    rnd = random.Random(23)
    start = len(m.bm.faces)
    plank_fence(m, (FENCE_X, FENCE_BACK_Y), (FENCE_X, FENCE_Y0), 1.8, mt["fence"], mt["post"], rnd)
    plank_fence(m, (FENCE_BACK_X1, FENCE_BACK_Y), (FENCE_X, FENCE_BACK_Y), 1.8, mt["fence"], mt["post"], rnd)
    grain_along(m, fresh(m, start), wooden(mt), random.Random(24))


def end_grain(m, mi):
    """Торцы брёвен — срезом с кольцами: круг картинки ложится на торец по его радиусу, у каждого торца повёрнут
    по-своему."""
    rnd = random.Random(33)
    cx, cy, cr = END_DISC
    for f in m.bm.faces:
        if f.material_index != mi:
            continue
        f.normal_update()
        c = f.calc_center_median()
        e1 = f.normal.orthogonal().normalized()
        e2 = f.normal.cross(e1)
        radius = max((v.co - c).length for v in f.verts)
        turn = rnd.uniform(0.0, 2.0 * math.pi)
        cos, sin = math.cos(turn), math.sin(turn)
        for loop in f.loops:
            q = loop.vert.co - c
            x, y = q.dot(e1) / radius, q.dot(e2) / radius
            loop[m.uv].uv = (cx + cr * (x * cos - y * sin), cy + cr * (x * sin + y * cos))
        m.manual.add(f)


def build(out=OUT):
    lib.reset()
    rnd = random.Random(1890)
    m = Estate("kontora")
    mt = materials(m)
    lower(m, mt, rnd)
    top = upper(m, mt, rnd)
    ze = roof(m, mt, top)
    cornice(m, mt, ze)
    downpipes(m, mt, ze)
    porch(m, mt)
    back_steps(m, mt)
    fences(m, mt)
    dense(m, {mt["wall"], mt["brick"]})
    end_grain(m, mt["log_end"])
    m.project()
    m.grid()
    for i, (line, half) in enumerate(PATHS):
        m.path(line, half, mt["path"], 80 + i)
    obj = m.finish()
    # Стёкла на время запекания отделены и не видны лучам: иначе занавески и комнаты за ними чернеют, будто за
    # стеной; самим стёклам — ровный светлый тон.
    panes = split_off(obj, [obj.data.materials[mt["glass"]]])
    panes.hide_render = True
    bake_occlusion(obj, mt["path"])
    panes.hide_render = False
    panes.data.attributes.remove(panes.data.attributes["fade"])
    shade = panes.data.color_attributes.new("occlusion", "BYTE_COLOR", "CORNER")
    for item in shade.data:
        item.color = (GLASS_SHADE, GLASS_SHADE, GLASS_SHADE, 1.0)
    bpy.ops.object.select_all(action="DESELECT")
    panes.select_set(True)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.join()
    obj.data.color_attributes.active_color = obj.data.color_attributes["occlusion"]
    obj.data.color_attributes.render_color_index = obj.data.color_attributes.active_color_index
    lib.text(obj, "КОНТОРА", 0.13, (0.0, Y0 - HEADER - 0.026, CANOPY_EAVE + 0.12), 0, "#f2efe6", depth=0.006)  # буквы — на эмали
    pts = [obj.matrix_world @ Vector(c) for c in obj.bound_box]
    print("kontora size", *(round(max(p[i] for p in pts) - min(p[i] for p in pts), 2) for i in range(3)), "faces", len(obj.data.polygons))
    os.makedirs(out, exist_ok=True)
    lib.export(os.path.join(out, "kontora.glb"), vertex_color="ACTIVE")
    print("written kontora")


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    build(args[0] if args else OUT)


if __name__ == "__main__":
    main()
