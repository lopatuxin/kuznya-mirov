# Склад порта — главный скрипт плагина blender-modeling: сцена, виды-образцы, черновые объёмы, сборка частей.
# В живом Blender после загрузки помощников плагина (BM = <плагин>/scripts):
#     exec(open(r"<папка>\warehouse.py", encoding="utf-8").read()); result = build("parts")
ASSET = r"C:\projects\kuznya-mirov\art\transport\port\warehouse"
load_dims(ASSET)

# вид: точки на метр, начало координат в точках картинки (середина стен на земле), только для деталей
REFS = {
    "front": (44.33, (731, 355), False),
    "left": (44.33, (273.5, 350), False),
    "right": (44.33, (370.5, 350), False),
    "back": (44.33, (731.5, 333), True),   # нарисован на 7% ниже вида спереди
}


def setup():
    sc = scene_for(SCENE)
    for view, (ppm, anchor, detail) in REFS.items():
        add_ref_view(sc, f"PW_ref_{view}", ASSET + rf"\ref\{view}.png", view, ppm=ppm, anchor_px=anchor,
                     detail_only=detail)
    add_ruler(sc, 8.0, x=WALL_X + 2.0)
    return sc


def blockout(sc):
    col = blockout_coll(sc, "warehouse")
    clear(col)
    bm = bmesh.new()
    y0, y1 = WALL_Y
    box(bm, (0, 0, EAVE_Z / 2), (2 * WALL_X, y1 - y0, EAVE_Z))
    box(bm, (0, (PLATFORM_Y + y0) / 2, PLATFORM_Z / 2), (2 * WALL_X, y0 - PLATFORM_Y, PLATFORM_Z))
    sx0, sx1, sy = STEPS
    box(bm, ((sx0 + sx1) / 2, (sy + PLATFORM_Y) / 2, PLATFORM_Z / 4), (sx1 - sx0, PLATFORM_Y - sy, PLATFORM_Z / 2))
    roof = [(EAVE_Y[0], EAVE_Z - 0.15), (EAVE_Y[1], EAVE_Z - 0.15), (EAVE_Y[1], EAVE_Z),
            (RIDGE[0], RIDGE[1]), (EAVE_Y[0], EAVE_Z)]
    prism(roof, 2 * ROOF_X, plane="YZ", bm=bm)
    box(bm, (VENT[0], RIDGE[0], (RIDGE[1] + VENT[1]) / 2 - 0.1), (0.35, 0.35, VENT[1] - RIDGE[1] + 0.2))
    for x in (-WALL_X, -WALL_X / 3, WALL_X / 3, WALL_X):
        box(bm, (x, PLATFORM_Y + 0.15, (PLATFORM_Z + EAVE_Z) / 2), (0.2, 0.2, EAVE_Z - PLATFORM_Z))
    obj_from_bm("PW_blk_warehouse", bm, col, [M("board")])


def build(stage="parts"):
    sc = setup()
    blockout(sc)
    out = ASSET + r"\review"
    if stage == "blockout":
        for pid in PARTS:
            show_blockout(sc, pid)
        return whole_gates(sc, out, need=0.85)
    for pid in PARTS:
        if os.path.exists(part_file(ASSET, pid)):
            build_part(sc, ASSET, pid)
    save_scene_copy(sc, ASSET + r"\work\calib.blend")
    return whole_gates(sc, out, budget_tris=BUDGET_TRIS)
