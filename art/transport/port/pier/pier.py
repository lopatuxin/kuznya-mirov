# Причал порта — главный скрипт плагина blender-modeling: сцена, виды-образцы, черновые объёмы, сборка частей.
# В живом Blender после загрузки помощников плагина (BM = <плагин>/scripts):
#     exec(open(r"<папка>\pier.py", encoding="utf-8").read()); result = build("parts")
ASSET = r"C:\projects\kuznya-mirov\art\transport\port\pier"
load_dims(ASSET)

# вид: точки на метр, начало в точках картинки (None — середина нижней полосы силуэта)
REFS = {"right": (37.05, (781, 250)), "front": (59.5, None), "back": (59.5, None)}


def setup():
    sc = scene_for(SCENE)
    for view, (ppm, anchor) in REFS.items():
        add_ref_view(sc, f"PP_ref_{view}", ASSET + rf"\ref\{view}.png", view, ppm=ppm, anchor_px=anchor)
    add_ruler(sc, 6.0, x=FENDER_X + 1.5)
    return sc


def blockout(sc):
    col = blockout_coll(sc, "pier")
    clear(col)
    bm = bmesh.new()
    for y0, y1 in CRIBS:
        box(bm, (0, (y0 + y1) / 2, DECK_UNDER / 2), (2 * HALF_W, y1 - y0, DECK_UNDER))
    for y0, y1 in PILES:
        box(bm, (0, (y0 + y1) / 2, DECK_UNDER / 2), (2 * HALF_W - 0.6, y1 - y0, DECK_UNDER))
    box(bm, (0, 0, (DECK_UNDER + CURB_TOP) / 2), (2 * HALF_W, 2 * HALF_LEN, CURB_TOP - DECK_UNDER))
    for s in (-1, 1):
        box(bm, (s * (HALF_W + 0.33), 0, (1.6 + DECK_TOP) / 2), (0.66, 2 * HALF_LEN - 1.0, DECK_TOP - 1.6))
        for y in BOLLARDS_Y:
            box(bm, (s * BOLLARD_X, y, (CURB_TOP + BOLLARD_TOP) / 2), (0.6, 0.6, BOLLARD_TOP - CURB_TOP))
    obj_from_bm("PP_blk_pier", bm, col, [M("log")])


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
