from pathlib import Path


def apply_reference(scene, names: list[str]) -> dict[str, object]:
    folder = Path(ASSET)
    material_script = folder / "materials.py"
    namespace = {"__file__": str(material_script)}
    exec(compile(material_script.read_text(encoding="utf-8"),
                 str(material_script), "exec"), namespace)
    namespace["ATLAS_FOLDER"] = folder
    return namespace["apply"](scene, names)
