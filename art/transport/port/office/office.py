# Контора порта — главный скрипт плагина blender-modeling: сцена, виды-образцы, черновые объёмы, сборка частей.
# В живом Blender после загрузки помощников плагина (BM = <плагин>/scripts):
#     exec(open(r"<папка>\office.py", encoding="utf-8").read()); result = build("parts")
ASSET = r"C:\projects\kuznya-mirov\art\transport\port\office"
load_dims(ASSET)

# вид: точки на метр; начало — середина нижней полосы силуэта (цоколь во всю ширину)
REFS = {"front": 78.1, "back": 78.1, "left": 79.0, "right": 79.0}


def setup():
    sc = scene_for(SCENE)
    for view, ppm in REFS.items():
        add_ref_view(sc, f"PO_ref_{view}", ASSET + rf"\ref\{view}.png", view, ppm=ppm, anchor="base")
    add_ruler(sc, 6.0, x=ROOF[0] + 1.5)
    return sc


def blockout(sc):
    col = blockout_coll(sc, "office")
    clear(col)
    bm = bmesh.new()
    wx, wy = WALL[0] / 2 + LOG_END, WALL[1] / 2 + LOG_END
    box(bm, (0, 0, EAVE_Z / 2), (2 * wx, 2 * wy, EAVE_Z))
    rx, ry = ROOF
    roof = [(-ry, EAVE_Z - 0.1), (ry, EAVE_Z - 0.1), (ry, EAVE_Z), (0, RIDGE_Z), (-ry, EAVE_Z)]
    prism(roof, 2 * rx, plane="YZ", bm=bm)
    cx, cy, cs, ctop = CHIMNEY
    box(bm, (cx, cy, (RIDGE_Z + ctop) / 2 - 0.3), (cs, cs, ctop - RIDGE_Z + 0.6))
    px0, px1, depth, top = PORCH
    fy = -WALL[1] / 2
    box(bm, ((px0 + px1) / 2, fy - depth / 2, BASE_Z / 2), (px1 - px0, depth, BASE_Z))
    box(bm, ((px0 + px1) / 2, fy - depth / 2, top - 0.1), (px1 - px0 + 0.2, depth + 0.1, 0.2))
    obj_from_bm("PO_blk_office", bm, col, [M("log")])


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
