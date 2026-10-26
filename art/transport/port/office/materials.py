# Materials of the port office: role recipes (blender-material-artist). Colours measured on
# ref/front.png, ref/back.png, ref/left.png, ref/right.png and ../concept.png; roles are the material
# names the part uses (Port_Office_<role>). Wood warm silver-grey; the roof is the warehouse's roof
# recipe (../warehouse/materials.py) so the port buildings share one iron; iron is non-metal
# #514439 for all four port buildings (director's decision, round 2).
# Two slots get their own material here, in the materials pass only - the part file is not touched:
# slot 1 of PO_logs (the corner log ends) -> Port_Office_log_end, and the plinth core of PO_plinth
# (the mortar between the rubble stones) -> Port_Office_mortar.
LIB = r"C:\projects\kuznya-mirov\art\transport\textures"
BAKE = {"size": 1024, "samples": 16}

BOARDS = ["PO_gables", "PO_door", "PO_porch", "PO_roof_frame"]
TRIM = ["PO_windows", "PO_plaque", "PO_door_iron"]
COURSE_STEP = 0.145   # half a log course: front/back rows and end-wall rows sit on half courses
LOG_SPREAD = {"front": 11, "back": 11, "left": 6, "right": 6}   # tone between log rows, L*


def R(role):
    return f"{SCENE}_{role}"


def _own_slot(obj, slot, role):
    """Give one slot of a part object its own role material (the part file keeps its roles)."""
    m = bpy.data.materials.get(R(role)) or bpy.data.materials.new(R(role))
    bpy.data.objects[obj].data.materials[slot] = m


def _streaks_down_slope(name):
    """The roof texture turned a quarter round, so its vertical rust streaks run down the slope
    (box projection lays them along the ridge otherwise). Copied from ../warehouse/materials.py."""
    for n in bpy.data.materials[name].node_tree.nodes:
        if n.type == "MAPPING":
            n.inputs["Rotation"].default_value = (0.0, 0.0, math.pi / 2)


def _row_tone(name, spread, seed, layer="tone"):
    """One tone per log row: every segment of a row (cut by the openings) gets the same tone, so
    rows do not flip light/dark at a window. Multiplies the `layer` written by piece_tone (run with
    spread 0 for the ground and the overhang). Rows are keyed by wall and height."""
    o = bpy.data.objects[name]
    me = o.data
    bm = bmesh.new()
    bm.from_mesh(me)
    lab, n = _islands(bm)
    m3 = np.array(o.matrix_world.to_3x3())
    t = np.array(o.matrix_world.translation)
    lo = np.full((n, 3), np.inf)
    hi = np.full((n, 3), -np.inf)
    for f in bm.faces:
        i = lab[f.index]
        for v in f.verts:
            p = np.array(tuple(v.co)) @ m3.T + t
            lo[i] = np.minimum(lo[i], p)
            hi[i] = np.maximum(hi[i], p)
    bm.free()
    rnd = random.Random(seed)
    rows, factor = {}, np.ones(n)
    for i in range(n):
        c, ext = (lo[i] + hi[i]) / 2, hi[i] - lo[i]
        wall = ("front" if c[1] < 0 else "back") if ext[0] >= ext[1] else ("left" if c[0] < 0 else "right")
        key = (wall, int(round(c[2] / COURSE_STEP)))
        if key not in rows:
            width = math.sqrt(12) * spread[wall]
            rows[key] = ((55 - rnd.random() * width + 16) / 71) ** 3
        factor[i] = rows[key]
    attr = me.color_attributes[layer]
    rgba = np.empty(len(me.loops) * 4, np.float32)
    attr.data.foreach_get("color", rgba)
    rgba = rgba.reshape(-1, 4)
    tot = np.empty(len(me.polygons), np.int64)
    me.polygons.foreach_get("loop_total", tot)
    f = factor[np.array(lab)[np.repeat(np.arange(len(me.polygons)), tot)]]
    rgba[:, :3] *= f[:, None]
    attr.data.foreach_set("color", rgba.ravel())
    me.update()
    return {"rows": len(rows), "segments": n}


def apply(sc, names):
    _own_slot("PO_logs", 1, "log_end")
    _own_slot("PO_plinth", 0, "mortar")
    piece_uv(names)
    piece_tone(["PO_logs"], spread=0, seed=7, ground_dark=0.25, ground_h=1.6,
               shade_from=["PO_roof"], shade_dist=1.5, shade_dark=0.18)
    tone = {"logs": _row_tone("PO_logs", LOG_SPREAD, seed=7)}
    piece_tone(["PO_stones"], spread=7, seed=41, ground_dark=0.25, ground_h=0.6)
    piece_tone(["PO_plinth"], spread=0, seed=43, ground_dark=0.2, ground_h=0.6)
    piece_tone(BOARDS, spread=5, seed=11, ground_dark=0.2, ground_h=1.0,
               shade_from=["PO_roof"], shade_dist=1.5, shade_dark=0.15)
    piece_tone(TRIM, spread=2, seed=19, ground_dark=0.0, shade_from=["PO_roof"], shade_dist=1.5, shade_dark=0.12)
    tone["roof"] = piece_tone(["PO_roof"], spread=6, seed=53, ground_dark=0.0)
    piece_tone(["PO_chimney"], spread=2, seed=61, ground_dark=0.0)

    wood = LIB + r"\Wood033"
    role_material(R("log"), "#a28d78", lib=wood, scale=3.0, contrast=3.0, rough=(0.80, 0.93),
                  dirt={"color": "#4a4234", "height": 1.0, "amount": 0.5})
    role_material(R("log_end"), "#7f6246", lib=wood, scale=0.6, contrast=0.7, rough=(0.85, 0.95), uv="box")
    role_material(R("board"), "#7f6c5a", lib=LIB + r"\Wood086", scale=1.2, contrast=1.0, rough=(0.80, 0.92),
                  dirt={"color": "#4a4234", "height": 0.8, "amount": 0.4})
    role_material(R("stone"), "#b8a892", lib=LIB + r"\Bricks096", scale=2.0, contrast=1.0,
                  rough=(0.85, 0.95), uv="box", dirt={"color": "#4a4234", "height": 0.4, "amount": 0.5})
    role_material(R("mortar"), "#6e6355", scale=0.5, contrast=0.6, rough=(0.90, 0.95), ao=0.0)
    # the warehouse's roof recipe, unchanged (../warehouse/materials.py, round 3): the negative
    # contrast turns the light rust streaks darker than the faded paint; grey unrusted iron in
    # patches, seams occluded
    role_material(R("roof"), "#c9684e", lib=LIB + r"\rusty_corrugated_iron", scale=8.0, contrast=-0.8,
                  rough=(0.70, 0.90), uv="box", ao=0.5,
                  bands=[{"color": "#78716a", "full": 3.6, "none": 3.5, "amount": 0.5, "noise": True}])
    _streaks_down_slope(R("roof"))
    role_material(R("brick"), "#a5613f", lib=LIB + r"\Bricks085", scale=2.0, contrast=1.0,
                  rough=(0.85, 0.95), uv="box")
    role_material(R("frame"), "#d0c2b0", lib=wood, scale=1.2, contrast=2.5, rough=(0.80, 0.92))
    role_material(R("glass"), "#5a5855", scale=1.0, contrast=0.3, rough=(0.10, 0.20), ao=0.0)
    role_material(R("plaque"), "#284676", scale=1.0, contrast=0.2, rough=(0.30, 0.40), ao=0.0)
    role_material(R("iron"), "#514439", scale=0.6, contrast=1.0, rough=(0.60, 0.80), metal=0.0)
    role_material(R("dark"), "#1e1b19", scale=1.0, contrast=0.5, rough=(0.9, 0.95), ao=0.0)
    return tone
