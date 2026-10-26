# Materials of the port crane: role recipes (blender-material-artist). Colours measured on
# ref/front.png and ref/back.png; roles are the material names the part uses
# (Port_Crane_timber / iron / rust / rope / handle). Timber kept close to the warm silver-grey
# wood of the pier and the warehouse on ../concept.png. Iron is non-metal dark brown-grey for
# all four port buildings (director's decision, round 2).
LIB = r"C:\projects\kuznya-mirov\art\transport\textures"
BAKE = {"size": 1024, "samples": 16}

TIMBER = "Port_Crane_timber"
DECK = ["PC_deck"]
LONG = ["PC_mast", "PC_boom"]  # two big pieces: near the role colour, as on the reference


def _tone_rgba(me):
    attr = me.color_attributes["tone"]
    col = np.empty(len(me.loops) * 4, np.float32)
    attr.data.foreach_get("color", col)
    return attr, col.reshape(-1, 4)


def end_grain(names, material, dark=0.72, min_ratio=2.0, cos_min=0.8):
    """Darken the end faces of long pieces in the "tone" layer: faces of `material` whose normal
    runs along the piece's long axis (|cos| >= cos_min) on islands at least `min_ratio` times
    longer than wide get their tone multiplied by `dark`. Returns the faces darkened per object."""
    out = {}
    for o in objs_of(names):
        me = o.data
        slots = {i for i, m in enumerate(me.materials) if m is not None and m.name == material}
        if not slots or "tone" not in me.color_attributes:
            continue
        bm = bmesh.new()
        bm.from_mesh(me)
        lab, _ = _islands(bm)
        bm.free()
        m3 = np.array(o.matrix_world.to_3x3())
        co = np.empty(len(me.vertices) * 3)
        me.vertices.foreach_get("co", co)
        co = co.reshape(-1, 3) @ m3.T
        groups = {}
        for p in me.polygons:
            groups.setdefault(lab[p.index], []).append(p)
        attr, col = _tone_rgba(me)
        hit = 0
        for polys in groups.values():
            Q = co[sorted({v for p in polys for v in p.vertices})]
            Q = Q - Q.mean(0)
            _, _, vt = np.linalg.svd(Q, full_matrices=False)
            ext = np.ptp(Q @ vt.T, axis=0)
            if len(ext) < 2 or ext[0] < min_ratio * max(ext[1], 1e-6):
                continue
            for p in polys:
                if p.material_index not in slots:
                    continue
                nrm = np.array(tuple(p.normal)) @ m3.T
                nrm /= max(np.linalg.norm(nrm), 1e-9)
                if abs(nrm @ vt[0]) >= cos_min:
                    col[p.loop_start:p.loop_start + p.loop_total, :3] *= dark
                    hit += 1
        attr.data.foreach_set("color", col.ravel())
        me.update()
        out[o.name] = hit
    return out


def tint(names, rgb):
    """Multiply the "tone" layer of `names` by a colour (each channel <= 1): deck boards drier."""
    for o in objs_of(names):
        attr, col = _tone_rgba(o.data)
        col[:, :3] *= np.array(rgb, np.float32)
        attr.data.foreach_set("color", col.ravel())
        o.data.update()


def apply(sc, names):
    piece_uv(names)
    tone = piece_tone(names, spread=7, seed=11, ground=0.0, ground_h=1.0, ground_dark=0.2)
    tone["long"] = piece_tone(LONG, spread=2, seed=3, ground=0.0, ground_h=1.0, ground_dark=0.2)
    tone["deck"] = piece_tone(DECK, spread=4, seed=5, ground=0.0, ground_h=1.0, ground_dark=0.2)
    tint(DECK, (0.97, 0.975, 1.0))
    tone["end_grain"] = end_grain(names, TIMBER)
    role_material(TIMBER, "#937a63", lib=LIB + r"\Wood033", scale=3.0, contrast=5.5,
                  rough=(0.80, 0.95), dirt={"color": "#4a4234", "height": 0.8, "amount": 0.5})
    role_material("Port_Crane_iron", "#5e5044", scale=0.6, rough=(0.60, 0.75), metal=0.0)
    role_material("Port_Crane_rust", "#8c654c", lib=LIB + r"\MetalPlates011", scale=0.4, contrast=1.2,
                  rough=(0.80, 0.92))
    role_material("Port_Crane_rope", "#574330", scale=0.3, rough=(0.85, 0.95), ao=0.6, ao_dist=0.06)
    role_material("Port_Crane_handle", "#b08a5e", lib=LIB + r"\Wood086", scale=0.6, contrast=1.0,
                  rough=(0.70, 0.85))
    return tone
