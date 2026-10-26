# Порт в начале партии — сцена показа: четыре запечённые постройки (склад, контора, причал, кран) расставлены
# по образцу concept.png, с землёй и водой для глаза. В игру эта сцена не идёт: каждая постройка выгружается сама.
# Начало — уровень моря у берега; суша к −Y, море к +Y; земля и настил причала на LAND_Z над водой.
# В живом Blender после загрузки помощников плагина blender-modeling (BM = <плагин>/scripts):
#     exec(open(r"<папка>\port.py", encoding="utf-8").read()); result = build()
import os

PORT = os.path.dirname(os.path.abspath(__file__)) if "__file__" in globals() else r"C:\projects\kuznya-mirov\art\transport\port"
SCENE_PORT = "Port"

DECK_TOP = 4.45                # верх настила причала над низом его срубов (pier/dims.py)
LAND_Z = DECK_TOP - 2.2        # земля и настил причала над водой (pier/dims.py WATER_Z 2,2)
WATER = "#3f5866"              # цвет воды: земля картинок beauty стоит на нуле, она и есть вода
SHORE_Y = 2.0                  # кромка берега: дальше к +Y море

# постройка: отделанная копия из surface.py (с надписями-детьми), место (x, y, z) и поворот вокруг Z в градусах
BUILDINGS = {
    "warehouse": (r"warehouse\work\surface\port_warehouse.blend", "port_warehouse", (-17.0, -6.0, LAND_Z), 0),
    "office": (r"office\work\surface\port_office.blend", "port_office", (17.0, -14.0, LAND_Z), 0),
    "pier": (r"pier\work\surface\port_pier.blend", "port_pier", (4.0, SHORE_Y + 20.0, LAND_Z - DECK_TOP), 0),
    "crane": (r"crane\work\surface\port_crane.blend", "port_crane", (4.0, SHORE_Y + 30.85, LAND_Z), 0),
}


def _append(blend, name, col):
    """Постройка и её надписи (все объекты файла surface.py)."""
    with bpy.data.libraries.load(blend, link=False) as (src, dst):
        if name not in src.objects:
            raise ValueError(f"{blend} has no object {name!r}")
        for n in src.objects:
            old = bpy.data.objects.get(n)
            if old is not None:
                bpy.data.objects.remove(old, do_unlink=True)
        dst.objects = list(src.objects)
    for o in dst.objects:
        col.objects.link(o)
    return next(o for o in dst.objects if o.name == name)


def build():
    sc = scene_for(SCENE_PORT)
    col = coll(sc, "MODEL")
    clear(col)
    placed = {}
    for key, (blend, name, loc, rot) in BUILDINGS.items():
        o = _append(os.path.join(PORT, blend), name, col)
        o.location = loc
        o.rotation_euler = (0.0, 0.0, math.radians(rot))
        placed[key] = o.name
    env = coll(sc, "ENV")
    clear(env)
    bm = bmesh.new()
    box(bm, (-5.0, (SHORE_Y - 25.0) / 2, LAND_Z / 2), (70.0, 25.0 + SHORE_Y, LAND_Z))
    obj_from_bm("Port_env_ground", bm, env, [mat("Port_env_ground", "#7d6a52", 0.95)])
    return {"scene": sc.name, "placed": placed}
