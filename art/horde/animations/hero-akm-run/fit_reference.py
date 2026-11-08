import json
import math
from pathlib import Path

import numpy as np
from mathutils import Euler, Matrix, Quaternion, Vector

ROOT = Path(__file__).parent
SOURCE = json.loads((ROOT / "source-measurements.json").read_text(encoding="utf-8"))
TARGET = json.loads((ROOT / "target-rig.json").read_text(encoding="utf-8"))
CONTACT = json.loads((ROOT / "contact-mesh.json").read_text(encoding="utf-8"))
MIRROR = np.diag([1.0, -1.0, 1.0])
START, END = 43 / 60, 99 / 60
COUNT = 64
BONES = TARGET["bones"]
INDEX = {bone["name"]: index for index, bone in enumerate(BONES)}
REST = {bone["name"]: np.array(bone["rest_global_columns"]).T for bone in BONES}
SOURCE_REST = {bone["name"]: np.array(bone["rest"]) for bone in SOURCE["bones"]}
SOURCE_POSES = [
    {name: np.array(matrix) for name, matrix in sample["bones"].items()}
    for sample in SOURCE["samples"]
]
MAPPING = {
    "Hips": "Hips", "Spine": "Chest", "Spine1": "Chest2", "Spine2": "Chest4",
    "Neck": "Neck", "Head": "Head",
}
for side in ("Left", "Right"):
    MAPPING.update({
        side + "Shoulder": side + "Collar", side + "Arm": side + "Shoulder",
        side + "ForeArm": side + "Elbow", side + "Hand": side + "Wrist",
        side + "UpLeg": side + "Hip", side + "Leg": side + "Knee",
        side + "Foot": side + "Ankle", side + "ToeBase": side + "Toe",
    })
MAPPING = {"mixamorig:" + key: value for key, value in MAPPING.items()}
CHILDREN = {
    "Spine": "Spine1", "Spine1": "Spine2", "Spine2": "Neck", "Neck": "Head",
}
for side in ("Left", "Right"):
    CHILDREN.update({
        side + "Shoulder": side + "Arm", side + "Arm": side + "ForeArm",
        side + "ForeArm": side + "Hand", side + "UpLeg": side + "Leg",
        side + "Leg": side + "Foot", side + "Foot": side + "ToeBase",
    })
CHILDREN = {"mixamorig:" + key: "mixamorig:" + value for key, value in CHILDREN.items()}
YAW = []
for pose in SOURCE_POSES:
    delta = pose["Hips"][:3, :3] @ SOURCE_REST["Hips"][:3, :3].T
    YAW.append(math.atan2(delta[1, 0], delta[0, 0]))
YAW = np.unwrap(YAW)
HEADING = np.array([
    np.mean(YAW[max(0, index - 15):min(len(YAW), index + 16)])
    for index in range(len(YAW))
])
POSITIONS = np.array([vertex["position"] + [1.0] for vertex in CONTACT["vertices"]])
WEIGHTS = np.array([vertex["weights"] for vertex in CONTACT["vertices"]])
JOINTS = np.array([vertex["joints"] for vertex in CONTACT["vertices"]])
SIDES = np.array([vertex["side"] for vertex in CONTACT["vertices"]])
TAILS = np.array(CONTACT["tail_columns"]).transpose(0, 2, 1)
SUPPORT = np.array(CONTACT["support"]) + np.array(CONTACT["item_shift"])
BASE = next(clip for clip in TARGET["clips"] if clip["name"] == "Бег")


def unit(vector: np.ndarray) -> np.ndarray:
    return vector / np.linalg.norm(vector)


def orthogonal(matrix: np.ndarray) -> np.ndarray:
    left, _, right = np.linalg.svd(matrix)
    return left @ right


def swing(before: np.ndarray, after: np.ndarray) -> np.ndarray:
    return np.array(Vector(before).rotation_difference(Vector(after)).to_matrix())


def source_at(time: float) -> tuple[dict[str, np.ndarray], float]:
    frame = time * 60
    before = min(int(frame), len(SOURCE_POSES) - 2)
    along = frame - before
    heading = float(np.interp(frame, np.arange(len(HEADING)), HEADING))
    c, s = math.cos(heading), math.sin(heading)
    align = MIRROR @ np.array([[c, s, 0], [-s, c, 0], [0, 0, 1]])
    result = {}
    for name in SOURCE_REST:
        first, last = SOURCE_POSES[before][name], SOURCE_POSES[before + 1][name]
        rotation = Matrix(orthogonal(first[:3, :3])).to_quaternion().slerp(
            Matrix(orthogonal(last[:3, :3])).to_quaternion(), along
        )
        matrix = np.eye(4)
        matrix[:3, :3] = align @ np.array(rotation.to_matrix())
        matrix[:3, 3] = align @ (first[:3, 3] * (1 - along) + last[:3, 3] * along)
        result[name] = matrix
    height = float(result["Hips"][2, 3])
    origin = result["Hips"][:3, 3].copy()
    for matrix in result.values():
        matrix[:3, 3] -= origin
    return result, height


FIRST, FIRST_HEIGHT = source_at(START)
LAST, LAST_HEIGHT = source_at(END)


def closed_source(phase: float) -> tuple[dict[str, np.ndarray], float]:
    poses, height = source_at(START + (END - START) * phase)
    weight = phase * phase * (3 - 2 * phase)
    for name, matrix in poses.items():
        gap = orthogonal(FIRST[name][:3, :3] @ LAST[name][:3, :3].T)
        correction = Quaternion().slerp(Matrix(gap).to_quaternion(), weight)
        matrix[:3, :3] = np.array(correction.to_matrix()) @ matrix[:3, :3]
        matrix[:3, 3] -= weight * (LAST[name][:3, 3] - FIRST[name][:3, 3])
    height -= weight * (LAST_HEIGHT - FIRST_HEIGHT)
    return poses, height


def base_locals(frame: int, idle: bool = False) -> list[np.ndarray]:
    clip = next(clip for clip in TARGET["clips"] if clip["name"] == "Стойка") if idle else BASE
    matrices = [np.array(value).T for value in clip["frames"][frame]["global_columns"]]
    result = []
    pelvis = INDEX["mixamorig:Hips"]
    heights = [sample["global_columns"][pelvis][3][2] for sample in BASE["frames"][:-1]]
    mean_height = float(np.mean(heights))
    for index, bone in enumerate(BONES):
        parent = bone["parent"]
        above = np.array(TARGET["head_columns"]).T if parent is None else matrices[parent]
        result.append(np.linalg.inv(above) @ matrices[index])
    if not idle:
        result[pelvis][1, 3] -= (heights[frame % COUNT] - mean_height) * 0.6
    return result


def forward(locals_: list[np.ndarray], goals: dict[str, np.ndarray]) -> list[np.ndarray]:
    result = []
    for index, bone in enumerate(BONES):
        parent = bone["parent"]
        above = np.array(TARGET["head_columns"]).T if parent is None else result[parent]
        matrix = above @ locals_[index]
        if bone["name"] in goals:
            matrix[:3, :3] = goals[bone["name"]]
        result.append(matrix)
    return result


def retarget_goals(poses: dict[str, np.ndarray]) -> dict[str, np.ndarray]:
    goals = {}
    for target, source in MAPPING.items():
        delta = poses[source][:3, :3] @ (MIRROR @ SOURCE_REST[source][:3, :3]).T
        rotation = delta @ orthogonal(REST[target][:3, :3])
        child = CHILDREN.get(target)
        if child is not None:
            target_vector = BONES[INDEX[child]]["rest_local"]["translation"]
            desired = poses[MAPPING[child]][:3, 3] - poses[source][:3, 3]
            rotation = swing(rotation @ target_vector, desired) @ rotation
        goals[target] = rotation
    return goals


def chain_ratio(side: str, target: tuple[str, str, str], source: tuple[str, str, str]) -> float:
    target_names = ["mixamorig:" + side + name for name in target]
    source_names = [side + name for name in source]
    target_length = sum(np.linalg.norm(REST[b][:3, 3] - REST[a][:3, 3]) for a, b in zip(target_names, target_names[1:]))
    source_length = sum(np.linalg.norm(SOURCE_REST[b][:3, 3] - SOURCE_REST[a][:3, 3]) for a, b in zip(source_names, source_names[1:]))
    return float(target_length / source_length)


def solve_chain(locals_: list[np.ndarray], goals: dict[str, np.ndarray], names: list[str], target: np.ndarray, pole: np.ndarray) -> None:
    indices = [INDEX[name] for name in names]
    current = forward(locals_, goals)
    hip, knee, ankle = [current[index][:3, 3] for index in indices]
    upper, lower = np.linalg.norm(knee - hip), np.linalg.norm(ankle - knee)
    axis = unit(target - hip)
    reach = float(np.clip(np.linalg.norm(target - hip), abs(upper - lower) + 0.0005, upper + lower - 0.00005))
    along = (upper * upper - lower * lower + reach * reach) / (2 * reach)
    bend = unit(pole - axis * np.dot(pole, axis))
    wanted_knee = hip + axis * along + bend * math.sqrt(max(0, upper * upper - along * along))
    goals[names[0]] = swing(knee - hip, wanted_knee - hip) @ orthogonal(current[indices[0]][:3, :3])
    current = forward(locals_, goals)
    knee, ankle = [current[index][:3, 3] for index in indices[1:]]
    goals[names[1]] = swing(ankle - knee, hip + axis * reach - knee) @ orthogonal(current[indices[1]][:3, :3])


def foot_bottom(matrices: list[np.ndarray], side: str) -> float:
    skins = np.array([matrices[index] for index in CONTACT["joints"]]) @ TAILS
    transformed = np.einsum("vjab,vb->vja", skins[JOINTS], POSITIONS)
    points = np.sum(transformed * WEIGHTS[:, :, None], axis=1)
    return float(np.min(points[SIDES == side, 2]))


def reachable_target(locals_: list[np.ndarray], goals: dict[str, np.ndarray], names: list[str], target: np.ndarray) -> np.ndarray:
    matrices = forward(locals_, goals)
    hip, knee, ankle = [matrices[INDEX[name]][:3, 3] for name in names]
    upper, lower = np.linalg.norm(knee - hip), np.linalg.norm(ankle - knee)
    reach = math.sqrt(upper * upper + lower * lower + 2 * upper * lower * math.cos(math.radians(20)))
    horizontal = target[:2] - hip[:2]
    maximum = math.sqrt(max(0.00001, reach * reach - (target[2] - hip[2]) ** 2))
    length = float(np.linalg.norm(horizontal))
    start = max(0, maximum - 0.025)
    if length > start:
        limited = start + (maximum - start) * math.tanh((length - start) / (maximum - start))
        target = target.copy()
        target[:2] = hip[:2] + horizontal * limited / length
    return target


def fit_legs(locals_: list[np.ndarray], goals: dict[str, np.ndarray], poses: dict[str, np.ndarray], height: float) -> None:
    pelvis = forward(locals_, goals)[INDEX["mixamorig:Hips"]][:3, 3]
    for side in ("Left", "Right"):
        names = ["mixamorig:" + side + name for name in ("UpLeg", "Leg", "Foot")]
        ratio = chain_ratio(side, ("UpLeg", "Leg", "Foot"), ("Hip", "Knee", "Ankle"))
        ankle, toe = poses[side + "Ankle"][:3, 3], poses[side + "Toe"][:3, 3]
        clearance = max(0, min(ankle[2] + height - 0.104, toe[2] + height - 0.032)) * ratio
        target = pelvis + ankle * ratio
        target[2] = REST[names[2]][2, 3] + clearance
        horizontal = target[:2].copy()
        axis = unit(ankle - poses[side + "Hip"][:3, 3])
        knee = poses[side + "Knee"][:3, 3] - poses[side + "Hip"][:3, 3]
        measured = knee - axis * np.dot(knee, axis)
        pole = np.array([0.0, 1.0, 0.0]) + unit(measured) * 0.25
        for _ in range(4):
            target[:2] = horizontal
            target = reachable_target(locals_, goals, names, target)
            solve_chain(locals_, goals, names, target, pole)
            target[2] += clearance - foot_bottom(forward(locals_, goals), side.lower())


def gun_rotation(poses: dict[str, np.ndarray]) -> np.ndarray:
    direction = unit(poses["LeftWrist"][:3, 3] - poses["RightWrist"][:3, 3])
    horizontal = unit(np.array([direction[0], direction[1], 0]))
    pitch = math.atan2(direction[2], np.linalg.norm(direction[:2])) - math.atan2(SUPPORT[2], SUPPORT[1])
    x_axis = np.cross(horizontal, np.array([0, 0, 1]))
    y_axis = horizontal * math.cos(pitch) + np.array([0, 0, math.sin(pitch)])
    return np.column_stack((x_axis, y_axis, np.cross(x_axis, y_axis)))


def fit_arms(locals_: list[np.ndarray], goals: dict[str, np.ndarray], poses: dict[str, np.ndarray]) -> None:
    ratio = chain_ratio("Right", ("Arm", "ForeArm", "Hand"), ("Shoulder", "Elbow", "Wrist"))
    matrices = forward(locals_, goals)
    shoulder = matrices[INDEX["mixamorig:RightArm"]][:3, 3]
    target = shoulder + (poses["RightWrist"][:3, 3] - poses["RightShoulder"][:3, 3]) * ratio
    pole = poses["RightElbow"][:3, 3] - poses["RightShoulder"][:3, 3]
    solve_chain(locals_, goals, ["mixamorig:Right" + name for name in ("Arm", "ForeArm", "Hand")], target, pole)
    gun = gun_rotation(poses)
    goals["mixamorig:RightHand"] = gun @ np.array([[1, 0, 0], [0, 0, 1], [0, 1, 0]])
    hand = forward(locals_, goals)[INDEX["mixamorig:RightHand"]][:3, 3]
    target = hand + gun @ SUPPORT
    pole = poses["LeftElbow"][:3, 3] - poses["LeftShoulder"][:3, 3]
    solve_chain(locals_, goals, ["mixamorig:Left" + name for name in ("Arm", "ForeArm", "Hand")], target, pole)


def corrections(frame: int, compatible: dict[str, Euler], idle: bool = False) -> dict[str, list[float]]:
    phase = 0.25 if idle else frame / COUNT
    poses, height = closed_source(phase)
    locals_ = base_locals(frame, idle)
    goals = retarget_goals(poses)
    if idle:
        goals = {name: rotation for name, rotation in goals.items() if any(name.endswith(side + part) for side in ("Left", "Right") for part in ("Shoulder", "Arm", "ForeArm", "Hand"))}
    else:
        fit_legs(locals_, goals, poses, height)
    fit_arms(locals_, goals, poses)
    matrices = forward(locals_, goals)
    result = {}
    for name in goals:
        index = INDEX[name]
        parent = BONES[index]["parent"]
        above = np.array(TARGET["head_columns"]).T if parent is None else matrices[parent]
        local = np.linalg.inv(above) @ matrices[index]
        base = locals_[index]
        delta = orthogonal(np.linalg.inv(base[:3, :3]) @ local[:3, :3])
        rotation = Matrix(delta).to_quaternion()
        euler = rotation.to_euler("XYZ", compatible.get(name, Euler((0, 0, 0))))
        compatible[name] = euler.copy()
        result[name] = [round(math.degrees(angle), 4) for angle in euler]
    return result


for idle in (False, True):
    compatible = {}
    keys = {}
    for frame in range(COUNT):
        for name, angles in corrections(frame, compatible, idle).items():
            keys.setdefault(name, []).append({"at": frame / COUNT, "turn": angles})
    motion = {
        "model": "hero", "clip": "Стойка" if idle else "Бег", "speed": 1.0 if idle else 1.8,
        "loop": True, "bones": keys,
        "reference": {"url": "https://animmade.com/animation/1427", "segment": [0, 2], "cycle": [START, END], "approved": True},
    }
    if not idle:
        motion["bounce"] = 0.4
    output = ROOT / ("idle-candidate.json" if idle else "candidate.json")
    output.write_text(json.dumps(motion, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"keys": COUNT, "bones": len(keys), "cycle": [START, END], "output": str(output)}))
