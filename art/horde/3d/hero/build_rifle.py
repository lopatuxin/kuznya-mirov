import importlib
import math
import sys
from pathlib import Path

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import (  # noqa: E402
    Frame,
    Part,
    beam,
    box,
    build_prop,
    export,
    turned,
)

import bpy  # noqa: E402

ROOT = Path(r"C:\projects\kuznya-mirov")
NAME = "rifle"
SCENE_NAME = "Horde_Rifle"
FOLDER = ROOT / "art" / "horde" / "3d" / "hero" / NAME
FRAME = Frame(1.4, 0.0, 1254, str(FOLDER))
# Рост исходного рига 0,835775 м в игре равен 1,73 м; предмет наследует этот масштаб.
EXPORT_SCALE = 0.8357751620933414 / 1.73
COLORS = {
    "wood": (0.32, 0.12, 0.045),
    "metal": (0.055, 0.065, 0.075),
    "steel": (0.18, 0.2, 0.22),
}


def build() -> dict[str, object]:
    scene = bpy.data.scenes.get(SCENE_NAME) or bpy.data.scenes.new(SCENE_NAME)
    old = bpy.context.window.scene
    bpy.context.window.scene = scene
    try:
        part = Part()
        box(part, "wood", (0.0, 0.12, -0.025), (0.055, 0.25, 0.065))
        box(part, "metal", (0.0, -0.075, 0.01), (0.06, 0.22, 0.07))
        box(part, "wood", (0.0, -0.27, 0.015), (0.06, 0.17, 0.06))
        beam(part, "metal", (0.0, -0.34, 0.035), (0.0, -0.62, 0.035), 0.022)
        beam(part, "metal", (0.0, -0.17, 0.065), (0.0, -0.43, 0.065), 0.018)
        beam(part, "wood", (0.0, 0.005, -0.02), (0.0, 0.04, -0.13), 0.038)
        beam(part, "metal", (0.0, -0.08, -0.04), (0.0, -0.115, -0.13), 0.04)
        beam(part, "metal", (0.0, -0.115, -0.13), (0.0, -0.17, -0.21), 0.04)
        box(part, "metal", (0.0, -0.64, 0.035), (0.03, 0.04, 0.03))
        box(part, "steel", (0.0, -0.16, 0.075), (0.035, 0.035, 0.025))
        box(part, "metal", (0.0, -0.56, 0.075), (0.035, 0.02, 0.065))
        result = build_prop(NAME, turned(part, -math.pi / 2), COLORS)
        support = bpy.data.objects.new("support", None)
        support.location = (-0.275, 0.0, 0.025)
        support.empty_display_type = "PLAIN_AXES"
        support.empty_display_size = 0.025
        scene.collection.objects.link(support)
        result["support"] = list(support.location)
        return result
    finally:
        bpy.context.window.scene = old


def render_guide() -> str:
    old = bpy.context.window.scene
    bpy.context.window.scene = bpy.data.scenes[SCENE_NAME]
    try:
        FRAME.render([("guide.png", {NAME})])
    finally:
        bpy.context.window.scene = old
    return str(FOLDER / "guide.png")


def export_painted() -> str:
    old = bpy.context.window.scene
    bpy.context.window.scene = bpy.data.scenes[SCENE_NAME]
    try:
        FRAME.fit("paint_raw.png", "guide.png", "paint.png")
        picture = FRAME.picture(NAME, FRAME.load("paint.png"), FRAME.load("guide.png"), 14)
        obj = FRAME.projected_copy(bpy.data.objects[NAME], NAME + "_game", picture)
        obj.rotation_euler.z = math.pi / 2
        obj.scale *= EXPORT_SCALE
        support = bpy.data.objects["support"]
        support.location = (0.0, -0.275, 0.025)
        support.location *= EXPORT_SCALE
        support.empty_display_size *= EXPORT_SCALE
        export(obj, ("support",), NAME + ".glb")
        bpy.data.objects.remove(obj, do_unlink=True)
    finally:
        bpy.context.window.scene = old
    return str(ROOT / "games" / "horde" / "models" / f"{NAME}.glb")


result = build()
