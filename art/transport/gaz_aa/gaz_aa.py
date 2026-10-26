# Проверка полуторки плагином blender-modeling: сцена с видами листа, линейкой и моделью из скрипта игры.
# Запуск в живом Blender после загрузки помощников плагина (BM = <плагин>/scripts):
#     exec(open(r"C:\projects\kuznya-mirov\art\transport\gaz_aa\gaz_aa.py", encoding="utf-8").read()); result = build()
# Выгрузка в игру — по-прежнему скриптом art/transport/models/gaz_aa.py.
ASSET = r"C:\projects\kuznya-mirov\art\transport\gaz_aa"
load_dims(ASSET)


def setup():
    """Сцена и виды листа. Главный вид — сбоку: масштаб по паспортной длине 5,335 м, начало — середина низа
    (опора по колёсам сдвинула бы машину: колёса стоят не посередине длины). Спереди и сзади — по высоте,
    которую лист даёт сбоку, начало — середина опоры."""
    sc = scene_for(SCENE)
    side = add_ref_view(sc, "GA_ref_right", ASSET + r"\ref\right.png", "right", real=5.335, measure="width", anchor="bottom")
    x0, y0, x1, y1 = side["subject_px"]
    height = (y1 - y0) / side["ppm"]
    for view in ("front", "back"):
        add_ref_view(sc, f"GA_ref_{view}", ASSET + rf"\ref\{view}.png", view, real=height, measure="height", anchor="base")
    add_ruler(sc, 2.0, x=1.6)
    return sc


def build():
    sc = setup()
    for pid in PARTS:
        build_part(sc, ASSET, pid)
    save_scene_copy(sc, ASSET + r"\work\calib.blend")
    return whole_gates(sc, ASSET + r"\review", need=0.85, budget_tris=BUDGET_TRIS)
