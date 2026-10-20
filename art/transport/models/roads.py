# Куски грунтовых дорог Транспортной компании («Фаза-43-вид-дороги»): прямой кусок каждого вида в двух вариантах
# и узел. Кусок — плоский прямоугольник вдоль −Y Blender (после выгрузки — вдоль +Z glTF, как ждёт движок):
# ширина по X, длина по Y, середина в нуле. Узел — плоский квадрат с кругом грунта. Карты — из
# art/transport/textures/road_*, их рисует tools/art/roads.mjs: цвет с прозрачностью (край уходит в землю —
# alphaMode BLEND), рельеф, шероховатость.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/roads.py [-- <папка>]
import os
import sys

import bmesh
import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import lib  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "..", "..", "..", "games", "transport", "models"))
TEXTURES = os.path.normpath(os.path.join(HERE, "..", "textures"))

# Ширина и длина куска в метрах — как в tools/art/roads.mjs.
KINDS = {"main": (12.0, 16.0), "street": (7.0, 14.0), "drive": (4.5, 9.0)}
VARIANTS = ("a", "b")
NODE = 9.0


def material(name, folder):
    """Материал из карт папки: цвет и прозрачность из color.png, рельеф, шероховатость и металл из arm.jpg."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nodes, links = m.node_tree.nodes, m.node_tree.links
    bsdf = nodes["Principled BSDF"]

    def image(file, data):
        node = nodes.new("ShaderNodeTexImage")
        node.image = bpy.data.images.load(os.path.join(TEXTURES, folder, file), check_existing=True)
        if data:
            node.image.colorspace_settings.name = "Non-Color"
        return node

    color = image("color.png", False)
    links.new(color.outputs["Color"], bsdf.inputs["Base Color"])
    links.new(color.outputs["Alpha"], bsdf.inputs["Alpha"])
    arm = image("arm.jpg", True)
    channels = nodes.new("ShaderNodeSeparateColor")
    links.new(arm.outputs["Color"], channels.inputs["Color"])
    links.new(channels.outputs["Green"], bsdf.inputs["Roughness"])
    links.new(channels.outputs["Blue"], bsdf.inputs["Metallic"])
    relief = image("normal.jpg", True)
    normal_map = nodes.new("ShaderNodeNormalMap")
    links.new(relief.outputs["Color"], normal_map.inputs["Color"])
    links.new(normal_map.outputs["Normal"], bsdf.inputs["Normal"])
    m.surface_render_method = "BLENDED"
    return m


def flat(name, width, length, folder):
    """Плоский прямоугольник `width` × `length` с развёрткой во всю картинку: u — поперёк, v — вдоль."""
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    uv = bm.loops.layers.uv.verify()
    w, l = width / 2, length / 2
    # Картинка сверху вниз — от начала куска к концу; начало — со стороны +Y Blender (−Z glTF).
    corners = [((-w, l, 0), (0, 1)), ((w, l, 0), (1, 1)), ((w, -l, 0), (1, 0)), ((-w, -l, 0), (0, 0))]
    face = bm.faces.new([bm.verts.new(p) for p, _ in corners])
    for loop, (_, t) in zip(face.loops, corners):
        loop[uv].uv = t
    face.normal_update()
    if face.normal.z < 0:
        face.normal_flip()
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(material(name, folder))
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def build(out=OUT):
    os.makedirs(out, exist_ok=True)
    parts = [(f"road_{kind}_{v}", w, l, f"road_{kind}_{v}") for kind, (w, l) in KINDS.items() for v in VARIANTS]
    parts.append(("road_node", NODE, NODE, "road_node"))
    for name, w, l, folder in parts:
        lib.reset()
        flat(name, w, l, folder)
        lib.export(os.path.join(out, name + ".glb"))
        print(name, os.path.getsize(os.path.join(out, name + ".glb")))


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    build(args[0] if args else OUT)


if __name__ == "__main__":
    main()
