import json
import math
from pathlib import Path

import numpy as np
from mathutils import Euler, Matrix, Quaternion, Vector

ROOT = Path(__file__).parent
RIG = ROOT.parent / "hero-akm-run"
SOURCE = json.loads((ROOT / "source-measurements.json").read_text(encoding="utf-8"))
TARGET = json.loads((RIG / "target-rig.json").read_text(encoding="utf-8"))
CONTACT = json.loads((RIG / "contact-mesh.json").read_text(encoding="utf-8"))
# The hero's motions before this transfer: the finger grip there was fitted over the idle upper body.
MOTIONS = json.loads((ROOT / "motions-before.json").read_text(encoding="utf-8"))
# Poses of the model's Бег and Спринт clips as the engine evaluates them (temporary native probe, 65 keys each).
TARGET["clips"] += [clip for clip in json.loads((ROOT / "base-clips.json").read_text(encoding="utf-8"))["clips"]
                    if clip["name"] == "Спринт"]
OLD_RUN = MOTIONS["бег_героя"]
# Rifle in akm.glb after the 2026-11-07 mount correction: grip at the item origin, support (+ building shift) as the
# engine measures it; the barrel runs along -Y, the top of the rifle along -X. The rifle was scaled to 0.85 on
# 2026-11-08 (owner: «уменьши автомат… подходящим под героя»): the hero's arms are about 15% shorter than a man's.
RIFLE_SCALE = 0.85
SUPPORT_NEW = np.array([-0.013, -0.182, 0.002]) * RIFLE_SCALE
MOUNT = np.array([[0.0, 0.0, -1.0], [0.0, -1.0, 0.0], [-1.0, 0.0, 0.0]])
SWAP = np.array([[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]])
# Left palm on the fore-end relative to the rifle, measured on the fitted grip of the previous motion.
LEFT_PALM = np.array([[0.0, 0.0, 1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]])
SCENE_SCALE = float(TARGET["scale"])
# The hero runs at the game speed of 4.5 m/s (owner, 2026-11-08: 2.2 was «очень медленно»), faster than the
# reference (3.2 m/s): the cycle is shorter and each foot stands on the ground for a smaller share of it.
GAME_SPEED = 4.5
# Спринт carries the pelvis higher than Бег, so the legs are not bent all the time; its bob is kept at BOUNCE.
BASE_CLIP = "Спринт"
BASE_LENGTH = 0.6666666
BOUNCE = 0.35
CYCLE_SECONDS = 0.54
GAME_SPEED_FACTOR = BASE_LENGTH / CYCLE_SECONDS
# Extra distance between the feet (scene metres): the reference runner sets them almost on one line.
FOOT_SPREAD = 0.085
# The higher pelvis of Спринт leaves the leg too short for the reference's late toe-off, so each plant is set this far
# forward (scene metres): the foot lands a little ahead of the body and leaves the ground before the leg is straight.
PLANT_FORWARD = 0.08
# The reference leans 22-25 degrees; the hero holds the rifle up against the shoulder, so the upper body is straightened.
STRAIGHTEN = math.radians(10)
# The butt points at the right shoulder pocket (chest frame, model units: +X to the hero's left, +Y forward).
BUTT = np.array([-0.04, 0.136, 0.037]) * RIFLE_SCALE
BUTT_FROM_SHOULDER = np.array([0.05, 0.08, -0.06])
# The chest is turned a little to the right, the left shoulder forward, as people carry a rifle.
BLADE = math.radians(10)
BLADED = {"Spine": 1 / 3, "Spine1": 2 / 3, "Spine2": 1.0, "LeftShoulder": 1.0, "RightShoulder": 1.0, "Neck": 0.5}
# Left arm of the lowered-rifle carry (owner, 2026-11-08: «рука у цевья наполовину согнута, локоть почти прижат к
# боку»), chest frame: the upper arm hangs by the side, the forearm points forward, inward and down.
LEFT_UPPER_ARM = np.array([0.12, 0.25, -0.96])
LEFT_FOREARM = np.array([-0.3, 0.92, -0.25])
# The reference runner turns the chest with the arm swing; holding a rifle the chest keeps only this share of it.
CHEST_SWING = 0.3
LEFT_ELBOW = np.array([0.3, -0.3, -1.0])
BLADED = {"Spine": 1 / 3, "Spine1": 2 / 3, "Spine2": 1.0, "LeftShoulder": 1.0, "RightShoulder": 1.0, "Neck": 0.5}
LEGS = ("Hip", "Knee", "Ankle", "Toe")
# A foot lower than this above the ground (reference metres) stays still on the ground: heel strike to toe-off.
PLANT_HEIGHT = 0.025
# Share of the cycle each foot stands on the ground in the game (the reference stands 0.375 of it).
STANCE_SHARE = 0.27
MIRROR = np.diag([1.0, -1.0, 1.0])
START, END = 70.25 / 60, 110.25 / 60
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
BASE = next(clip for clip in TARGET["clips"] if clip["name"] == BASE_CLIP)


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
_hips = [SOURCE_POSES[i]["Hips"][:3, 3] for i in range(int(START * 60), int(END * 60) + 1)]
SOURCE_SPEED = float(np.linalg.norm(_hips[-1][:2] - _hips[0][:2]) / ((len(_hips) - 1) / 60))
LAST, LAST_HEIGHT = source_at(END)


SIDEWAYS = np.diag([-1.0, 1.0, 1.0])
LEG_PARTS = ("Hip", "Knee", "Ankle", "Toe", "Toe_end")


def mirrored_legs(poses: dict[str, np.ndarray], other: dict[str, np.ndarray]) -> None:
    """The reference runner holds the right foot above the ground before landing and sets it far ahead, so the right
    leg repeats the clean left one half a cycle later, mirrored sideways (the rest pose is symmetric)."""
    for part in LEG_PARTS:
        left, right = "Left" + part, "Right" + part
        delta = other[left][:3, :3] @ (MIRROR @ SOURCE_REST[left][:3, :3]).T
        matrix = np.eye(4)
        matrix[:3, :3] = SIDEWAYS @ delta @ SIDEWAYS @ MIRROR @ SOURCE_REST[right][:3, :3]
        matrix[:3, 3] = SIDEWAYS @ other[left][:3, 3]
        poses[right] = matrix


def warp(phase: float) -> float:
    """Reference phase shown at game phase `phase`: the reference runs slowly and stands on each foot for about 40% of
    the cycle; at the game speed the body would pass the planted foot farther than the leg reaches, so the stance plays
    faster and the flight slower, both feet alike (the perturbation repeats every half cycle)."""
    turns, offset = divmod(phase - WARP_CENTER + 0.25, 0.5)
    return WARP_CENTER - 0.25 + 0.5 * turns + float(hermite(WARP_KNOTS, WARP_VALUES, offset))


def hermite(knots: list[float], values: list[float], at: float) -> float:
    """Monotone cubic through the points (Fritsch-Carlson slopes): no overshoot, so time never runs backwards."""
    steps = np.diff(knots)
    slopes = np.diff(values) / steps
    tangents = [slopes[0]] + [0.0 if a * b <= 0 else 2 / (1 / a + 1 / b) for a, b in zip(slopes, slopes[1:])] + [slopes[-1]]
    k = min(int(np.searchsorted(knots, at, side="right")) - 1, len(steps) - 1)
    t = (at - knots[k]) / steps[k]
    h = steps[k]
    return (values[k] * (2 * t**3 - 3 * t**2 + 1) + h * tangents[k] * (t**3 - 2 * t**2 + t)
            + values[k + 1] * (-2 * t**3 + 3 * t**2) + h * tangents[k + 1] * (t**3 - t**2))


def closed_source(phase: float) -> tuple[dict[str, np.ndarray], float]:
    phase = warp(phase) % 1.0
    poses, height = closed_raw(phase)
    mirrored_legs(poses, closed_raw((phase + 0.5) % 1.0)[0])
    turn = np.eye(4)
    c, s = math.cos(STRAIGHTEN), math.sin(STRAIGHTEN)
    turn[:3, :3] = [[1, 0, 0], [0, c, -s], [0, s, c]]
    for name, matrix in poses.items():
        if name != "Hips" and not any(part in name for part in LEGS):
            poses[name] = turn @ matrix
    return poses, height


def closed_raw(phase: float) -> tuple[dict[str, np.ndarray], float]:
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
        result[pelvis][1, 3] -= (heights[frame % COUNT] - mean_height) * (1 - BOUNCE)
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


def stride_scale(side: str) -> float:
    ratio = chain_ratio(side, ("UpLeg", "Leg", "Foot"), ("Hip", "Knee", "Ankle"))
    cycle_seconds = BASE_LENGTH / GAME_SPEED_FACTOR
    return (GAME_SPEED / SCENE_SCALE) * cycle_seconds / (SOURCE_SPEED * (END - START) * ratio)


def mean_ankle_y(side: str) -> float:
    return float(np.mean([closed_source(k / COUNT)[0][side + "Ankle"][1, 3] for k in range(COUNT)]))


def stance_clearance(poses: dict[str, np.ndarray], height: float, side: str) -> float:
    ankle, toe = poses[side + "Ankle"][:3, 3], poses[side + "Toe"][:3, 3]
    return max(0, min(ankle[2] + height - 0.104, toe[2] + height - 0.032))


def stance_center() -> float:
    """Middle of the left foot stance in the reference, as a phase of the cycle."""
    down = [stance_clearance(*source_at(START + (END - START) * k / COUNT), "Left") < PLANT_HEIGHT for k in range(COUNT)]
    angles = [2 * math.pi * k / COUNT for k in range(COUNT) if down[k]]
    return (math.atan2(sum(map(math.sin, angles)), sum(map(math.cos, angles))) / (2 * math.pi)) % 1.0


WARP_CENTER = stance_center()
REFERENCE_SHARE = sum(stance_clearance(*source_at(START + (END - START) * k / COUNT), "Left") < PLANT_HEIGHT for k in range(COUNT)) / COUNT
# Within one half cycle, centred on the stance: game offset -> reference offset (stance middle stays in place).
WARP_KNOTS = [0.0, 0.25 - STANCE_SHARE / 2, 0.25, 0.25 + STANCE_SHARE / 2, 0.5]
WARP_VALUES = [0.0, 0.25 - REFERENCE_SHARE / 2, 0.25, 0.25 + REFERENCE_SHARE / 2, 0.5]


def ground_clearances() -> dict[str, list[float]]:
    """Height of each foot above the ground per key straight from the reference, not from the closed cycle: closing it
    spreads the seam mismatch over the whole cycle and lifts a planted foot. The seam is closed inside one swing of the
    reference; the stance squeeze is applied after that, by sampling the closed heights at the warped phase."""
    values = [stance_clearance(*source_at(START + (END - START) * k / COUNT), "Left") for k in range(COUNT + 1)]
    gap = values[0] - values[COUNT]
    down = [value < PLANT_HEIGHT for value in values]
    last = max(k for k in range(COUNT + 1) if not down[k])
    first = last
    while first > 0 and not down[first - 1]:
        first -= 1
    closed = []
    for k in range(COUNT):
        along = min(1.0, max(0.0, (k - first) / max(1, last + 1 - first)))
        closed.append(max(0.0, values[k] + gap * along * along * (3 - 2 * along)))

    def at(phase: float) -> float:
        position = (phase % 1.0) * COUNT
        before = int(position) % COUNT
        along = position - int(position)
        return closed[before] * (1 - along) + closed[(before + 1) % COUNT] * along

    return {"Left": [at(warp(k / COUNT)) for k in range(COUNT)], "Right": [at(warp(k / COUNT) + 0.5) for k in range(COUNT)]}


def ground_ankle(side: str, key: float, middle: float) -> float:
    if side == "Right":
        return ground_ankle("Left", key + COUNT / 2, middle + COUNT / 2)
    """How far forward the reference ankle has moved on the ground since key `middle` (root path kept, in the
    heading of `middle`): nothing while the foot is flat, forward as the foot rolls over the toe."""
    def at(value: float) -> np.ndarray:
        frame = (START + (END - START) * warp(value / COUNT)) * 60
        before = min(int(frame), len(SOURCE_POSES) - 2)
        along = frame - before
        first, last = SOURCE_POSES[before][side + "Ankle"], SOURCE_POSES[before + 1][side + "Ankle"]
        return first[:3, 3] * (1 - along) + last[:3, 3] * along

    frame = (START + (END - START) * warp(middle / COUNT)) * 60
    heading = float(np.interp(frame, np.arange(len(HEADING)), HEADING))
    c, s = math.cos(heading), math.sin(heading)
    align = MIRROR @ np.array([[c, s, 0], [-s, c, 0], [0, 0, 1]])
    return float((align @ (at(key) - at(middle)))[1])


def planted_paths() -> dict[str, list[float]]:
    """Forward position of each ankle per key: while the reference foot is on the ground it moves back at exactly the
    game speed, so it stands still on the ground; the swing keeps the scaled reference path, eased onto the plants."""
    seconds = BASE_LENGTH / GAME_SPEED_FACTOR / COUNT
    back = GAME_SPEED / SCENE_SCALE * seconds
    pelvis = [forward(base_locals(frame), {})[INDEX["mixamorig:Hips"]][1, 3] for frame in range(COUNT)]
    samples = [closed_source(frame / COUNT) for frame in range(COUNT)]
    paths = {}
    for side in ("Left", "Right"):
        ratio = chain_ratio(side, ("UpLeg", "Leg", "Foot"), ("Hip", "Knee", "Ankle"))
        scaled = [
            pelvis[k] + (MEAN_Y[side] + (poses[side + "Ankle"][1, 3] - MEAN_Y[side]) * STRIDE[side]) * ratio
            for k, (poses, _) in enumerate(samples)
        ]
        down = [value < PLANT_HEIGHT for value in CLEARANCE[side]]
        start = next(k for k in range(COUNT) if down[k] and not down[k - 1])
        runs, k = [], start
        while True:
            first = k
            while down[k % COUNT]:
                k += 1
            runs.append((first, k - 1))
            while not down[k % COUNT]:
                k += 1
            if k % COUNT == start:
                break
        wanted = [None] * COUNT
        for first, last in runs:
            middle = (first + last) / 2
            roll = {j: ground_ankle(side, j, middle) * ratio for j in range(first, last + 1)}
            anchor = float(np.mean([scaled[j % COUNT] - roll[j] for j in range(first, last + 1)]))
            anchor += PLANT_FORWARD / SCENE_SCALE
            for j in range(first, last + 1):
                wanted[j % COUNT] = anchor - back * (j - middle) + roll[j]
        for index, (first, last) in enumerate(runs):
            following = runs[(index + 1) % len(runs)][0]
            if following <= last:
                following += COUNT
            leave = wanted[last % COUNT] - scaled[last % COUNT]
            land = wanted[following % COUNT] - scaled[following % COUNT]
            for j in range(last + 1, following):
                along = (j - last) / (following - last)
                ease = along * along * (3 - 2 * along)
                wanted[j % COUNT] = scaled[j % COUNT] + leave + (land - leave) * ease
        paths[side] = wanted
    return paths


def fit_legs(locals_: list[np.ndarray], goals: dict[str, np.ndarray], poses: dict[str, np.ndarray], height: float, frame: int) -> None:
    pelvis = forward(locals_, goals)[INDEX["mixamorig:Hips"]][:3, 3]
    for side in ("Left", "Right"):
        names = ["mixamorig:" + side + name for name in ("UpLeg", "Leg", "Foot")]
        ratio = chain_ratio(side, ("UpLeg", "Leg", "Foot"), ("Hip", "Knee", "Ankle"))
        ankle, toe = poses[side + "Ankle"][:3, 3].copy(), poses[side + "Toe"][:3, 3]
        ankle[1] = MEAN_Y[side] + (ankle[1] - MEAN_Y[side]) * STRIDE[side]
        clearance = CLEARANCE[side][frame] * ratio
        target = pelvis + ankle * ratio
        target[2] = REST[names[2]][2, 3] + clearance
        horizontal = target[:2].copy()
        horizontal[1] = PLANTED[side][frame] + TOE_SHIFT[side][frame]
        horizontal[0] += (1 if side == "Left" else -1) * FOOT_SPREAD / 2 / SCENE_SCALE
        axis = unit(ankle - poses[side + "Hip"][:3, 3])
        knee = poses[side + "Knee"][:3, 3] - poses[side + "Hip"][:3, 3]
        measured = knee - axis * np.dot(knee, axis)
        pole = np.array([0.0, 1.0, 0.0]) + unit(measured) * 0.25
        for _ in range(4):
            target[:2] = horizontal
            target = reachable_target(locals_, goals, names, target)
            solve_chain(locals_, goals, names, target, pole)
            target[2] += clearance - foot_bottom(forward(locals_, goals), side.lower())
        matrices = forward(locals_, goals)
        FITTED[side][frame] = (matrices[INDEX["mixamorig:" + side + "ToeBase"]][1, 3], matrices[INDEX[names[2]]][2, 3])


MEAN_Y = {side: mean_ankle_y(side) for side in ("Left", "Right")}
STRIDE = {side: stride_scale(side) for side in ("Left", "Right")}
CLEARANCE = ground_clearances()
TOE_SHIFT = {side: [0.0] * COUNT for side in ("Left", "Right")}
FITTED = {side: [(0.0, 0.0)] * COUNT for side in ("Left", "Right")}
EASE_OUT = 8


def toe_shifts() -> None:
    """Once the heel lifts, the fitted toe still on the ground must keep moving back at the game speed; the ankle is
    shifted by what the toe missed, and the shift eases out over the start of the swing."""
    back = GAME_SPEED / SCENE_SCALE * BASE_LENGTH / GAME_SPEED_FACTOR / COUNT
    for side in ("Left", "Right"):
        down = [value < PLANT_HEIGHT for value in CLEARANCE[side]]
        lowest = min(height for _, height in FITTED[side])
        start = next(k for k in range(COUNT) if not down[k] and down[k - 1])
        extra = [0.0] * COUNT
        k = start
        for _ in range(COUNT):
            last = k % COUNT
            if not down[last] and down[(k - 1) % COUNT]:
                first = k - 1
                while down[(first - 1) % COUNT]:
                    first -= 1
                heel = next((j for j in range(first, k) if FITTED[side][j % COUNT][1] > lowest + 0.006), None)
                if heel is not None:
                    anchor = FITTED[side][heel % COUNT][0]
                    for j in range(heel, k):
                        wanted = anchor - back * (j - heel)
                        extra[j % COUNT] = wanted - FITTED[side][j % COUNT][0]
                    for step in range(1, EASE_OUT + 1):
                        along = step / (EASE_OUT + 1)
                        extra[(k - 1 + step) % COUNT] = extra[(k - 1) % COUNT] * (1 - along * along * (3 - 2 * along))
            k += 1
        TOE_SHIFT[side] = [TOE_SHIFT[side][j] + extra[j] for j in range(COUNT)]
PLANTED = planted_paths()


def yaw_pitch(vector: np.ndarray) -> tuple[float, float]:
    return math.atan2(vector[0], vector[1]), math.atan2(vector[2], float(np.linalg.norm(vector[:2])))


def level_frame(yaw: float, pitch: float) -> np.ndarray:
    """Rifle frame whose barrel axis points at `yaw`, `pitch` with the top of the rifle kept up (no roll)."""
    horizontal = np.array([math.sin(yaw), math.cos(yaw), 0.0])
    x_axis = np.cross(horizontal, np.array([0, 0, 1]))
    y_axis = horizontal * math.cos(pitch) + np.array([0, 0, math.sin(pitch)])
    return np.column_stack((x_axis, y_axis, np.cross(x_axis, y_axis)))


def aimed_item(direction: np.ndarray) -> np.ndarray:
    """Rifle rotation that lays the line from the butt to the left-hand point along `direction`."""
    along = MOUNT @ (SUPPORT_NEW - BUTT)
    yaw, pitch = yaw_pitch(direction)
    for _ in range(6):
        got = yaw_pitch(level_frame(yaw, pitch) @ along)
        want = yaw_pitch(direction)
        yaw, pitch = yaw + want[0] - got[0], pitch + want[1] - got[1]
    return level_frame(yaw, pitch) @ MOUNT


def fit_arms(locals_: list[np.ndarray], goals: dict[str, np.ndarray], poses: dict[str, np.ndarray]) -> None:
    """The left arm is placed first, as the owner holds a lowered rifle: elbow by the side, arm half bent, the hand in
    front of the belly; the fore-end lies in that hand and the butt points at the right shoulder pocket, so the barrel
    looks forward and down across the body. The hero's arms are short for anything straighter (left arm stretched)."""
    def measured_yaw() -> float:
        turn = orthogonal(forward(locals_, goals)[INDEX["mixamorig:Spine2"]][:3, :3] @ REST["mixamorig:Spine2"][:3, :3].T)
        ahead = turn @ np.array([0.0, 1.0, 0.0])
        return math.atan2(ahead[0], ahead[1])

    turn_by = BLADE + (1 - CHEST_SWING) * measured_yaw()
    for name, share in BLADED.items():
        c, s = math.cos(turn_by * share), math.sin(turn_by * share)
        goals["mixamorig:" + name] = np.array([[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]) @ goals["mixamorig:" + name]
    matrices = forward(locals_, goals)
    chest_yaw = measured_yaw()
    twist = np.array([[math.cos(chest_yaw), math.sin(chest_yaw), 0.0], [-math.sin(chest_yaw), math.cos(chest_yaw), 0.0], [0.0, 0.0, 1.0]])
    left = ["mixamorig:Left" + name for name in ("Arm", "ForeArm", "Hand")]
    right = ["mixamorig:Right" + name for name in ("Arm", "ForeArm", "Hand")]
    upper, lower = (float(np.linalg.norm(REST[b][:3, 3] - REST[a][:3, 3])) for a, b in zip(left, left[1:]))
    wrist = matrices[INDEX[left[0]]][:3, 3] + twist @ (unit(LEFT_UPPER_ARM) * upper + unit(LEFT_FOREARM) * lower)
    pocket = matrices[INDEX["mixamorig:RightArm"]][:3, 3] + twist @ BUTT_FROM_SHOULDER
    item = aimed_item(wrist - pocket)
    hand = wrist - item @ SUPPORT_NEW
    solve_chain(locals_, goals, right, hand, twist @ np.array([-0.4, -0.3, -1.0]))
    goals["mixamorig:RightHand"] = item @ SWAP
    target = forward(locals_, goals)[INDEX["mixamorig:RightHand"]][:3, 3] + item @ SUPPORT_NEW
    solve_chain(locals_, goals, left, target, twist @ LEFT_ELBOW)
    goals["mixamorig:LeftHand"] = item @ LEFT_PALM


def corrections(frame: int, compatible: dict[str, Euler], idle: bool = False) -> dict[str, list[float]]:
    phase = 0.25 if idle else frame / COUNT
    poses, height = closed_source(phase)
    locals_ = base_locals(frame, idle)
    goals = retarget_goals(poses)
    if idle:
        goals = {name: rotation for name, rotation in goals.items() if any(name.endswith(side + part) for side in ("Left", "Right") for part in ("Shoulder", "Arm", "ForeArm", "Hand"))}
    else:
        fit_legs(locals_, goals, poses, height, frame)
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


def finger_corrections() -> dict[str, list[dict]]:
    """Grip corrections of the previous motion were made over the idle upper body; re-express them over the base clip."""
    idle = next(clip for clip in TARGET["clips"] if clip["name"] == "Стойка")
    result = {}
    for name, keys in OLD_RUN["bones"].items():
        if "Hand" not in name or name.endswith("Hand"):
            continue
        index = INDEX[name]
        compatible = Euler((0, 0, 0))
        converted = []
        for frame, key in enumerate(keys):
            assert abs(key["at"] - frame / COUNT) < 1e-6
            run_local = base_locals(frame)[index][:3, :3]
            idle_local = base_locals(frame, True)[index][:3, :3]
            old = np.array(Euler([math.radians(v) for v in key["turn"]], "XYZ").to_matrix())
            delta = orthogonal(np.linalg.inv(run_local) @ idle_local @ old)
            euler = Matrix(delta).to_quaternion().to_euler("XYZ", compatible)
            compatible = euler.copy()
            converted.append({"at": key["at"], "turn": [round(math.degrees(a), 4) for a in euler]})
        result[name] = converted
    return result


for _ in range(3):
    compatible = {}
    keys = {}
    for frame in range(COUNT):
        for name, angles in corrections(frame, compatible).items():
            keys.setdefault(name, []).append({"at": frame / COUNT, "turn": angles})
    toe_shifts()
keys.update(finger_corrections())
motion = {
    "model": "hero", "clip": BASE_CLIP, "speed": round(GAME_SPEED_FACTOR, 4), "bounce": BOUNCE, "loop": True, "bones": keys,
    "reference": {"url": "https://animmade.com/animation/1404", "segment": [0, 2], "cycle": [START, END],
                  "approved": True, "note": "Lowered-rifle run, owner-approved 2026-11-07; stride scaled to the game speed."},
}
(ROOT / "candidate.json").write_text(json.dumps(motion, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"bones": len(keys), "stride": STRIDE, "source_speed": SOURCE_SPEED, "cycle": [START, END]}))
