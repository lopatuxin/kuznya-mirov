import hashlib
import json
import math
from pathlib import Path

import bpy
import numpy as np
from mathutils.kdtree import KDTree

BM = r"C:\Users\lopat\.codex\plugins\cache\anton-toolkit-marketplace\blender-modeling\0.2.3-codex.ha8b4c4edceb3\scripts"
exec(open(BM + "/bm.py", encoding="utf-8").read())
ROOT = Path(r"C:\projects\kuznya-mirov\art\horde\3d\hero\rifle\akm_v1")
OUT = ROOT / "work" / "mat" / "uv-fix"
OUT.mkdir(parents=True, exist_ok=True)
load_dims(str(ROOT))
sc = bpy.data.scenes[SCENE]
bpy.context.window.scene = sc
obj = bpy.data.objects[PREFIX + "_envelope"]
mesh = obj.data


def digest():
    return hashlib.sha256(json.dumps([tuple(v.co) for v in mesh.vertices]).encode()).hexdigest()


def point_pixel(point):
    return point.y * PPM + ANCHOR_PX[0], ANCHOR_PX[1] - point.z * PPM


def chart_role(px, py):
    if px > 1605 and py < 450:
        return "stock"
    if 600 < px < 990 and 150 < py < 280:
        return "forewood"
    if 630 < px < 875 and py < 150:
        return "upperwood"
    if 1400 < px < 1620 and py > 275:
        return "grip"
    if py > 280 and px < 1300:
        return "magazine"
    return "metal"


def cap_uv(role, px, py, depth):
    across = max(0.0, min(1.0, depth / 0.036 + 0.5))
    if role == "stock":
        x = 1680 + (px - 1605) / 555 * 380
        y = 210 + across * 65
    elif role == "forewood":
        x = 665 + (px - 600) / 390 * 260
        y = 185 + across * 35
    elif role == "upperwood":
        x = 650 + (px - 630) / 245 * 185
        y = 100 + across * 35
    elif role == "grip":
        x = 1700 + (py - 275) / 270 * 330
        y = 215 + across * 60
    elif role == "magazine":
        x = 1100 + (py - 275) / 431 * 430
        y = 148 + across * 16
    else:
        x = 1100 + (px - 990) / 620 * 430 if px >= 990 else 1100 + px / 990 * 430
        y = 148 + across * 16
    return min(0.99, max(0.01, x / IMAGE_WIDTH)), 1 - y / IMAGE_HEIGHT


def mask_weight(vertex):
    value = min(1.0, max(0.0, (0.83 - abs(vertex.normal.x)) / 0.45))
    return value * value * (3 - 2 * value)


before = digest()
print('STAGE loaded', flush=True)
positions = [v.co.copy() for v in mesh.vertices]
normals = [v.normal.copy() for v in mesh.vertices]
centers = [p.center.copy() for p in mesh.polygons]
boundary_positions = [point for point in positions if abs(point.x) <= 0.0012]
contour_tree = KDTree(len(boundary_positions))
for index, point in enumerate(boundary_positions):
    contour_tree.insert((point.y, point.z, 0), index)
contour_tree.balance()
side = mesh.uv_layers.get("art_side") or mesh.uv_layers.new(name="art_side")
cap = mesh.uv_layers.get("art_cap") or mesh.uv_layers.new(name="art_cap")
mix = mesh.color_attributes.get("cap_mix") or mesh.color_attributes.new(name="cap_mix", type="FLOAT_COLOR", domain="CORNER")
tone = mesh.color_attributes.get("cap_tone") or mesh.color_attributes.new(name="cap_tone", type="FLOAT_COLOR", domain="CORNER")
roles = {}
side_values = np.zeros((len(mesh.loops), 2), np.float32)
cap_values = np.zeros((len(mesh.loops), 2), np.float32)
mix_values = np.ones((len(mesh.loops), 4), np.float32)
tone_values = np.ones((len(mesh.loops), 4), np.float32)
for polygon in mesh.polygons:
    px, py = point_pixel(centers[polygon.index])
    role = chart_role(px, py)
    roles[role] = roles.get(role, 0) + 1
    for index in polygon.loop_indices:
        vertex_index = mesh.loops[index].vertex_index
        position, normal = positions[vertex_index], normals[vertex_index]
        px, py = point_pixel(position)
        side_values[index] = px / IMAGE_WIDTH, 1 - py / IMAGE_HEIGHT
        cap_values[index] = cap_uv(role, px, py, position.x)
        value = min(1.0, max(0.0, (0.83 - abs(normal.x)) / 0.45))
        if role == 'magazine':
            distance = contour_tree.find((position.y, position.z, 0))[2]
            value = min(1.0, max(0.0, (0.009 - distance) / 0.006))
        weight = value * value * (3 - 2 * value)
        mix_values[index] = (weight, weight, weight, 1)
        factor = 0.60 if role == "magazine" else 0.85 if role == "metal" else 1.0
        tone_values[index] = (factor, factor, factor, 1)
side.data.foreach_set('uv', side_values.ravel())
cap.data.foreach_set('uv', cap_values.ravel())
mix.data.foreach_set('color', mix_values.ravel())
tone.data.foreach_set('color', tone_values.ravel())
print('STAGE source maps', flush=True)

source = bpy.data.images.load(str(ROOT / "akm-basecolor.png"), check_existing=False)
material = bpy.data.materials.new(PREFIX + "_stable_source")
material.use_nodes = True
nodes, links = material.node_tree.nodes, material.node_tree.links
nodes.clear()
out = nodes.new("ShaderNodeOutputMaterial")
emit = nodes.new("ShaderNodeEmission")
side_map = nodes.new("ShaderNodeUVMap"); side_map.uv_map = "art_side"
cap_map = nodes.new("ShaderNodeUVMap"); cap_map.uv_map = "art_cap"
side_image = nodes.new("ShaderNodeTexImage"); side_image.image = source; side_image.extension = "EXTEND"
cap_image = nodes.new("ShaderNodeTexImage"); cap_image.image = source; cap_image.extension = "EXTEND"
links.new(side_map.outputs["UV"], side_image.inputs["Vector"])
links.new(cap_map.outputs["UV"], cap_image.inputs["Vector"])
weight_node = nodes.new("ShaderNodeVertexColor"); weight_node.layer_name = "cap_mix"
tone_node = nodes.new("ShaderNodeVertexColor"); tone_node.layer_name = "cap_tone"
multiply = nodes.new("ShaderNodeMixRGB"); multiply.blend_type = "MULTIPLY"; multiply.inputs[0].default_value = 1
links.new(cap_image.outputs["Color"], multiply.inputs[1]); links.new(tone_node.outputs["Color"], multiply.inputs[2])
blend_node = nodes.new("ShaderNodeMixRGB")
links.new(weight_node.outputs["Color"], blend_node.inputs[0])
links.new(side_image.outputs["Color"], blend_node.inputs[1]); links.new(multiply.outputs["Color"], blend_node.inputs[2])
links.new(blend_node.outputs["Color"], emit.inputs["Color"]); links.new(emit.outputs["Emission"], out.inputs["Surface"])
mesh.materials.clear(); mesh.materials.append(material)
for polygon in mesh.polygons:
    polygon.material_index = 0

for other in sc.objects:
    other.select_set(False)
obj.select_set(True); bpy.context.view_layer.objects.active = obj
bake_uv = mesh.uv_layers.get("bake") or mesh.uv_layers.new(name="bake")
mesh.uv_layers.active = bake_uv
bpy.ops.object.mode_set(mode="EDIT")
bpy.ops.mesh.select_all(action="SELECT")
bpy.ops.uv.smart_project(angle_limit=math.radians(80), island_margin=0.004, area_weight=0, correct_aspect=True, scale_to_bounds=True)
bpy.ops.object.mode_set(mode="OBJECT")
bake_uv = mesh.uv_layers['bake']
print('STAGE smart UV', flush=True)
image = bpy.data.images.new(PREFIX + "_stable_atlas", 4096, 4096, alpha=False)
image_node = nodes.new("ShaderNodeTexImage"); image_node.image = image; nodes.active = image_node
sc.render.engine = "CYCLES"; sc.cycles.device = "CPU"; sc.cycles.samples = 1
bpy.ops.object.bake(type="EMIT", use_clear=True, margin=12, margin_type="EXTEND")
print('STAGE baked', flush=True)
image.filepath_raw = str(OUT / "akm-stable-basecolor.png"); image.file_format = "PNG"; image.save()
uv_data = [[float(c) for c in item.uv] for item in bake_uv.data]
(OUT / "uv-map.json").write_text(json.dumps({"geometry_hash": before, "uv": uv_data}, separators=(",", ":")), encoding="utf-8")
for name in [layer.name for layer in mesh.uv_layers if layer.name != 'bake']:
    mesh.uv_layers.remove(mesh.uv_layers[name])
bake_uv = mesh.uv_layers['bake']
bake_uv.name = "UVMap"
mesh.uv_layers.active = bake_uv; bake_uv.active_render = True
finished = mat(PREFIX + "_stable_artwork", "#ffffff", 1.0)
texture = finished.node_tree.nodes.new("ShaderNodeTexImage"); texture.image = image
shader = node_of(finished, "ShaderNodeBsdfPrincipled")
finished.node_tree.links.new(texture.outputs["Color"], shader.inputs["Base Color"])
shader.inputs["Specular IOR Level"].default_value = .15
mesh.materials.clear(); mesh.materials.append(finished)
image.pack()
assert before == digest()
bpy.ops.wm.save_as_mainfile(filepath=str(OUT / "textured.blend"), check_existing=False)
light = {"sun": .3, "sky": .7, "sun_color": "#ffffff", "sky_color": "#ffffff", "ground": "#9e9991"}
beauty(sc, [obj.name], str(OUT / "top.png"), az=80, el=60, px=(1600,800), lens=0, light=light)
beauty(sc, [obj.name], str(OUT / "side.png"), az=70, el=24, px=(1600,800), lens=0, light=light)
result = {"geometry_hash": digest(), "vertices": len(mesh.vertices), "triangles": len(mesh.polygons), "roles": roles, "atlas": str(OUT / "akm-stable-basecolor.png"), "uv": str(OUT / "uv-map.json"), "blend": str(OUT / "textured.blend")}
(OUT / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
print(json.dumps(result))
