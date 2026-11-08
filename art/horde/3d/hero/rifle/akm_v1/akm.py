from pathlib import Path

import bpy
from mathutils import Matrix, Vector

ASSET = r"C:\projects\kuznya-mirov\art\horde\3d\hero\rifle\akm_v1"
load_dims(ASSET)


def setup():
    scene = scene_for(SCENE)
    add_ref_view(scene, PREFIX + "_ref", REFERENCE, "right", ppm=PPM,
                 anchor_px=ANCHOR_PX, mask_dir=str(Path(ASSET) / "ref"))
    part_coll(scene, "envelope")
    blockout_coll(scene, "envelope")
    return scene


def build():
    scene = setup()
    names = build_part(scene, ASSET, "envelope")
    report = whole_gates(scene, str(Path(ASSET) / "review"))
    save_scene_copy(scene, str(Path(ASSET) / "work" / "calib.blend"))
    report["names"] = names
    return report


def texture_asset():
    exec(compile((Path(ASSET) / "texture.py").read_text(encoding="utf-8"),
                 str(Path(ASSET) / "texture.py"), "exec"), globals())
    scene = bpy.data.scenes[SCENE]
    return apply_reference(scene, model_names(scene))


def export_asset():
    scene = bpy.data.scenes[SCENE]
    collection = coll(scene, "EXPORT")
    clear(collection)
    collection.hide_render = False
    collection.hide_viewport = False
    grip_height = (ANCHOR_PX[1] - GRIP_PX[1]) / PPM
    transform = Matrix.Scale(EXPORT_SCALE, 4) @ Matrix.Translation((0, 0, -grip_height))
    for name in model_names(scene):
        source = bpy.data.objects[name]
        obj = source.copy()
        obj.data = source.data.copy()
        obj.name = PREFIX + "_export"
        collection.objects.link(obj)
        obj.matrix_world = transform @ source.matrix_world
    support = bpy.data.objects.new("support", None)
    collection.objects.link(support)
    support.location = Vector((0, (SUPPORT_PX[0] - GRIP_PX[0]) / PPM,
                               (GRIP_PX[1] - SUPPORT_PX[1]) / PPM)) * EXPORT_SCALE
    support.empty_display_size = 0.01
    previous = bpy.context.window.scene
    output = str(Path(ASSET) / "akm-v1.glb")
    try:
        bpy.context.window.scene = scene
        for obj in scene.objects:
            obj.select_set(obj.name in collection.objects)
        bpy.context.view_layer.objects.active = next(obj for obj in collection.objects if obj.type == "MESH")
        bpy.ops.export_scene.gltf(filepath=output, use_selection=True, use_active_scene=True,
                                  export_format="GLB", export_yup=True, export_apply=True,
                                  export_animations=False, export_materials="EXPORT")
    finally:
        bpy.context.window.scene = previous
        collection.hide_render = True
        collection.hide_viewport = True
    return glb_report(output)


def render_asset():
    scene = bpy.data.scenes[SCENE]
    names = model_names(scene)
    output = Path(ASSET) / "review"
    light = {"sun": 0.3, "sky": 0.7, "sun_color": "#ffffff",
             "sky_color": "#ffffff", "ground": "#9e9991"}
    paths = {}
    for name, azimuth, elevation in (("threequarter", 70, 24),
                                      ("back", 250, 24), ("top", 80, 60)):
        path = str(output / (name + ".png"))
        beauty(scene, names, path, az=azimuth, el=elevation,
               px=(1600, 800), lens=0, light=light)
        paths[name] = path
    return paths
