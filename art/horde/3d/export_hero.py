"""Выгружает героя Орды со скелетом и движениями в ролевую игру.

Запускать в Blender с открытой сценой Horde_Hero (hero_run.blend). Движок ждёт модель лицом
вдоль +Z glTF — в Blender это -Y, поэтому армата стоит без поворота: модель Mixamo в позе
покоя смотрит в -Y.
"""
import io
import sys

import bpy

OUT = r"C:\projects\kuznya-mirov\games\rpg\models\hero.glb"
BLEND = r"C:\projects\kuznya-mirov\art\horde\3d\hero_run.blend"
# Имена движений видны в редакторе, поэтому они русские; движения набора UAL перенесены под английскими.
RUSSIAN = {
    "Death": "Смерть",
    "Hit": "Попадание",
    "Idle": "Стойка",
    "Jog": "Бег",
    "Repair": "Ремонт",
    "Roll": "Перекат",
    "Sprint": "Спринт",
}

for action in bpy.data.actions:
    action.name = RUSSIAN.get(action.name, action.name)

scene = bpy.data.scenes["Horde_Hero"]
scene.render.fps = 30
window = bpy.context.window_manager.windows[0]
window.scene = scene
armature = bpy.data.objects["hero_armature"]
mesh = bpy.data.objects["hero_rigged"]
armature.rotation_euler = (0.0, 0.0, 0.0)
layer = scene.view_layers[0]
layer.update()
bones = armature.data.bones
facing = armature.matrix_world.to_3x3() @ (bones["mixamorig:LeftToeBase"].head_local - bones["mixamorig:LeftFoot"].head_local)
assert facing.y < 0, "герой должен смотреть в -Y"

log, stdout = io.StringIO(), sys.stdout
sys.stdout = log
try:
    with bpy.context.temp_override(window=window, scene=scene, view_layer=layer):
        for obj in scene.objects:
            obj.select_set(False)
        armature.select_set(True)
        mesh.select_set(True)
        layer.objects.active = armature
        bpy.ops.export_scene.gltf(
            filepath=OUT, export_format="GLB", use_selection=True, use_active_scene=True,
            export_animations=True, export_animation_mode="ACTIONS", export_skins=True, export_yup=True,
            export_force_sampling=True, export_optimize_animation_size=False, export_def_bones=False)
finally:
    sys.stdout = stdout
bpy.data.libraries.write(BLEND, {scene, *bpy.data.actions}, fake_user=True, path_remap="ABSOLUTE")
