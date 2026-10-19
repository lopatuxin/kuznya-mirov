# Лист моделей: все .glb из games/transport/models в ряд под солнцем, вид сверху под углом, как в игре.
# Запуск: blender --background --factory-startup --python art/transport/models/sheet.py -- <out.png> [имя ...]
# Без <out.png> лист ложится во временную папку системы, а не в папку игры.
import math
import os
import sys
import tempfile

import bpy
from mathutils import Vector

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "models"))
args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
out = args[0] if args else os.path.join(tempfile.gettempdir(), "transport-models-sheet.png")
names = args[1:] or sorted(f[:-4] for f in os.listdir(ROOT) if f.endswith(".glb"))

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
cols = math.ceil(math.sqrt(len(names) * 1.6))
x = y = 0.0
row_depth = 0.0
placed = []
for i, name in enumerate(names):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=os.path.join(ROOT, name + ".glb"))
    new = [o for o in bpy.data.objects if o not in before]
    roots = [o for o in new if o.parent is None]
    pts = [o.matrix_world @ Vector(c) for o in new if o.type == "MESH" for c in o.bound_box]
    minx = min(p.x for p in pts); maxx = max(p.x for p in pts)
    miny = min(p.y for p in pts); maxy = max(p.y for p in pts)
    w, d = maxx - minx, maxy - miny
    if i and i % cols == 0:
        x = 0.0
        y += row_depth + 6
        row_depth = 0.0
    for r in roots:
        r.location.x += x - minx
        r.location.y += -y - maxy
    placed.append((name, x + w / 2, -y - d / 2))
    x += w + 6
    row_depth = max(row_depth, d)

ground = bpy.data.meshes.new("ground")
gobj = bpy.data.objects.new("ground", ground)
scene.collection.objects.link(gobj)
size = max(x, y) + 200
ground.from_pydata([(-100, 100, 0), (size, 100, 0), (size, -size, 0), (-100, -size, 0)], [], [(0, 1, 2, 3)])
gm = bpy.data.materials.new("ground")
gm.use_nodes = True
gm.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.28, 0.33, 0.2, 1)
ground.materials.append(gm)

sun = bpy.data.lights.new("sun", "SUN")
sun.energy = 4.0
sun.angle = math.radians(3)
so = bpy.data.objects.new("sun", sun)
so.rotation_euler = (math.radians(48), 0, math.radians(-35))
scene.collection.objects.link(so)
world = bpy.data.worlds.new("w")
world.use_nodes = True
world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.55, 0.65, 0.8, 1)
world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.8
scene.world = world

cam = bpy.data.cameras.new("cam")
cam.type = "ORTHO"
xs = [p[1] for p in placed]; ys = [p[2] for p in placed]
cx, cy = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
span = max(max(xs) - min(xs), (max(ys) - min(ys)) * 1.4) + 40
cam.ortho_scale = span
co = bpy.data.objects.new("cam", cam)
pitch = math.radians(55)
co.location = (cx, cy - math.cos(pitch) * 300, math.sin(pitch) * 300)
co.rotation_euler = (math.pi / 2 - pitch, 0, 0)
scene.collection.objects.link(co)
scene.camera = co

for engine in ("BLENDER_EEVEE_NEXT", "BLENDER_EEVEE", "BLENDER_WORKBENCH"):
    try:
        scene.render.engine = engine
        break
    except TypeError:
        continue
scene.render.resolution_x = 2000
scene.render.resolution_y = 1400
scene.render.filepath = out
bpy.ops.render.render(write_still=True)
print("sheet", out, scene.render.engine, [p[0] for p in placed])
