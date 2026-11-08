import hashlib
import json
from pathlib import Path

import bpy
import numpy as np

ATLAS_FOLDER = Path(r"C:\projects\kuznya-mirov\art\horde\3d\hero\rifle\akm_v1\work\mat\uv-fix")


def apply(scene, names: list[str]) -> dict[str, object]:
    data = json.loads((ATLAS_FOLDER / "uv-map.json").read_text(encoding="utf-8"))
    image = bpy.data.images.load(str(ATLAS_FOLDER / "akm-stable-basecolor.png"), check_existing=False)
    material = bpy.data.materials.get("HordeAKMv1_stable_artwork") or bpy.data.materials.new("HordeAKMv1_stable_artwork")
    material.use_nodes = True
    nodes, links = material.node_tree.nodes, material.node_tree.links
    shader = next(node for node in nodes if node.bl_idname == "ShaderNodeBsdfPrincipled")
    texture = next((node for node in nodes if node.bl_idname == "ShaderNodeTexImage"), None)
    if texture is None:
        texture = nodes.new("ShaderNodeTexImage")
    texture.image = image
    texture.extension = "EXTEND"
    links.new(texture.outputs["Color"], shader.inputs["Base Color"])
    shader.inputs["Roughness"].default_value = 1.0
    shader.inputs["Specular IOR Level"].default_value = 0.15
    for name in names:
        mesh = bpy.data.objects[name].data
        geometry = json.dumps([tuple(vertex.co) for vertex in mesh.vertices]).encode()
        if hashlib.sha256(geometry).hexdigest() != data["geometry_hash"]:
            raise ValueError("Approved geometry changed: regenerate atlas instead of reusing UV coordinates")
        if len(mesh.loops) != len(data["uv"]):
            raise ValueError("Approved mesh loop ordering changed")
        for layer_name in [layer.name for layer in mesh.uv_layers]:
            mesh.uv_layers.remove(mesh.uv_layers[layer_name])
        layer = mesh.uv_layers.new(name="UVMap")
        layer.data.foreach_set("uv", np.asarray(data["uv"], dtype=np.float32).ravel())
        mesh.uv_layers.active = layer
        layer.active_render = True
        for attribute_name in ("cap_mix", "cap_tone"):
            attribute = mesh.color_attributes.get(attribute_name)
            if attribute:
                mesh.color_attributes.remove(attribute)
        mesh.materials.clear()
        mesh.materials.append(material)
        for polygon in mesh.polygons:
            polygon.material_index = 0
    image.pack()
    return {"atlas": str(ATLAS_FOLDER / "akm-stable-basecolor.png"),
            "size": list(image.size), "geometry_hash": data["geometry_hash"],
            "uv_layers": ["UVMap"], "projection": "stable side and cap charts baked into atlas"}
