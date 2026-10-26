# Деревянный кран причала — главный скрипт плагина blender-modeling: сцена, виды-образцы, черновые объёмы,
# сборка частей. В живом Blender после загрузки помощников плагина (BM = <плагин>/scripts):
#     exec(open(r"<папка>\crane.py", encoding="utf-8").read()); result = build("parts")
ASSET = r"C:\projects\kuznya-mirov\art\transport\port\crane"
load_dims(ASSET)

# вид: точки на метр, только для деталей; начало — середина нижней полосы силуэта (низ рамы основания)
REFS = {"front": (56.0, False), "back": (56.2, False), "left": (58.5, True), "right": (58.5, True)}


def setup():
    sc = scene_for(SCENE)
    for view, (ppm, detail) in REFS.items():
        add_ref_view(sc, f"PC_ref_{view}", ASSET + rf"\ref\{view}.png", view, ppm=ppm, anchor="base",
                     detail_only=detail)
    add_ruler(sc, 9.0, x=BOOM_TIP[0] + 1.5)
    return sc


def blockout(sc):
    col = blockout_coll(sc, "crane")
    clear(col)
    bm = bmesh.new()
    box(bm, (0, 0, BASE_TOP / 2), (BASE, BASE, BASE_TOP))
    mx, my, ms, mtop = MAST
    box(bm, (mx, my, (BASE_TOP + mtop) / 2), (ms, ms, mtop - BASE_TOP))
    beam(bm, BOOM_FOOT, BOOM_TIP, BOOM_W, BOOM_W)
    wx, wy, wz0, wz1, wh = WINCH
    box(bm, ((wx + mx - ms / 2) / 2, wy, (wz0 + wz1) / 2), (mx - ms / 2 - wx, 2 * wh, wz1 - wz0))
    box(bm, ((CRANK_X + wx) / 2, wy, 1.8), (wx - CRANK_X, 0.1, 0.1))
    hx, hy, hz = HOOK
    box(bm, (hx, hy, (hz + BOOM_TIP[2]) / 2), (0.25, 0.25, BOOM_TIP[2] - hz))
    # канаты замыкают контур между мачтой и стрелой: проверка силуэта заливает его, как на образце
    top = (mx, my, mtop - 0.3)
    beam(bm, top, BOOM_TIP, 0.06, 0.06)
    h = BASE / 2 - 0.1
    for cx, cy in ((-h, -h), (-h, h), (h, -h), (h, h)):
        beam(bm, top, (cx, cy, BASE_TOP), 0.05, 0.05)
    obj_from_bm("PC_blk_crane", bm, col, [M("timber")])


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
