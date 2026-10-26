# Materials of the port warehouse: role recipes (blender-material-artist). Colours measured on
# ref/front.png, ref/left.png, ref/right.png and ../concept.png; roles are the material names the
# part uses (Port_Warehouse_<role>). Wood warm silver-grey and roof iron rusty red-brown, the
# same as the office on ../concept.png, so the four port buildings read as one port.
# PW_mortar (the rubble plinth's core) carries Port_Warehouse_dark in the part; here, in the
# materials pass only, its slots get their own material Port_Warehouse_mortar - the part file is
# not touched. PW_wall_boards gets Port_Warehouse_skin the same way.
LIB = r"C:\projects\kuznya-mirov\art\transport\textures"
BAKE = {"size": 2048, "samples": 16}

WALLS = ["PW_wall_boards", "PW_battens", "PW_doors", "PW_frame"]
DECK = ["PW_platform", "PW_posts", "PW_stairs"]
ROOF = ["PW_roof", "PW_vent"]
STONE = ["PW_plinth", "PW_mortar"]


def R(role):
    return f"{SCENE}_{role}"


def _mortar_slots():
    me = bpy.data.objects["PW_mortar"].data
    mortar = bpy.data.materials.get(R("mortar")) or bpy.data.materials.new(R("mortar"))
    for i in range(len(me.materials)):
        me.materials[i] = mortar


def _streaks_down_slope(name):
    """The roof texture turned a quarter round, so its vertical rust streaks run down the slope
    (box projection lays them along the ridge otherwise)."""
    for n in bpy.data.materials[name].node_tree.nodes:
        if n.type == "MAPPING":
            n.inputs["Rotation"].default_value = (0.0, 0.0, math.pi / 2)


def _skin_slots():
    """PW_wall_boards, the backing skin between the battens, is one piece per wall: its own
    material with a flat grain, or Wood033 lays wide diagonal stripes across the whole wall."""
    me = bpy.data.objects["PW_wall_boards"].data
    skin = bpy.data.materials.get(R("skin")) or bpy.data.materials.new(R("skin"))
    for i in range(len(me.materials)):
        me.materials[i] = skin


def apply(sc, names):
    _mortar_slots()
    _skin_slots()
    piece_uv(names)
    piece_tone(WALLS, spread=5, seed=11, ground_dark=0.22, ground_h=1.6,
               shade_from=["PW_roof"], shade_dist=1.6, shade_dark=0.15)
    piece_tone(["PW_dark"], spread=0, ground_dark=0.0)
    piece_tone(DECK, spread=5, seed=23, ground_dark=0.25, ground_h=1.2,
               shade_from=["PW_roof"], shade_dist=2.6, shade_dark=0.15)
    piece_tone(["PW_rafters"], spread=4, seed=31, shade_from=["PW_roof"], shade_dist=0.6, shade_dark=0.2)
    piece_tone(STONE, spread=7, seed=41, ground_dark=0.25, ground_h=0.6,
               shade_from=["PW_platform"], shade_dist=1.5, shade_dark=0.6)
    tone = piece_tone(ROOF, spread=6, seed=53, ground_dark=0.0)
    role_material(R("board"), "#b4a090", lib=LIB + r"\Wood033", scale=3.0, contrast=3.5,
                  rough=(0.80, 0.93), dirt={"color": "#4a4234", "height": 1.0, "amount": 0.5})
    role_material(R("skin"), "#b4a090", lib=LIB + r"\Wood033", scale=3.0, contrast=1.0,
                  rough=(0.80, 0.93), dirt={"color": "#4a4234", "height": 1.0, "amount": 0.5})
    role_material(R("timber"), "#a08a7a", lib=LIB + r"\Wood033", scale=3.0, contrast=3.5,
                  rough=(0.80, 0.93), dirt={"color": "#4a4234", "height": 0.8, "amount": 0.5})
    role_material(R("stone"), "#bdad9d", lib=LIB + r"\Bricks096", scale=2.0,
                  contrast=1.0, rough=(0.85, 0.95), dirt={"color": "#4a4234", "height": 0.4, "amount": 0.5})
    role_material(R("mortar"), "#95897f", scale=1.0, contrast=0.6, rough=(0.88, 0.95),
                  dirt={"color": "#4a4234", "height": 0.4, "amount": 0.4})
    # roof iron: matte faded red-brown paint with rust streaks down the slope (the texture's
    # vertical streaks, turned by _streaks_down_slope - box projection lays them along the
    # ridge); a negative contrast inverts the pattern, so the streaks are darker than the paint,
    # not lighter; grey unrusted iron in noise patches, occlusion darkens each standing seam
    role_material(R("roof"), "#c9684e", lib=LIB + r"\rusty_corrugated_iron", scale=8.0, contrast=-0.8,
                  rough=(0.70, 0.90), uv="box", ao=0.5,
                  bands=[{"color": "#78716a", "full": 3.6, "none": 3.5, "amount": 0.5, "noise": True}])
    _streaks_down_slope(R("roof"))
    role_material(R("iron"), "#514439", scale=0.6, contrast=1.0, rough=(0.60, 0.70), metal=0.0)
    role_material(R("dark"), "#4f453e", scale=1.0, contrast=0.5, rough=(0.9, 0.95), ao=0.0)
    return tone
