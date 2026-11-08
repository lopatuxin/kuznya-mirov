# Reviewer's own check: clay close-ups of joints and vertex-colour (baked AO) views.
import bpy, math, os, json
from mathutils import Vector

objs = objs_of(names)

def cam_at(loc, target, lens, ortho=0):
    cam = bpy.data.objects.get("RV_CAM")
    if cam is None:
        cam = bpy.data.objects.new("RV_CAM", bpy.data.cameras.new("RV_CAM"))
        sc.collection.objects.link(cam)
    loc, target = Vector(loc), Vector(target)
    cam.rotation_mode = "QUATERNION"
    cam.rotation_quaternion = (target - loc).to_track_quat("-Z", "Y")
    cam.location = loc
    cam.data.type = "ORTHO" if ortho else "PERSP"
    cam.data.ortho_scale = ortho or 10
    cam.data.lens = lens
    cam.data.sensor_fit, cam.data.sensor_width = "HORIZONTAL", 36.0
    cam.data.clip_start, cam.data.clip_end = 0.02, 500
    return cam

SH = json.load(open(os.path.join(os.path.dirname(OUT), "shots_clay2.json")))
done = []
shd = sc.display.shading
for s in SH:
    name, loc, target, lens, mode = s[0], s[1], s[2], s[3], s[4]
    ortho = s[5] if len(s) > 5 else 0
    cam = cam_at(loc, target, lens, ortho)
    if mode == "vcol":
        # flat vertex colour = the baked occlusion the engine multiplies in
        keep = (shd.light, shd.color_type, shd.show_cavity, shd.show_shadows, shd.show_object_outline)
        sc.render.engine = "BLENDER_WORKBENCH"
        for o in objs:
            if o.type == "MESH" and o.data.color_attributes:
                o.data.color_attributes.active_color_index = 0
        render_objs(sc, objs, cam, (1200, 900), os.path.join(OUT, name + ".png"), "clay")
        # re-render with vertex colour: render_objs forces SINGLE colour in clay, so set it by hand
        r = sc.render
        r.engine = "BLENDER_WORKBENCH"; r.resolution_x, r.resolution_y = 1200, 900
        r.film_transparent = False
        shd.light, shd.color_type = "FLAT", "VERTEX"
        shd.show_cavity = shd.show_shadows = shd.show_object_outline = False
        sc.view_settings.view_transform = "Standard"
        sc.camera = cam
        r.filepath = os.path.join(OUT, name + ".png")
        bpy.ops.render.render(write_still=True, scene=sc.name)
        shd.light, shd.color_type, shd.show_cavity, shd.show_shadows, shd.show_object_outline = keep
    else:
        render_objs(sc, objs, cam, (1200, 900), os.path.join(OUT, name + ".png"), mode)
    done.append(name)
result = {"shots": done}
