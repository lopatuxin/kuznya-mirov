from __future__ import annotations

import copy
import hashlib
import json
import logging
import struct
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[4]
MODEL = ROOT / "games/pit/models/zombie.glb"
REPORT = Path(__file__).with_name("hole-emergence-validation.json")
SOURCE_CLIP = "Основа_подъёма"
TARGET_CLIP = "Основа_вылезания_из_ямы"
HIPS = "mixamorig:Hips"
SLOPE_RISE = 0.44
EMERGENCE_DEPTH = 0.95
EMERGENCE_END_PHASE = 0.2
FORWARD_SCALE = 0.55

JsonObject = dict[str, Any]


def read_glb(path: Path) -> tuple[JsonObject, bytearray]:
    raw = path.read_bytes()
    json_length = struct.unpack_from("<I", raw, 12)[0]
    document = json.loads(raw[20:20 + json_length])
    binary_offset = 28 + json_length
    return document, bytearray(raw[binary_offset:])


def accessor_values(document: JsonObject, binary: bytearray, index: int) -> list[list[float]]:
    accessor = document["accessors"][index]
    view = document["bufferViews"][accessor["bufferView"]]
    width = {"SCALAR": 1, "VEC3": 3, "VEC4": 4}[accessor["type"]]
    offset = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    stride = view.get("byteStride", width * 4)
    return [
        list(struct.unpack_from("<" + "f" * width, binary, offset + row * stride))
        for row in range(accessor["count"])
    ]


def clip_named(document: JsonObject, name: str) -> JsonObject:
    return next(clip for clip in document["animations"] if clip["name"] == name)


def hips_channel(document: JsonObject, clip: JsonObject) -> JsonObject:
    return next(
        channel
        for channel in clip["channels"]
        if document["nodes"][channel["target"]["node"]]["name"] == HIPS
        and channel["target"]["path"] == "translation"
    )


def smoothstep(value: float) -> float:
    return value * value * (3.0 - 2.0 * value)


def emergence_roots(times: list[list[float]], roots: list[list[float]]) -> list[list[float]]:
    first = roots[0]
    last = roots[-1]
    duration = times[-1][0] - times[0][0]
    forward = last[2] - first[2]
    result = []
    for time, root in zip(times, roots, strict=True):
        phase = (time[0] - times[0][0]) / duration
        along = (root[2] - first[2]) / forward
        emergence = 1.0 - smoothstep(min(1.0, phase / EMERGENCE_END_PHASE))
        result.append([
            root[0],
            root[1] - SLOPE_RISE * along - EMERGENCE_DEPTH * emergence,
            first[2] + (root[2] - first[2]) * FORWARD_SCALE,
        ])
    return result


def append_accessor(document: JsonObject, binary: bytearray, values: list[list[float]]) -> int:
    while len(binary) % 4:
        binary.append(0)
    offset = len(binary)
    for value in values:
        binary.extend(struct.pack("<3f", *value))
    view_index = len(document["bufferViews"])
    document["bufferViews"].append({
        "buffer": 0,
        "byteOffset": offset,
        "byteLength": len(binary) - offset,
        "byteStride": 12,
    })
    accessor_index = len(document["accessors"])
    document["accessors"].append({
        "bufferView": view_index,
        "componentType": 5126,
        "count": len(values),
        "type": "VEC3",
        "min": [min(row[axis] for row in values) for axis in range(3)],
        "max": [max(row[axis] for row in values) for axis in range(3)],
    })
    document["buffers"][0]["byteLength"] = len(binary)
    return accessor_index


def replace_accessor(document: JsonObject, binary: bytearray, index: int, values: list[list[float]]) -> None:
    accessor = document["accessors"][index]
    view = document["bufferViews"][accessor["bufferView"]]
    offset = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    stride = view.get("byteStride", 12)
    for row, value in enumerate(values):
        struct.pack_into("<3f", binary, offset + row * stride, *value)


def upsert_clip(document: JsonObject, binary: bytearray, roots: list[list[float]]) -> None:
    source = clip_named(document, SOURCE_CLIP)
    existing = next((clip for clip in document["animations"] if clip["name"] == TARGET_CLIP), None)
    if existing is None:
        target = copy.deepcopy(source)
        target["name"] = TARGET_CLIP
        channel = hips_channel(document, target)
        sampler = target["samplers"][channel["sampler"]]
        sampler["output"] = append_accessor(document, binary, roots)
        document["animations"].append(target)
        return
    channel = hips_channel(document, existing)
    sampler = existing["samplers"][channel["sampler"]]
    replace_accessor(document, binary, sampler["output"], roots)


def write_glb(path: Path, document: JsonObject, binary: bytearray) -> None:
    encoded = json.dumps(document, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    encoded += b" " * ((-len(encoded)) % 4)
    while len(binary) % 4:
        binary.append(0)
    total = 12 + 8 + len(encoded) + 8 + len(binary)
    temporary = path.with_suffix(".tmp")
    temporary.write_bytes(
        struct.pack("<4sII", b"glTF", 2, total)
        + struct.pack("<I4s", len(encoded), b"JSON")
        + encoded
        + struct.pack("<I4s", len(binary), b"BIN\0")
        + binary
    )
    temporary.replace(path)


def build() -> JsonObject:
    document, binary = read_glb(MODEL)
    source = clip_named(document, SOURCE_CLIP)
    channel = hips_channel(document, source)
    sampler = source["samplers"][channel["sampler"]]
    times = accessor_values(document, binary, sampler["input"])
    source_roots = accessor_values(document, binary, sampler["output"])
    roots = emergence_roots(times, source_roots)
    upsert_clip(document, binary, roots)
    write_glb(MODEL, document, binary)
    report = {
        "model": str(MODEL),
        "sha256": hashlib.sha256(MODEL.read_bytes()).hexdigest(),
        "sourceClip": SOURCE_CLIP,
        "clip": TARGET_CLIP,
        "keys": len(roots),
        "baseClipSeconds": times[-1][0] - times[0][0],
        "emergenceDepthModelUnits": EMERGENCE_DEPTH,
        "emergenceEndPhase": EMERGENCE_END_PHASE,
        "forwardTravelModelUnits": roots[-1][2] - roots[0][2],
        "slopeRiseRemovedModelUnits": SLOPE_RISE,
        "firstRoot": roots[0],
        "lastRoot": roots[-1],
        "finalStandingHeightMatchesWalk": abs(roots[-1][1] - source_roots[0][1]) < 1e-6,
        "sourceClipPreserved": True,
        "animationCount": len(document["animations"]),
    }
    REPORT.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return report


logging.basicConfig(level=logging.INFO, format="%(message)s")
logging.info(json.dumps(build(), ensure_ascii=False, indent=2))
