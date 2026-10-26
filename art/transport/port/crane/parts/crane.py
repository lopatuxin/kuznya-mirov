# Wooden hand derrick crane of the port: the whole crane is one part. Built by
# blender-part-builder; numbers it shares come from dims.py (BASE, BASE_TOP, MAST, BOOM_FOOT,
# BOOM_TIP, BOOM_W, HOOK, WINCH, CRANK_X). Everything else is measured on ref/front.png
# (56 px/m, x = 2.589 + (u - 345) / 56, z = 4.241 + (277.5 - v) / 56) and checked on ref/back.png.
#
# Construction:
#   base   - crib frame of squared timbers: bottom sills, corner stacks of beam ends, centre
#            stacks, lapped X braces in every bay, top rails, joists, a plank deck over it all
#            (boards 1 cm apart, split round the mast), iron straps bent over the rails with
#            bolts, braces' crossing plates
#   mast   - square timber standing on its joist through the deck; the head is split by a slot
#            along X with a sheave on a through pin; iron foot band and two bolted collars,
#            eyes for the guys under the lower collar, topping lugs, gooseneck lugs at the foot,
#            the lead block (wood-shelled) on the +X face
#   boom   - one straight tapered timber (loft) on the measured axis from the hinge boss at
#            BOOM_FOOT to the tip; heel in an iron shoe with the boss on the hinge pin, two
#            bands, the upper one with eyes for the topping ropes and the fall block, two small
#            side plates carrying the tip sheave at BOOM_TIP
#   winch  - sleepers, two iron standards, drum with flanges 0.78 m apart and the rope wound on
#            it as a helix, spur gear, two crank shafts with pinions, cranks with wooden grips
#   rigging- 2 topping ropes, hoist rope off the drum coil -> lead block -> boom tip sheave,
#            falls through two running blocks to the hook, 4 guys with turnbuckles to the deck
#   ladder - short ladder on the +Y side up onto the deck

# ---- measured on the reference (metres) --------------------------------------------------
SILL_H = 0.34                    # bottom sills
MID_TOP = 0.77                   # top of the corner stacks / braces, bottom of the top rails
DECK_T = 0.05                    # deck boards: top at BASE_TOP, on rails and joists
DECK_N = 21                      # boards across the 5.5 m deck, 1 cm gaps
RAIL_TOP = BASE_TOP - DECK_T
BASE_OUT = 2.81                  # sills and stacks stand 0.1 proud of the 5.5 m top rails
RAIL_W = 0.25
SILL_W = 0.30
STACK = 0.29                     # beam end blocks in the corner stacks
CENTRE_X = 0.42                  # centre stack under the mast (front view 0.12..0.77)
CENTRE_W = 0.70

COLLARS = ((6.31, 6.52, 0.89), (6.77, 6.96, 0.83), (1.01, 1.24, 0.84))   # z0, z1, side
SLOT = (6.84, 0.06)              # mast-head slot: bottom z, half width in Y
HEAD_SHEAVE = (7.02, 0.15)       # z of the pin, radius (top 7.17, below MAST top 7.22)
MAST_X0_TOP = 0.05               # the -X face leans in. front.png: x 0.0 at z 3.3, 0.107 at 7.1;
                                 # back.png: plumb. Half the front lean (0.05) scores best on both;
                                 # the +X face (boom side) is plumb at 0.75..0.77 (front.png rows)

# boom: axis from the hinge boss BOOM_FOOT through the timber centre measured at u = 610
BOOM_LO = (1.012, 0.341)         # boom lower edge on front.png: z = k * x + b (u 310..610)
BOOM_UP = (0.933, 1.422)         # boom upper edge (u 385..595); the timber tapers between them
BOOM_D = (0.40, 0.34)            # depth along Y heel -> end, BOOM_W on average
HEEL_S = 0.36                    # timber heel face: clears the mast face
TIP_GAP = 0.21                   # timber end stands this far behind the tip sheave centre
TIP_R = 0.20                     # tip sheave radius (top at 8.48, brief: 8.5)

TOP_LUG = ((0.88, 0.13, 7.01), (0.88, -0.12, 6.86))   # topping rope ends at the mast
TOP_END = ((7.40, 8.30), (7.25, 8.11))                 # and at the boom head (+Y, -Y side plate)
GUY_TOP_Z = 6.30
GUY_ANCHOR_X = (-2.38, 2.48)     # guy lines on front.png extended to the deck (z 1.0)
RAIL_Y = BASE / 2 - RAIL_W / 2

LEAD = (0.99, -0.33, 2.30, 3.08, 0.21)   # lead block: x, y, z bottom, z of the round top, radius

A_BLOCK = (6.82, 0.18)           # running blocks, z of centre and radius (front.png)
B_BLOCK = (4.23, 0.21)
FALL_DX = 0.075                  # the two parts of the fall either side of the hook line
FALL_X = 7.50                    # line of the falls and blocks (front.png 7.43 / 7.58 ropes)

DRUM_Z = 1.90
DRUM_X = (-1.57, -0.79)          # inner faces of the flanges: 0.78 m (front.png)
COIL_R, COIL_ROPE = 0.46, 0.039  # helix of rope turns: radius to the rope axis, rope radius
GEAR_R = 0.70
CRANK_Y = 0.84
CRANK_Z = 1.85
GRIP_Z = 2.25
STD_X = (-1.72, -0.25)           # winch standards

R_ROPE = 0.014


# ---- small geometry helpers ----------------------------------------------------------------

def _new(bm):
    bm.verts.ensure_lookup_table()
    return len(bm.verts)


def _tail(bm, n0):
    bm.verts.ensure_lookup_table()
    return [bm.verts[i] for i in range(n0, len(bm.verts))]


def _faces_of(verts):
    fs = set()
    for v in verts:
        fs.update(v.link_faces)
    return list(fs)


def _basis(axis):
    a = Vector(axis).normalized()
    e1 = a.cross(Vector((0, 0, 1)))
    if e1.length < 1e-6:
        e1 = a.cross(Vector((1, 0, 0)))
    e1.normalize()
    return a, e1, a.cross(e1).normalized()


def lathe(bm, profile, origin, axis, segs=32, mat=0):
    """Revolve [(r, h)] around `axis` through `origin`; r = 0 at an end closes to a pole."""
    a, e1, e2 = _basis(axis)
    o = Vector(origin)
    n0 = _new(bm)
    rings = []
    for r, h in profile:
        c = o + a * h
        if r <= 1e-9:
            rings.append([bm.verts.new(c)])
        else:
            rings.append([bm.verts.new(c + (e1 * math.cos(2 * math.pi * i / segs) +
                                            e2 * math.sin(2 * math.pi * i / segs)) * r) for i in range(segs)])
    for ra, rb in zip(rings, rings[1:]):
        for i in range(segs):
            j = (i + 1) % segs
            if len(ra) == 1:
                f = bm.faces.new((ra[0], rb[j], rb[i]))
            elif len(rb) == 1:
                f = bm.faces.new((ra[i], ra[j], rb[0]))
            else:
                f = bm.faces.new((ra[i], ra[j], rb[j], rb[i]))
            f.material_index = mat
    for ring, first in ((rings[0], True), (rings[-1], False)):
        if len(ring) > 1:
            f = bm.faces.new(list(reversed(ring)) if first else ring)
            f.material_index = mat
    fs = _faces_of(_tail(bm, n0))
    bmesh.ops.recalc_face_normals(bm, faces=fs)
    return fs


def sheave(bm, c, axis, r, half, segs=32, mat=0):
    """Grooved sheave or block shell: flat sides, a groove round the rim."""
    prof = [(r * 0.25, -half), (r, -half), (r * 0.82, 0.0), (r, half), (r * 0.25, half)]
    return lathe(bm, prof, c, axis, segs, mat)


def tube(bm, pts, r, segs=12, mat=0):
    """Round rod along a polyline (ropes, hook, shackles); r is a number or one per point."""
    P = [Vector(p) for p in pts]
    rs = list(r) if isinstance(r, (list, tuple)) else [r] * len(P)
    n = len(P)
    T = []
    for i in range(n):
        t = P[min(i + 1, n - 1)] - P[max(i - 1, 0)]
        T.append(t.normalized())
    _, nrm, _ = _basis(T[0])
    n0 = _new(bm)
    rings = []
    for i in range(n):
        nrm = (nrm - T[i] * nrm.dot(T[i])).normalized()
        b = T[i].cross(nrm)
        rings.append([bm.verts.new(P[i] + (nrm * math.cos(2 * math.pi * k / segs) +
                                           b * math.sin(2 * math.pi * k / segs)) * rs[i]) for k in range(segs)])
    for ra, rb in zip(rings, rings[1:]):
        for k in range(segs):
            j = (k + 1) % segs
            bm.faces.new((ra[k], ra[j], rb[j], rb[k])).material_index = mat
    bm.faces.new(list(reversed(rings[0]))).material_index = mat
    bm.faces.new(rings[-1]).material_index = mat
    fs = _faces_of(_tail(bm, n0))
    bmesh.ops.recalc_face_normals(bm, faces=fs)
    return fs


def torus(bm, c, axis, R, r, nmaj=24, nmin=10, mat=0):
    a, e1, e2 = _basis(axis)
    c = Vector(c)
    n0 = _new(bm)
    rings = []
    for i in range(nmaj):
        t = 2 * math.pi * i / nmaj
        rad = e1 * math.cos(t) + e2 * math.sin(t)
        rings.append([bm.verts.new(c + rad * R + (rad * math.cos(2 * math.pi * k / nmin) +
                                                  a * math.sin(2 * math.pi * k / nmin)) * r) for k in range(nmin)])
    for i in range(nmaj):
        ra, rb = rings[i], rings[(i + 1) % nmaj]
        for k in range(nmin):
            j = (k + 1) % nmin
            bm.faces.new((ra[k], ra[j], rb[j], rb[k])).material_index = mat
    fs = _faces_of(_tail(bm, n0))
    bmesh.ops.recalc_face_normals(bm, faces=fs)
    return fs


def rod(bm, p0, p1, r, segs=12, mat=0):
    return tube(bm, [p0, p1], r, segs, mat)


def hull2(pts):
    pts = sorted(set((round(p[0], 5), round(p[1], 5)) for p in pts))

    def cr(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    lo, hi = [], []
    for p in pts:
        while len(lo) >= 2 and cr(lo[-2], lo[-1], p) <= 0:
            lo.pop()
        lo.append(p)
    for p in reversed(pts):
        while len(hi) >= 2 and cr(hi[-2], hi[-1], p) <= 0:
            hi.pop()
        hi.append(p)
    return lo[:-1] + hi[:-1]


def circle2(cx, cy, r, n=24):
    return [(cx + r * math.cos(2 * math.pi * i / n), cy + r * math.sin(2 * math.pi * i / n)) for i in range(n)]


def plate(bm, outline, thick, plane, offset, mat=0):
    n0 = _new(bm)
    prism(outline, thick, plane=plane, offset=offset, bm=bm, mat_index=mat)
    return _faces_of(_tail(bm, n0))


def triangulate_ngons(bm):
    ng = [f for f in bm.faces if len(f.verts) > 4]
    if ng:
        bmesh.ops.triangulate(bm, faces=ng, quad_method='BEAUTY', ngon_method='EAR_CLIP')


def finish(name, bm, col, mats, bevel=None, angle=30, segs=2):
    triangulate_ngons(bm)
    o = obj_from_bm(name, bm, col, [M(m) for m in mats])
    if bevel:
        add_bevel(o, bevel, segs, 'ANGLE', 30)
    finish_shading(o, angle)
    return o


# ---- boom frame: s runs along the axis from the hinge boss ------------------------------------

# the axis is the bisector of the two measured edges; it passes 3.5 cm under the hinge boss
_XP = (BOOM_UP[1] - BOOM_LO[1]) / (BOOM_LO[0] - BOOM_UP[0])     # where the edge lines meet
_P0 = Vector((_XP, 0.0, BOOM_LO[0] * _XP + BOOM_LO[1]))
_A_LO, _A_UP = math.atan(BOOM_LO[0]), math.atan(BOOM_UP[0])
_HALF = math.sin((_A_LO - _A_UP) / 2)
_D = Vector((math.cos((_A_LO + _A_UP) / 2), 0.0, math.sin((_A_LO + _A_UP) / 2)))   # along the boom
_F = _P0 - _D * ((_XP - BOOM_FOOT[0]) / _D.x)                   # axis at the boss x
_S0 = (_P0 - _F).length
_N = Vector((-_D.z, 0.0, _D.x))                                  # up-left, in the XZ plane
_TIP = Vector(BOOM_TIP)
_S_TIP = (_TIP - _F).dot(_D)
_S_END = _S_TIP - TIP_GAP


def boom_axis(s):
    return _F + _D * s


def boom_h(s):
    return 2 * _HALF * (_S0 - s)


def boom_d(s):
    return BOOM_D[0] + (BOOM_D[1] - BOOM_D[0]) * s / _S_END


def boom_section(s, grow=0.0):
    c = boom_axis(s)
    hh, dd = boom_h(s) / 2 + grow, boom_d(s) / 2 + grow
    Y = Vector((0, 1, 0))
    lo, up = c - _N * hh, c + _N * hh
    return [lo - Y * dd, lo + Y * dd, up + Y * dd, up - Y * dd]


def _band_s():
    """Station of the upper band: the middle of its underside right above the hook line."""
    s = _S_END - 0.5
    for _ in range(30):
        x = (boom_axis(s) - _N * (boom_h(s) / 2 + 0.015)).x
        s -= (x - FALL_X) / (_D.x + _N.x * -_HALF)
    return s


_S_BAND = _band_s()


def mast_x0(z):
    """-X face of the mast at height z."""
    x_bot = MAST[0] - MAST[2] / 2
    return x_bot + (MAST_X0_TOP - x_bot) * (z - RAIL_TOP) / (MAST[3] - RAIL_TOP)


GUY_EYE_X = (MAST[0] - MAST[2] / 2 + 0.05, MAST[0] + MAST[2] / 2)


# ---- parts -------------------------------------------------------------------------------------

def build_base(col):
    bm = bmesh.new()          # main timbers (bevelled)
    T = 0
    O = BASE_OUT
    h = BASE / 2
    yc = O - SILL_W / 2
    # bottom sills: along X full length, along Y between them
    for sy in (-1, 1):
        box(bm, (0, sy * yc, SILL_H / 2), (2 * O, SILL_W, SILL_H), T)
    for sx in (-1, 1):
        box(bm, (sx * yc, 0, SILL_H / 2), (SILL_W, 2 * (O - SILL_W), SILL_H), T)
    # top rails, deck boards rest on them
    rz = (MID_TOP + RAIL_TOP) / 2
    rh = RAIL_TOP - MID_TOP
    for sy in (-1, 1):
        box(bm, (0, sy * RAIL_Y, rz), (BASE, RAIL_W, rh), T)
    for sx in (-1, 1):
        box(bm, (sx * RAIL_Y, 0, rz), (RAIL_W, BASE - 2 * RAIL_W, rh), T)
    finish("PC_base_timber", bm, col, ["timber"], bevel=0.02, segs=1)

    bm = bmesh.new()          # deck: joists, boards along X over them, 1 cm gaps, cut round the mast
    # joists between the front and back rails: mast, winch standards, one under the boom side
    jl = BASE - 2 * RAIL_W
    mx, my, ms, _ = MAST
    box(bm, (mx, 0, rz), (ms + 0.06, jl, rh), 0)
    for x in STD_X + (1.75,):
        box(bm, (x, 0, rz), (0.30, jl, rh), 0)
    gap = 0.01
    bw = (BASE - gap * (DECK_N - 1)) / DECK_N
    zc = RAIL_TOP + DECK_T / 2
    for k in range(DECK_N):
        y0 = -h + k * (bw + gap)
        y1 = y0 + bw
        yc_ = (y0 + y1) / 2
        if y1 > my - ms / 2 - 0.005 and y0 < my + ms / 2 + 0.005:
            xa, xb = mx - ms / 2 - 0.01, mx + ms / 2 + 0.01
            box(bm, ((-h + xa) / 2, yc_, zc), (xa + h, bw, DECK_T), 0)
            box(bm, ((xb + h) / 2, yc_, zc), (h - xb, bw, DECK_T), 0)
        else:
            box(bm, (0, yc_, zc), (BASE, bw, DECK_T), 0)
    finish("PC_deck", bm, col, ["timber"])

    bm = bmesh.new()          # beam-end stacks and braces
    c1 = (SILL_H + MID_TOP) / 2
    ch = (MID_TOP - SILL_H) / 2
    zs = (SILL_H + ch / 2, c1 + ch / 2)
    a_out, a_in = O - STACK / 2, O - STACK * 1.5 - 0.01
    for sx in (-1, 1):
        for sy in (-1, 1):
            for z in zs:
                for ax, ay in ((a_out, a_out), (a_out, a_in), (a_in, a_out)):   # L: each block on a sill
                    box(bm, (sx * ax, sy * ay, z), (STACK, STACK, ch), 0)
    # centre stacks: front/back under the mast, left/right at y = 0
    for s in (-1, 1):
        for z in zs:
            box(bm, (CENTRE_X, s * a_out, z), (CENTRE_W - 0.05, STACK, ch), 0)
            box(bm, (s * a_out, 0, z), (STACK, 0.6, ch), 0)
    # X braces, lapped: the outer diagonal in front of the inner one
    inner = O - 2 * STACK - 0.01
    z0, z1 = SILL_H + 0.07, MID_TOP - 0.07
    bw, bt = 0.15, 0.09
    face_out = O - 0.02

    def bay_braces(u0, u1, along_x, side):
        for k, (ua, ub) in enumerate(((u0, u1), (u1, u0))):
            depth = side * (face_out - bt / 2 - k * bt)
            if along_x:
                p0, p1, sd = (ua, depth, z0), (ub, depth, z1), (0, 1, 0)
            else:
                p0, p1, sd = (depth, ua, z0), (depth, ub, z1), (1, 0, 0)
            beam(bm, p0, p1, bt, bw, side=sd, mat_index=0)
    for side in (-1, 1):
        bay_braces(-inner + 0.06, CENTRE_X - CENTRE_W / 2 + 0.02, True, side)
        bay_braces(CENTRE_X + CENTRE_W / 2 - 0.02, inner - 0.06, True, side)
        bay_braces(-inner + 0.06, -0.32, False, side)
        bay_braces(0.32, inner - 0.06, False, side)
    finish("PC_base_frame", bm, col, ["timber"])

    bm = bmesh.new()          # iron straps bent over the rails, crossing plates, bolts
    I, R = 0, 1
    st, sw = 0.015, 0.12

    def strap(u, along_x, side):
        if along_x:
            box(bm, (u, side * (O + st / 2), (0.04 + MID_TOP) / 2), (sw, st, MID_TOP - 0.04), I)
            box(bm, (u, side * (h + st / 2), (MID_TOP + BASE_TOP - 0.02) / 2), (sw, st, BASE_TOP - 0.02 - MID_TOP + 0.015), I)
            box(bm, (u, side * (h + 0.005 + O + st) / 2, MID_TOP + st / 2), (sw - 0.006, O + st - h - 0.005, st), I)
            rod(bm, (u, side * (h - 0.03), 0.86), (u, side * (h + st + 0.02), 0.86), 0.025, 4, R)
        else:
            box(bm, (side * (O + st / 2), u, (0.04 + MID_TOP) / 2), (st, sw, MID_TOP - 0.04), I)
            box(bm, (side * (h + st / 2), u, (MID_TOP + BASE_TOP - 0.02) / 2), (st, sw, BASE_TOP - 0.02 - MID_TOP + 0.015), I)
            box(bm, (side * (h + 0.005 + O + st) / 2, u, MID_TOP + st / 2), (O + st - h - 0.005, sw - 0.006, st), I)
            rod(bm, (side * (h - 0.03), u, 0.86), (side * (h + st + 0.02), u, 0.86), 0.025, 4, R)
    for side in (-1, 1):
        for u in (-inner + 0.02, CENTRE_X - CENTRE_W / 2 - 0.01, CENTRE_X + CENTRE_W / 2 + 0.01, inner - 0.02):
            strap(u, True, side)
        for u in (-inner + 0.02, -0.33, 0.33, inner - 0.02):
            strap(u, False, side)
        # plates with a bolt where the braces cross
        zc = (z0 + z1) / 2
        for u in ((-inner + 0.06 + CENTRE_X - CENTRE_W / 2 + 0.02) / 2, (CENTRE_X + CENTRE_W / 2 - 0.02 + inner - 0.06) / 2):
            box(bm, (u, side * (face_out + 0.008), zc), (0.16, 0.016, 0.16), I)
            rod(bm, (u, side * (face_out - 0.03), zc), (u, side * (face_out + 0.04), zc), 0.03, 4, R)
        for u in ((-inner + 0.06 - 0.32) / 2, (0.32 + inner - 0.06) / 2):
            box(bm, (side * (face_out + 0.008), u, zc), (0.016, 0.16, 0.16), I)
            rod(bm, (side * (face_out - 0.03), u, zc), (side * (face_out + 0.04), u, zc), 0.03, 4, R)
    finish("PC_base_iron", bm, col, ["iron", "rust"], angle=35)


def build_mast(col):
    mx, my, ms, mtop = MAST
    lx, ly, lz0, lzt, lr = LEAD
    sz, sw = SLOT
    face = mx + ms / 2

    def sec(z, y0, y1, grow=0.0):
        x0 = mast_x0(z) - grow
        return [(x0, y0, z), (face + grow, y0, z), (face + grow, y1, z), (x0, y1, z)]
    bm = bmesh.new()          # timber: the shaft on its joist, the head split by the sheave slot
    loft([sec(RAIL_TOP, my - ms / 2, my + ms / 2), sec(sz, my - ms / 2, my + ms / 2)], bm=bm)
    for sy in (-1, 1):
        ya, yb = sorted((my + sy * sw, my + sy * ms / 2))
        loft([sec(sz, ya, yb), sec(mtop, ya, yb)], bm=bm)
    finish("PC_mast", bm, col, ["timber"], bevel=0.015)

    bm = bmesh.new()          # iron of the mast: collars, bolts, eyes, lugs, gooseneck; blocks
    I, R, RB = 0, 1, 2
    for k, (z0, z1, s) in enumerate(COLLARS):
        zc = (z0 + z1) / 2
        t = (s - ms) / 2                                       # collar plate thickness
        xa, xb = mast_x0(zc) - t, face + t
        box(bm, ((xa + xb) / 2, my, zc), (xb - xa, s, z1 - z0), I)
        for bx, by in () if k == 2 else ((xb + 0.012, my), (xa - 0.012, my), ((xa + xb) / 2, my + s / 2 + 0.012), ((xa + xb) / 2, my - s / 2 - 0.012)):
            box(bm, (bx, by, zc), (0.025 if by == my else 0.05, 0.05 if by == my else 0.025, 0.05), R)   # bolt heads
    # head sheave in the slot on a through pin, pin heads on the cheeks
    hz, hr = HEAD_SHEAVE
    hx = (mast_x0(hz) + face) / 2
    sheave(bm, (hx, my, hz), (0, 1, 0), hr, sw - 0.012, 12, RB)
    rod(bm, (hx, my - ms / 2 - 0.03, hz), (hx, my + ms / 2 + 0.03, hz), 0.03, 6, R)
    # topping lugs on the +X face: the upper one on its own plate on the cheek, the lower on the collar
    for (x, y, z) in TOP_LUG:
        if z > COLLARS[1][1]:
            box(bm, (mx + ms / 2 + 0.01, y, z), (0.02, 0.12, 0.16), I)
        box(bm, (mx + ms / 2 + 0.075, y, z), (0.13, 0.03, 0.1), I)
    # eyes for the guys under the lower collar
    for gx in GUY_EYE_X:
        for sy in (-1, 1):
            torus(bm, (gx, sy * 0.34, GUY_TOP_Z - 0.06), (1, 0, 0), 0.04, 0.012, 8, 4, R)
    # gooseneck: plate on the mast face and two lugs carrying the hinge pin at BOOM_FOOT
    px, py, pz = BOOM_FOOT
    box(bm, (face + 0.0125, my, pz), (0.025, 0.66, 0.50), I)
    for sy in (-1, 1):
        lug = hull2([(face + 0.02, pz - 0.2), (face + 0.02, pz + 0.2)] + circle2(px, pz, 0.08, 10))
        plate(bm, lug, 0.04, "XZ", sy * 0.30, I)
    # lead block: wood-shelled block on an iron strap to the mast, sheave pin
    shell = [(lx - lr, lz0), (lx + lr, lz0)] + [(lx + lr * math.cos(a * math.pi / 6), lzt + lr * math.sin(a * math.pi / 6)) for a in range(7)]
    plate(bm, shell, 0.22, "XZ", ly, RB)
    box(bm, (face + 0.02, ly, (lz0 + lzt + lr) / 2), (0.04, 0.14, lzt + lr - lz0 + 0.1), I)
    rod(bm, (lx, ly - 0.14, lzt), (lx, ly + 0.14, lzt), 0.03, 6, R)
    finish("PC_mast_iron", bm, col, ["iron", "rust", "rust"], angle=35)


def build_boom(col):
    bm = bmesh.new()          # one straight timber, heel face square to the axis
    loft([boom_section(HEEL_S), boom_section(_S_END)], bm=bm)
    finish("PC_boom", bm, col, ["timber"], bevel=0.015)

    bm = bmesh.new()
    I, R = 0, 1
    Y = Vector((0, 1, 0))
    # iron shoe on the heel: heel plate, two side straps round the boss, the hinge boss and pin
    px, py, pz = BOOM_FOOT
    loft([boom_section(HEEL_S - 0.03, 0.012), boom_section(HEEL_S + 0.12, 0.012)], bm=bm, mat_index=I)
    hs = [boom_axis(s) + _N * (sg * (boom_h(s) / 2 - 0.04)) for s in (HEEL_S, HEEL_S + 0.28) for sg in (-1, 1)]
    heel = hull2([(p.x, p.z) for p in hs] + circle2(px, pz, 0.11, 12))
    dh = boom_d(HEEL_S) / 2
    for sy in (-1, 1):
        plate(bm, heel, 0.02, "XZ", sy * (dh + 0.022), I)
        rod(bm, (px, sy * (dh + 0.03), pz), (px, sy * (dh + 0.07), pz), 0.12, 10, I)
        box(bm, ((hs[2].x + hs[3].x) / 2, sy * (dh + 0.04), (hs[2].z + hs[3].z) / 2), (0.05, 0.02, 0.05), R)
    rod(bm, (px, -0.36, pz), (px, 0.36, pz), 0.04, 8, R)
    # iron bands; the upper band carries the eye for the fall block underneath
    for sb in (_S_END - 1.3, _S_BAND):
        loft([boom_section(sb - 0.05, 0.015), boom_section(sb + 0.05, 0.015)], bm=bm, mat_index=I)
    bot = boom_axis(_S_BAND) - _N * (boom_h(_S_BAND) / 2 + 0.015)
    torus(bm, bot - Vector((0, 0, 0.035)), (0, 1, 0), 0.04, 0.014, 8, 4, R)
    # two small side plates at the end carrying the tip sheave on its pin, eyes for the topping ropes
    de = boom_d(_S_END) / 2
    s_pl = min(_S_END - 0.3, min((Vector((x, 0, z)) - _F).dot(_D) for x, z in TOP_END) - 0.08)
    for (ex, ez), sy in zip(TOP_END, (1, -1)):
        es = [boom_axis(s) + _N * (sg * (boom_h(s) / 2 - 0.02)) for s in (s_pl, _S_END) for sg in (-1, 1)]
        endp = hull2([(p.x, p.z) for p in es] + circle2(_TIP.x, _TIP.z, 0.09, 10))
        plate(bm, endp, 0.02, "XZ", sy * (de + 0.01), I)
        torus(bm, (ex, sy * (de + 0.06), ez), _D, 0.035, 0.012, 6, 4, R)
    rod(bm, (_TIP.x, -de - 0.06, _TIP.z), (_TIP.x, de + 0.06, _TIP.z), 0.03, 6, R)
    sheave(bm, _TIP, (0, 1, 0), TIP_R, 0.07, 12, R)
    finish("PC_boom_iron", bm, col, ["iron", "rust"], angle=35)


def build_winch(col):
    wx, wy, wz0, wz1, wh = WINCH
    bm = bmesh.new()
    I, T, RP, H, R = 0, 1, 2, 3, 4
    # sleepers on the deck
    for sy in (-1, 1):
        box(bm, ((wx + 0.15 - 0.08) / 2, wy + sy * 0.95, wz0 + 0.1), (-0.08 - wx - 0.15, 0.22, 0.2), T)
    zb = wz0 + 0.2
    # standards: foot bar, posts carrying the crank shafts, cross bar for the drum, A-top
    for x in STD_X:
        box(bm, (x, wy, zb + 0.05), (0.12, 2 * wh - 0.1, 0.1), I)
        for sy in (-1, 1):
            beam(bm, (x, wy + sy * CRANK_Y, zb + 0.1), (x, wy + sy * CRANK_Y, 2.15), 0.1, 0.12, side=(0, 1, 0), mat_index=I)
            beam(bm, (x, wy + sy * CRANK_Y, 2.1), (x, wy + sy * 0.06, wz1 - 0.1), 0.1, 0.12, side=(1, 0, 0), mat_index=I)
            beam(bm, (x, wy + sy * (CRANK_Y + 0.05), zb + 0.1), (x, wy + sy * 0.3, DRUM_Z - 0.05), 0.08, 0.1, side=(1, 0, 0), mat_index=I)
        box(bm, (x, wy, DRUM_Z), (0.1, 2 * CRANK_Y, 0.1), I)
        rod(bm, (x - 0.09, wy, DRUM_Z), (x + 0.09, wy, DRUM_Z), 0.12, 8, I)
        rod(bm, (x - 0.11, wy, wz1 - 0.13), (x + 0.11, wy, wz1 - 0.13), 0.13, 8, I)
        for sy in (-1, 1):
            rod(bm, (x - 0.09, wy + sy * CRANK_Y, CRANK_Z), (x + 0.09, wy + sy * CRANK_Y, CRANK_Z), 0.08, 8, I)
    # drum: shaft, flanges 0.78 m apart, barrel, rope coil as a helix of turns, hub, gear
    xa, xb = DRUM_X
    rod(bm, (STD_X[0] - 0.12, wy, DRUM_Z), (STD_X[1] + 0.12, wy, DRUM_Z), 0.06, 8, I)
    rod(bm, (xa - 0.06, wy, DRUM_Z), (xa, wy, DRUM_Z), 0.6, 16, T)
    rod(bm, (xb, wy, DRUM_Z), (xb + 0.06, wy, DRUM_Z), 0.6, 16, T)
    rod(bm, (xa, wy, DRUM_Z), (xb, wy, DRUM_Z), COIL_R - COIL_ROPE + 0.006, 12, T)
    per = 8
    pitch = 2 * COIL_ROPE
    turns = int(round((xb - xa - pitch) / pitch))
    a_end = math.pi / 2
    pts = []
    for i in range(turns * per + 1):
        a = a_end - 2 * math.pi * (turns * per - i) / per
        pts.append((xa + COIL_ROPE + pitch * i / per, wy + COIL_R * math.cos(a), DRUM_Z + COIL_R * math.sin(a)))
    rb = bmesh.new()            # the coil on its own object: a square rope section shaded round
    tube(rb, pts, COIL_ROPE, 10, 0)
    finish("PC_winch_rope", rb, col, ["rope"], angle=95)
    rod(bm, (xb + 0.06, wy, DRUM_Z), (-0.60, wy, DRUM_Z), 0.22, 8, I)

    def gear(xc, r, teeth, depth, ry, rz):
        pts = []
        p = 2 * math.pi / teeth
        for k in range(teeth):
            a = k * p
            rr = r - 0.06 if r > 0.3 else r - 0.04
            for da, rad in ((0, rr), (0.2, r), (0.5, r), (0.7, rr)):
                pts.append((ry + rad * math.cos(a + da * p), rz + rad * math.sin(a + da * p)))
        plate(bm, pts, depth, "YZ", xc, I)
    gear(-0.54, GEAR_R, 18, 0.14, wy, DRUM_Z)
    rod(bm, (-0.47, wy, DRUM_Z), (-0.40, wy, DRUM_Z), 0.18, 8, I)
    # crank shafts with pinions, cranks with wooden grips at the -X end
    for sy in (-1, 1):
        y = wy + sy * CRANK_Y
        rod(bm, (wx + 0.04, y, CRANK_Z), (STD_X[1] + 0.12, y, CRANK_Z), 0.04, 8, I)
        gear(-0.54, 0.17, 6, 0.12, y, CRANK_Z)
        beam(bm, (wx + 0.03, y, CRANK_Z - 0.06), (wx + 0.03, y, GRIP_Z + 0.04), 0.06, 0.1, side=(1, 0, 0), mat_index=I)
        rod(bm, (wx + 0.05, y, GRIP_Z), (CRANK_X + 0.02, y, GRIP_Z), 0.025, 8, I)
        rod(bm, (wx - 0.05, y, GRIP_Z), (CRANK_X + 0.05, y, GRIP_Z), 0.055, 10, H)
        rod(bm, (CRANK_X + 0.05, y, GRIP_Z), (CRANK_X, y, GRIP_Z), 0.04, 8, R)
    finish("PC_winch", bm, col, ["iron", "timber", "rope", "handle", "rust"], angle=35)
    return pts[-1]


def build_rigging(col, coil_end):
    bm = bmesh.new()
    RP, I, R = 0, 1, 2
    mx, my, ms, mtop = MAST
    Y = Vector((0, 1, 0))
    # topping ropes: mast lugs -> eyes on top of the upper boom band
    de = boom_d(_S_END) / 2
    for (x, y, z), (ex, ez), sy in zip(TOP_LUG, TOP_END, (1, -1)):
        rod(bm, (x + 0.02, y, z), (ex - 0.03, sy * (de + 0.06), ez - 0.005), R_ROPE, 6, RP)
    # hoist rope: off the top of the coil -> past the gear and the mast front -> lead block
    lx, ly, lz0, lzt, lr = LEAD
    c0 = Vector(coil_end)
    tube(bm, [c0, c0 + Vector((0.05, -0.2, 0.06)), (-0.55, -0.45, 2.70), (lx - 0.04, ly - 0.09, 3.0)], R_ROPE, 12, RP)
    # lead block -> over the tip sheave, and the second part along the boom top
    rod(bm, (lx, ly + 0.02, lzt + lr - 0.03), (_TIP.x, 0.0, _TIP.z + TIP_R - 0.03), R_ROPE, 6, RP)
    q1 = boom_axis(_S_END * 0.7) + _N * (boom_h(_S_END * 0.7) / 2 + R_ROPE + 0.002) - Y * 0.08
    q2 = boom_axis(_S_END - 1.45) + _N * (boom_h(_S_END - 1.45) / 2 + R_ROPE + 0.002) - Y * 0.08
    tube(bm, [(lx + 0.05, ly + 0.06, lzt + 0.1), q1, q2], R_ROPE, 12, RP)
    # falls: block hung from the band eye -> running block A -> running block B -> hook
    hx, hy, hz = FALL_X, HOOK[1], HOOK[2]
    eye = boom_axis(_S_BAND) - _N * (boom_h(_S_BAND) / 2 + 0.015) - Vector((0, 0, 0.035))
    tr = 0.12
    tz = eye.z - 0.04 - 0.02 - tr
    rod(bm, (hx, hy - 0.03, eye.z - 0.04), (hx, hy - 0.03, tz), 0.014, 6, R)      # shackle strap
    rod(bm, (hx, hy + 0.03, eye.z - 0.04), (hx, hy + 0.03, tz), 0.014, 6, R)
    za, ra = A_BLOCK
    zb, rb = B_BLOCK

    def block(z, r):
        sheave(bm, (hx, hy, z), (0, 1, 0), r, 0.06, 8, R)
        rod(bm, (hx, hy - 0.09, z), (hx, hy + 0.09, z), 0.035, 6, R)
    block(tz, tr)
    block(za, ra)
    block(zb, rb)
    for sx in (-1, 1):
        x = hx + sx * FALL_DX
        rod(bm, (x, hy, tz - 0.04), (x, hy, za + 0.08), R_ROPE, 6, RP)
        rod(bm, (x, hy, za - 0.08), (x, hy, zb + 0.12), R_ROPE, 6, RP)
    # shackle under B, eye and hook (bowl bottom at HOOK)
    ys = 0.05
    sh = [(hx, hy - ys, zb - 0.12), (hx, hy - ys, 3.76)]
    sh += [(hx, hy - ys * math.cos(a * math.pi / 6), 3.76 - ys * math.sin(a * math.pi / 6)) for a in range(1, 6)]
    sh += [(hx, hy + ys, 3.76), (hx, hy + ys, zb - 0.12)]
    tube(bm, sh, 0.018, 12, R)
    eye_z = 3.66
    torus(bm, (hx, hy, eye_z), (0, 1, 0), 0.06, 0.02, 12, 6, R)
    rb_ = 0.17
    rw = 0.04
    cz = hz + rw + rb_
    path = [(hx, hy, eye_z - 0.05), (hx, hy, 3.55), (hx + 0.07, hy, 3.47)]
    radii = [rw, rw, rw]
    for k in range(0, 14):
        a = math.radians(30 - k * 18.5)
        path.append((hx + rb_ * math.cos(a), hy, cz + rb_ * math.sin(a)))
        radii.append(rw - (rw - 0.016) * max(0, k - 8) / 5)
    tube(bm, path, radii, 12, R)
    # guys: collar eyes -> turnbuckles -> shackles on eye plates on the deck
    for gx_top, ax in zip(GUY_EYE_X, GUY_ANCHOR_X):
        for sy in (-1, 1):
            tp = Vector((gx_top, sy * 0.34, GUY_TOP_Z - 0.06))
            anc = Vector((ax, sy * RAIL_Y, BASE_TOP))
            u = (tp - anc).normalized()
            box(bm, (ax, sy * RAIL_Y, BASE_TOP + 0.01), (0.18, 0.12, 0.02), I)
            torus(bm, anc + Vector((0, 0, 0.06)), u.cross(Vector((0, 0, 1))).normalized() if abs(u.z) < 0.99 else (1, 0, 0), 0.045, 0.014, 6, 4, R)
            b0, b1 = anc + u * 0.32, anc + u * 0.72
            rod(bm, anc + Vector((0, 0, 0.1)), b0, 0.018, 6, I)
            tube(bm, [b0 - u * 0.02, b0 + u * 0.06, b1 - u * 0.06, b1 + u * 0.02], [0.075, 0.06, 0.06, 0.075], 16, I)
            rod(bm, b1, b1 + u * 0.14, 0.018, 6, I)
            rod(bm, b1 + u * 0.12, tp + u * 0.01, R_ROPE, 6, RP)
    finish("PC_rigging", bm, col, ["rope", "iron", "rust"], angle=35)


def build_ladder(col):
    bm = bmesh.new()
    xs = (1.35, 1.85)
    h = BASE / 2
    y0, z0 = BASE_OUT + 0.32, 0.02
    y1, z1 = h + 0.03, BASE_TOP - 0.01          # the stringers lean on the deck edge
    for x in xs:
        beam(bm, (x, y0, z0), (x, y1, z1), 0.06, 0.1, side=(1, 0, 0))
    for k in range(1, 4):                        # rungs every 0.25 m, the deck is the top step
        z = k * BASE_TOP / 4
        y = y0 + (y1 - y0) * (z - z0) / (z1 - z0)
        rod(bm, (xs[0] - 0.02, y, z), (xs[1] + 0.02, y, z), 0.025, 6, 0)
    finish("PC_ladder", bm, col, ["timber"])


def build(sc, col):
    build_base(col)
    build_mast(col)
    build_boom(col)
    coil_end = build_winch(col)
    build_rigging(col, coil_end)
    build_ladder(col)
