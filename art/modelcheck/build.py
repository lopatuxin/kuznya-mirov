# Пробные модели фазы 25 «Модели»: дом с картой цвета и картой мелкого рельефа,
# берёза с цветом вершин, машина из частей с разными материалами и стрелка осей.
# Модели смотрят вперёд по −Y Blender — после выгрузки это +Z glTF, лицо модели.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/modelcheck/build.py
import math
import os

import bmesh
import bpy
from mathutils import Matrix

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)))


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def material(name, color, roughness=0.8, metallic=0.0):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Roughness"].default_value = roughness
    bsdf.inputs["Metallic"].default_value = metallic
    return mat


def vertex_color_material(name, roughness=0.85):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nodes = mat.node_tree.nodes
    bsdf = nodes["Principled BSDF"]
    attr = nodes.new("ShaderNodeVertexColor")
    attr.layer_name = "Col"
    mat.node_tree.links.new(attr.outputs["Color"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = roughness
    return mat


def mesh_object(name, bm, mats):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = bpy.data.objects.new(name, me)
    bpy.context.collection.objects.link(obj)
    for m in mats:
        me.materials.append(m)
    return obj


def box(bm, x0, y0, z0, x1, y1, z1, mat_index=0):
    verts = [bm.verts.new(v) for v in [
        (x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
        (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    out = []
    for f in faces:
        face = bm.faces.new([verts[i] for i in f])
        face.material_index = mat_index
        out.append(face)
    return out


def box_uv(bm, scale):
    """Развёртка коробкой: каждая грань — по своей плоскости, 1 единица развёртки — scale метров."""
    uv = bm.loops.layers.uv.verify()
    for face in bm.faces:
        n = face.normal
        ax = max(range(3), key=lambda i: abs(n[i]))
        for loop in face.loops:
            co = loop.vert.co
            if ax == 2:
                u, v = co.x, co.y
            elif ax == 0:
                u, v = co.y, co.z
            else:
                u, v = co.x, co.z
            loop[uv].uv = (u / scale, v / scale)


def brick_images(size=256):
    """Карта цвета кирпича и карта мелкого рельефа (OpenGL): швы ниже кирпичей."""
    rows, cols = 8, 4
    h = [[1.0] * size for _ in range(size)]
    for y in range(size):
        row = y * rows // size
        for x in range(size):
            shift = (size // cols // 2) if row % 2 else 0
            bx = ((x + shift) % (size // cols))
            by = y % (size // rows)
            if bx < 3 or by < 3:
                h[y][x] = 0.0
    color = []
    for y in range(size):
        row = y * rows // size
        for x in range(size):
            if h[y][x] == 0.0:
                c = (0.78, 0.76, 0.70)
            else:
                k = ((row * 7 + ((x + (size // cols // 2 if row % 2 else 0)) // (size // cols)) * 13) % 5) / 40.0
                c = (0.55 + k, 0.24 + k * 0.5, 0.17)
            color.extend((*c, 1.0))
    normal = []
    for y in range(size):
        for x in range(size):
            dx = h[y][(x + 1) % size] - h[y][(x - 1) % size]
            dy = h[(y + 1) % size][x] - h[(y - 1) % size][x]
            nx, ny, nz = -dx * 1.5, -dy * 1.5, 1.0
            ln = math.sqrt(nx * nx + ny * ny + nz * nz)
            normal.extend((nx / ln * 0.5 + 0.5, ny / ln * 0.5 + 0.5, nz / ln * 0.5 + 0.5, 1.0))
    ci = bpy.data.images.new("brick_color", size, size)
    ci.pixels = color
    ci.file_format = "PNG"
    ci.pack()
    ni = bpy.data.images.new("brick_normal", size, size)
    ni.colorspace_settings.name = "Non-Color"
    ni.pixels = normal
    ni.file_format = "PNG"
    ni.pack()
    return ci, ni


def brick_material():
    ci, ni = brick_images()
    mat = bpy.data.materials.new("brick")
    mat.use_nodes = True
    nodes = mat.node_tree.nodes
    links = mat.node_tree.links
    bsdf = nodes["Principled BSDF"]
    tc = nodes.new("ShaderNodeTexImage")
    tc.image = ci
    tn = nodes.new("ShaderNodeTexImage")
    tn.image = ni
    nm = nodes.new("ShaderNodeNormalMap")
    links.new(tc.outputs["Color"], bsdf.inputs["Base Color"])
    links.new(tn.outputs["Color"], nm.inputs["Color"])
    links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    bsdf.inputs["Roughness"].default_value = 0.9
    return mat


def export(name):
    path = os.path.join(OUT, name + ".glb")
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", export_yup=True, export_apply=True,
                              export_tangents=True, export_vertex_color="MATERIAL")
    print("written", path)


def build_house():
    reset()
    walls = brick_material()
    roof = material("roof", (0.42, 0.12, 0.10), roughness=0.7)
    wood = material("door", (0.30, 0.18, 0.10), roughness=0.8)
    bm = bmesh.new()
    box(bm, -4, -3, 0, 4, 3, 4.2, 0)
    box_uv(bm, 2.0)
    obj = mesh_object("house_walls", bm, [walls])
    # Двускатная крыша вдоль X, свес 0,4.
    bm = bmesh.new()
    a = [bm.verts.new(v) for v in [(-4.4, -3.4, 4.2), (4.4, -3.4, 4.2), (4.4, 0, 7.0), (-4.4, 0, 7.0)]]
    b = [bm.verts.new(v) for v in [(-4.4, 3.4, 4.2), (4.4, 3.4, 4.2), (4.4, 0, 7.0), (-4.4, 0, 7.0)]]
    bm.faces.new([a[0], a[1], a[2], a[3]])
    bm.faces.new([b[3], b[2], b[1], b[0]])
    g1 = [bm.verts.new(v) for v in [(-4.0, -3.0, 4.2), (-4.0, 3.0, 4.2), (-4.0, 0, 6.6)]]
    g2 = [bm.verts.new(v) for v in [(4.0, 3.0, 4.2), (4.0, -3.0, 4.2), (4.0, 0, 6.6)]]
    bm.faces.new(g1)
    bm.faces.new(g2)
    mesh_object("house_roof", bm, [roof])
    # Дверь на лице (−Y).
    bm = bmesh.new()
    box(bm, -0.6, -3.08, 0, 0.6, -3.0, 2.2, 0)
    door = mesh_object("house_door", bm, [wood])
    door.parent = obj
    export("house")


def build_tree():
    reset()
    mat = vertex_color_material("birch")
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=8, radius1=0.18, radius2=0.12, depth=4.0,
                          matrix=Matrix.Translation((0, 0, 2.0)))
    crown = bmesh.ops.create_icosphere(bm, subdivisions=2, radius=1.7,
                                       matrix=Matrix.Translation((0, 0, 5.0)))
    for v in crown["verts"]:
        v.co.z = 5.0 + (v.co.z - 5.0) * 1.35
        v.co.x += 0.15 * math.sin(v.co.z * 3.1)
    col = bm.loops.layers.color.new("Col")
    crown_set = set(crown["verts"])
    for face in bm.faces:
        for loop in face.loops:
            v = loop.vert
            if v in crown_set:
                k = 0.85 + 0.15 * math.sin(v.co.x * 2.3 + v.co.y * 1.7)
                loop[col] = (0.32 * k, 0.55 * k, 0.18 * k, 1.0)
            else:
                stripe = (int(v.co.z * 3.0) % 4 == 0)
                loop[col] = (0.08, 0.08, 0.08, 1.0) if stripe else (0.92, 0.92, 0.88, 1.0)
    mesh_object("birch", bm, [mat])
    export("birch")


def build_car():
    reset()
    body = material("body", (0.93, 0.93, 0.92), roughness=0.25, metallic=0.6)
    glass = material("glass", (0.06, 0.08, 0.10), roughness=0.1)
    tyre = material("tyre", (0.05, 0.05, 0.05), roughness=0.95)
    cargo = material("cargo", (0.45, 0.47, 0.44), roughness=0.9)
    # Перед машины — −Y. Длина 5,5, ширина 2,1.
    bm = bmesh.new()
    box(bm, -1.0, -2.75, 0.45, 1.0, -0.95, 2.1, 0)
    mesh_object("cab", bm, [body])
    bm = bmesh.new()
    box(bm, -0.95, -2.78, 1.35, 0.95, -2.74, 1.95, 0)
    mesh_object("windshield", bm, [glass])
    bm = bmesh.new()
    box(bm, -1.05, -0.9, 0.5, 1.05, 2.75, 2.25, 0)
    mesh_object("cargo", bm, [cargo])
    for i, (x, y) in enumerate([(-0.95, -2.0), (0.95, -2.0), (-0.95, 1.7), (0.95, 1.7)]):
        bm = bmesh.new()
        bmesh.ops.create_cone(bm, cap_ends=True, segments=12, radius1=0.38, radius2=0.38, depth=0.25,
                              matrix=Matrix.Translation((x, y, 0.38))
                              @ Matrix.Rotation(math.pi / 2, 4, "Y"))
        mesh_object("wheel%d" % i, bm, [tyre])
    export("car")


def build_axes():
    reset()
    red = material("x_red", (0.85, 0.1, 0.1))
    green = material("y_green", (0.1, 0.7, 0.1))
    blue = material("front_blue", (0.1, 0.25, 0.9))
    bm = bmesh.new()
    box(bm, 0, -0.1, 0, 2.0, 0.1, 0.2, 0)
    mesh_object("plus_x", bm, [red])
    bm = bmesh.new()
    box(bm, -0.1, -0.1, 0, 0.1, 0.1, 2.0, 0)
    mesh_object("up", bm, [green])
    bm = bmesh.new()
    box(bm, -0.1, -2.0, 0, 0.1, 0, 0.2, 0)
    tip = [bm.verts.new(v) for v in [(-0.4, -2.0, 0), (0.4, -2.0, 0), (0, -2.8, 0), (0, -2.0, 0.4)]]
    bm.faces.new([tip[0], tip[1], tip[2]])
    bm.faces.new([tip[0], tip[2], tip[3]])
    bm.faces.new([tip[1], tip[3], tip[2]])
    bm.faces.new([tip[0], tip[3], tip[1]])
    mesh_object("front", bm, [blue])
    export("axes")


build_house()
build_tree()
build_car()
build_axes()
