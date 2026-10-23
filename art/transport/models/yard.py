# Двор крестьянской семьи начальной деревушки, 1930 год, по образцу art/transport/village/concept-max.png — пока
# обычный, средний по достатку. Изба 6 × 7 м — сруб «в обло» из круглых выветренных брёвен с выпусками на углах,
# на каменном цоколе, щипцом на улицу; на уличном фасаде три окна, сбоку ещё три, сзади одно; окна — с обсадой,
# переплётом и белыми резными наличниками: подоконная доска с подзором, сверху карниз и кокошник с солнышком.
# Крыша — дранка рядами на крутых скатах, охлупень с коньком-головой, фронтон обшит досками с нащельниками,
# слуховое окно, причелины и полотенце; кирпичная труба. Справа к избе пристроен крытый двор, обшитый досками до
# самой крыши, щипцом на улицу: ворота, калитка, сеновал. У ворот колодец с воротом под крышей, у правой изгороди
# поленница берёзовых и сосновых дров; за жердевой изгородью огород — картошка в окученных рядах, капуста на
# грядах, три грядки лука в дощатых бортах — и баня в глубине участка. С улицы — штакетник с воротами и калиткой
# напротив ворот и двери крытого двора, сбоку от него палисадник; между дворами и сзади — жердевые изгороди.
# Земли, травы и деревьев в модели нет: землю и траву рисует движок, деревья — свои модели; тропы — часть двора, их
# край прозрачный и растворяется в земле карты. Верх земли — ноль.
# Цвета частей разведены по светлоте: светлее всего дранка крыш и штакетник, темнее — бревенчатые стены, ещё темнее —
# ворота и жерди; каждое бревно, доска и дощечка дранки — одним из нескольких тонов своей части.
# Улица — со стороны −Y Blender, начало — середина участка по ширине на линии улицы, единица — метр; размер
# настоящий. Материалы — карты ambientCG и Poly Haven из art/transport/textures в настоящем размере, как у гаража;
# затенение запечено в цвет вершин.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/yard.py [-- <папка>]
import math
import os
import random
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402
from bakery import GRIME, noise  # noqa: E402
from garage import Yard  # noqa: E402
from gazel import smoothstep  # noqa: E402

OUT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "models"))

GROUND = 0.0  # земля участка — земля карты движка; ниже неё в модели ничего нет: движок ставит самую низкую точку модели на землю

PLOT = (-10.0, 0.3, 10.0, 34.0)  # изгороди участка

# Изба: оси венцов, радиус бревна, шаг венца по высоте — брёвна врублены в чашу и лежат плотнее своего диаметра,
# выпуск брёвен за угол, верх цоколя, число венцов боковых стен.
HX0, HX1, HY0, HY1 = -9.0, -3.0, 3.0, 10.0
XM = (HX0 + HX1) / 2
LOG_R = 0.13
STEP = 0.22
OUTLET = 0.3
BASE = GROUND + 0.4
COURSES = 13
SLOPE = 0.85
OVER = 0.65  # свес крыши от оси боковой стены
RAKE = 0.25  # свес крыши за выпуски брёвен над фронтонами
SHINGLE = 0.15  # на столько открыт ряд дранки
WIN_W, WIN_Z0, WIN_Z1 = 0.72, BASE + 1.0, BASE + 2.05

# Крытый двор справа от избы, щипцом на улицу: стены высотой `SH` — левый скат крыши проходит под свесом крыши
# избы, — ворота и калитка в уличной стене.
SX0, SX1, SY0, SY1 = HX1 + LOG_R, HX1 + LOG_R + 7.0, 5.0, 13.0
SH = 2.45
S_SLOPE = 0.55
BOARD = 0.05  # толщина досок обшивки
GATE, GATE_H = (SX0 + 0.6, SX0 + 3.6), 2.2
DOOR, DOOR_H = (SX0 + 4.3, SX0 + 5.2), 1.9
BACK_DOOR = (2.5, 3.3)

WELL = (5.6, 2.5)
WOODPILE = (8.3, 4.2, 9.2)  # x торцов, начало и конец по y
FENCE_Y = 14.2  # жердевая изгородь между двором и огородом
GARDEN_GATE = (3.2, 4.6)
GARDEN = (-9.6, 14.7, 9.6, 30.4)
WALK = 3.9  # тропа через огород к бане: между картошкой и капустой
BATH = (4.6, 7.4, 31.0, 33.2)
PALISADE = -2.6  # правая сторона палисадника

# Тропы — часть двора: протоптанная середина, неровный край растворяется в земле карты. Ломаная, полуширина, колея
# телеги (две колеи и трава между ними): от улицы через ворота к воротам крытого двора, от калитки к его двери, от неё
# к колодцу и к поленнице, от задней двери через огород к бане.
# Концы троп уходят под ворота, двери, колодец и поленницу: тропа тает к концу, и протоптанное доходит до них вплотную.
# С улицы тропы начинаются под укатанным краем дороги, иначе между тропой и дорогой остаётся полоса травы. Двор ставится
# началом в 9,3 м от оси главной улицы (12 м); её укатанный грунт кончается в 4,5 м от оси и дальше растворяется в траве
# до 6 м — тропа доходит в полную силу до 4,7 м от оси и гаснет уже под дорогой.
STREET = -5.6
PATHS = [
    ([((GATE[0] + GATE[1]) / 2, STREET), ((GATE[0] + GATE[1]) / 2 + 0.1, 2.5), ((GATE[0] + GATE[1]) / 2, SY0 + 0.7)], 1.35, True),
    ([((DOOR[0] + DOOR[1]) / 2, STREET), ((DOOR[0] + DOOR[1]) / 2 - 0.1, 2.6), ((DOOR[0] + DOOR[1]) / 2, SY0 + 0.6)], 0.42, False),
    ([((DOOR[0] + DOOR[1]) / 2, 3.2), (3.6, 2.7), (WELL[0] - 0.1, WELL[1] - 0.1)], 0.36, False),
    ([((DOOR[0] + DOOR[1]) / 2 + 0.2, 4.4), (4.6, 4.9), (6.6, 6.0), (WOODPILE[0] + 0.3, (WOODPILE[1] + WOODPILE[2]) / 2)], 0.38, False),
    ([((BACK_DOOR[0] + BACK_DOOR[1]) / 2, SY1 - 0.6), (WALK - 0.2, FENCE_Y), (WALK, 22.0), (WALK + 0.1, 29.4), (5.3, BATH[2] + 0.5)], 0.4, False),
]


class Face:
    """Стена в своих осях: u — вдоль стены, z — вверх, d — от оси стены наружу. `side` — куда смотрит стена:
    front — на улицу (−Y), back — +Y, left — −X, right — +X; `axis` — координата оси стены."""

    def __init__(self, side, axis):
        self.side, self.axis = side, axis

    def at(self, u, z, d):
        if self.side == "front":
            return Vector((u, self.axis - d, z))
        if self.side == "back":
            return Vector((u, self.axis + d, z))
        if self.side == "left":
            return Vector((self.axis - d, u, z))
        return Vector((self.axis + d, u, z))

    def box(self, m, u0, u1, z0, z1, d0, d1, mi):
        a, b = self.at(u0, z0, d0), self.at(u1, z1, d1)
        m.box(min(a.x, b.x), min(a.y, b.y), min(a.z, b.z), max(a.x, b.x), max(a.y, b.y), max(a.z, b.z), mi)

    def shape(self, m, outline, d0, d1, mi):
        """Плоская выпуклая фигура по точкам (u, z) толщиной от `d0` до `d1`."""
        m.prism([self.at(u, z, d0) for u, z in outline], self.at(0.0, 0.0, d1) - self.at(0.0, 0.0, d0), mi)


class Homestead(Yard):
    """Сетка двора: к брусьям и пластинам гаража — круглые брёвна с корой или торцами своим материалом и комки
    ботвы."""

    def log(self, a, b, r0, r1, mi, end, segments=12, ring=0.7, shift=(0.0, 0.0)):
        """Бревно от `a` до `b`: радиус у `a` — `r0`, у `b` — `r1`; кольца вершин через `ring` м — затенению в
        вершинах нужны вершины вдоль бревна. Развёртка — вдоль бревна и по окружности в метрах материала, сдвинутая на
        `shift`, чтобы соседние брёвна не повторяли друг друга; торцы — материалом `end`, их рёбра острые."""
        a, b = Vector(a), Vector(b)
        axis = b - a
        length = axis.length
        d = axis / length
        e1 = d.orthogonal().normalized()
        e2 = d.cross(e1)
        meters = self.tiles.get(mi, (2.0, False))[0]
        n = max(1, round(length / ring))
        dirs = [e1 * math.cos(2 * math.pi * k / segments) + e2 * math.sin(2 * math.pi * k / segments) for k in range(segments)]
        radius = [r0 + (r1 - r0) * i / n for i in range(n + 1)]
        rings = [[self.bm.verts.new(a + axis * (i / n) + v * radius[i]) for v in dirs] for i in range(n + 1)]
        for i in range(n):
            for k in range(segments):
                corners = ((i, k), (i, k + 1), (i + 1, k + 1), (i + 1, k))
                f = self.bm.faces.new([rings[ii][kk % segments] for ii, kk in corners])
                f.material_index = mi
                f.smooth = True
                self.manual.add(f)
                for loop, (ii, kk) in zip(f.loops, corners):
                    loop[self.uv].uv = (length * ii / n / meters + shift[0], 2 * math.pi * radius[ii] * kk / segments / meters + shift[1])
        for cap in (list(reversed(rings[0])), rings[-1]):
            self.bm.faces.new(cap).material_index = end
            for k in range(segments):
                self.bm.edges.get((cap[k], cap[(k + 1) % segments])).smooth = False

    def path(self, line, half, mi, seed, ruts=False, step=0.25, columns=12):
        """Тропа по ломаной `line` на плане: полоса с поперечными рядами вершин через `step` м, середина гуляет в
        стороны, ширина около `2 × half` меняется по длине. Насколько тропа видна, пишется в слой вершин `fade`: в
        середине 1, к неровному краю и к концам — 0; у колеи телеги (`ruts`) между колеями трава — тропа там бледнее.
        Развёртка — в метрах материала по плану."""
        fade = self.bm.verts.layers.float.get("fade") or self.bm.verts.layers.float.new("fade")
        meters = self.tiles[mi][0]
        lengths = [0.0]
        for (ax, ay), (bx, by) in zip(line, line[1:]):
            lengths.append(lengths[-1] + math.hypot(bx - ax, by - ay))
        total = lengths[-1]

        def at(s):
            """Точка ломаной на `s` м от начала и направление там."""
            i = max(0, min(len(line) - 2, next((k for k in range(len(lengths) - 1) if s <= lengths[k + 1]), len(line) - 2)))
            (ax, ay), (bx, by) = line[i], line[i + 1]
            t = (s - lengths[i]) / max(1e-6, lengths[i + 1] - lengths[i])
            d = Vector((bx - ax, by - ay)).normalized()
            return Vector((ax + (bx - ax) * t, ay + (by - ay) * t)), d

        rows = max(2, round(total / step))
        grid = []
        for r in range(rows + 1):
            s = total * r / rows
            c, d = at(s)
            n = Vector((-d.y, d.x))
            c = c + n * (0.3 * half * (noise(s / 2.2, 0.5, seed) - 0.5))
            w = half * (0.8 + 0.4 * noise(s / 3.0, 1.5, seed + 1))
            ends = smoothstep(0.0, 0.9, s) * smoothstep(0.0, 0.9, total - s)
            ring = []
            for j in range(columns + 1):
                t = -1.35 + 2.7 * j / columns
                p = c + n * (t * w)
                ragged = abs(t) + 0.5 * (noise(s * 1.4, t * 2.0 + 3.0, seed + 2) - 0.5)
                a = (1.0 - smoothstep(0.55, 1.25, ragged)) * ends
                if ruts:
                    a *= 1.0 - 0.6 * math.exp(-(t / 0.3) ** 2) * (0.6 + 0.4 * noise(s / 1.5, 7.0, seed + 3))
                v = self.bm.verts.new((p.x, p.y, GROUND + 0.012 + 0.006 * a))
                v[fade] = a
                ring.append(v)
            grid.append(ring)
        for r in range(rows):
            for j in range(columns):
                f = self.bm.faces.new((grid[r][j], grid[r][j + 1], grid[r + 1][j + 1], grid[r + 1][j]))
                f.material_index = mi
                f.smooth = True
                self.manual.add(f)
                self.up.append(f)
                for loop in f.loops:
                    loop[self.uv].uv = (loop.vert.co.x / meters, loop.vert.co.y / meters)

    def clump(self, center, radius, mi, rnd, squash=0.85, subdiv=2, wobble=0.22):
        """Комок ботвы: гладкая сфера радиусом `radius`, сплюснутая по высоте до `squash`, с буграми на `wobble`
        радиуса."""
        res = bmesh.ops.create_icosphere(self.bm, subdivisions=subdiv, radius=1.0)
        seed = rnd.randint(0, 9999)
        c = Vector(center)
        for v in res["verts"]:
            n = v.co.normalized()
            k = 1.0 + wobble * 2.0 * (noise(n.x * 2.2 + 3.0 * n.z, n.y * 2.2 - 2.0 * n.z, seed) - 0.5)
            v.co = c + Vector((n.x * radius * k, n.y * radius * k, n.z * radius * squash * k))
        for f in {f for v in res["verts"] for f in v.link_faces}:
            f.material_index = mi
            f.smooth = True


def tones(m, key, folder, meters, tints, swap=False):
    """Несколько материалов одной картинки разными тонами: каждое бревно или доска берёт один из них."""
    return [m.tex(f"{key}_{i}", folder, meters, tint=t, swap=swap) for i, t in enumerate(tints)]


def faded(m, key, folder, meters, tint):
    """Материал, прозрачный по альфе цвета вершин `occlusion`: край тропы растворяется в земле карты. Выгрузка пишет
    его режимом BLEND, движок умножает прозрачность на альфу цвета вершины, как у кусков дорог. Цвет вершин умножен и
    на цвет материала — так его понимает glTF; без этого выгрузка теряет альфу («Vertex Color alpha without color»)."""
    mi = m.tex(key, folder, meters, tint=tint)
    mat = m.materials[mi]
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = lib.bsdf(mat)
    attr = nodes.new("ShaderNodeVertexColor")
    attr.layer_name = "occlusion"
    mix = nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs[0].default_value = 1.0
    links.new(bsdf.inputs["Base Color"].links[0].from_socket, mix.inputs[6])
    links.new(attr.outputs["Color"], mix.inputs[7])
    links.new(mix.outputs[2], bsdf.inputs["Base Color"])
    links.new(attr.outputs["Alpha"], bsdf.inputs["Alpha"])
    mat.surface_render_method = "BLENDED"
    return mi


def materials(m):
    return {
        # Светлота частей после карты и подкраски, примерно: дранка крыш 0,5, штакетник 0,52, бревенчатые стены 0,33,
        # обшивка двора 0,47, фронтон избы 0,4, ворота 0,3, жерди 0,24, наличники 0,85.
        "logs": tones(m, "log", "Wood033", 1.6, ("#8f7c66", "#83715d", "#9a8771", "#7a6a59")),
        "bath_logs": tones(m, "bath_log", "Wood033", 1.6, ("#6f604f", "#655747", "#7a6a58")),
        "log_end": m.mat("log_end", "#8a7660", 0.9),
        "roofs": tones(m, "roof", "Wood033", 1.8, ("#c4bdb2", "#b6aea2", "#a9a094", "#cdc6bb", "#a8ae8c")),
        "gables": tones(m, "gable", "Wood086", 1.6, ("#d0c4b4", "#c2b5a4", "#dad0c2"), swap=True),
        "boards": tones(m, "board", "Wood033", 1.6, ("#b8b0a4", "#a9a195", "#c3bbaf", "#a19a8f"), swap=True),
        "batten": m.tex("batten", "Wood086", 1.6, tint="#a4988a", swap=True),
        "gate": m.tex("gate", "Wood059", 1.4, tint="#9a958c", swap=True),
        "picket": m.tex("picket", "Wood033", 1.2, tint="#cbc5ba"),
        "pole": m.tex("pole", "Wood059", 1.4, tint="#8b8079"),
        "plank": m.tex("plank", "Planks012", 2.1, tint="#c8c2ba"),
        "paint": m.tex("paint", "PaintedWood009C", 1.2, tint="#fbf9f4"),
        "stone": m.tex("stone", "white_sandstone_bricks_03", 2.0, tint="#a39e95"),
        "brick": m.tex("brick", "brick_wall_001", 3.0),
        "soil": m.tex("soil", "Ground103", 2.5, tint="#8a7866"),
        "path": faded(m, "path", "Ground103", 2.5, "#e6dccd"),
        "glass": m.mat("glass", "#222b2b", 0.1, 0.3),
        "dark": m.mat("dark", "#161412", 0.95),
        "tin": m.mat("tin", "#7c8285", 0.5, 0.6),
        "iron": m.mat("iron", "#2c2d2f", 0.55, 0.5),
        "potato": m.mat("potato", "#4a6a2b", 0.85),
        "potato_light": m.mat("potato_light", "#5b7c33", 0.85),
        "potato_dark": m.mat("potato_dark", "#3b5321", 0.85),
        "cabbage": m.mat("cabbage", "#9cb487", 0.55),
        "cabbage_leaf": m.mat("cabbage_leaf", "#6b8c55", 0.7),
        "onion": m.mat("onion", "#5f8a3c", 0.7),
        "birch": m.mat("birch", "#d8d3c6", 0.8),
        "pine": m.mat("pine", "#6f5a45", 0.9),
        "firewood_end": m.mat("firewood_end", "#bd9f75", 0.9),
    }


def wall_log(m, mt, a, b, r, holes, logs, rnd):
    """Бревно венца от `a` до `b` одним из тонов `logs`, разрезанное проёмами `holes` (u0, u1, z0, z1), через которые
    проходит: комель толще вершины, комли соседних венцов — в разные стороны."""
    a, b = Vector(a), Vector(b)
    along = 0 if abs(b.x - a.x) > abs(b.y - a.y) else 1
    zc = a.z
    cuts = sorted((u0, u1) for u0, u1, z0, z1 in holes if zc + r * 0.55 > z0 and zc - r * 0.55 < z1)
    rr = r * rnd.uniform(0.93, 1.07)
    butt = rnd.random() < 0.5
    tone = rnd.choice(logs)
    lo, hi = a[along], b[along]
    pieces, start = [], lo
    for u0, u1 in cuts:
        pieces.append((start, u0))
        start = u1
    pieces.append((start, hi))
    shift = (rnd.uniform(0.0, 1.0), rnd.uniform(0.0, 1.0))

    def radius(u):
        t = (u - lo) / (hi - lo)
        return rr * (1.05 - 0.1 * (t if butt else 1.0 - t))

    for s0, s1 in pieces:
        if s1 - s0 < 0.05:
            continue
        p0, p1 = a.copy(), a.copy()
        p0[along], p1[along] = s0, s1
        m.log(p0, p1, radius(s0), radius(s1), tone, mt["log_end"], shift=shift)


def cabin(m, mt, box, r, step, courses, base, outlet, holes, logs, rnd):
    """Сруб «в обло» по осям стен `box` = (x0, x1, y0, y1) из брёвен тонов `logs`: боковые стены (вдоль Y) — `courses`
    венцов через `step`, стены вдоль X сдвинуты на полвенца вверх и на венец ниже — над ними фронтоны; нижний венец
    боковых стен утоплен в цоколь. На углах брёвна выпущены на `outlet`. `holes` — проёмы по стенам
    {"front"|"back"|"left"|"right": [(u0, u1, z0, z1)]}. Возвращает высоту верха боковых стен."""
    x0, x1, y0, y1 = box
    for i in range(courses):
        zy = base + r + (i - 0.5) * step
        for x, side in ((x0, "left"), (x1, "right")):
            wall_log(m, mt, (x, y0 - outlet, zy), (x, y1 + outlet, zy), r, holes.get(side, ()), logs, rnd)
        if i < courses - 1:
            zx = base + r + i * step
            for y, side in ((y0, "front"), (y1, "back")):
                wall_log(m, mt, (x0 - outlet, y, zx), (x1 + outlet, y, zx), r, holes.get(side, ()), logs, rnd)
    return base + 2 * r + (courses - 1.5) * step


def boards(m, face, u0, u1, z0, top, d0, d1, palette, rnd, peak=None, width=0.2):
    """Обшивка вертикальными досками по стене `face` от `u0` до `u1`: снизу `z0`, верх доски — `top(u)`, по скату
    крыши; если доска закрывает конёк `peak`, её верх ломается по нему. Доски около `width` м со щелями по сантиметру,
    толщиной от `d0` до `d1`, каждая своим тоном из `palette`."""
    n = max(1, round((u1 - u0) / width))
    w = (u1 - u0) / n
    for i in range(n):
        a, b = u0 + i * w + 0.005, u0 + (i + 1) * w - 0.005
        outline = [(a, z0), (b, z0), (b, top(b))]
        if peak is not None and a < peak < b:
            outline.append((peak, top(peak)))
        outline.append((a, top(a)))
        face.shape(m, outline, d0, d1, rnd.choice(palette))


def window(m, mt, face, r, u0, u1, z0, z1, carved=True):
    """Окно в срубе: обсада из досок в проёме, рама с переплётом и стеклом посередине стены; снаружи наличник —
    боковые доски, подоконная доска с зубчатым подзором, карниз и кокошник с солнышком (`carved`), или только
    гладкие доски."""
    plank, paint = mt["plank"], mt["paint"]
    depth = r * 0.85
    # Венец, задевший верх проёма, разрезан, и до низа следующего целого венца остаётся щель: её закрывает
    # высокая верхняя доска обсады.
    for a0, a1, b0, b1 in ((u0 - 0.07, u0, z0 - 0.07, z1 + 0.17), (u1, u1 + 0.07, z0 - 0.07, z1 + 0.17),
                           (u0, u1, z0 - 0.07, z0), (u0, u1, z1, z1 + 0.17)):
        face.box(m, a0, a1, b0, b1, -depth, depth, plank)
    face.box(m, u0, u1, z0, z1, -0.01, 0.0, mt["glass"])
    um = (u0 + u1) / 2
    zt = z1 - (z1 - z0) * 0.33
    for a0, a1, b0, b1 in ((u0, u0 + 0.05, z0, z1), (u1 - 0.05, u1, z0, z1), (u0, u1, z0, z0 + 0.05), (u0, u1, z1 - 0.05, z1),
                           (um - 0.022, um + 0.022, z0, z1), (u0, u1, zt - 0.02, zt + 0.02)):
        face.box(m, a0, a1, b0, b1, 0.0, 0.04, paint)
    lo, hi = r - 0.025, r + 0.015
    face.box(m, u0 - 0.15, u0 - 0.01, z0 - 0.11, z1 + 0.06, lo, hi, paint)
    face.box(m, u1 + 0.01, u1 + 0.15, z0 - 0.11, z1 + 0.06, lo, hi, paint)
    face.box(m, u0 - 0.21, u1 + 0.21, z0 - 0.17, z0 - 0.11, lo, hi + 0.07, paint)
    face.box(m, u0 - 0.24, u1 + 0.24, z1 + 0.06, z1 + 0.13, lo, hi + 0.06, paint)
    if not carved:
        return
    face.box(m, u0 - 0.15, u1 + 0.15, z0 - 0.3, z0 - 0.17, lo, hi, paint)
    teeth = 6
    width = (u1 - u0 + 0.3) / teeth
    for i in range(teeth):
        a0 = u0 - 0.15 + i * width
        face.shape(m, [(a0, z0 - 0.3), (a0 + width, z0 - 0.3), (a0 + width / 2, z0 - 0.38)], lo, hi, paint)
    face.shape(m, [(u0 - 0.2, z1 + 0.13), (u1 + 0.2, z1 + 0.13), (um, z1 + 0.43)], lo, hi, paint)
    face.shape(m, [(um + 0.12 * math.cos(math.pi * k / 8), z1 + 0.15 + 0.12 * math.sin(math.pi * k / 8)) for k in range(9)],
               hi, hi + 0.015, paint)


def gable_end(m, mt, face, x0, x1, z_wall, z_eave, slope, depth, rnd):
    """Фронтон сруба, обшитый вертикальными досками с нащельниками через 0,22 м: от верха стены `z_wall` до скатов,
    которые у краёв `x0`, `x1` стоят на высоте `z_eave` и поднимаются на `slope` к середине. Обшивка — на `depth` от
    оси стены."""
    xm = (x0 + x1) / 2

    def roof_z(u):
        return z_eave + ((x1 - x0) / 2 - abs(u - xm)) * slope - 0.04

    boards(m, face, x0, x1, z_wall, roof_z, depth - 0.05, depth, mt["gables"], rnd, peak=xm, width=0.22)
    u = x0 + 0.22
    while u < x1 - 0.1:
        face.box(m, u - 0.03, u + 0.03, z_wall, roof_z(u) - 0.02, depth, depth + 0.022, mt["batten"])
        u += 0.22


def roof(m, mt, x0, x1, y0, y1, z0, slope, over_left, over_right, rnd):
    """Крыша из дранки, конёк вдоль Y: скаты от осей стен `x0` и `x1`, где низ ската на высоте `z0`, к коньку
    посередине; свесы за оси стен — `over_*`, вдоль Y крыша от `y0` до `y1`. Дранка — колотые дощечки по 10–18 см
    рядами от свеса к коньку: ряд открыт на `SHINGLE` м, нижним краем лежит на предыдущем, стыки соседних рядов
    вразбежку; каждая дощечка своим тоном, редкая — с зеленцой мха. Возвращает высоту конька."""
    xm = (x0 + x1) / 2
    ridge = z0 + (xm - x0) * slope
    for edge, wall in ((x0 - over_left, x0), (x1 + over_right, x1)):
        ze = z0 - abs(edge - wall) * slope
        start = Vector((edge, 0.0, ze))
        up = Vector((xm - edge, 0.0, ridge - ze))
        length = up.length
        up /= length
        normal = Vector((up.z, 0.0, -up.x))
        if normal.z < 0:
            normal = -normal
        rows = math.ceil(length / SHINGLE)
        for k in range(rows):
            s0 = k * SHINGLE
            s1 = min(length, s0 + SHINGLE + 0.12)
            ya = y0 - rnd.uniform(0.0, 0.12)
            while ya < y1:
                yb = min(y1, ya + rnd.uniform(0.1, 0.18))
                a = max(ya, y0)
                lo = s0 + rnd.uniform(-0.01, 0.01)
                tone = mt["roofs"][-1] if rnd.random() < 0.06 else rnd.choice(mt["roofs"][:-1])
                m.plate([start + up * lo + Vector((0.0, a + 0.004, 0.0)) + normal * 0.036,
                         start + up * lo + Vector((0.0, yb - 0.004, 0.0)) + normal * 0.036,
                         start + up * s1 + Vector((0.0, yb - 0.004, 0.0)) + normal * 0.006,
                         start + up * s1 + Vector((0.0, a + 0.004, 0.0)) + normal * 0.006], 0.012, tone)
                ya = yb
    return ridge


def ridge_log(m, mt, xm, y0, y1, ridge, horse, rnd):
    """Охлупень по коньку; на уличном конце `horse` — конёк-голова, выпиленный из толстой доски."""
    z = ridge + 0.07
    tone = rnd.choice(mt["logs"])
    m.log((xm, y0 - 0.1, z), (xm, y1 + 0.1, z), 0.16, 0.14, tone, mt["log_end"], 10, ring=1.0)
    if not horse:
        return
    ye = y0 - 0.1
    for outline in ([(0.0, 0.0), (0.0, 0.25), (-0.35, 0.55), (-0.45, 0.4)], [(-0.35, 0.55), (-0.75, 0.62), (-0.8, 0.5), (-0.45, 0.4)]):
        m.prism([(xm - 0.06, ye + dy, z + dz) for dy, dz in outline], (0.12, 0.0, 0.0), tone)


def house(m, mt, rnd):
    """Изба: цоколь, сруб с окнами, фронтоны, крыша из дранки с охлупнем, причелины, полотенце, слуховое окно, труба."""
    win = [(c - WIN_W / 2, c + WIN_W / 2, WIN_Z0, WIN_Z1) for c in (XM - 1.9, XM, XM + 1.9)]
    holes = {
        "front": win,
        "left": [(c - WIN_W / 2, c + WIN_W / 2, WIN_Z0, WIN_Z1) for c in (5.1, 7.9)],
        "right": [(3.95 - WIN_W / 2, 3.95 + WIN_W / 2, WIN_Z0, WIN_Z1)],
        "back": [(XM - WIN_W / 2, XM + WIN_W / 2, WIN_Z0, WIN_Z1)],
    }
    m.box(HX0 - LOG_R - 0.04, HY0 - LOG_R - 0.04, GROUND, HX1 + LOG_R + 0.04, HY1 + LOG_R + 0.04, BASE, mt["stone"])
    top = cabin(m, mt, (HX0, HX1, HY0, HY1), LOG_R, STEP, COURSES, BASE, OUTLET, holes, mt["logs"], rnd)
    faces = {"front": Face("front", HY0), "back": Face("back", HY1), "left": Face("left", HX0), "right": Face("right", HX1)}
    for side, openings in holes.items():
        for u0, u1, z0, z1 in openings:
            window(m, mt, faces[side], LOG_R, u0, u1, z0, z1)

    rz = top + 0.25  # скаты подняты над верхним венцом: под ними на углах проходят выпуски брёвен уличной стены
    wall_top = top - STEP / 2
    gable_z = rz - LOG_R * SLOPE
    for side in ("front", "back"):
        gable_end(m, mt, faces[side], HX0 - LOG_R, HX1 + LOG_R, wall_top, gable_z, SLOPE, LOG_R * 0.4, rnd)
    ya, yb = HY0 - OUTLET - 0.05 - RAKE, HY1 + OUTLET + 0.05 + RAKE
    ridge = roof(m, mt, HX0, HX1, ya, yb, rz, SLOPE, OVER, OVER, rnd)
    ridge_log(m, mt, XM, ya, yb, ridge, True, rnd)

    # Причелины по краям уличного ската и полотенце под коньком — белые, как наличники.
    y = ya + 0.045
    edge_z = rz - OVER * SLOPE
    for x in (HX0 - OVER, HX1 + OVER):
        m.beam((x, y, edge_z - 0.15), (XM, y, ridge - 0.15), 0.24, 0.03, mt["paint"], up=(0.0, 1.0, 0.0))
    front = Face("front", y)
    front.shape(m, [(XM - 0.12, ridge - 0.2), (XM + 0.12, ridge - 0.2), (XM + 0.12, ridge - 0.65), (XM, ridge - 0.78), (XM - 0.12, ridge - 0.65)],
                -0.015, 0.015, mt["paint"])

    # Слуховое окно во фронтоне — ниже полотенца: камера смотрит сверху, и полотенце на свесе крыши ложится на фронтон.
    gable = faces["front"]
    d = LOG_R * 0.4 + 0.022
    z0 = wall_top + 0.85
    gable.box(m, XM - 0.3, XM + 0.3, z0, z0 + 0.65, d, d + 0.03, mt["paint"])
    gable.box(m, XM - 0.22, XM + 0.22, z0 + 0.08, z0 + 0.57, d + 0.03, d + 0.032, mt["glass"])
    gable.box(m, XM - 0.02, XM + 0.02, z0 + 0.08, z0 + 0.57, d + 0.03, d + 0.05, mt["paint"])
    gable.shape(m, [(XM - 0.36, z0 + 0.65), (XM + 0.36, z0 + 0.65), (XM, z0 + 0.87)], d, d + 0.04, mt["paint"])

    # Кирпичная труба у конька на правом скате, с напуском поверху.
    cy = HY0 + 4.2
    m.box(XM + 0.3, cy - 0.27, rz, XM + 0.84, cy + 0.27, ridge + 0.75, mt["brick"])
    m.box(XM + 0.24, cy - 0.33, ridge + 0.75, XM + 0.9, cy + 0.33, ridge + 0.85, mt["brick"])


def covered_yard(m, mt, rnd):
    """Крытый двор: стены, обшитые вертикальными досками до самой крыши, угловые столбы, ворота в две створки и
    калитка на улицу, задняя дверь к огороду, дверца сеновала в уличном щипце, крыша из дранки, левым скатом
    подсунутая под свес крыши избы."""
    xm = (SX0 + SX1) / 2
    low = GROUND

    def gable_top(u):
        return SH + ((SX1 - SX0) / 2 - abs(u - xm)) * S_SLOPE

    def flat(_):
        return SH

    front, back = Face("front", SY0), Face("back", SY1)
    for u0, u1, z0 in ((SX0, GATE[0], low), (GATE[0], GATE[1], GATE_H), (GATE[1], DOOR[0], low), (DOOR[0], DOOR[1], DOOR_H), (DOOR[1], SX1, low)):
        boards(m, front, u0, u1, z0, gable_top, 0.0, BOARD, mt["boards"], rnd, peak=xm)
    for u0, u1, z0 in ((SX0, BACK_DOOR[0], low), (BACK_DOOR[0], BACK_DOOR[1], 1.95), (BACK_DOOR[1], SX1, low)):
        boards(m, back, u0, u1, z0, gable_top, 0.0, BOARD, mt["boards"], rnd, peak=xm)
    back.box(m, BACK_DOOR[0], BACK_DOOR[1], GROUND, 1.95, -0.04, 0.0, mt["gate"])
    boards(m, Face("right", SX1), SY0, SY1, low, flat, 0.0, BOARD, mt["boards"], rnd)
    boards(m, Face("left", SX0), HY1, SY1, low, flat, 0.0, BOARD, mt["boards"], rnd)  # от заднего угла избы
    for x, y in ((SX1, SY0), (SX1, SY1), (SX0, SY1)):
        m.box(x - 0.08, y - 0.08, low, x + 0.08, y + 0.08, SH, mt["pole"])

    # Ворота и калитка закрыты; столбы ворот, перекладина над ними, ручка калитки.
    half = (GATE[1] - GATE[0]) / 2
    battens = (0.45, GATE_H - 0.5)
    m.leaf((GATE[0], SY0 - 0.07), 1, 1, 0, half, lambda s: GATE_H - 0.03, mt["gate"], battens)
    m.leaf((GATE[1], SY0 - 0.07), -1, 1, 0, half, lambda s: GATE_H - 0.03, mt["gate"], battens)
    m.leaf((DOOR[0], SY0 - 0.07), 1, 1, 0, DOOR[1] - DOOR[0], lambda s: DOOR_H - 0.03, mt["gate"], (0.35, DOOR_H - 0.4))
    for x in (GATE[0] - 0.09, GATE[1] + 0.09):
        m.box(x - 0.09, SY0 - 0.1, low, x + 0.09, SY0 + 0.02, GATE_H + 0.15, mt["pole"])
    m.box(GATE[0] - 0.18, SY0 - 0.1, GATE_H + 0.01, GATE[1] + 0.18, SY0 + 0.02, GATE_H + 0.17, mt["pole"])
    m.box(DOOR[1] - 0.14, SY0 - 0.1, 0.95, DOOR[1] - 0.1, SY0 - 0.07, 1.08, mt["iron"])

    front.box(m, xm - 0.5, xm + 0.5, SH + 0.25, SH + 1.2, BOARD, BOARD + 0.04, mt["gate"])
    front.box(m, xm - 0.58, xm + 0.58, SH + 1.2, SH + 1.27, BOARD, BOARD + 0.07, mt["pole"])
    ridge = roof(m, mt, SX0, SX1, SY0 - 0.4, SY1 + 0.4, SH + 0.05, S_SLOPE, 0.0, 0.45, rnd)
    m.log((xm, SY0 - 0.5, ridge + 0.06), (xm, SY1 + 0.5, ridge + 0.06), 0.13, 0.12, rnd.choice(mt["logs"]), mt["log_end"], 10, ring=1.0)


def well(m, mt, rnd):
    """Колодец: сруб из тонких брёвен в пять венцов, два столба с воротом и рукоятью, двускатная крышка на коньковом
    брусе по столбам, ведро на срубе."""
    cx, cy = WELL
    a, r = 0.5, 0.085
    top = GROUND
    for i in range(5):
        z = GROUND + r + i * 0.16
        for y in (cy - a, cy + a):
            m.log((cx - a - 0.12, y, z), (cx + a + 0.12, y, z), r, r * 0.92, rnd.choice(mt["logs"]), mt["log_end"], 10, shift=(rnd.random(), 0.0))
        for x in (cx - a, cx + a):
            m.log((x, cy - a - 0.12, z + 0.08), (x, cy + a + 0.12, z + 0.08), r * 0.95, r * 0.9, rnd.choice(mt["logs"]), mt["log_end"], 10,
                  shift=(rnd.random(), 0.0))
        top = z + 0.08 + r
    m.box(cx - a + 0.08, cy - a + 0.08, top - 0.45, cx + a - 0.08, cy + a - 0.08, top - 0.44, mt["dark"])
    for x in (cx - a - 0.02, cx + a + 0.02):
        m.beam((x, cy, GROUND), (x, cy, 2.16), 0.12, 0.12, mt["pole"])
    m.beam((cx - 0.9, cy, 2.21), (cx + 0.9, cy, 2.21), 0.1, 0.1, mt["pole"])
    # Ворот проходит сквозь столбы, справа на нём колено рукояти.
    m.log((cx - a - 0.06, cy, 1.3), (cx + a + 0.12, cy, 1.3), 0.1, 0.1, rnd.choice(mt["logs"]), mt["log_end"], 10)
    m.beam((cx + a + 0.1, cy, 1.3), (cx + a + 0.1, cy - 0.32, 1.3), 0.04, 0.04, mt["iron"])
    m.beam((cx + a + 0.1, cy - 0.32, 1.3), (cx + a + 0.28, cy - 0.32, 1.3), 0.035, 0.035, mt["pole"])
    m.beam((cx, cy, 1.2), (cx, cy, top - 0.35), 0.02, 0.02, mt["iron"])
    for sign in (-1, 1):
        m.plate([(cx - 0.9, cy + sign * 0.78, 1.8), (cx + 0.9, cy + sign * 0.78, 1.8), (cx + 0.9, cy, 2.28), (cx - 0.9, cy, 2.28)],
                0.03, rnd.choice(mt["roofs"][:-1]))
    m.lathe((cx + a, cy - 0.2, top - 0.01), (0.0, 0.0, 1.0), [(0.0, 0.0), (0.0, 0.11), (0.27, 0.14), (0.27, 0.0)], mt["tin"], 14)


def woodpile(m, mt, rnd):
    """Поленница у правой изгороди: поленья по 0,55 м торцами к избе, берёзовые и сосновые вперемешку, на двух
    слегах, сверху накрыта тремя досками внахлёст."""
    x, y0, y1 = WOODPILE
    for xx in (x + 0.1, x + 0.45):
        m.log((xx, y0 - 0.1, GROUND + 0.06), (xx, y1 + 0.1, GROUND + 0.06), 0.06, 0.055, mt["pole"], mt["log_end"], 7, ring=2.0)
    rows = 10
    for row in range(rows):
        z = GROUND + 0.18 + row * 0.125
        y = y0 + (0.065 if row % 2 else 0.0)
        while y < y1:
            r = 0.062 * rnd.uniform(0.85, 1.15)
            side = mt["birch"] if rnd.random() < 0.55 else mt["pine"]
            jut = rnd.uniform(-0.03, 0.03)
            m.log((x + jut, y, z), (x + 0.55 + jut, y, z), r, r, side, mt["firewood_end"], 7, ring=1.0)
            y += 0.13
    top = GROUND + 0.18 + (rows - 1) * 0.125 + 0.07  # верх верхнего ряда
    for k in range(3):
        xa = x - 0.1 + k * 0.25
        z = top + 0.03 + 0.025 * k
        m.plate([(xa, y0 - 0.2, z), (xa, y1 + 0.2, z), (xa + 0.28, y1 + 0.2, z + 0.02), (xa + 0.28, y0 - 0.2, z + 0.02)], 0.03,
                rnd.choice(mt["boards"]))


def picket(m, mt, face, u0, u1, h, gates, rnd):
    """Штакетник по стене `face` от `u0` до `u1`: столбы через 2,4 м и у ворот, две слеги, штакетины по 7,5 см через
    14 см с заострённым верхом. Ворота и калитки `gates` закрыты: у них столбы толще и выше, изнутри раскос."""
    pole = mt["pole"]
    posts = sorted({u0, u1} | {g for gate in gates for g in gate})
    us = []
    for a, b in zip(posts, posts[1:]):
        n = max(1, math.ceil((b - a) / 2.4))
        us += [a + (b - a) * i / n for i in range(n)]
    us.append(u1)
    for u in us:
        tall = any(abs(u - g) < 1e-6 for gate in gates for g in gate)
        w = 0.08 if tall else 0.055
        face.box(m, u - w, u + w, GROUND, h + (0.3 if tall else 0.05), -0.04 - 2 * w, -0.04, pole)
    for z in (0.38, h - 0.3):
        face.box(m, u0, u1, z - 0.04, z + 0.04, -0.075, -0.025, pole)
    u = u0 + 0.07
    while u < u1 - 0.04:
        top = h + rnd.uniform(-0.02, 0.02)
        face.shape(m, [(u - 0.0375, GROUND + 0.05), (u + 0.0375, GROUND + 0.05), (u + 0.0375, top - 0.06), (u, top), (u - 0.0375, top - 0.06)],
                   -0.025, 0.0, mt["picket"])
        u += 0.14
    for g0, g1 in gates:
        a, b = face.at(g0 + 0.1, 0.4, -0.05), face.at(g1 - 0.1, h - 0.32, -0.05)
        m.beam(a, b, 0.05, 0.08, mt["picket"], up=tuple(face.at(0.0, 0.0, 1.0) - face.at(0.0, 0.0, 0.0)))


def pole_fence(m, mt, a, b, rnd, gaps=()):
    """Жердевая изгородь от `a` до `b` на плане: кривоватые колья через ~2,5 м и три жерди между соседними кольями;
    проходы `gaps` (отрезки вдоль изгороди в метрах от `a`) — между кольями на их краях, без жердей."""
    ax, ay = a
    bx, by = b
    length = math.hypot(bx - ax, by - ay)
    n = max(1, math.ceil(length / 2.5))
    ends = [s for gap in gaps for s in gap]
    stations = sorted(set(ends) | {length * i / n for i in range(n + 1) if all(abs(length * i / n - s) > 0.6 for s in ends)})
    posts = []
    for s in stations:
        t = s / length
        x, y = ax + (bx - ax) * t, ay + (by - ay) * t
        top = GROUND + 1.25 + rnd.uniform(-0.08, 0.08)
        m.log((x, y, GROUND), (x + rnd.uniform(-0.05, 0.05), y + rnd.uniform(-0.05, 0.05), top), 0.065, 0.05,
              mt["pole"], mt["log_end"], 7, ring=2.0, shift=(rnd.random(), 0.0))
        posts.append((x, y, s))
    for (x0, y0, s0), (x1, y1, s1) in zip(posts, posts[1:]):
        if any(g0 < (s0 + s1) / 2 < g1 for g0, g1 in gaps):
            continue
        for z in (0.45, 0.85, 1.18):
            za, zb = GROUND + z + rnd.uniform(-0.05, 0.05), GROUND + z + rnd.uniform(-0.05, 0.05)
            m.log((x0, y0, za), (x1, y1, zb), 0.04, 0.032, mt["pole"], mt["log_end"], 6, ring=3.0, shift=(rnd.random(), 0.0))


def garden(m, mt, rnd):
    """Огород: картошка на окученных грядах вдоль участка — гряды вплотную, земли между ними не видно; капуста на
    низких грядах поперёк; три грядки лука в дощатых бортах."""
    g = GROUND
    potato = (mt["potato"], mt["potato"], mt["potato_light"], mt["potato_dark"])
    x = GARDEN[0] + 0.375
    while x < WALK - 0.75:
        y0, y1 = GARDEN[1] + 0.4, GARDEN[3] - 0.6
        m.prism([(x - 0.375, y0, g), (x + 0.375, y0, g), (x + 0.15, y0, g + 0.16), (x - 0.15, y0, g + 0.16)], (0.0, y1 - y0, 0.0), mt["soil"])
        y = y0 + 0.2
        while y < y1 - 0.15:
            if rnd.random() > 0.06:
                m.clump((x + rnd.uniform(-0.05, 0.05), y, g + 0.28), 0.24 * rnd.uniform(0.8, 1.15), rnd.choice(potato), rnd,
                        squash=0.72, subdiv=1, wobble=0.35)
            y += rnd.uniform(0.32, 0.44)
        x += 0.75
    x0, x1 = WALK + 0.6, GARDEN[2] - 0.1
    for row in range(8):
        y = GARDEN[1] + 0.6 + row * 0.62
        m.prism([(x0, y - 0.31, g), (x0, y + 0.31, g), (x0, y + 0.13, g + 0.11), (x0, y - 0.13, g + 0.11)], (x1 - x0, 0.0, 0.0), mt["soil"])
        x = x0 + 0.3
        while x < x1 - 0.2:
            m.clump((x, y, g + 0.2), 0.3, mt["cabbage_leaf"], rnd, squash=0.3, wobble=0.3)
            m.clump((x, y, g + 0.3), 0.16 * rnd.uniform(0.9, 1.1), mt["cabbage"], rnd, squash=0.9, wobble=0.06)
            x += 0.6
    for i in range(3):
        y0 = 21.3 + i * 1.6
        bx0, bx1 = WALK + 0.8, GARDEN[2] - 0.3
        for a, b in (((bx0, y0), (bx1, y0 + 0.04)), ((bx0, y0 + 1.06), (bx1, y0 + 1.1)), ((bx0, y0), (bx0 + 0.04, y0 + 1.1)), ((bx1 - 0.04, y0), (bx1, y0 + 1.1))):
            m.box(a[0], a[1], g, b[0], b[1], g + 0.2, mt["plank"])
        m.box(bx0 + 0.04, y0 + 0.04, g, bx1 - 0.04, y0 + 1.06, g + 0.16, mt["soil"])
        for row in range(3):
            yy = y0 + 0.25 + row * 0.3
            xx = bx0 + 0.2
            while xx < bx1 - 0.15:
                for _ in range(4):
                    m.cylinder(xx + rnd.uniform(-0.04, 0.04), yy + rnd.uniform(-0.04, 0.04), g + 0.15, 0.012, rnd.uniform(0.2, 0.32),
                               mt["onion"], 3, r_top=0.003)
                xx += 0.16


def bathhouse(m, mt, rnd):
    """Баня: сруб из тонких закопчённых брёвен на угловых камнях, дверь и оконце к огороду, крыша из дранки, жестяная
    труба."""
    x0, x1, y0, y1 = BATH
    r, step, courses, base = 0.11, 0.19, 10, GROUND + 0.18
    door, win = (4.95, 5.65, base, base + 1.55), (6.45, 6.9, base + 0.85, base + 1.2)
    for x in (x0, x1):
        for y in (y0, y1):
            m.clump((x, y, GROUND + 0.2), 0.28, mt["stone"], rnd, squash=0.6, wobble=0.15)
    top = cabin(m, mt, (x0, x1, y0, y1), r, step, courses, base, 0.22, {"front": [door, win]}, mt["bath_logs"], rnd)
    face = Face("front", y0)
    face.box(m, door[0], door[1], door[2], door[3], -0.03, 0.03, mt["gate"])
    face.box(m, door[1] - 0.12, door[1] - 0.08, base + 0.75, base + 0.85, 0.03, 0.06, mt["iron"])
    window(m, mt, face, r, *win, carved=False)
    rz = top + 0.18
    gable_z = rz - r * 0.6
    for side, axis in (("front", y0), ("back", y1)):
        gable_end(m, mt, Face(side, axis), x0 - r, x1 + r, top - step / 2, gable_z, 0.6, r * 0.4, rnd)
    ridge = roof(m, mt, x0, x1, y0 - 0.5, y1 + 0.5, rz, 0.6, 0.45, 0.45, rnd)
    ridge_log(m, mt, (x0 + x1) / 2, y0 - 0.5, y1 + 0.5, ridge, False, rnd)
    m.cylinder(x1 - 0.7, y1 - 0.6, rz, 0.08, ridge - rz + 0.9, mt["tin"], 10)
    m.cylinder(x1 - 0.7, y1 - 0.6, ridge + 0.9, 0.15, 0.12, mt["tin"], 10, r_top=0.04)


def fences(m, mt, rnd):
    """Штакетник на улицу с воротами и калиткой напротив крытого двора и палисадник сбоку от избы; жердевые
    изгороди по бокам, сзади и между двором и огородом — с проходом к огороду."""
    picket(m, mt, Face("front", PLOT[1]), PLOT[0], PLOT[2], 1.3, (GATE, (DOOR[0] - 0.05, DOOR[1] + 0.05)), rnd)
    picket(m, mt, Face("right", PALISADE), PLOT[1] + 0.05, HY0 - OUTLET - 0.1, 1.0, (), rnd)
    pole_fence(m, mt, (PLOT[0], PLOT[1]), (PLOT[0], PLOT[3]), rnd)
    pole_fence(m, mt, (PLOT[2], PLOT[1]), (PLOT[2], PLOT[3]), rnd)
    pole_fence(m, mt, (PLOT[0], PLOT[3]), (PLOT[2], PLOT[3]), rnd)
    pole_fence(m, mt, (PLOT[0], FENCE_Y), (PLOT[2], FENCE_Y), rnd, gaps=((GARDEN_GATE[0] - PLOT[0], GARDEN_GATE[1] - PLOT[0]),))


def bake_occlusion(obj, path):
    """Затенение в цвет вершин: запечённое Cycles над плоскостью на месте земли карты, стены темнеют к земле до
    `GRIME`. У граней троп (материал `path`) в альфу цвета ложится слой вершин `fade` — как тропа видна; сам слой
    после этого убирается."""
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 32
    world = bpy.data.worlds.new("bake")
    world.light_settings.distance = 3.0
    scene.world = world
    bpy.ops.mesh.primitive_plane_add(size=300.0, location=(0.0, 0.0, GROUND - 0.002))
    plane = bpy.context.active_object
    attr = obj.data.color_attributes.new("occlusion", "BYTE_COLOR", "CORNER")
    obj.data.color_attributes.active_color = attr
    obj.data.color_attributes.render_color_index = obj.data.color_attributes.active_color_index
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    scene.render.bake.target = "VERTEX_COLORS"
    bpy.ops.object.bake(type="AO")
    mesh = obj.data
    fade = mesh.attributes["fade"].data
    for poly in mesh.polygons:
        flat = abs(poly.normal.z) > 0.5
        for li in poly.loop_indices:
            vi = mesh.loops[li].vertex_index
            co = mesh.vertices[vi].co
            ao = attr.data[li].color[0]
            if flat:
                v = 0.45 + 0.55 * ao
            else:
                v = (0.75 + 0.25 * smoothstep(0.0, GRIME, co.z - GROUND)) * (0.55 + 0.45 * ao)
            attr.data[li].color = (v, v, v, fade[vi].value if poly.material_index == path else 1.0)
    mesh.attributes.remove(mesh.attributes["fade"])
    bpy.data.objects.remove(plane)


def build(out=OUT, mirror=False):
    """Двор `yard_middle`; `mirror` — зеркальный `yard_middle_mirror`: изба справа, крытый двор слева, свои тона брёвен и
    досок, — чтобы соседние дворы деревни не повторяли друг друга."""
    name = "yard_middle_mirror" if mirror else "yard_middle"
    lib.reset()
    rnd = random.Random(1931 if mirror else 1930)
    m = Homestead(name)
    mt = materials(m)
    house(m, mt, rnd)
    covered_yard(m, mt, rnd)
    well(m, mt, rnd)
    woodpile(m, mt, rnd)
    fences(m, mt, rnd)
    garden(m, mt, rnd)
    bathhouse(m, mt, rnd)
    m.project()
    m.grid()
    for i, (line, half, ruts) in enumerate(PATHS):
        m.path(line, half, mt["path"], 40 + i, ruts)
    obj = m.finish()
    if mirror:
        obj.data.transform(Matrix.Diagonal((-1.0, 1.0, 1.0, 1.0)))
        obj.data.flip_normals()
    bake_occlusion(obj, mt["path"])
    pts = [obj.matrix_world @ Vector(c) for c in obj.bound_box]
    print(name, "size", *(round(max(p[i] for p in pts) - min(p[i] for p in pts), 2) for i in range(3)), "faces", len(obj.data.polygons))
    os.makedirs(out, exist_ok=True)
    lib.export(os.path.join(out, name + ".glb"), vertex_color="ACTIVE")
    print("written", name)


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    for mirror in (False, True):
        build(args[0] if args else OUT, mirror)


if __name__ == "__main__":
    main()
