# Помощники сборки моделей Транспортной компании в Blender: материалы, коробки, крыши, окна, цилиндры,
# надписи. Модель — один объект с несколькими материалами; основание — в нуле, лицо смотрит на −Y Blender
# (после выгрузки это +Z glTF — к игроку). Единица — метр.
import math
import os

import bmesh
import bpy
from mathutils import Matrix, Vector

FONT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "fonts", "Rubik.ttf")


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def hex_color(h):
    h = h.lstrip("#")
    srgb = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    return tuple(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in srgb)


class Model:
    """Сетка одной модели с материалами по именам."""

    def __init__(self, name):
        self.name = name
        self.bm = bmesh.new()
        self.materials = []
        self.index = {}

    def mat(self, key, color, rough=0.8, metal=0.0):
        if key in self.index:
            return self.index[key]
        m = bpy.data.materials.new(f"{self.name}_{key}")
        m.use_nodes = True
        bsdf = m.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Base Color"].default_value = (*hex_color(color), 1.0)
        bsdf.inputs["Roughness"].default_value = rough
        bsdf.inputs["Metallic"].default_value = metal
        self.materials.append(m)
        self.index[key] = len(self.materials) - 1
        return self.index[key]

    def quad(self, pts, mi):
        verts = [self.bm.verts.new(p) for p in pts]
        f = self.bm.faces.new(verts)
        f.material_index = mi
        return f

    def box(self, x0, y0, z0, x1, y1, z1, mi, skip=()):
        v = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
             (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
        faces = {"bottom": (0, 3, 2, 1), "top": (4, 5, 6, 7), "front": (0, 1, 5, 4),
                 "right": (1, 2, 6, 5), "back": (2, 3, 7, 6), "left": (3, 0, 4, 7)}
        verts = [self.bm.verts.new(p) for p in v]
        for name, f in faces.items():
            if name in skip:
                continue
            face = self.bm.faces.new([verts[i] for i in f])
            face.material_index = mi

    def cbox(self, cx, cy, z0, sx, sy, h, mi, skip=()):
        self.box(cx - sx / 2, cy - sy / 2, z0, cx + sx / 2, cy + sy / 2, z0 + h, mi, skip)

    def gable(self, x0, y0, x1, y1, z0, ridge, mi, gable_mi=None, over=0.4, along="x"):
        """Двускатная крыша над прямоугольником: конёк вдоль `along`, свес `over`, фронтоны цветом `gable_mi`."""
        g = gable_mi if gable_mi is not None else mi
        if along == "x":
            ym = (y0 + y1) / 2
            ax0, ax1 = x0 - over, x1 + over
            self.quad([(ax0, y0 - over, z0 - over * 0.5), (ax1, y0 - over, z0 - over * 0.5), (ax1, ym, ridge), (ax0, ym, ridge)], mi)
            self.quad([(ax1, y1 + over, z0 - over * 0.5), (ax0, y1 + over, z0 - over * 0.5), (ax0, ym, ridge), (ax1, ym, ridge)], mi)
            self._tri([(x0, y1, z0), (x0, y0, z0), (x0, ym, ridge - 0.05)], g)
            self._tri([(x1, y0, z0), (x1, y1, z0), (x1, ym, ridge - 0.05)], g)
        else:
            xm = (x0 + x1) / 2
            ay0, ay1 = y0 - over, y1 + over
            self.quad([(x1 + over, ay0, z0 - over * 0.5), (x1 + over, ay1, z0 - over * 0.5), (xm, ay1, ridge), (xm, ay0, ridge)], mi)
            self.quad([(x0 - over, ay1, z0 - over * 0.5), (x0 - over, ay0, z0 - over * 0.5), (xm, ay0, ridge), (xm, ay1, ridge)], mi)
            self._tri([(x0, y0, z0), (x1, y0, z0), (xm, y0, ridge - 0.05)], g)
            self._tri([(x1, y1, z0), (x0, y1, z0), (xm, y1, ridge - 0.05)], g)

    def hip(self, x0, y0, x1, y1, z0, top, mi, over=0.4):
        """Вальмовая крыша: конёк вдоль длинной стороны."""
        x0, y0, x1, y1 = x0 - over, y0 - over, x1 + over, y1 + over
        z0 -= over * 0.4
        w, d = x1 - x0, y1 - y0
        if w >= d:
            inset = d / 2
            a, b = (x0 + inset, (y0 + y1) / 2, top), (x1 - inset, (y0 + y1) / 2, top)
            self.quad([(x0, y0, z0), (x1, y0, z0), b, a], mi)
            self.quad([(x1, y1, z0), (x0, y1, z0), a, b], mi)
            self._tri([(x1, y0, z0), (x1, y1, z0), b], mi)
            self._tri([(x0, y1, z0), (x0, y0, z0), a], mi)
        else:
            inset = w / 2
            a, b = ((x0 + x1) / 2, y0 + inset, top), ((x0 + x1) / 2, y1 - inset, top)
            self.quad([(x1, y0, z0), (x1, y1, z0), b, a], mi)
            self.quad([(x0, y1, z0), (x0, y0, z0), a, b], mi)
            self._tri([(x0, y0, z0), (x1, y0, z0), a], mi)
            self._tri([(x1, y1, z0), (x0, y1, z0), b], mi)

    def _tri(self, pts, mi):
        verts = [self.bm.verts.new(p) for p in pts]
        f = self.bm.faces.new(verts)
        f.material_index = mi

    def window(self, face, u, z, w, h, glass, frame, depth=0.06, trim=0.12):
        """Окно на стене `face` = ('front'|'back'|'left'|'right', координата стены): стекло и наличник."""
        side, coord = face
        t = trim
        if side in ("front", "back"):
            s = -1 if side == "front" else 1
            y = coord + s * depth
            self.box(u - w / 2 - t, min(coord, y), z - t, u + w / 2 + t, max(coord, y), z + h + t, frame)
            yg = coord + s * (depth + 0.01)
            self.box(u - w / 2, min(coord, yg), z, u + w / 2, max(coord, yg), z + h, glass)
        else:
            s = -1 if side == "left" else 1
            x = coord + s * depth
            self.box(min(coord, x), u - w / 2 - t, z - t, max(coord, x), u + w / 2 + t, z + h + t, frame)
            xg = coord + s * (depth + 0.01)
            self.box(min(coord, xg), u - w / 2, z, max(coord, xg), u + w / 2, z + h, glass)

    def cylinder(self, cx, cy, z0, r, h, mi, segments=12, r_top=None):
        res = bmesh.ops.create_cone(self.bm, cap_ends=True, segments=segments, radius1=r,
                                    radius2=r if r_top is None else r_top, depth=h,
                                    matrix=Matrix.Translation((cx, cy, z0 + h / 2)))
        self._paint(res["verts"], mi)

    def cylinder_x(self, cx, cy, cz, r, length, mi, segments=12):
        res = bmesh.ops.create_cone(self.bm, cap_ends=True, segments=segments, radius1=r, radius2=r, depth=length,
                                    matrix=Matrix.Translation((cx, cy, cz)) @ Matrix.Rotation(math.pi / 2, 4, "Y"))
        self._paint(res["verts"], mi)

    def blob(self, cx, cy, cz, rx, ry, rz, mi, subdiv=1, seed=0, jitter=0.12):
        res = bmesh.ops.create_icosphere(self.bm, subdivisions=subdiv, radius=1.0)
        import random
        rnd = random.Random(seed)
        for v in res["verts"]:
            k = 1 + (rnd.random() - 0.5) * jitter * 2
            v.co = Vector((cx + v.co.x * rx * k, cy + v.co.y * ry * k, cz + v.co.z * rz * k))
        self._paint(res["verts"], mi)

    def _paint(self, verts, mi):
        for f in {f for v in verts for f in v.link_faces}:
            f.material_index = mi

    def finish(self):
        me = bpy.data.meshes.new(self.name)
        bmesh.ops.recalc_face_normals(self.bm, faces=self.bm.faces)
        self.bm.to_mesh(me)
        self.bm.free()
        for m in self.materials:
            me.materials.append(m)
        obj = bpy.data.objects.new(self.name, me)
        bpy.context.collection.objects.link(obj)
        return obj


def text(model_obj, words, size, location, rotation_z, color, depth=0.04):
    """Надпись сеткой, дочерним объектом модели: например, вывеска «ХЛЕБ»."""
    font = bpy.data.fonts.load(os.path.normpath(FONT), check_existing=True)
    cu = bpy.data.curves.new(f"{model_obj.name}_text", "FONT")
    cu.body = words
    cu.font = font
    cu.size = size
    cu.extrude = depth
    cu.align_x = "CENTER"
    cu.align_y = "CENTER"
    t = bpy.data.objects.new(f"{model_obj.name}_text", cu)
    bpy.context.collection.objects.link(t)
    t.location = location
    t.rotation_euler = (math.pi / 2, 0, rotation_z)
    m = bpy.data.materials.new(f"{model_obj.name}_textmat")
    m.use_nodes = True
    m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*hex_color(color), 1.0)
    cu.materials.append(m)
    deps = bpy.context.evaluated_depsgraph_get()
    mesh = bpy.data.meshes.new_from_object(t.evaluated_get(deps))
    mobj = bpy.data.objects.new(f"{model_obj.name}_sign", mesh)
    bpy.context.collection.objects.link(mobj)
    mobj.matrix_world = t.matrix_world
    bpy.data.objects.remove(t)
    mobj.parent = model_obj
    mobj.matrix_parent_inverse = model_obj.matrix_world.inverted()
    return mobj


def export(path):
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", export_yup=True, export_apply=True,
                              export_vertex_color="MATERIAL")
