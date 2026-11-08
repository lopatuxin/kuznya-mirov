# Reviewer's own check: material inventory and EEVEE close-ups with the game light.
import bpy, math, os, json, bmesh
from mathutils import Vector

objs = objs_of(names)
inv = {}
for o in objs:
    if o.type != "MESH":
        continue
    for slot in o.material_slots:
        m = slot.material
        if not m or m.name in inv:
            continue
        info = {}
        for n in m.node_tree.nodes:
            if n.bl_idname == "ShaderNodeTexImage" and n.image:
                info.setdefault("images", []).append([n.image.name, list(n.image.size)])
            if n.bl_idname == "ShaderNodeBsdfPrincipled":
                b = n
                info["rough"] = None if b.inputs["Roughness"].links else round(b.inputs["Roughness"].default_value, 3)
                info["metal"] = None if b.inputs["Metallic"].links else round(b.inputs["Metallic"].default_value, 3)
                info["alpha_linked"] = bool(b.inputs["Alpha"].links)
            if n.bl_idname == "ShaderNodeMix" and n.data_type == "RGBA":
                info.setdefault("mix", []).append([n.blend_type, [round(x, 3) for x in n.inputs[7].default_value]])
        inv[m.name] = info
OBJINFO = {o.name: [o.type, len(o.data.polygons) if o.type == "MESH" else 0] for o in objs}

def cam_at(loc, target, lens):
    cam = bpy.data.objects.get("RV_CAM")
    if cam is None:
        cam = bpy.data.objects.new("RV_CAM", bpy.data.cameras.new("RV_CAM"))
        sc.collection.objects.link(cam)
    loc, target = Vector(loc), Vector(target)
    cam.rotation_mode = "QUATERNION"
    cam.rotation_quaternion = (target - loc).to_track_quat("-Z", "Y")
    cam.location = loc
    cam.data.type = "PERSP"
    cam.data.lens = lens
    cam.data.sensor_fit, cam.data.sensor_width = "HORIZONTAL", 36.0
    cam.data.clip_start, cam.data.clip_end = 0.02, 500
    return cam

# game light, as bm_review.beauty with light=LIGHT
r, ee, vs = sc.render, sc.eevee, sc.view_settings
t = _tint(LIGHT["sun_color"])
sun = bpy.data.objects.new("RV_SUN", bpy.data.lights.new("RV_SUN", "SUN"))
sun.data.color = tuple(t / t.max())
sun.data.energy = LIGHT["sun"] * math.pi * float(t.max())
sun.data.angle = math.radians(2)
sun.rotation_mode = "QUATERNION"
sun.rotation_quaternion = (-_dirv(LIGHT["sun_az"], LIGHT["sun_el"])).to_track_quat("-Z", "Y")
sc.collection.objects.link(sun)
gbm = bmesh.new()
box(gbm, (0, 10, -0.005), (200, 200, 0.01))
g = obj_from_bm("RV_GROUND", gbm, sc.collection, [mat("rv_ground", LIGHT["ground"], 0.95)])
w = bpy.data.worlds.new("RV_SKY"); w.use_nodes = True
nt = w.node_tree; bg = node_of(w, "ShaderNodeBackground")
tc = nt.nodes.new("ShaderNodeTexCoord"); sep = nt.nodes.new("ShaderNodeSeparateXYZ")
nt.links.new(tc.outputs["Generated"], sep.inputs["Vector"])
up = nt.nodes.new("ShaderNodeMapRange"); nt.links.new(sep.outputs["Z"], up.inputs["Value"])
up.inputs["From Min"].default_value, up.inputs["From Max"].default_value = -0.02, 0.02
mx = nt.nodes.new("ShaderNodeMix"); mx.data_type = "RGBA"
nt.links.new(up.outputs["Result"], mx.inputs[0])
mx.inputs[6].default_value = (0, 0, 0, 1)
mx.inputs[7].default_value = (*(_tint(LIGHT["sky_color"]) * LIGHT["sky"]), 1.0)
lp = nt.nodes.new("ShaderNodeLightPath"); seen = nt.nodes.new("ShaderNodeMix"); seen.data_type = "RGBA"
nt.links.new(lp.outputs["Is Camera Ray"], seen.inputs[0]); nt.links.new(mx.outputs[2], seen.inputs[6])
seen.inputs[7].default_value = (*hex_linear("#c9d3dc"), 1.0)
nt.links.new(seen.outputs[2], bg.inputs["Color"])
sc.world = w
vs.view_transform, vs.look, vs.exposure, vs.gamma = "Standard", "None", 0.0, 1.0
r.engine = "BLENDER_EEVEE"
ee.taa_render_samples = 32
if hasattr(ee, "use_raytracing"): ee.use_raytracing = False
r.resolution_percentage = 100
r.film_transparent = False
r.image_settings.file_format = "PNG"
keep = set(o.name for o in objs) | {"RV_GROUND"}
for o in sc.objects:
    if o.type in ("MESH", "CURVE", "FONT"):
        o.hide_render = o.name not in keep

SHOTS = json.load(open(os.path.join(os.path.dirname(OUT), "shots2.json")))
done = []
for s in SHOTS:
    name, loc, target, lens = s[0], s[1], s[2], s[3]
    px = s[4] if len(s) > 4 else (1200, 900)
    g.hide_render = len(s) > 5 and s[5] == "noground"
    sc.camera = cam_at(loc, target, lens)
    r.resolution_x, r.resolution_y = px
    r.filepath = os.path.join(OUT, name + ".png")
    bpy.ops.render.render(write_still=True, scene=sc.name)
    done.append(name)
result = {"materials": inv, "objects": OBJINFO, "shots": done}
# game view at the game's scale: orthographic, CAMERA elev/az, px_per_m, EEVEE with the game light
import numpy as np
cam = bpy.data.objects["RV_CAM"]
g.hide_render = False
for tag, ppm in (("game_actual", CAMERA["px_per_m"]), ("game_x3", CAMERA["px_per_m"] * 3)):
    d = _dirv(CAMERA["az"], CAMERA["elev"])
    c = Vector((0.0, 9.0, 3.0))
    n = int(round(34 * ppm))
    cam.location = c + d * 80
    cam.rotation_quaternion = (-d).to_track_quat("-Z", "Y")
    cam.data.type = "ORTHO"; cam.data.ortho_scale = n / ppm
    sc.camera = cam
    r.resolution_x = r.resolution_y = n
    r.filepath = os.path.join(OUT, tag + ".png")
    bpy.ops.render.render(write_still=True, scene=sc.name)
    if tag == "game_actual":
        a = load_rgba(r.filepath)
        save_rgba(np.repeat(np.repeat(a, 4, 0), 4, 1), os.path.join(OUT, "game_actual_x4.png"))
