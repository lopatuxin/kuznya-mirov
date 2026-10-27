"""Переносит движения Quaternius UAL (скелет в духе UE-манекена) на героя со скелетом Mixamo.

Перенос по мировым поворотам: для каждой кости берётся поворот относительно её позы покоя
у источника, разворачивается на угол между направлениями взгляда двух скелетов и
накладывается на позу покоя героя. Обе позы покоя — T-поза.
"""
import bpy
from mathutils import Matrix, Quaternion

SIDES = {"l": "Left", "r": "Right"}
BONE_MAP = {"pelvis": "Hips", "spine_01": "Spine", "spine_02": "Spine1", "spine_03": "Spine2",
            "neck_01": "Neck", "Head": "Head"}
for s, side in SIDES.items():
    BONE_MAP.update({
        f"clavicle_{s}": f"{side}Shoulder", f"upperarm_{s}": f"{side}Arm",
        f"lowerarm_{s}": f"{side}ForeArm", f"hand_{s}": f"{side}Hand",
        f"thigh_{s}": f"{side}UpLeg", f"calf_{s}": f"{side}Leg",
        f"foot_{s}": f"{side}Foot", f"ball_{s}": f"{side}ToeBase",
    })
    for finger, mixamo_finger in (("index", "Index"), ("middle", "Middle"), ("ring", "Ring"),
                                  ("pinky", "Pinky"), ("thumb", "Thumb")):
        for i in (1, 2, 3):
            BONE_MAP[f"{finger}_0{i}_{s}"] = f"{side}Hand{mixamo_finger}{i}"
BONE_MAP = {k: "mixamorig:" + v for k, v in BONE_MAP.items()}

# Источник смотрит в +Y, герой в -Y: разворот на 180° вокруг вертикали.
FACING = Matrix.Rotation(3.141592653589793, 3, "Z")


def world_rest(obj, bone):
    return obj.matrix_world.to_3x3().normalized() @ bone.matrix_local.to_3x3()


def retarget(src, tgt, action_name, new_name):
    scene = src.users_scene[0]
    src.animation_data.action = bpy.data.actions[action_name]
    start, end = (int(x) for x in bpy.data.actions[action_name].frame_range)

    pairs = [(sb, tgt.pose.bones[BONE_MAP[sb]]) for sb in BONE_MAP
             if sb in src.pose.bones and BONE_MAP[sb] in tgt.pose.bones]
    order = sorted(pairs, key=lambda p: len(p[1].parent_recursive))
    tgt_world = tgt.matrix_world.to_3x3().normalized()
    src_world = src.matrix_world.to_3x3().normalized()
    src_hips = src.data.bones["pelvis"]
    tgt_hips = tgt.data.bones["mixamorig:Hips"]
    height_ratio = (tgt.matrix_world @ tgt_hips.head_local).z / (src.matrix_world @ src_hips.head_local).z

    if tgt.animation_data is None:
        tgt.animation_data_create()
    action = bpy.data.actions.new(new_name)
    tgt.animation_data.action = action
    for _, tb in pairs:
        tb.rotation_mode = "QUATERNION"

    for frame in range(start, end + 1):
        scene.frame_set(frame)
        pose_arm = {}
        for sname, tb in order:
            sb = src.pose.bones[sname]
            src_rest_w = world_rest(src, sb.bone)
            src_pose_w = src_world @ sb.matrix.to_3x3()
            delta = FACING @ (src_pose_w @ src_rest_w.inverted()) @ FACING.inverted()
            tgt_pose_arm = tgt_world.inverted() @ delta @ tgt_world @ tb.bone.matrix_local.to_3x3()
            pose_arm[tb.name] = tgt_pose_arm
            rest = tb.bone.matrix_local.to_3x3()
            parent = tb.parent
            if parent is not None and parent.name in pose_arm:
                prest = parent.bone.matrix_local.to_3x3()
                basis = (pose_arm[parent.name] @ prest.inverted() @ rest).inverted() @ tgt_pose_arm
            else:
                basis = rest.inverted() @ tgt_pose_arm
            tb.rotation_quaternion = basis.to_quaternion()
            tb.keyframe_insert("rotation_quaternion", frame=frame)

        offset_w = FACING @ (src_world @ (src.pose.bones["pelvis"].head - src_hips.head_local)) * height_ratio
        hips = tgt.pose.bones["mixamorig:Hips"]
        hips.location = tgt_hips.matrix_local.to_3x3().inverted() @ (tgt_world.inverted() @ offset_w)
        hips.keyframe_insert("location", frame=frame)

    return action, start, end
