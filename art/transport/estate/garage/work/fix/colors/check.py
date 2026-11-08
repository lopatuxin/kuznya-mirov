# Средний цвет каждого материала в виде игры: EEVEE со светом игры, ортокамера игры 13,5 точки на метр; луч из
# каждой второй точки картинки находит грань и её материал. Цвета — sRGB 0..255, как на образце.
import bpy, math, os
import numpy as np
from mathutils import Vector
objs = objs_of(names)
scene = sc
light = LIGHT
r, ee, vs = scene.render, scene.eevee, scene.view_settings
sun = bpy.data.objects.new("BM_SUN", bpy.data.lights.new("BM_SUN", 'SUN'))
sun.data.angle = math.radians(2)
t = _tint(light["sun_color"])
sun.data.color = tuple(t / t.max())
sun.data.energy = light["sun"] * math.pi * float(t.max())
sun.rotation_mode = 'QUATERNION'
sun.rotation_quaternion = (-_dirv(light["sun_az"], light["sun_el"])).to_track_quat("-Z", "Y")
scene.collection.objects.link(sun)
gbm = bmesh.new()
box(gbm, (0, 0, -0.005), (200, 200, 0.01))
g = obj_from_bm("BM_GROUND", gbm, scene.collection, [mat("bm_ground", light["ground"], 0.95)])
w = bpy.data.worlds.new("BM_SKY")
w.use_nodes = True
nt = w.node_tree
bg = node_of(w, "ShaderNodeBackground")
tc = nt.nodes.new("ShaderNodeTexCoord"); sep = nt.nodes.new("ShaderNodeSeparateXYZ")
nt.links.new(tc.outputs["Generated"], sep.inputs["Vector"])
up = nt.nodes.new("ShaderNodeMapRange"); nt.links.new(sep.outputs["Z"], up.inputs["Value"])
up.inputs["From Min"].default_value, up.inputs["From Max"].default_value = -0.02, 0.02
mx = nt.nodes.new("ShaderNodeMix"); mx.data_type = 'RGBA'
nt.links.new(up.outputs["Result"], mx.inputs[0])
mx.inputs[6].default_value = (0, 0, 0, 1)
mx.inputs[7].default_value = (*(_tint(light["sky_color"]) * light["sky"]), 1.0)
nt.links.new(mx.outputs[2], bg.inputs["Color"]); bg.inputs["Strength"].default_value = 1.0
vs.view_transform, vs.look, vs.exposure, vs.gamma = 'Standard', 'None', 0.0, 1.0
vs.use_curve_mapping = False
scene.world = w
keep = {o.name for o in objs} | {g.name}
for o in scene.objects:
    if o.type in ('MESH', 'CURVE', 'FONT'):
        o.hide_render = o.name not in keep
r.engine = 'BLENDER_EEVEE'
ee.taa_render_samples = 16
if hasattr(ee, "use_raytracing"): ee.use_raytracing = False
r.film_transparent = False
r.image_settings.file_format = 'PNG'
cam = _camera(scene, "BM_COLOR_CAM")
scene.camera = cam
d = _dirv(CAMERA["az"], CAMERA["elev"])
c = Vector((-1.0, 2.0, 2.0))
n, ppm = 560, 13.5
cam.data.type = 'ORTHO'
cam.data.ortho_scale = n / ppm
cam.location = c + d * 80
cam.rotation_mode = 'QUATERNION'
cam.rotation_quaternion = (-d).to_track_quat("-Z", "Y")
cam.data.clip_start, cam.data.clip_end = 1, 300
r.resolution_x = r.resolution_y = n
r.resolution_percentage = 100
r.filepath = os.path.join(OUT, "game_colors.png")
bpy.ops.render.render(write_still=True, scene=scene.name)
img = load_rgba(r.filepath)
dg = bpy.context.evaluated_depsgraph_get()
m3 = cam.matrix_world.to_3x3()
right, upv, fwd = m3.col[0].normalized(), m3.col[1].normalized(), -m3.col[2].normalized()
sums = {}
for py in range(0, n, 2):
    for px in range(0, n, 2):
        o = cam.location + right * ((px + 0.5) / n - 0.5) * cam.data.ortho_scale + upv * (0.5 - (py + 0.5) / n) * cam.data.ortho_scale
        hit, loc, nrm, fi, ob, _ = scene.ray_cast(dg, o, fwd)
        if not hit or ob.name not in keep or ob.name == g.name or ob.type != 'MESH':
            continue
        mn = ob.data.materials[ob.data.polygons[fi].material_index].name if ob.data.materials else ob.name
        s = sums.setdefault(mn, [0.0, 0.0, 0.0, 0])
        col = img[py, px, :3]
        s[0] += col[0]; s[1] += col[1]; s[2] += col[2]; s[3] += 1
result = {k: {"srgb": [round(255 * v[i] / v[3]) for i in range(3)], "px": v[3]} for k, v in sorted(sums.items(), key=lambda kv: -kv[1][3]) if v[3] >= 20}
