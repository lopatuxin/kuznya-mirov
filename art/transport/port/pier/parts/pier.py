# Port pier: four log cribs filled with stone, pile rows under cap logs in the spans, stringers
# and edge beams, a deck of heavy planks laid across the pier with staggered butt joints, a wheel
# curb, cast-iron bollards and log fenders hung on short chains. Built by blender-part-builder;
# the numbers it shares (heights, crib and pile spans, bollard stations, water line) come from
# dims.py. Pier along Y, land end at -Y, crib bottoms on z = 0 (sea bed).
#
# Triangles go above WATER_Z: logs wholly under water get 6 sides, stones only in the bands that
# show above the water, round pieces above it (crib top courses, pile caps, fender tops) get more.

import math, random
import bmesh
from mathutils import Vector, Matrix

SEED = 7

# --- cribs ---------------------------------------------------------------------------------
# Side walls (logs along Y) and end walls (logs along X), each wall solid from the sea bed with
# logs overlapping like notched logs, then one open band of stone right under the top:
#   side walls: logs to SIDE_TOP, stones between SIDE_TOP and CRIB_TOP (under the edge beams,
#               the end-wall log ends standing at the corners) - ref/right.png
#   end walls:  logs to END_BAND[0], stones in END_BAND, a top log to CRIB_TOP - ref/front.png,
#               ref/back.png; end walls facing a span are solid (a filler log in the band)
LOG_D = 0.55
LOG_JIT = 0.03                 # radius +-3 %: crossing logs still touch
SEG_WET, SEG_DRY = 16, 24      # sides of a log wholly under water / reaching above it
OUTRUN = 0.10                  # log ends past the outer face of the crossing wall
END_OFF = [0.0, 0.026, 0.009, 0.042, 0.017, 0.034, 0.05, 0.021]   # log end set-back course by course:
                                                                  # overlapping logs never end in one plane
SIDE_Z = [0.275, 0.755, 1.235, 1.715, 2.195, 2.675]           # side-wall log centres
END_Z = [0.275, 0.7425, 1.21, 1.6775, 2.145]                 # end-wall log centres below the band
END_TOP_Z = CRIB_TOP - LOG_D / 2                              # top course of the end walls
END_FILL_Z = (END_Z[-1] + END_TOP_Z) / 2                     # filler log of an inner end wall

# stones: rounded boulders 0.3-0.7 m, one big stone or two stacked small ones per column
STONE_BIG = (0.45, 0.70)
STONE_SMALL = (0.30, 0.45)
P_STACK = 0.4
STONE_N = 6                    # ring points of a boulder: 24 triangles

# --- piles: twin piles across the pier under a round cap log -------------------------------
PILE_CLUSTERS = [(-13.15, -12.19), (-12.19, -11.23), (-9.45, -8.35), (-0.40, 0.81), (1.05, 2.32), (11.58, 12.93)]
PILE_X = (-3.5, 0.0, 3.5)
PILE_SEG, CAP_SEG = 16, 24
CAP_D = 0.5

# --- deck frame ----------------------------------------------------------------------------
FRAME_Y = (-19.80, 19.40)      # land end overhangs crib 1 by 0.4, sea end flush with crib 4
PLANK_T = 0.15
FRAME_TOP = DECK_TOP - PLANK_T # 4.30: planks rest here
EDGE_X = (3.55, HALF_W)        # edge beams, two stacked timbers CRIB_TOP .. FRAME_TOP
STRINGERS_X = [0.0, 1.05, 2.15]            # main stringers CRIB_TOP .. FRAME_TOP, mirrored
STRINGER_W = 0.30
UNDER_X = [(0.0, 0.36), (3.375, 0.45)]     # under-beams in the spans DECK_UNDER .. CRIB_TOP (x, width);
                                           # the outer one 0.4 behind the edge face: the dark recess
END_BEAM_T = 0.40
BEVEL_W = 0.015

# --- deck ----------------------------------------------------------------------------------
DECK_Y = (-20.0, 19.7)         # planks overhang at both ends (ref/right.png)
PLANK_W, PLANK_GAP = 0.29, 0.015
JOINTS_X = (-1.05, 0.0, 1.05)  # butt joints over the stringers: planks 2.95-5.05 m

# --- curb, bollards, fenders --------------------------------------------------------------
CURB_X, CURB_W = 3.76, 0.26
SEA_CURB_HALF = 3.0
FENDER_D = 0.67
FENDER_Z = (1.35, 4.2)
FENDER_SEG = 20
FENDERS_Y = [-18.45, -13.1, -12.37, -4.79, 1.08, 7.35, 13.8, 18.37]   # both sides, ref/right.png
SEA_FENDERS_X = [-2.97, 0.0, 2.95]                                    # ref/back.png
BOLLARD_SEG = 32
BOLLARD_PROFILE = [(0.46, DECK_TOP - 0.01), (0.46, 4.82), (0.29, 4.99), (0.26, 5.30),
                   (0.38, 5.43), (0.38, 5.58), (0.0, BOLLARD_TOP)]
SHADE_ANGLE = {"PP_crib_logs": 65, "PP_crib_stone": 89, "PP_piles": 65, "PP_bollards": 50,
               "PP_fenders": 50}


def _bw(bm):
    return bm.edges.layers.float.get("bevel_weight_edge") or bm.edges.layers.float.new("bevel_weight_edge")


def _timber(bm, center, size, axis, face):
    """Sawn timber box; the two long edges (along `axis`, 0..2) of its face with normal
    `face` (the face that shows) carry the bevel weight."""
    fs = box(bm, center, size)
    lay = _bw(bm)
    for f in fs:
        if f.normal.dot(Vector(face)) > 0.99:
            for e in f.edges:
                d = e.verts[1].co - e.verts[0].co
                if abs(d[axis]) >= d.length - 1e-6:
                    e[lay] = 1.0
    return fs


def _log(bm, p0, p1, r, rnd):
    seg = SEG_WET if max(p0[2], p1[2]) + r < WATER_Z - 0.05 else SEG_DRY
    cylinder(bm, p0, p1, r, segments=seg, phase=rnd.uniform(0, math.pi))


def _stone(bm, c, u, v, w, L, H, depth, back, rnd):
    """Rounded boulder: an elliptic ring (L x H) in the wall plane, a smaller ring and a pole
    toward the face, a pole behind; c is on the wall plane, w points out of the wall."""
    ph = rnd.uniform(0, math.pi)
    jit = [rnd.uniform(0.86, 1.1) for _ in range(STONE_N)]
    r0 = [c + u * (L / 2 * jit[i] * math.cos(ph + i * 2 * math.pi / STONE_N)) + v * (H / 2 * jit[i] * math.sin(ph + i * 2 * math.pi / STONE_N))
          for i in range(STONE_N)]
    k = rnd.uniform(0.66, 0.78)
    d1 = depth * rnd.uniform(0.55, 0.68)
    r1 = [c + (p - c) * k + w * d1 for p in r0]
    tip = c + w * depth + u * (L * rnd.uniform(-0.08, 0.08)) + v * (H * rnd.uniform(-0.08, 0.08))
    a = [bm.verts.new(p) for p in r0]
    b = [bm.verts.new(p) for p in r1]
    t = bm.verts.new(tip)
    bk = bm.verts.new(c - w * back)
    fs = []
    for i in range(STONE_N):
        j = (i + 1) % STONE_N
        fs.append(bm.faces.new((a[i], a[j], b[j], b[i])))
        fs.append(bm.faces.new((b[i], b[j], t)))
        fs.append(bm.faces.new((a[j], a[i], bk)))
    bmesh.ops.recalc_face_normals(bm, faces=fs)


def _stone_band(bm, rnd, origin, u, w, a0, a1, z0, z1, depth):
    """Stones filling the band z0..z1 of a wall: origin is the wall plane point at a = 0,
    u the direction along the wall, w the outward normal."""
    v = Vector((0, 0, 1))
    hb = z1 - z0
    a = a0
    while a < a1 - 0.12:
        if rnd.random() < P_STACK:
            cols = [(rnd.uniform(*STONE_SMALL), rnd.uniform(0.52, 0.62) * hb, z0 + 0.27 * hb),
                    (rnd.uniform(*STONE_SMALL), rnd.uniform(0.50, 0.60) * hb, z0 + 0.74 * hb)]
        else:
            cols = [(rnd.uniform(*STONE_BIG), rnd.uniform(0.95, 1.15) * hb, z0 + hb * rnd.uniform(0.46, 0.54))]
        width = max(c[0] for c in cols)
        width = min(width, max(a1 - a, 0.3))
        for L, H, zc in cols:
            L = min(L, width)
            ac = a + width / 2 + rnd.uniform(-0.05, 0.05) * (width - L + 0.1)
            c = origin + u * ac + Vector((0, 0, zc))
            _stone(bm, c, u, v, w, L * rnd.uniform(1.02, 1.1), H, depth * rnd.uniform(0.9, 1.05), 0.3, rnd)
        a += width * rnd.uniform(0.9, 0.97)


def _crib(bm_log, bm_stone, y0, y1, rnd, open_ends):
    r0 = LOG_D / 2
    xc = HALF_W - OUTRUN - r0              # side-wall log axis
    yc0, yc1 = y0 + OUTRUN + r0, y1 - OUTRUN - r0   # end-wall log axes
    jr = lambda: r0 * rnd.uniform(1 - LOG_JIT, 1 + LOG_JIT)
    # side walls, along Y
    sh = rnd.randrange(len(END_OFF))
    off = lambda k: END_OFF[(k + sh) % len(END_OFF)] + rnd.uniform(0, 0.003)
    for s in (-1, 1):
        for k, z in enumerate(SIDE_Z):
            r = jr()
            zc = r if z == SIDE_Z[0] else z
            _log(bm_log, (s * xc, y0 + off(k + s), zc), (s * xc, y1 - off(k + 2 * s), zc), r, rnd)
    side_top = SIDE_Z[-1] + r0
    # end walls, along X
    for s, yc in ((-1, yc0), (1, yc1)):
        zs = END_Z + ([END_TOP_Z] if s in open_ends else [END_FILL_Z, END_TOP_Z])
        for k, z in enumerate(zs):
            r = jr()
            zc = r if z == END_Z[0] else (CRIB_TOP - r if z == END_TOP_Z else z)
            _log(bm_log, (-HALF_W + off(k + 3 + s), yc, zc), (HALF_W - off(k + 5 - s), yc, zc), r, rnd)
    # stone fill: a core inside the walls, boulders in the open bands
    box(bm_stone, (0, (yc0 + yc1) / 2, (CRIB_TOP - 0.1) / 2), (2 * xc, yc1 - yc0, CRIB_TOP - 0.1))
    depth = r0 - 0.03
    for s in (-1, 1):
        _stone_band(bm_stone, rnd, Vector((s * xc, 0, 0)), Vector((0, 1, 0)), Vector((s, 0, 0)),
                    yc0 + r0 - 0.05, yc1 - r0 + 0.05, side_top - 0.03, CRIB_TOP + 0.02, depth)
    for s in open_ends:
        yc = yc0 if s < 0 else yc1
        _stone_band(bm_stone, rnd, Vector((0, yc, 0)), Vector((1, 0, 0)), Vector((0, s, 0)),
                    -xc + r0 - 0.05, xc - r0 + 0.05, END_Z[-1] + r0 - 0.03, END_TOP_Z - r0 + 0.03, depth)


def _piles(bm, rnd):
    for c0, c1 in PILE_CLUSTERS:
        d = (c1 - c0) / 2
        top = DECK_UNDER - CAP_D + 0.05
        for x in PILE_X:
            for k in range(2):
                y = c0 + d / 2 + k * d
                r = d / 2 * rnd.uniform(0.96, 1.0)
                cylinder(bm, (x, y, 0.0), (x, y, top + rnd.uniform(0, 0.03)), r, segments=PILE_SEG,
                         phase=rnd.uniform(0, 0.5))
        yc = (c0 + c1) / 2
        rc = CAP_D / 2
        cylinder(bm, (-HALF_W + 0.08, yc, DECK_UNDER + 0.01 - rc), (HALF_W - 0.06, yc, DECK_UNDER + 0.01 - rc),
                 rc, segments=CAP_SEG)


def _segments(y0, y1, joints, gap=0.01):
    cuts = [y0] + [j for j in joints if y0 + 2 < j < y1 - 2] + [y1]
    return [(a + (gap / 2 if i else 0), b - (gap / 2 if i < len(cuts) - 2 else 0))
            for i, (a, b) in enumerate(zip(cuts, cuts[1:]))]


def _frame(bm):
    crib_mid = [(a + b) / 2 for a, b in CRIBS]
    pile_mid = [(a + b) / 2 for a, b in PILE_CLUSTERS]
    ja = [crib_mid[1], crib_mid[2]]                 # joints over cribs 2 and 3
    jb = [pile_mid[0], pile_mid[3], pile_mid[5]]     # joints over pile rows
    z0, z1 = CRIB_TOP, FRAME_TOP
    zm = (z0 + z1) / 2
    ye0, ye1 = FRAME_Y[0] + END_BEAM_T - 0.01, FRAME_Y[1] - END_BEAM_T + 0.01
    # main stringers on the crib tops, butting the end beams
    for i, xc in enumerate(STRINGERS_X):
        for s in ((1,) if xc == 0 else (-1, 1)):
            for a, b in _segments(ye0, ye1, ja if i % 2 == 0 else jb):
                box(bm, (s * xc, (a + b) / 2, zm), (STRINGER_W, b - a, z1 - z0))
    # edge beams: two stacked timbers at each side, joints staggered
    xe = (EDGE_X[0] + EDGE_X[1]) / 2
    for s in (-1, 1):
        for lo, hi, js in ((z0, zm, ja), (zm, z1, jb)):
            for a, b in _segments(FRAME_Y[0], FRAME_Y[1], js):
                _timber(bm, (s * xe, (a + b) / 2, (lo + hi) / 2), (EDGE_X[1] - EDGE_X[0], b - a, hi - lo), 1, (s, 0, 0))
    # end beams across both ends, between the edge beams
    for yb in (FRAME_Y[0] + END_BEAM_T / 2, FRAME_Y[1] - END_BEAM_T / 2):
        for lo, hi in ((z0, zm), (zm, z1)):
            _timber(bm, (0, yb, (lo + hi) / 2), (2 * EDGE_X[0] + 0.02, END_BEAM_T, hi - lo), 0,
                    (0, 1 if yb > 0 else -1, 0))
    # under-beams over the spans, on the pile caps, their ends let into the crib tops
    for (a0, a1), (b0, b1) in zip(CRIBS, CRIBS[1:]):
        for xc, w in UNDER_X:
            for s in ((1,) if xc == 0 else (-1, 1)):
                ya, yb = a1 - 0.45, b0 + 0.45
                box(bm, (s * xc, (ya + yb) / 2, (DECK_UNDER + CRIB_TOP + 0.01) / 2),
                    (w, yb - ya, CRIB_TOP + 0.01 - DECK_UNDER))


def _deck(bm, rnd):
    n = int((DECK_Y[1] - DECK_Y[0] + PLANK_GAP) / (PLANK_W + PLANK_GAP))
    w = (DECK_Y[1] - DECK_Y[0] - PLANK_GAP * (n - 1)) / n
    prev = None
    for i in range(n):
        y0 = DECK_Y[0] + i * (w + PLANK_GAP)
        xj = rnd.choice([j for j in JOINTS_X if j != prev])
        prev = xj
        x0, x1 = -HALF_W + rnd.uniform(0, 0.05), HALF_W - rnd.uniform(0, 0.05)
        for a, b in ((x0, xj - 0.006), (xj + 0.006, x1)):
            ww = w * rnd.uniform(0.93, 1.0)
            yc = y0 + ww / 2 + (w - ww) * rnd.random()
            t = PLANK_T - rnd.uniform(0, 0.015)
            box(bm, ((a + b) / 2, yc, FRAME_TOP + t / 2), (b - a, ww, t))


def _curb(bm):
    ys = sorted(BOLLARDS_Y)
    spans = [(DECK_Y[0] + 0.05, ys[0] - 0.5)] + [(a + 0.5, b - 0.5) for a, b in zip(ys, ys[1:])] + \
            [(ys[-1] + 0.5, DECK_Y[1] - 0.05)]
    zr0 = DECK_TOP - 0.02          # let into the planks
    for s in (-1, 1):
        for a, b in spans:
            if b - a < 0.6:
                continue
            _timber(bm, (s * CURB_X, (a + b) / 2, (zr0 + CURB_TOP) / 2), (CURB_W, b - a, CURB_TOP - zr0), 1, (0, 0, 1))
    # wheel guard across the sea end, clear of the end bollards (ref/back.png)
    ye = DECK_Y[1] - 0.05 - CURB_W / 2
    _timber(bm, (0, ye, (zr0 + CURB_TOP) / 2), (2 * SEA_CURB_HALF, CURB_W, CURB_TOP - zr0), 0, (0, 0, 1))


def _bollards(bm):
    for s in (-1, 1):
        for y in BOLLARDS_Y:
            b = revolve(BOLLARD_PROFILE, segments=BOLLARD_SEG)
            # turned off the axes: no facet parallel to the plank sides
            bmesh.ops.rotate(b, verts=b.verts, cent=(0, 0, 0), matrix=Matrix.Rotation(0.15, 3, 'Z'))
            bmesh.ops.translate(b, verts=b.verts, vec=(s * BOLLARD_X, y, 0))
            me = bpy.data.meshes.new("PP_tmp_bollard")
            b.to_mesh(me)
            b.free()
            bm.from_mesh(me)
            bpy.data.meshes.remove(me)


def _fender(bm, bm_chain, c, top_dir, rnd):
    """Vertical log at c (x, y) from FENDER_Z[0] to FENDER_Z[1] with a chamfered top; top_dir:
    unit vector from the fender axis toward the pier (where the chain goes)."""
    r = FENDER_D / 2 * rnd.uniform(0.97, 1.0)
    z0, z1 = FENDER_Z[0] + rnd.uniform(-0.05, 0.05), FENDER_Z[1]
    x, y = c
    f = revolve([(r, z0), (r, z1 - 0.08), (r * 0.76, z1)], segments=FENDER_SEG)
    bmesh.ops.rotate(f, verts=f.verts, cent=(0, 0, 0), matrix=Matrix.Rotation(rnd.uniform(0, 1), 3, 'Z'))
    bmesh.ops.translate(f, verts=f.verts, vec=(x, y, 0))
    me = bpy.data.meshes.new("PP_tmp_fender")
    f.to_mesh(me)
    f.free()
    bm.from_mesh(me)
    bpy.data.meshes.remove(me)
    # chain: two links from the fender top to the deck edge
    d = Vector((top_dir[0], top_dir[1], 0))
    p0 = Vector((x, y, z1)) + d * (r * 0.3) + Vector((0, 0, -0.03))
    p1 = Vector((x, y, DECK_TOP)) + d * (r + 0.08) + Vector((0, 0, -0.02))
    n = 2
    for k in range(n):
        a = p0 + (p1 - p0) * (k / n)
        b = p0 + (p1 - p0) * ((k + 1) / n)
        ext = (b - a).normalized() * 0.03
        beam(bm_chain, a - ext, b + ext, 0.07 if k % 2 == 0 else 0.025, 0.025 if k % 2 == 0 else 0.07)


def _fenders(bm, bm_chain, rnd):
    r = FENDER_D / 2
    for s in (-1, 1):
        for y in FENDERS_Y:
            _fender(bm, bm_chain, (s * (HALF_W + r - 0.002), y), (-s, 0), rnd)
    for x in SEA_FENDERS_X:
        _fender(bm, bm_chain, (x, FRAME_Y[1] + r - 0.002), (0, -1), rnd)


def _tri_ngons(bm):
    ng = [f for f in bm.faces if len(f.verts) > 4]
    if ng:
        bmesh.ops.triangulate(bm, faces=ng)


def _obj(name, bm, col, role):
    _tri_ngons(bm)
    return obj_from_bm(name, bm, col, [M(role)])


def build(sc, col):
    rnd = random.Random(SEED)
    b_log, b_stone = bmesh.new(), bmesh.new()
    n = len(CRIBS)
    for i, (y0, y1) in enumerate(CRIBS):
        open_ends = ([-1] if i == 0 else []) + ([1] if i == n - 1 else [])
        _crib(b_log, b_stone, y0, y1, rnd, open_ends)
    _obj("PP_crib_logs", b_log, col, "log")
    _obj("PP_crib_stone", b_stone, col, "stone")

    b = bmesh.new()
    _piles(b, rnd)
    _obj("PP_piles", b, col, "timber")

    b = bmesh.new()
    _frame(b)
    frame = _obj("PP_frame", b, col, "timber")

    b = bmesh.new()
    _deck(b, rnd)
    _obj("PP_deck", b, col, "plank")

    b = bmesh.new()
    _curb(b)
    curb = _obj("PP_curb", b, col, "timber")

    b = bmesh.new()
    _bollards(b)
    _obj("PP_bollards", b, col, "iron")

    b, bc = bmesh.new(), bmesh.new()
    _fenders(b, bc, rnd)
    _obj("PP_fenders", b, col, "timber")
    _obj("PP_chains", bc, col, "chain")
    # light bevel on the long edges of the sawn timber that shows (edge and end beams, curb),
    # by bevel weight so the hidden stringers and the short ends stay cheap
    for o in (frame, curb):
        add_bevel(o, BEVEL_W, 2, 'WEIGHT')
    for o in col.objects:
        finish_shading(o, SHADE_ANGLE.get(o.name, 30))
