# ГАЗель Бизнес (ГАЗ-3302) бортовая с тентом по настоящим размерам: длина 5,54 м, ширина кабины 2,06,
# высота кабины 2,12 и по тенту 2,57, база 2,9, колёса 185/75 R16, задние двускатные. Кабина — поверхность
# по поперечным сечениям; стёкла, фары, решётка и щели дверей — накладки, спроецированные на неё лучами.
# Перед — −Y Blender, левый борт (сторона водителя) — +X, основание — в нуле. Единица — метр.
# Запуск только ГАЗелей: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/gazel.py [-- <папка>]
# Без папки обе ГАЗели ложатся в games/transport/models.
import math
import os
import sys

import bmesh
from mathutils import Vector
from mathutils.bvhtree import BVHTree

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402

OUT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "models"))

LENGTH = 5.54
FRONT_AXLE = 1.05  # от переднего края бампера
REAR_AXLE = FRONT_AXLE + 2.9
WHEEL_R = 0.343
FRONT_TRACK = 1.75
REAR_TRACK = 1.56
DUAL_GAP = 0.214  # между серединами спаренных задних шин

# Профиль кабины вдоль машины, s — от переднего края бампера назад.
S_NOSE = 0.12
S_HOOD = 0.36
S_COWL = 0.92
S_GLASS_TOP = 1.50
S_ROOF = 1.70
S_ROOF_END = 2.20
S_BACK = 2.30
Z_BOTTOM = 0.44
BELT = 1.36
TUMBLE = 0.25  # на сколько боковина над поясом уходит внутрь на метр высоты
HALF = 1.03
ARCH_R = 0.44
ARCH_EDGE = math.sqrt(ARCH_R ** 2 - (Z_BOTTOM - WHEEL_R) ** 2)

# Платформа и тент.
S_BED = 2.36
S_BED_END = 5.48
FLOOR = 0.98
BORT_TOP = 1.36
TENT_TOP = 2.57
SHARP = math.radians(38)


def P(x, s, z):
    return Vector((x, s - LENGTH / 2, z))


def clamp(v, lo=0.0, hi=1.0):
    return max(lo, min(hi, v))


def smoothstep(e0, e1, x):
    t = clamp((x - e0) / (e1 - e0))
    return t * t * (3 - 2 * t)


def bezier_z(s, p0, p1, p2, p3):
    """Высота кубической кривой Безье в (s, z) на отметке `s`; s вдоль кривой растёт."""
    lo, hi = 0.0, 1.0
    for _ in range(40):
        t = (lo + hi) / 2
        u = 1 - t
        if u ** 3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t ** 3 * p3[0] < s:
            lo = t
        else:
            hi = t
    t = (lo + hi) / 2
    u = 1 - t
    return u ** 3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t ** 3 * p3[1]


def cab_top(s):
    if s < S_HOOD:
        # Наклонное лицо под фары и решётку и тугая кромка капота над ними.
        return bezier_z(s, (S_NOSE, 0.62), (S_NOSE + 0.085, 0.875), (S_HOOD - 0.10, 1.04 - 0.10 * 0.45), (S_HOOD, 1.04))
    if s < S_COWL:
        t = (s - S_HOOD) / (S_COWL - S_HOOD)
        return 1.04 + 0.29 * t + 0.012 * math.sin(math.pi * t)
    if s < S_GLASS_TOP:
        return 1.33 + 0.62 * (s - S_COWL) / (S_GLASS_TOP - S_COWL)
    if s < S_ROOF:
        slope = 0.62 / (S_GLASS_TOP - S_COWL)
        return bezier_z(s, (S_GLASS_TOP, 1.95), (S_GLASS_TOP + 0.08, 1.95 + 0.08 * slope), (S_ROOF - 0.1, 2.09), (S_ROOF, 2.09))
    if s < S_ROOF_END:
        return 2.09
    t = (s - S_ROOF_END) / (S_BACK - S_ROOF_END)
    return 2.09 - 0.06 * t * t


def cab_half(s):
    t = clamp((s - S_NOSE) / 0.30)
    return HALF - 0.30 + 0.30 * math.sqrt(max(0.0, 1 - (1 - t) ** 2))


def cab_bottom(s):
    d = abs(s - FRONT_AXLE)
    if d >= ARCH_R:
        return Z_BOTTOM
    return max(Z_BOTTOM, WHEEL_R + math.sqrt(ARCH_R ** 2 - d * d))


def cab_section(s):
    """Правая половина сечения кабины (x ≥ 0) от середины низа до середины верха."""
    zt, zb, w = cab_top(s), cab_bottom(s), cab_half(s)
    rt = 0.10 - 0.025 * smoothstep(0.8, 1.0, s) + 0.045 * smoothstep(1.45, 1.75, s)
    rb = 0.05
    # Кромка капота посередине опущена к решётке, дальше капот выпуклый, лобовое стекло гнутое.
    crown = 0.025 - 0.11 * (1 - smoothstep(0.25, 0.60, s)) + 0.03 * smoothstep(0.85, 1.0, s) - 0.025 * smoothstep(1.45, 1.75, s)

    def side_x(z):
        return w - max(0.0, z - BELT) * TUMBLE

    pts = []
    xb = side_x(zb) - rb
    pts += [(xb * i / 4, zb) for i in range(4)]
    pts += [(xb + rb * math.cos(a), zb + rb + rb * math.sin(a)) for a in (-math.pi / 2 + math.pi / 6 * i for i in range(3))]
    z0, z2 = zb + rb, zt - rt
    z1 = min(BELT, z0 + 0.5 * (z2 - z0))
    pts += [(side_x(z0 + (z1 - z0) * i / 5), z0 + (z1 - z0) * i / 5) for i in range(5)]
    pts += [(side_x(z1 + (z2 - z1) * i / 6), z1 + (z2 - z1) * i / 6) for i in range(6)]
    xc = side_x(z2) - rt
    pts += [(xc + rt * math.cos(a), z2 + rt * math.sin(a)) for a in (math.pi / 2 * i / 5 for i in range(5))]
    for i in range(6):
        x = xc * (1 - i / 6)
        pts.append((x, zt + crown * (1 - (x / xc) ** 2)))
    pts.append((0.0, zt + crown))
    return pts


def cab_stations():
    ss = set()
    s = S_NOSE
    while s < S_BACK:
        ss.add(round(s, 4))
        s += 0.018 if s < 0.5 else 0.035
    for key in (S_NOSE, S_HOOD, S_COWL, S_GLASS_TOP, S_ROOF, S_ROOF_END, S_BACK):
        ss.add(key)
    for edge in (FRONT_AXLE - ARCH_EDGE, FRONT_AXLE + ARCH_EDGE):
        for d in (-0.02, -0.006, 0.0, 0.003, 0.008, 0.015, 0.025, 0.04):
            ss.add(round(edge + (d if edge > FRONT_AXLE else -d), 4))
    for k in range(-24, 25):
        ss.add(round(FRONT_AXLE + ARCH_R * 0.98 * k / 24, 4))
    return sorted(v for v in ss if S_NOSE <= v <= S_BACK)


def resample(poly, step, closed=True):
    out = []
    n = len(poly)
    for i in range(n if closed else n - 1):
        a, b = poly[i], poly[(i + 1) % n]
        k = max(1, math.ceil(math.dist(a, b) / step))
        out += [(a[0] + (b[0] - a[0]) * j / k, a[1] + (b[1] - a[1]) * j / k) for j in range(k)]
    if not closed:
        out.append(poly[-1])
    return out


def fillet(poly, r, k=4):
    """Многоугольник со скруглёнными углами радиуса около `r`."""
    out = []
    n = len(poly)
    for i in range(n):
        p, a, b = Vector(poly[i]), Vector(poly[i - 1]), Vector(poly[(i + 1) % n])
        u1, u2 = (a - p).normalized(), (b - p).normalized()
        d = min(r / math.tan(u1.angle(u2) / 2), (a - p).length * 0.45, (b - p).length * 0.45)
        t1, t2 = p + u1 * d, p + u2 * d
        for j in range(k + 1):
            t = j / k
            q = t1 * (1 - t) ** 2 + p * 2 * t * (1 - t) + t2 * t * t
            out.append((q.x, q.y))
    return out


def ellipse(cu, cv, ru, rv, n=20):
    return [(cu + ru * math.cos(2 * math.pi * i / n), cv + rv * math.sin(2 * math.pi * i / n)) for i in range(n)]


def catmull(points, per=6):
    out = []
    pts = [points[0]] + points + [points[-1]]
    for i in range(1, len(pts) - 2):
        p0, p1, p2, p3 = (Vector(p) for p in pts[i - 1:i + 3])
        for j in range(per):
            t = j / per
            q = 0.5 * (2 * p1 + (p2 - p0) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t + (3 * p1 - p0 - 3 * p2 + p3) * t ** 3)
            out.append((q.x, q.y))
    out.append(points[-1])
    return out


def shade(color, k):
    h = color.lstrip("#")
    return "#" + "".join(f"{min(255, int(int(h[i:i + 2], 16) * k)):02x}" for i in (0, 2, 4))


class Car(lib.Model):
    """Сетка машины: части с гладким затенением, накладки по лучам на поверхность кабины."""

    def __init__(self, name):
        super().__init__(name)
        self.outward = {}

    def face(self, verts, mi, smooth=True):
        f = self.bm.faces.new(verts)
        f.material_index = mi
        f.smooth = smooth
        return f

    def loft(self, loops, mi, smooth=True, caps=True):
        """Поверхность через замкнутые сечения `loops`; `mi` — материал или функция (сечение, отрезок) → материал."""
        rows = [[self.bm.verts.new(p) for p in loop] for loop in loops]
        n = len(rows[0])
        faces = []
        for k, (a, b) in enumerate(zip(rows, rows[1:])):
            for i in range(n):
                j = (i + 1) % n
                faces.append(self.face([a[i], a[j], b[j], b[i]], mi(k, i) if callable(mi) else mi, smooth))
        if caps:
            first = mi(0, -1) if callable(mi) else mi
            faces.append(self.face(rows[0][::-1], first, False))
            faces.append(self.face(rows[-1], first, False))
        bmesh.ops.recalc_face_normals(self.bm, faces=faces)
        return faces

    def sweep(self, frames, profile, mi, smooth=True, caps=True):
        """Замкнутый профиль (a, b) вдоль кадров (точка, ось a, ось b)."""
        return self.loft([[o + na * a + nb * b for a, b in profile] for o, na, nb in frames], mi, smooth, caps)

    def arc(self, s0, z0, radius, a0, a1, x0, x1, thick, mi, n=18):
        """Дуга вокруг оси колеса (в плоскости s–z) прямоугольного сечения: накладка арки, крыло."""
        frames = []
        for i in range(n + 1):
            a = math.radians(a0 + (a1 - a0) * i / n)
            radial = Vector((0.0, math.cos(a), math.sin(a)))
            frames.append((P((x0 + x1) / 2, s0, z0) + radial * radius, radial, Vector((1.0, 0.0, 0.0))))
        hx = (x1 - x0) / 2
        return self.sweep(frames, [(0, -hx), (thick, -hx), (thick, hx), (0, hx)], mi, smooth=False)

    def rounded_box(self, c, half, r, mi, m=3, flat=2, smooth=True):
        """Коробка со скруглёнными рёбрами радиуса `r`; `c` и `half` — в осях Blender; `flat` — число
        отрезков плоской части, одно на все оси или по оси."""
        def ticks(h, steps):
            rr = min(r, h)
            left = [-(h - rr) - rr * math.tan(math.radians(45 * (m - k) / m)) for k in range(m)]
            mid = [-(h - rr) + 2 * (h - rr) * i / steps for i in range(steps + 1)]
            right = [(h - rr) + rr * math.tan(math.radians(45 * k / m)) for k in range(1, m + 1)]
            return left + mid + right

        steps = flat if isinstance(flat, tuple) else (flat, flat, flat)
        axes = [ticks(h, k) for h, k in zip(half, steps)]
        made = []
        faces = []
        for axis in range(3):
            u, v = [a for a in range(3) if a != axis]
            for sign in (-1, 1):
                grid = []
                for a in axes[u]:
                    row = []
                    for b in axes[v]:
                        q = [0.0, 0.0, 0.0]
                        q[axis], q[u], q[v] = sign * half[axis], a, b
                        q = Vector(q)
                        inner = Vector([clamp(q[i], -(half[i] - min(r, half[i])), half[i] - min(r, half[i])) for i in range(3)])
                        d = q - inner
                        p = inner + d.normalized() * min(r, min(half)) if d.length > 1e-9 else q
                        vert = self.bm.verts.new(Vector(c) + p)
                        made.append(vert)
                        row.append(vert)
                    grid.append(row)
                for i in range(len(grid) - 1):
                    for j in range(len(grid[0]) - 1):
                        faces.append(self.face([grid[i][j], grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1]], mi, smooth))
        bmesh.ops.remove_doubles(self.bm, verts=made, dist=1e-6)
        faces = [f for f in faces if f.is_valid]
        bmesh.ops.recalc_face_normals(self.bm, faces=faces)
        return faces

    def revolve(self, center, profile, mi_of, side, segments=24, closed=True):
        """Тело вращения вокруг оси x: профиль (радиус, смещение наружу); `side` — куда наружу по x."""
        rings = []
        for k in range(segments):
            a = 2 * math.pi * k / segments
            rings.append([self.bm.verts.new(center + Vector((side * w, 0.0, 0.0)) + Vector((0.0, r * math.cos(a), r * math.sin(a))))
                          for r, w in profile])
        m = len(profile)
        faces = []
        for k in range(segments):
            a, b = rings[k], rings[(k + 1) % segments]
            for i in range(m if closed else m - 1):
                j = (i + 1) % m
                faces.append(self.face([a[i], a[j], b[j], b[i]], mi_of((profile[i][0] + profile[j][0]) / 2), True))
        if closed:
            bmesh.ops.recalc_face_normals(self.bm, faces=faces)
        else:
            self.bm.normal_update()
            probe = max(faces, key=lambda f: abs(f.normal.x))
            if probe.normal.x * side < 0:
                for f in faces:
                    f.normal_flip()
        return faces

    def decal(self, bvh, outline, start, direction, mi, offset=0.005, rings=3, step=0.035):
        """Накладка по контуру `outline` (u, v): каждая точка — луч из `start(u, v)` по `direction` на кабину."""
        pts = resample(outline, step)
        cu = sum(p[0] for p in pts) / len(pts)
        cv = sum(p[1] for p in pts) / len(pts)
        rows = []
        for k in range(rings):
            t = 1 - k / rings
            rows.append([self._vert(bvh, start(cu + (u - cu) * t, cv + (v - cv) * t), direction, offset) for u, v in pts])
        center = self._vert(bvh, start(cu, cv), direction, offset)
        faces = []
        n = len(pts)
        for a, b in zip(rows, rows[1:]):
            for i in range(n):
                faces.append(self.face([a[i], a[(i + 1) % n], b[(i + 1) % n], b[i]], mi))
        for i in range(n):
            faces.append(self.face([rows[-1][i], rows[-1][(i + 1) % n], center], mi))
        self._face_out(faces)
        return faces

    def strip(self, bvh, line, width, start, direction, mi, closed=True, offset=0.004):
        """Узкая полоса вдоль ломаной `line` (u, v), спроецированная на кабину: щель двери, дворник."""
        pts = resample(line, 0.03, closed)
        n = len(pts)
        pairs = []
        for i in range(n):
            prev = pts[i - 1] if (closed or i > 0) else pts[i]
            nxt = pts[(i + 1) % n] if (closed or i < n - 1) else pts[i]
            t = Vector((nxt[0] - prev[0], nxt[1] - prev[1])).normalized()
            nu, nv = -t.y * width / 2, t.x * width / 2
            u, v = pts[i]
            pairs.append((self._vert(bvh, start(u + nu, v + nv), direction, offset),
                          self._vert(bvh, start(u - nu, v - nv), direction, offset)))
        faces = []
        for i in range(n if closed else n - 1):
            a, b = pairs[i], pairs[(i + 1) % n]
            faces.append(self.face([a[0], b[0], b[1], a[1]], mi))
        self._face_out(faces)
        return faces

    def _vert(self, bvh, origin, direction, offset):
        """Точка накладки над кабиной там, куда попал луч; запоминает наружную сторону кабины в этом месте."""
        loc, normal, _, _ = bvh.ray_cast(origin, direction)
        if loc is None:
            raise ValueError(f"луч из {tuple(origin)} не попал в кабину")
        if normal.dot(direction) > 0:
            normal = -normal
        vert = self.bm.verts.new(loc + normal * offset)
        self.outward[vert] = normal
        return vert

    def _face_out(self, faces):
        """Разворачивает грани накладки лицом наружу кабины."""
        self.bm.normal_update()
        for f in faces:
            if f.normal.dot(sum((self.outward[v] for v in f.verts), Vector())) < 0:
                f.normal_flip()

    def finish(self):
        self.bm.normal_update()
        for e in self.bm.edges:
            if len(e.link_faces) == 2:
                a, b = e.link_faces
                if a.normal.angle(b.normal, 0.0) > SHARP:
                    e.smooth = False
        me = lib.bpy.data.meshes.new(self.name)
        self.bm.to_mesh(me)
        self.bm.free()
        for m in self.materials:
            me.materials.append(m)
        obj = lib.bpy.data.objects.new(self.name, me)
        lib.bpy.context.collection.objects.link(obj)
        return obj


def cab(car, body, under):
    """Кабина по сечениям; низ и арка колеса — тёмные. Возвращает дерево лучей по её поверхности."""
    stations = cab_stations()
    loops = []
    for s in stations:
        half = cab_section(s)
        loop = [P(x, s, z) for x, z in half] + [P(-x, s, z) for x, z in reversed(half[1:-1])]
        loops.append(loop)
    n = len(loops[0])
    half_n = (n + 2) // 2

    def from_bottom(i):
        """Номер точки сечения, считая от середины низа по любой стороне."""
        i %= n
        return i if i < half_n else n - i

    def material(k, i):
        if i < 0:
            return body
        return under if max(from_bottom(i), from_bottom(i + 1)) <= 4 else body

    car.loft(loops, material)
    verts = [p for loop in loops for p in loop]
    polys = [(k * n + j, k * n + (j + 1) % n, (k + 1) * n + (j + 1) % n, (k + 1) * n + j)
             for k in range(len(loops) - 1) for j in range(n)]
    polys += [tuple(range(n)), tuple(range((len(loops) - 1) * n, len(loops) * n))]
    return BVHTree.FromPolygons(verts, polys)


def front(x, z):
    return P(x, -1.0, z)


def side_start(sign):
    return lambda s, z: P(sign * 3.0, s, z)


def top(x, s):
    return P(x, s, 4.0)


FORWARD = Vector((0.0, 1.0, 0.0))
DOWN = Vector((0.0, 0.0, -1.0))


def glass_and_lights(car, bvh, m):
    # Лобовое стекло, жалюзи под ним и дворники.
    windshield = fillet([(-0.90, 1.40), (0.90, 1.40), (0.78, 1.925), (-0.78, 1.925)], 0.06)
    car.decal(bvh, windshield, front, FORWARD, m["glass"], offset=0.004, rings=4)
    car.decal(bvh, fillet([(-0.95, 0.86), (0.95, 0.86), (0.95, 0.955), (-0.95, 0.955)], 0.03), top, DOWN, m["plastic"], offset=0.005)
    for x0, x1 in ((-0.70, -0.05), (0.08, 0.72)):
        car.strip(bvh, [(x0, 1.425), (x1, 1.47)], 0.016, front, FORWARD, m["plastic"], closed=False, offset=0.012)

    # Боковые окна вплотную к стойке и щели дверей, по обе стороны. Дверь снизу спереди огибает арку.
    window = fillet([(1.135, 1.43), (2.10, 1.43), (2.10, 1.93), (1.603, 1.93)], 0.04)
    arc_top = math.degrees(math.asin((0.47 - WHEEL_R) / 0.52))
    arc_side = math.degrees(math.acos((1.06 - FRONT_AXLE) / 0.52))
    door = [(1.06, 1.40), (1.593, 1.97), (1.62, 1.995), (2.18, 1.995), (2.18, 0.47)]
    door += [(FRONT_AXLE + 0.52 * math.cos(math.radians(a)), WHEEL_R + 0.52 * math.sin(math.radians(a)))
             for a in (arc_top + (arc_side - arc_top) * i / 8 for i in range(9))]
    for sign in (1, -1):
        direction = Vector((-sign, 0.0, 0.0))
        car.decal(bvh, window, side_start(sign), direction, m["glass"], offset=0.004)
        car.decal(bvh, [(1.13, 1.425), (1.285, 1.425), (1.285, 1.59)], side_start(sign), direction, m["plastic"], offset=0.007,
                  rings=2)  # чёрный треугольник под зеркало
        car.strip(bvh, door, 0.010, side_start(sign), direction, m["gap"], offset=0.003)
        car.decal(bvh, fillet([(1.93, 1.255), (2.07, 1.255), (2.07, 1.29), (1.93, 1.29)], 0.012), side_start(sign), direction,
                  m["plastic"], offset=0.012, rings=1)
        car.decal(bvh, ellipse(0.52, 0.90, 0.035, 0.015, 12), side_start(sign), direction, m["turn"], offset=0.006, rings=1)

    # Решётка от проёма в бампере до опущенной кромки капота: планки, знак ГАЗ сверху, номер на её низу.
    grille = fillet([(-0.40, 0.50), (0.40, 0.50), (0.415, 0.80)] + [(0.42 * (1 - 2 * i / 8), 0.875 + 0.045 * (1 - 2 * i / 8) ** 2)
                                                                     for i in range(9)] + [(-0.415, 0.80)], 0.03)
    car.decal(bvh, grille, front, FORWARD, m["grille"], offset=0.005, rings=9, step=0.025)
    car.strip(bvh, grille, 0.016, front, FORWARD, m["chrome_dark"], offset=0.008)
    for z in (0.66, 0.72, 0.78, 0.84):
        hw = 0.37 + 0.045 * (z - 0.53) / 0.27 - 0.03
        car.strip(bvh, [(-hw, z), (hw, z)], 0.016, front, FORWARD, m["chrome"], closed=False, offset=0.007)
    car.decal(bvh, fillet([(-0.045, 0.80), (0.045, 0.80), (0.05, 0.865), (0.0, 0.88), (-0.05, 0.865)], 0.015), front, FORWARD,
              m["chrome"], offset=0.012, rings=1)

    # Фары по углам: узкий конец у решётки, низ на бампере, верх под кромкой капота, наружный конец поднят и заходит на крыло.
    lamp = [(0.47, 0.76), (0.53, 0.712), (0.70, 0.70), (0.88, 0.715), (0.975, 0.76), (0.995, 0.85), (0.985, 0.945),
            (0.93, 0.99), (0.80, 0.985), (0.64, 0.945), (0.52, 0.885)]
    for sign in (1, -1):
        outline = fillet([(sign * u, v) for u, v in lamp], 0.04)
        car.decal(bvh, outline, front, FORWARD, m["lens"], offset=0.006, rings=9, step=0.025)
        car.strip(bvh, outline, 0.008, front, FORWARD, m["gap"], offset=0.008)
        for cu, cv, r in ((0.665, 0.81, 0.055), (0.845, 0.865, 0.07)):
            car.decal(bvh, ellipse(sign * cu, cv, r, r * 0.95, 18), front, FORWARD, m["reflector"], offset=0.009, rings=2)
            car.decal(bvh, ellipse(sign * cu, cv, r * 0.45, r * 0.45, 12), front, FORWARD, m["bulb"], offset=0.011, rings=1)


def bumper(car, m):
    """Пластиковый бампер Бизнеса: огибает нос и заходит на бока до арок; номер, нижний проём, противотуманки."""
    plan = [(0.0, 0.0), (0.40, 0.012), (0.70, 0.05), (0.88, 0.115), (0.985, 0.215), (1.04, 0.33), (1.05, 0.45), (1.05, 0.58)]
    right = catmull(plan, 6)
    face = [(0.0, 0.43), (-0.008, 0.38), (-0.03, 0.335)]

    def top_at(x):
        """Верх бампера: под фарами 0,70, посередине опущен — проём под решётку."""
        return 0.70 - 0.17 * (1 - smoothstep(0.36, 0.46, abs(x)))

    def profile_at(x):
        zt = top_at(x)
        return [(-0.15, zt), (-0.05, zt + 0.003), (-0.012, zt - 0.02), (0.0, zt - 0.06)] + face + [(-0.07, 0.30), (-0.15, 0.30)]

    def frames_of(path):
        out = []
        for i, (x, s) in enumerate(path):
            a = Vector(path[max(0, i - 1)])
            b = Vector(path[min(len(path) - 1, i + 1)])
            t = (b - a).normalized()
            out.append((P(x, s, 0.0), Vector((t.y, -t.x, 0.0)), Vector((0.0, 0.0, 1.0))))
        return out

    def plan_s(x):
        for (x0, s0), (x1, s1) in zip(right, right[1:]):
            if x0 <= abs(x) <= x1:
                return s0 + (s1 - s0) * (abs(x) - x0) / (x1 - x0)
        raise ValueError(x)

    def face_a(z):
        for (a0, z0), (a1, z1) in zip(face, face[1:]):
            if z1 <= z <= z0:
                return a0 + (a1 - a0) * (z0 - z) / (z0 - z1)
        raise ValueError(z)

    def patch(xmax, z0, z1, mi, lift=0.004, n=14):
        """Накладка на лицо бампера между высотами z0..z1 и |x| ≤ xmax, по его изгибу."""
        path = [(xmax * (2 * i / n - 1), plan_s(xmax * (2 * i / n - 1))) for i in range(n + 1)]
        zs = [z0 + (z1 - z0) * k / 4 for k in range(5)]
        profile = [(face_a(z) + lift, z) for z in zs] + [(face_a(z) - 0.02, z) for z in reversed(zs)]
        car.sweep(frames_of(path), profile, mi, smooth=False)

    path = [(-x, s) for x, s in reversed(right[1:])] + right
    path += [(x, plan_s(x)) for x in (-0.46, -0.43, -0.40, -0.37, 0.37, 0.40, 0.43, 0.46)]  # там, где верх опускается
    path.sort(key=lambda p: math.atan2(p[0], 0.6 - p[1]))  # слева направо вокруг носа
    frames = frames_of(path)
    car.loft([[o + na * a + nb * b for a, b in profile_at(x)] for (o, na, nb), (x, _) in zip(frames, path)], m["bumper"])
    patch(0.46, 0.345, 0.40, m["grille"], lift=0.003)
    y = -LENGTH / 2
    car.box(-0.27, y + 0.11, 0.528, 0.27, y + 0.16, 0.654, m["plastic"])  # рамка номера на низу решётки
    car.box(-0.26, y + 0.105, 0.535, 0.26, y + 0.115, 0.647, m["plate"])
    for sign in (1, -1):
        x = sign * 0.80  # противотуманка в тёмном гнезде
        o, na, _ = frames_of([(x - 0.01, plan_s(x - 0.01)), (x, plan_s(x)), (x + 0.01, plan_s(x + 0.01))])[1]
        tangent = Vector((na.y, -na.x, 0.0)) if sign > 0 else Vector((-na.y, na.x, 0.0))
        for r, lift, mi in ((0.055, 0.003, m["plastic"]), (0.04, 0.006, m["lens"])):
            c = o + na * lift + Vector((0.0, 0.0, 0.43))
            verts = [car.bm.verts.new(c + tangent * (r * math.cos(a)) + Vector((0.0, 0.0, r * math.sin(a))))
                     for a in (2 * math.pi * k / 18 for k in range(18))]
            f = car.face(verts, mi, False)
            car.bm.normal_update()
            if f.normal.dot(na) < 0:
                f.normal_flip()


def mirrors(car, m):
    """Большие чёрные зеркала Бизнеса на двух кронштейнах у переднего угла двери; стекло смотрит назад."""
    for sign in (1, -1):
        car.rounded_box(P(sign * 1.235, 1.25, 1.62), (0.10, 0.045, 0.165), 0.035, m["plastic"])
        x0, x1 = sorted((sign * 1.15, sign * 1.32))
        car.box(x0, 1.2945 - LENGTH / 2, 1.475, x1, 1.299 - LENGTH / 2, 1.765, m["mirror"])
        car.rounded_box(P(sign * 1.075, 1.25, 1.47), (0.085, 0.025, 0.02), 0.012, m["plastic"])
        car.rounded_box(P(sign * 1.075, 1.25, 1.70), (0.085, 0.018, 0.014), 0.010, m["plastic"])


def wheels(car, m):
    tyre = [(0.203, -0.075), (0.25, -0.090), (0.29, -0.0925), (0.315, -0.088), (0.333, -0.075), (0.341, -0.055), (0.343, -0.03),
            (0.343, 0.0), (0.343, 0.03), (0.341, 0.055), (0.333, 0.075), (0.315, 0.088), (0.29, 0.0925), (0.25, 0.090), (0.203, 0.075)]
    rim = [(0.214, 0.080), (0.207, 0.084), (0.199, 0.074), (0.190, 0.055), (0.168, 0.036), (0.130, 0.024), (0.100, 0.024),
           (0.088, 0.032), (0.072, 0.044), (0.055, 0.052), (0.02, 0.054), (0.0, 0.054)]

    def rim_mat(r):
        return m["rim"] if r > 0.097 else m["hub"]

    def wheel(x, s, side, with_rim=True):
        c = P(x, s, WHEEL_R)
        car.revolve(c, tyre, lambda r: m["tyre"], side)
        if with_rim:
            car.revolve(c, rim, rim_mat, side, closed=False)
            for k in range(6):  # гайки
                a = 2 * math.pi * k / 6 + math.pi / 6
                car.cylinder_x(c.x + side * 0.05, c.y + 0.075 * math.cos(a), c.z + 0.075 * math.sin(a), 0.012, 0.024, m["hub"], 6)

    for sign in (1, -1):
        wheel(sign * FRONT_TRACK / 2, FRONT_AXLE, sign)
        wheel(sign * (REAR_TRACK / 2 + DUAL_GAP / 2), REAR_AXLE, sign)
        wheel(sign * (REAR_TRACK / 2 - DUAL_GAP / 2), REAR_AXLE, sign, with_rim=False)


def chassis(car, m):
    d = m["chassis"]
    y = -LENGTH / 2
    car.box(-0.62, y + 0.20, 0.24, 0.62, y + S_BACK, 0.47, d)  # мотор, коробка, днище кабины
    for sign in (1, -1):
        x0, x1 = sorted((sign * 0.64, sign * 0.69))
        car.box(x0, y + FRONT_AXLE - 0.45, 0.30, x1, y + FRONT_AXLE + 0.45, 0.82, d)  # внутренняя стенка арки
        x0, x1 = sorted((sign * 0.42, sign * 0.49))
        car.box(x0, y + 0.45, 0.56, x1, y + 5.44, 0.70, d)  # лонжерон рамы
        x0, x1 = sorted((sign * 0.40, sign * 0.52))
        car.box(x0, y + S_BED, 0.70, x1, y + 5.44, 0.80, d)  # продольный брус платформы
        x0, x1 = sorted((sign * 0.43, sign * 0.51))
        car.box(x0, y + REAR_AXLE - 0.55, 0.40, x1, y + REAR_AXLE + 0.55, 0.47, d)  # рессора
        # Крыло над спаренными колёсами и брызговик за ними.
        x0, x1 = sorted((sign * 0.55, sign * 1.0))
        car.arc(REAR_AXLE, WHEEL_R, 0.40, 18, 162, x0, x1, 0.014, m["plastic"])
        car.box(x0, y + REAR_AXLE + 0.405, 0.13, x1, y + REAR_AXLE + 0.418, 0.52, m["plastic"])
        # Накладка передней арки.
        x0, x1 = sorted((sign * 1.0, sign * 1.052))
        car.arc(FRONT_AXLE, WHEEL_R, ARCH_R - 0.01, 4, 176, x0, x1, 0.07, m["plastic"], n=24)
    car.cylinder_x(0, y + REAR_AXLE, WHEEL_R, 0.05, 1.32, d, 12)
    car.rounded_box(P(0, REAR_AXLE, WHEEL_R), (0.13, 0.15, 0.13), 0.09, d)
    car.box(-1.0, y + 1.50, 0.33, 1.0, y + S_BACK - 0.02, 0.46, m["plastic"])  # подножка под дверью
    car.rounded_box(P(0.72, 2.85, 0.53), (0.16, 0.30, 0.14), 0.05, m["tank"])  # бак слева
    # Задний брус с фонарями и номером.
    car.box(-0.98, y + 5.44, 0.50, 0.98, y + 5.50, 0.62, d)
    for sign in (1, -1):
        x0, x1 = sorted((sign * 0.70, sign * 0.95))
        car.box(x0, y + 5.50, 0.515, x1, y + 5.515, 0.605, m["red"])
    car.box(-0.27, y + 5.495, 0.497, 0.27, y + 5.51, 0.625, m["plastic"])
    car.box(-0.26, y + 5.505, 0.505, 0.26, y + 5.516, 0.617, m["plate"])


def platform(car, m):
    y = -LENGTH / 2
    car.box(-HALF, y + S_BED, 0.90, HALF, y + S_BED_END, FLOOR, m["floor"])
    for s in (2.5, 3.1, 3.7, 4.3, 4.9, 5.38):
        car.box(-0.99, y + s - 0.03, 0.80, 0.99, y + s + 0.03, 0.90, m["chassis"])
    b = m["bort"]
    for sign in (1, -1):
        x0, x1 = sorted((sign * (HALF - 0.025), sign * HALF))
        car.box(x0, y + S_BED + 0.02, FLOOR, x1, y + S_BED_END - 0.02, BORT_TOP, b)
        r0, r1 = sorted((sign * HALF, sign * (HALF + 0.01)))
        for z in (1.07, 1.17):
            car.box(r0, y + S_BED + 0.05, z, r1, y + S_BED_END - 0.05, z + 0.03, b)
        for s in (S_BED + 0.02, S_BED_END - 0.06):
            car.box(r0, y + s, FLOOR, r1, y + s + 0.04, BORT_TOP - 0.06, b)  # угловые стойки
        l0, l1 = sorted((sign * HALF, sign * (HALF + 0.02)))
        for s in (S_BED + 0.09, S_BED_END - 0.15):
            car.box(l0, y + s, 1.20, l1, y + s + 0.07, 1.27, m["chassis"])  # запоры борта
    car.box(-HALF, y + S_BED, FLOOR, HALF, y + S_BED + 0.03, BORT_TOP, b)
    car.box(-HALF, y + S_BED_END - 0.03, FLOOR, HALF, y + S_BED_END, BORT_TOP, b)
    for z in (1.07, 1.17):
        car.box(-HALF + 0.04, y + S_BED_END, z, HALF - 0.04, y + S_BED_END + 0.01, z + 0.03, b)


def tent(car, m):
    mid = (S_BED + S_BED_END + 0.02) / 2
    half_len = (S_BED_END + 0.02 - S_BED) / 2
    faces = car.rounded_box(P(0, mid, (1.24 + TENT_TOP) / 2), (HALF + 0.012, half_len, (TENT_TOP - 1.24) / 2), 0.07, m["tent"],
                            m=3, flat=(8, 40, 6))
    # Брезент провисает между пятью дугами каркаса: крыша и верх боков чуть ниже посередине пролёта.
    y = -LENGTH / 2
    arcs = 4
    for v in {v for f in faces for v in f.verts}:
        t = clamp((v.co.y - (y + S_BED)) / (S_BED_END + 0.02 - S_BED))
        sag = 0.016 * math.sin(math.pi * t * arcs) ** 2 * smoothstep(TENT_TOP - 0.45, TENT_TOP, v.co.z)
        v.co.z -= sag
        v.co.x -= math.copysign(0.4 * sag, v.co.x) * smoothstep(TENT_TOP - 0.12, TENT_TOP - 0.02, v.co.z)
    # Подол с верёвкой поверх бортов и две полосы-завязки сзади.
    for sign in (1, -1):
        x0, x1 = sorted((sign * (HALF + 0.012), sign * (HALF + 0.018)))
        car.box(x0, y + S_BED + 0.07, 1.30, x1, y + S_BED_END - 0.05, 1.335, m["tent_dark"])
    car.box(-0.010, y + S_BED_END + 0.02, 1.32, 0.010, y + S_BED_END + 0.026, TENT_TOP - 0.06, m["tent_dark"])
    for x in (-0.55, 0.55):
        car.box(x - 0.02, y + S_BED_END + 0.02, 1.30, x + 0.02, y + S_BED_END + 0.027, TENT_TOP - 0.07, m["tent_dark"])


def build(name, tent_color, out=OUT):
    lib.reset()
    car = Car(name)
    m = {
        "body": car.mat("body", "#f2f2ef", 0.32),
        "under": car.mat("under", "#1f2022", 0.85),
        "plastic": car.mat("plastic", "#2b2c2e", 0.7),
        "bumper": car.mat("bumper", "#3b3d40", 0.65),
        "gap": car.mat("gap", "#4a4c4f", 0.6),
        "glass": car.mat("glass", "#1c2630", 0.08, 0.3),
        "lens": car.mat("lens", "#dde4ea", 0.1, 0.2),
        "reflector": car.mat("reflector", "#b4bcc3", 0.18, 0.4),
        "bulb": car.mat("bulb", "#eef2f5", 0.1, 0.2),
        "turn": car.mat("turn", "#e8952e", 0.3),
        "chrome": car.mat("chrome", "#cfd2d6", 0.2, 0.6),
        "chrome_dark": car.mat("chrome_dark", "#7d8186", 0.3, 0.5),
        "grille": car.mat("grille", "#151617", 0.6),
        "plate": car.mat("plate", "#efefea", 0.5),
        "mirror": car.mat("mirror", "#5d6f80", 0.05, 0.5),
        "tyre": car.mat("tyre", "#1c1c1d", 0.92),
        "rim": car.mat("rim", "#b9bbbd", 0.45, 0.3),
        "hub": car.mat("hub", "#6a6d70", 0.5, 0.3),
        "chassis": car.mat("chassis", "#232426", 0.8),
        "tank": car.mat("tank", "#303235", 0.6, 0.2),
        "floor": car.mat("floor", "#45484b", 0.7),
        "bort": car.mat("bort", "#a3a8ac", 0.45, 0.3),
        "red": car.mat("red", "#b3202a", 0.3),
        "tent": car.mat("tent", tent_color, 0.9),
        "tent_dark": car.mat("tent_dark", shade(tent_color, 0.72), 0.9),
    }
    bvh = cab(car, m["body"], m["under"])
    glass_and_lights(car, bvh, m)
    bumper(car, m)
    mirrors(car, m)
    wheels(car, m)
    chassis(car, m)
    platform(car, m)
    tent(car, m)
    car.finish()
    os.makedirs(out, exist_ok=True)
    lib.export(os.path.join(out, name + ".glb"))
    print("written", name)


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = args[0] if args else OUT
    build("gazel", "#2f6aa3", out)
    build("gazel_green", "#3f7a4a", out)


if __name__ == "__main__":
    main()
