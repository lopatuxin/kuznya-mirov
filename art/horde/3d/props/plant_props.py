"""Посадка предметов Орды в землю: подкладка для нейросети и выгрузка посаженных моделей с заплатками земли.

Каждый предмет строится своим скриптом в текущей сцене, его раскраска paint.png кладётся поверх земли карты с
места, где он стоит в Орде первым (games/horde/scene.json), — props/<имя>/plant_in.png. Нейросеть дорисовывает
присыпку, траву и тень у основания — props/<имя>/plant_raw.png; export() выгружает <имя>.glb и <имя>_soil.glb.
Общее — art/horde/3d/painted_lib.py.

Запускать в живом Blender (MCP), в своей сцене: exec(open(путь).read()), затем prepare(имена) или export(имена).
"""
import importlib
import json
import os
import sys

sys.path.insert(0, r"C:\projects\kuznya-mirov\art\horde\3d")
import painted_lib  # noqa: E402

importlib.reload(painted_lib)
from painted_lib import PROPS, ROOT, export_planted_prop, prepare_prop_planting  # noqa: E402

# Предмет → скрипт, который строит его объём.
SCRIPTS = {
    "barrel": "barrel/build_barrel.py",
    "crate": "crate/build_crate.py",
    "tires": "tires/build_tires.py",
    "firepit": "firepit/build_firepit.py",
    "vaz2106": "vaz2106/build_vaz2106.py",
    "uaz452": "uaz452/build_uaz452.py",
    "guardrail": "guardrail/build_guardrail.py",
    "power_line": "power_line/build_power_line.py",
    **{name: "bushes/build_bushes.py" for name in ("bush_a", "bush_b", "bush_c")},
    **{name: "trees/build_trees.py" for name in ("tree_a", "tree_b")},
    **{name: "rocks/build_rocks.py" for name in ("rock_a", "rock_b", "rock_c")},
}


# Какой по счёту в сцене предмет даёт землю под посадку, если не первый: первые эти кусты стоят на асфальте, а
# заплатка с асфальтом легла бы пятном на землю под остальными.
PLACE_INDEX = {"bush_b": 1, "bush_c": 1}
# Какой по счёту предмет стоит на асфальте: ему своя заплатка <имя>_soil_road, посаженная на дорогу, — заплатка с
# землёй легла бы на асфальт песчаным кругом. Картина самого предмета общая.
ROAD_INDEX = {"rock_a": 1, "rock_c": 1, "bush_a": 1, "bush_b": 0, "bush_c": 0}


def place(name, index):
    """Середина `index`-го по счёту в сцене предмета `name` в клетках сцены: точка объекта сцены — угол его клетки
    `size`, середина модели — посреди клетки."""
    with open(os.path.join(ROOT, "games", "horde", "scene.json"), encoding="utf-8") as file:
        objects = json.load(file)["objects"]
    obj = [obj for obj in objects if obj["model"] == name][index]
    return obj["position"][0] + obj["size"][0] / 2, obj["position"][1] + obj["size"][1] / 2


def first_place(name):
    """Середина предмета там, откуда берётся земля под посадку."""
    return place(name, PLACE_INDEX.get(name, 0))


def build(name):
    path = os.path.join(PROPS, SCRIPTS[name])
    exec(open(path, encoding="utf-8").read(), {"__name__": "plant_" + name})


def prepare(names):
    for name in names:
        build(name)
        prepare_prop_planting(name, first_place(name))


def export(names):
    """Выгрузка посаженных предметов; возвращает растяжение и сдвиг подгонки каждой раскраски."""
    fitted = {}
    for name in names:
        build(name)
        fitted[name] = export_planted_prop(name)
    return fitted


def road_folder(name):
    return os.path.join(PROPS, name, "road")


def prepare_road(names):
    """Подкладки для заплаток на асфальте: посаженная картина предмета поверх дороги с места, где он стоит на ней, —
    props/<имя>/road/plant_in.png."""
    for name in names:
        build(name)
        source = painted_lib.plant_frame([name], os.path.join(PROPS, name))
        os.makedirs(road_folder(name), exist_ok=True)
        painted_lib.prepare_planting([name], road_folder(name), source.load("plant.png"), source,
                                     painted_lib.scene_origin([name], place(name, ROAD_INDEX[name])))


def export_road(names):
    """<имя>_soil_road.glb из посаженной на асфальт раскраски road/plant_raw.png."""
    fitted = {}
    for name in names:
        build(name)
        frame, planted, _, fitted[name] = painted_lib.finish_planting([name], road_folder(name))
        painted_lib.export_soil(frame, planted, [name], name + "_soil_road")
    return fitted
