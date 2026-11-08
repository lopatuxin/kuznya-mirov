"""Плита нарисованной земли Ямы: картина первой ямы, положенная на землю под камерой игры.

Запускать Blender без окна: blender --background --factory-startup --python build_ground.py. Плита строится в осях сцены игры (x вправо, y к игроку;
в Blender y = -y сцены) и выгружается в games/pit/models. Камера игры ортографическая с наклоном 45°:
клетка земли вглубь видна на экране в sin 45° ≈ 0,707 клетки. Картина 2117 × 941 (≈ 2,25 : 1, шире
телефона боком) при глубине плиты 28 клеток занимает на экране 28 · 0,707 = 19,8 клетки по высоте и 44,5 по
ширине — плита 44,5 × 28, посередине сцены 45 × 28. Окно уже картины видит её середину, полос по бокам нет.
"""
import math
import os

import bmesh
import bpy

ROOT = r"C:\projects\kuznya-mirov"
OUT = os.path.join(ROOT, "games", "pit", "models")
PICTURE = os.path.join(ROOT, "art", "pit", "pit-01-ground.png")

DEPTH = 28.0
WIDTH = DEPTH * math.sin(math.radians(45)) * 2117 / 941


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


def plate(name):
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    uv_layer = bm.loops.layers.uv.new("UVMap")
    corners = [(0.0, DEPTH), (WIDTH, DEPTH), (WIDTH, 0.0), (0.0, 0.0)]
    face = bm.faces.new([bm.verts.new((x, -y, 0.0)) for x, y in corners])
    for loop, (x, y) in zip(face.loops, corners):
        loop[uv_layer].uv = (x / WIDTH, 1 - y / DEPTH)
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(picture_material(name, PICTURE))
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def build():
    os.makedirs(OUT, exist_ok=True)
    name = "pit_ground"
    obj = plate(name)
    for other in bpy.context.scene.objects:
        other.select_set(other is obj)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.export_scene.gltf(filepath=os.path.join(OUT, "ground.glb"), use_selection=True,
                              export_format="GLB", export_yup=True, export_apply=True,
                              export_animations=False, use_active_scene=True)
    bpy.data.objects.remove(obj, do_unlink=True)
    return {"plate": "ground.glb", "width": WIDTH, "depth": DEPTH}


result = build()
