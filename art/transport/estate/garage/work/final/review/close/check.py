# Close-up renders (EEVEE, game light) from a person's inspection distance around the yard.
import bpy, math, os, json, bmesh
from mathutils import Vector
SHOTS = json.load(open(os.path.join(OUT, "shots.json")))
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
lp = nt.nodes.new("ShaderNodeLightPath"); seen = nt.nodes.new("ShaderNodeMix"); seen.data_type = 'RGBA'
nt.links.new(lp.outputs["Is Camera Ray"], seen.inputs[0]); nt.links.new(mx.outputs[2], seen.inputs[6])
seen.inputs[7].default_value = (*hex_linear("#c9d3dc"), 1.0)
nt.links.new(seen.outputs[2], bg.inputs["Color"]); bg.inputs["Strength"].default_value = 1.0
vs.view_transform, vs.look, vs.exposure, vs.gamma = 'Standard', 'None', 0.0, 1.0
vs.use_curve_mapping = False
scene.world = w
keep = {o.name for o in objs} | {g.name}
for o in scene.objects:
    if o.type in ('MESH', 'CURVE', 'FONT'):
        o.hide_render = o.name not in keep
r.engine = 'BLENDER_EEVEE'
ee.taa_render_samples = 24
if hasattr(ee, "use_raytracing"): ee.use_raytracing = False
r.resolution_x, r.resolution_y, r.resolution_percentage = 1000, 750, 100
r.film_transparent = False
r.image_settings.file_format = 'PNG'
cam = _camera(scene, "BM_CLOSE_CAM")
scene.camera = cam
done = []
for s in SHOTS:
    if "only" in globals() and s["name"] not in only: continue
    loc, tgt = Vector(s["cam"]), Vector(s["at"])
    cam.location = loc
    cam.rotation_mode = 'QUATERNION'
    cam.rotation_quaternion = (tgt - loc).to_track_quat("-Z", "Y")
    cam.data.type, cam.data.lens = 'PERSP', s.get("lens", 35)
    cam.data.sensor_fit, cam.data.sensor_width = 'HORIZONTAL', 36.0
    cam.data.clip_start, cam.data.clip_end = s.get("clip", 0.02), 500
    r.filepath = os.path.join(OUT, s["name"] + ".png")
    bpy.ops.render.render(write_still=True, scene=scene.name)
    done.append(s["name"])
result = {"rendered": done}
