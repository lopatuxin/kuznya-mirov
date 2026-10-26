# Materials of the port pier: role recipes (blender-material-artist). Colours measured on
# ref/right.png, ref/front.png, ref/back.png and ../concept.png; roles are the material names the
# part uses (Port_Pier_<role>), plus two the pass adds by a slot swap on its own copy of the
# model (calib.blend is not changed): Port_Pier_fender (the fender logs, own colour against the
# timber band behind them) and Port_Pier_log_end (the end faces of the crib logs).
# Below WATER_Z the crib logs, piles, fenders and stones are wet, dark and algae-green up to
# about WATER_Z + 0.4, fading out by 3.0 m: a band layer of each role, not a material of its own.
LIB = r"C:\projects\kuznya-mirov\art\transport\textures"
BAKE = {"size": 2048, "samples": 16}   # prominent 40 m pier: px_per_m at 1024 is 7.3, at 2048 14.6

WET_FULL, WET_NONE = WATER_Z + 0.4, 3.0
WET = {"color": "#36332a", "full": WET_FULL, "none": WET_NONE, "amount": 0.8, "noise": False}
ALGAE = {"color": "#5c5636", "full": WATER_Z + 0.1, "none": WET_FULL + 0.15, "amount": 0.4, "noise": True}
WET_BANDS = [WET, ALGAE]


def _swap_slot(obj, new):
    """Give the object's only material slot a material of its own (a role of its own)."""
    me = bpy.data.objects[obj].data
    me.materials[0] = bpy.data.materials.get(new) or bpy.data.materials.new(new)


def _log_ends(obj, new, cos_min=0.85):
    """Faces of `obj` facing along their log's axis (the end grain) get material `new`."""
    o = bpy.data.objects[obj]
    me = o.data
    m = bpy.data.materials.get(new) or bpy.data.materials.new(new)
    if m.name not in [x.name for x in me.materials if x]:
        me.materials.append(m)
    idx = [x.name if x else None for x in me.materials].index(m.name)
    m3 = o.matrix_world.to_3x3()
    bm = bmesh.new()
    bm.from_mesh(me)
    lab, n = _islands(bm)
    lo = np.full((n, 3), 1e9)
    hi = np.full((n, 3), -1e9)
    for f in bm.faces:
        for v in f.verts:
            p = np.array(m3 @ v.co)
            lo[lab[f.index]] = np.minimum(lo[lab[f.index]], p)
            hi[lab[f.index]] = np.maximum(hi[lab[f.index]], p)
    axis = np.argmax(hi - lo, 1)
    k = 0
    for f in bm.faces:
        nw = (m3 @ f.normal).normalized()
        if abs(nw[int(axis[lab[f.index]])]) > cos_min:
            f.material_index = idx
            k += 1
    bm.to_mesh(me)
    bm.free()
    me.update()
    return k


def apply(sc, names):
    _swap_slot("PP_fenders", "Port_Pier_fender")
    ends = _log_ends("PP_crib_logs", "Port_Pier_log_end")
    piece_uv(names)
    under = ["PP_deck", "PP_frame"]
    # crib logs and piles: strong tone between logs, darker to the sea bed, shaded by the deck
    tone = piece_tone(["PP_crib_logs", "PP_piles"], spread=10, seed=11, ground_dark=0.25, ground_h=1.5,
                      shade_from=under, shade_dist=1.5, shade_dark=0.2)
    tone["log_end_faces"] = ends
    piece_tone(["PP_crib_stone"], spread=6, seed=5, ground_dark=0.2, ground_h=1.2,
               shade_from=under, shade_dist=1.5, shade_dark=0.2)
    piece_tone(["PP_fenders"], spread=8, seed=17, ground_dark=0.0)
    piece_tone(["PP_frame", "PP_curb"], spread=9, seed=23, ground_dark=0.0,
               shade_from=["PP_deck"], shade_dist=1.3, shade_dark=0.18)
    piece_tone(["PP_deck"], spread=3, seed=31, ground_dark=0.0)   # 0.29 m planks: more makes a barcode
    piece_tone(["PP_bollards", "PP_chains"], spread=2, seed=41, ground_dark=0.0)

    wood = LIB + r"\Wood033"
    role_material("Port_Pier_plank", "#ad9783", lib=wood, scale=2.0, contrast=2.5, rough=(0.82, 0.94))
    role_material("Port_Pier_timber", "#a28e78", lib=wood, scale=2.5, contrast=3.5, rough=(0.82, 0.94),
                  bands=[dict(b) for b in WET_BANDS])
    role_material("Port_Pier_fender", "#7c6652", lib=wood, scale=2.5, contrast=3.0, rough=(0.82, 0.94),
                  bands=[dict(b) for b in WET_BANDS])
    role_material("Port_Pier_log", "#927c66", lib=wood, scale=3.0, contrast=3.0, rough=(0.80, 0.92),
                  bands=[dict(b) for b in WET_BANDS])
    role_material("Port_Pier_log_end", "#5e4c3c", lib=wood, scale=0.6, contrast=0.7, rough=(0.85, 0.95),
                  uv="box", bands=[dict(WET, color="#26241e"), dict(ALGAE, amount=0.25)])
    # rubble wall (ambientCG Bricks096); its own hue is dropped, the pattern is used
    role_material("Port_Pier_stone", "#a89c8c", lib=LIB + r"\Bricks096", scale=2.0, contrast=1.0,  # rubble tile 2 m: stones 0.3-0.7 m
                  rough=(0.85, 0.95), bands=[dict(WET, amount=0.7), dict(ALGAE, amount=0.4)])
    # cast iron, non-metal (director's decision for the port); red rust on the cap and the flange
    role_material("Port_Pier_iron", "#3a3430", scale=0.6, rough=(0.55, 0.7), metal=0.0, ao=0.2,
                  bands=[{"color": "#6c4636", "full": 5.45, "none": 5.33, "amount": 0.85, "noise": False},
                         {"color": "#6c4636", "full": 4.78, "none": 4.92, "amount": 0.75, "noise": False}])
    role_material("Port_Pier_chain", "#6a5444", lib=LIB + r"\MetalPlates011", scale=0.6, contrast=1.2,
                  rough=(0.82, 0.95), ao=0.0)
    return tone
