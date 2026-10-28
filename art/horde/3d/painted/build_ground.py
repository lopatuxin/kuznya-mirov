"""Плиты нарисованной земли Орды: две картины, положенные на землю под камерой игры.

Запускать в живом Blender (MCP): exec(open(путь).read()). Плиты строятся в осях сцены игры (x вправо, y к
игроку; в Blender y = -y сцены) и выгружаются в games/horde/models. Камера игры ортографическая с наклоном
30°: клетка земли вглубь видна на экране в полклетки, поэтому картина 32 × 18 клеток экрана ложится на
плиту 32 × 36 клеток. Вторая картина в прежней плоской Орде лежала поверх первой, поэтому первая плита
обрезана там, где начинается вторая. Вышка — отдельный объём, art/horde/3d/tower/build_tower.py.
"""
import os

import bmesh
import bpy

ROOT = r"C:\projects\kuznya-mirov"
OUT = os.path.join(ROOT, "games", "horde", "models")
IMAGES = os.path.join(ROOT, "art", "horde", "painted")

# Плиты: картина, левый край и ширина плиты.
GROUND = [
    ("ground_01", "ground_01.png", 0.0, 21.2823),
    ("ground_02", "ground_02.png", 21.2823, 32.0),
]
PICTURE_WIDTH, DEPTH = 32.0, 36.0


def picture_material(name, path):
    material = bpy.data.materials.new(name)
    material.use_nodes = True
    nodes, links = material.node_tree.nodes, material.node_tree.links
    bsdf = next(n for n in nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
    texture = nodes.new("ShaderNodeTexImage")
    texture.image = bpy.data.images.load(path, check_existing=True)
    texture.extension = "EXTEND"
    links.new(texture.outputs["Color"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 1.0
    return material


def plate(name, picture, x0, width):
    """Плита от x0 шириной width на всю глубину; UV — доля картины, которую плита накрывает."""
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    uv_layer = bm.loops.layers.uv.new("UVMap")
    corners = [(x0, DEPTH), (x0 + width, DEPTH), (x0 + width, 0.0), (x0, 0.0)]
    face = bm.faces.new([bm.verts.new((x, -y, 0.0)) for x, y in corners])
    for loop, (x, y) in zip(face.loops, corners):
        loop[uv_layer].uv = ((x - x0) / PICTURE_WIDTH, 1 - y / DEPTH)
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(picture_material(name, os.path.join(IMAGES, picture)))
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def build():
    for name, picture, x0, width in GROUND:
        old = bpy.data.objects.get(name)
        if old:
            bpy.data.objects.remove(old, do_unlink=True)
        obj = plate(name, picture, x0, width)
        for other in bpy.context.scene.objects:
            other.select_set(other is obj)
        bpy.context.view_layer.objects.active = obj
        bpy.ops.export_scene.gltf(filepath=os.path.join(OUT, name + ".glb"), use_selection=True,
                                  export_format="GLB", export_yup=True, export_apply=True,
                                  export_animations=False, use_active_scene=True)
        bpy.data.objects.remove(obj, do_unlink=True)
    return {"plates": [name for name, *_ in GROUND]}


result = build()
