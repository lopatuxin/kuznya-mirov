# Проверка самосвала плагином blender-modeling: сцена с видами листа, линейкой и моделью из скрипта игры.
# Запуск в живом Blender после загрузки помощников плагина (BM = <плагин>/scripts):
#     exec(open(r"C:\projects\kuznya-mirov\art\transport\gaz_aa\dump\dump.py", encoding="utf-8").read()); result = build()
# Выгрузка в игру — по-прежнему скриптом art/transport/models/gaz_aa.py.
ASSET = r"C:\projects\kuznya-mirov\art\transport\gaz_aa\dump"
load_dims(ASSET)

# Лист рисует базу 3,62 м при паспортных 3,34: вид сбоку выровнен по кабине, общей у всех трёх машин, —
# высота 1,97 м на 529 точках ref/right.png, — и по передней оси (178 точек от левого края картинки),
# начало — середина паспортной длины 4,67 м на земле (549 точек от верха).
SIDE_PPM = 529 / 1.97
FRONT_AXLE_PX = (178, 549)


def setup():
    """Сцена и виды листа: сбоку — по кабине и передней оси, спереди и сзади — по высоте кабины 1,97 м."""
    sc = scene_for(SCENE)
    ax = FRONT_AXLE_PX[0] + (SIZE["y"] / 2 - 0.62) * SIDE_PPM
    add_ref_view(sc, "GD_ref_right", ASSET + r"\ref\right.png", "right", ppm=SIDE_PPM, anchor_px=(ax, FRONT_AXLE_PX[1]))
    for view in ("front", "back"):
        add_ref_view(sc, f"GD_ref_{view}", ASSET + rf"\ref\{view}.png", view, real=SIZE["z"], measure="height", anchor="base")
    return sc


def build():
    sc = setup()
    for pid in PARTS:
        build_part(sc, ASSET, pid)
    save_scene_copy(sc, ASSET + r"\work\calib.blend")
    return whole_gates(sc, ASSET + r"\review", need=0.85, budget_tris=BUDGET_TRIS)
