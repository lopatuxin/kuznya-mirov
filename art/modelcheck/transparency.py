# Пробные модели прозрачности для проверок движка («Фаза-43-вид-дороги»): забор с щелями между досками —
# прозрачность вырезом (alphaMode MASK), и окно-стекло в раме — прозрачность смешиванием (alphaMode BLEND).
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/modelcheck/transparency.py [-- <папка>]
import os
import sys

import bmesh
import bpy

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.normpath(os.path.join(HERE, "..", "transport", "models")))

import lib  # noqa: E402

OUT = HERE


def plane(name, width, height, mat, uv=True):
    """Стоячий прямоугольник `width` × `height` лицом к −Y Blender (к игроку), низ — в нуле."""
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    layer = bm.loops.layers.uv.verify()
    w = width / 2
    corners = [((-w, 0, 0), (0, 0)), ((w, 0, 0), (1, 0)), ((w, 0, height), (1, 1)), ((-w, 0, height), (0, 1))]
    face = bm.faces.new([bm.verts.new(p) for p, _ in corners])
    if uv:
        for loop, (_, t) in zip(face.loops, corners):
            loop[layer].uv = t
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def fence(out):
    lib.reset()
    m = bpy.data.materials.new("fence")
    m.use_nodes = True
    nodes, links = m.node_tree.nodes, m.node_tree.links
    bsdf = nodes["Principled BSDF"]
    tex = nodes.new("ShaderNodeTexImage")
    tex.image = bpy.data.images.load(os.path.join(HERE, "fence.png"))
    links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    # Порог: выгрузка glTF видит «больше чем» после альфы и пишет alphaMode MASK с alphaCutoff.
    cut = nodes.new("ShaderNodeMath")
    cut.operation = "GREATER_THAN"
    cut.inputs[1].default_value = 0.5
    links.new(tex.outputs["Alpha"], cut.inputs[0])
    links.new(cut.outputs["Value"], bsdf.inputs["Alpha"])
    bsdf.inputs["Roughness"].default_value = 0.85
    m.surface_render_method = "DITHERED"
    plane("fence", 4.0, 2.0, m)
    lib.export(os.path.join(out, "fence.glb"))


def window(out):
    lib.reset()
    frame = bpy.data.materials.new("frame")
    frame.use_nodes = True
    frame.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*lib.hex_color("#e8e2d4"), 1.0)
    glass = bpy.data.materials.new("glass")
    glass.use_nodes = True
    bsdf = glass.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*lib.hex_color("#9cc4d8"), 1.0)
    bsdf.inputs["Alpha"].default_value = 0.35
    bsdf.inputs["Roughness"].default_value = 0.1
    glass.surface_render_method = "BLENDED"
    for name, (x0, x1, z0, z1) in {"left": (-1.0, -0.9, 0, 2), "right": (0.9, 1.0, 0, 2), "bottom": (-0.9, 0.9, 0, 0.1), "top": (-0.9, 0.9, 1.9, 2)}.items():
        part = plane(name, x1 - x0, z1 - z0, frame, uv=False)
        part.location = ((x0 + x1) / 2, 0, z0)
    pane = plane("glass", 1.8, 1.8, glass, uv=False)
    pane.location = (0, 0.01, 0.1)
    lib.export(os.path.join(out, "window.glb"))


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = args[0] if args else OUT
    os.makedirs(out, exist_ok=True)
    fence(out)
    window(out)


if __name__ == "__main__":
    main()
