# Port office: the whole building. Rubble plinth (two courses of rounded stones proud of a dark
# mortar core, quoins folded round the corners), crib of round logs (front and back walls on whole
# courses, end walls on half courses, ends sticking out past the corners, logs cut at the openings),
# windows with frame, sash bars, glass and white trim, plank door with face battens, strap hinges and
# a latch handle, porch with landing, three steps, two posts and an iron canopy, enamel plaque plate
# standing off the logs on furring strips, rafter roof with iron sheet, standing seams and
# ridge cap, gables of vertical boards, brick chimney on the ridge. Built by blender-part-builder;
# the shared numbers (WALL, LOG_END, BASE_Z, EAVE_Z, RIDGE_Z, ROOF, CHIMNEY, DOOR_X, PORCH, PLAQUE)
# come from dims.py. RIDGE_Z is taken as the ridge of the rafters (underside of the roof deck).

import random

R_LOG = 0.165         # log radius (diameter 0.33: corner discs of ref/front.png, 0.32 apart)
COURSE = 0.29         # course height, 0.88 of the diameter (ref: about 0.31)
N_COURSES = 10        # front/back courses k = 0..9; end walls j = -1..8 on half courses
SEG = 24              # log segments
END_STEPS = [0.05 + 0.012 * i for i in range(6)]   # log ends stick out LOG_END minus 0.05..0.11
                        # (outline of ref/front.png: 0.24, ref/left.png: 0.22)
CUT_STEPS = [0.0, 0.015, 0.005, 0.02, 0.01]   # log ends at an opening run 0..2 cm into the jamb
PLINTH = (4.30, 2.96)  # plinth half sizes over the stone crowns, ref/front.png and ref/left.png
STONE_PROUD = 0.08    # stone crowns stand this far out of the mortar core
STONE_COURSES = [(0.0, 0.31), (0.31, BASE_Z)]   # two courses, ref/front.png
STONE_GAP = 0.018     # mortar joint
STONE_RING = 8        # vertices round a stone (30 tris a stone)
STONE_W = 0.50        # mean stone width, ref/front.png
STONE_ROUND = 0.80    # front ring / outline: how far the stone's edge rounds in
STONE_EDGE = 0.035    # front ring this far behind the crown
STONE_CORNER_R = 0.25  # rounding of the core corners the quoin stones fold round
STONE_QUOIN = 0.66    # quoin stone length along the perimeter, ref/front.png corners
STONE_P = 3.2         # superellipse exponent of the stone outline (2 = ellipse)
ROOF_T = 0.05         # roof deck + iron, perpendicular to the slope
RAFTER = (0.07, 0.12)  # width, depth
RAFTER_X = [-3.9, -2.95, -2.0, -1.05, -0.1, 0.85, 1.8, 2.55, 3.45, 3.9]   # clear of the chimney
BARGE_X = 4.57        # barge rafters carrying the gable overhang
RAFTER_END = 3.24     # rafter ends under the eave (|y|)
SEAM_X = [0.89 * (i - 4) for i in range(9)]   # standing seams, ref/front.png
CAP_HALF, CAP_RISE = 0.2, 0.075   # ridge cap of folded iron
GABLE_W = (4.06, 4.09)  # gable boards back / front face, |x|
GABLE_SILL_Z = (3.38, 3.60)

# windows: glass interval along the wall (u0, u1) and height (z0, z1), measured on the views
FRONT_GLASS_Z = (1.55, 2.61)
SIDE_GLASS_Z = (1.71, 2.78)
WINDOWS = {
    "front": [(-3.115, -2.455), (1.215, 1.875), (2.665, 3.325)],
    "back": [(1.27, 1.93), (-2.09, -1.43)],
    "left": [(-0.40, 0.40)],
    "right": [(-0.40, 0.40)],
}
JAMB = 0.07           # window frame (box) member
TRIM_W, TRIM_T = 0.17, 0.035
GABLE_WIN = (-0.19, 0.19, 3.92, 4.34)   # right gable window glass, ref/right.png
DOOR_Z = (0.70, 2.61)  # leaf bottom/top
DOOR_BATTENS = [(0.76, 0.90), (2.42, 2.56)]   # cross battens on the leaf face, ref/front.png
DOOR_HANDLE = (0.06, 0.31, 1.72)   # latch grip: from/to past the leaf's left edge, height (ref: z 1.72)
PLAQUE_STANDOFF = 0.02  # plate back off the log crowns
PLAQUE_T = 0.012
CANOPY_K = 0.287      # canopy slope (16 deg)
POST_X = (-1.875, -0.275)
POST_Y = -3.2
LANDING_Y = -3.30     # outer edge of the landing


def slope():
    return (RIDGE_Z - EAVE_Z) / ROOF[1]


def zu(y):
    """Underside of the roof deck (top of the rafters) at |y|."""
    return RIDGE_Z - abs(y) * slope()


def aabb(bm, lo, hi, mi=0):
    c = [(a + b) / 2 for a, b in zip(lo, hi)]
    s = [abs(b - a) for a, b in zip(lo, hi)]
    return box(bm, c, s, mi)


def wall_pt(wall, u, w, z):
    """Wall frame: u along the wall (world x or y), w outward from the log axis plane, z up."""
    hy, hx = WALL[1] / 2, WALL[0] / 2
    if wall == "front":
        return (u, -(hy + w), z)
    if wall == "back":
        return (u, hy + w, z)
    if wall == "right":
        return (hx + w, u, z)
    return (-(hx + w), u, z)


def wbox(bm, wall, u0, u1, w0, w1, z0, z1, mi=0):
    a, b = wall_pt(wall, u0, w0, z0), wall_pt(wall, u1, w1, z1)
    return aabb(bm, [min(p, q) for p, q in zip(a, b)], [max(p, q) for p, q in zip(a, b)], mi)


def tri_ngons(bm):
    ng = [f for f in bm.faces if len(f.verts) > 4]
    if ng:
        bmesh.ops.triangulate(bm, faces=ng)


# ---------------------------------------------------------------- log crib
def openings():
    """Per wall: (u0, u1, zc0, zc1) - logs whose centre lies in (zc0, zc1) are cut between u0 and u1."""
    out = {w: [] for w in WINDOWS}
    for wall, wins in WINDOWS.items():
        gz = FRONT_GLASS_Z if wall in ("front", "back") else SIDE_GLASS_Z
        for u0, u1 in wins:
            out[wall].append((u0 - JAMB, u1 + JAMB, gz[0] - COURSE / 2, gz[1] + COURSE / 2))
    out["front"].append((DOOR_X[0], DOOR_X[1], 0.0, DOOR_Z[1] + COURSE / 2))
    return out


def courses(wall):
    if wall in ("front", "back"):
        return [(k, BASE_Z + R_LOG + k * COURSE) for k in range(N_COURSES)]
    return [(j, BASE_Z + R_LOG + (j + 0.5) * COURSE) for j in range(-1, N_COURSES - 1)]


def opening_z(wall, u0, u1):
    """Seam-to-seam z-range of the logs an opening (u0, u1) of `wall` cuts."""
    for a, b, z0, z1 in openings()[wall]:
        if abs(a - u0) < 1e-6 and abs(b - u1) < 1e-6:
            zs = [zc for _, zc in courses(wall) if z0 < zc < z1]
            lo = min(zs) - COURSE / 2
            return (BASE_Z if lo < BASE_Z + R_LOG else lo), max(zs) + COURSE / 2
    raise ValueError(f"no opening {wall} {u0} {u1}")


def build_logs(col, rnd):
    bm = bmesh.new()
    ops = openings()
    for wall in ("front", "back", "left", "right"):
        half = (WALL[0] if wall in ("front", "back") else WALL[1]) / 2
        for k, zc in courses(wall):
            r = R_LOG * (1 + rnd.uniform(-0.05, 0.05))
            if k == 0 and wall in ("front", "back"):
                zc = BASE_Z + r - 0.01          # the first course lies on the plinth
            # ends differ course to course by whole steps, so no two end discs share a plane
            ends = [(-(half + LOG_END - END_STEPS[(2 * k + 6) % 6]), True)]
            for u0, u1 in sorted((a, b) for a, b, z0, z1 in ops[wall] if z0 < zc < z1):
                cv = CUT_STEPS[(3 * k + 3) % 5]
                ends += [(u0 + cv, False), (u1 - cv, False)]
            ends.append((half + LOG_END - END_STEPS[(2 * k + 9) % 6], True))
            for (a, ca), (b, cb) in zip(ends[0::2], ends[1::2]):
                if b - a < 0.05:
                    continue
                # end grain (slot 1) only at the corner ends; a cut at an opening stays plain
                cylinder(bm, wall_pt(wall, a, 0, zc), wall_pt(wall, b, 0, zc), r, segments=SEG,
                         phase=rnd.uniform(0, 2 * math.pi), cap_index=(int(ca), int(cb)))
    tri_ngons(bm)
    return obj_from_bm("PO_logs", bm, col, [M("log"), M("log")])


class Ring:
    """Perimeter of the plinth core: a rectangle (half sizes cx, cy) with corners rounded by
    radius rc. s runs counter-clockwise from the left end of the front straight; at(s, w, z) is
    the point w outward of the perimeter. Stones laid in s fold round the corners."""

    def __init__(self, cx, cy, rc):
        self.cx, self.cy, self.rc = cx, cy, rc
        lx, ly, q = 2 * (cx - rc), 2 * (cy - rc), math.pi * rc / 2
        # (length, kind, data): straights carry start point and direction, arcs centre and start angle
        self.segs = [(lx, "L", ((-(cx - rc), -cy), (1, 0), (0, -1))), (q, "A", ((cx - rc, -(cy - rc)), -math.pi / 2)),
                     (ly, "L", ((cx, -(cy - rc)), (0, 1), (1, 0))), (q, "A", ((cx - rc, cy - rc), 0.0)),
                     (lx, "L", ((cx - rc, cy), (-1, 0), (0, 1))), (q, "A", ((-(cx - rc), cy - rc), math.pi / 2)),
                     (ly, "L", ((-cx, cy - rc), (0, -1), (-1, 0))), (q, "A", ((-(cx - rc), -(cy - rc)), math.pi))]
        self.length = sum(L for L, _, _ in self.segs)
        acc, self.corner_s = 0.0, []
        for L, kind, _ in self.segs:
            if kind == "A":
                self.corner_s.append(acc + L / 2)
            acc += L

    def at(self, s, w, z):
        s %= self.length
        for L, kind, d in self.segs:
            if s <= L:
                if kind == "L":
                    (x0, y0), (dx, dy), (nx, ny) = d
                    return Vector((x0 + dx * s + nx * w, y0 + dy * s + ny * w, z))
                (ax, ay), a0 = d
                a = a0 + s / self.rc
                return Vector((ax + (self.rc + w) * math.cos(a), ay + (self.rc + w) * math.sin(a), z))
            s -= L
        raise ValueError(s)

    def outline(self, per_arc=3):
        pts = []
        acc = 0.0
        for L, kind, _ in self.segs:
            n = per_arc if kind == "A" else 1
            for i in range(n):
                pts.append(self.at(acc + L * i / n, 0.0, 0.0))
            acc += L
        return [(p.x, p.y) for p in pts]


def stone(bm, ring, s, z, rx, rz, w_back, w_out, rnd):
    """One rubble stone: a closed pillow laid along the plinth perimeter, back ring just inside
    the mortar core (its outline is the stone's outline on the core face), front ring rounded in,
    crown at w_out. Rings are rounded rectangles (superellipse) with a little irregularity per
    vertex, scaled so each side reaches its half size: stones fill their slots to the joint."""
    n = STONE_RING
    e = 2 / STONE_P
    pts = []
    for i in range(n):
        a = 2 * math.pi * (i + 0.5) / n
        c, sn = math.cos(a), math.sin(a)
        pts.append([math.copysign(abs(c) ** e, c) * (1 + rnd.uniform(-0.08, 0.0)),
                    math.copysign(abs(sn) ** e, sn) * (1 + rnd.uniform(-0.08, 0.0))])
    for ax, r in ((0, rx), (1, rz)):
        hi = max(q[ax] for q in pts)
        lo = -min(q[ax] for q in pts)
        for q in pts:
            q[ax] *= r / (hi if q[ax] > 0 else lo)
    back, front = [], []
    for du, dz in pts:
        back.append(bm.verts.new(ring.at(s + du, w_back, z + dz)))
        k = STONE_ROUND + rnd.uniform(-0.03, 0.03)
        front.append(bm.verts.new(ring.at(s + du * k, w_out - STONE_EDGE + rnd.uniform(-0.008, 0.008), z + dz * k)))
    tip = bm.verts.new(ring.at(s + rnd.uniform(-0.05, 0.05) * rx, w_out, z + rnd.uniform(-0.1, 0.1) * rz))
    fs = [bm.faces.new(list(reversed(back)))]
    for i in range(n):
        j = (i + 1) % n
        fs.append(bm.faces.new((back[i], back[j], front[j], front[i])))
        fs.append(bm.faces.new((front[i], front[j], tip)))
    bmesh.ops.recalc_face_normals(bm, faces=fs)


def lay(bm, ring, s0, s1, z0, z1, rnd):
    """Stones edge to edge between perimeter positions s0 and s1 in the course z0..z1."""
    L = s1 - s0
    if L < 0.15:
        return
    cnt = max(1, round(L / STONE_W))
    wts = [rnd.uniform(0.75, 1.25) for _ in range(cnt)]
    tot = sum(wts)
    s = s0
    for wt in wts:
        wd = L * wt / tot
        rx = wd / 2 - STONE_GAP / 2 - rnd.uniform(0.0, 0.012)
        rz = (z1 - z0) / 2 - STONE_GAP / 2 - rnd.uniform(0.0, 0.018)
        zc = (z0 + z1) / 2 + rnd.uniform(-0.012, 0.012)
        zc = min(max(zc, rz + 0.004), BASE_Z - rz)
        stone(bm, ring, s + wd / 2 + rnd.uniform(-0.008, 0.008), zc, rx, rz,
              -0.008 - rnd.uniform(0.0, 0.02), STONE_PROUD + rnd.uniform(-0.012, 0.0), rnd)
        s += wd


def build_plinth(col):
    """Rubble plinth: a dark mortar core with rounded corners and two courses of rounded stones
    set proud of it on every face. A big quoin stone takes each corner, centred on it in the
    lower course and set off it in the upper one, so the joints do not run through. Behind the
    porch steps the core stays bare."""
    rnd = random.Random(64)
    ring = Ring(PLINTH[0] - STONE_PROUD, PLINTH[1] - STONE_PROUD, STONE_CORNER_R)
    bm = bmesh.new()
    prism(ring.outline(), BASE_Z, plane="XY", offset=BASE_Z / 2, bm=bm)
    tri_ngons(bm)
    core = obj_from_bm("PO_plinth", bm, col, [M("dark")])
    # the porch gap on the front straight, in perimeter coordinates
    x0 = ring.cx - ring.rc
    gap = (PORCH[0] + x0, PORCH[1] + x0)
    bm = bmesh.new()
    for c, (z0, z1) in enumerate(STONE_COURSES):
        cuts = []                      # (s0, s1) of the quoins, in perimeter order
        for i, sc in enumerate(ring.corner_s):
            lead = 0.5 if c == 0 else (0.25 if i % 2 else 0.75)
            wq = STONE_QUOIN * rnd.uniform(0.92, 1.08)
            cuts.append((sc - lead * wq, sc + (1 - lead) * wq))
        for a, b in cuts:
            lay(bm, ring, a, b, z0, z1, rnd)
        # fill between the quoins; the run from the last quoin wraps to the first one
        for (a0, a1), (b0, b1) in zip(cuts, cuts[1:] + [(cuts[0][0] + ring.length, 0)]):
            if a1 < gap[0] + ring.length and b0 > gap[1] + ring.length:
                lay(bm, ring, a1, gap[0] + ring.length, z0, z1, rnd)
                lay(bm, ring, gap[1] + ring.length, b0, z0, z1, rnd)
            else:
                lay(bm, ring, a1, b0, z0, z1, rnd)
    tri_ngons(bm)
    stones = obj_from_bm("PO_stones", bm, col, [M("stone")])
    return core, stones


# ---------------------------------------------------------------- windows
def window(bm, wall, gu0, gu1, gz0, gz1, oz0, oz1):
    """Frame box in the cut opening, sash bars, glass, white trim on the wall face.
    Material slots: 0 frame, 1 glass."""
    ou0, ou1 = gu0 - JAMB, gu1 + JAMB
    wb, wf = -0.12, R_LOG - 0.018      # frame front just into the trim's back
    wbox(bm, wall, ou0, gu0, wb, wf, oz0, oz1)
    wbox(bm, wall, gu1, ou1, wb, wf, oz0, oz1)
    wbox(bm, wall, gu0, gu1, wb + 0.005, wf - 0.005, gz1, oz1)
    wbox(bm, wall, gu0, gu1, wb + 0.005, wf - 0.005, oz0, gz0)
    wbox(bm, wall, gu0 - 0.02, gu1 + 0.02, -0.004, 0.004, gz0 - 0.01, gz1 + 0.01, 1)
    um = (gu0 + gu1) / 2
    zt = gz0 + 0.64 * (gz1 - gz0)
    sw = 0.045
    # sash bars, each member on its own depth so no two faces share a plane
    wbox(bm, wall, um - sw / 2, um + sw / 2, 0.0, 0.05, gz0 - 0.004, gz1 + 0.004)
    wbox(bm, wall, gu0 - 0.004, gu1 + 0.004, 0.003, 0.047, zt - sw / 2, zt + sw / 2)
    wbox(bm, wall, gu0 - 0.01, gu0 + 0.035, 0.006, 0.044, gz0 - 0.01, gz1 + 0.01)
    wbox(bm, wall, gu1 - 0.035, gu1 + 0.01, 0.006, 0.044, gz0 - 0.01, gz1 + 0.01)
    wbox(bm, wall, gu0 - 0.007, gu1 + 0.007, 0.009, 0.041, gz1 - 0.035, gz1 + 0.007)
    wbox(bm, wall, gu0 - 0.007, gu1 + 0.007, 0.009, 0.041, gz0 - 0.007, gz0 + 0.035)
    # trim (nalichnik): side boards with legs, head board, cornice, sill board; backs 2 cm into the logs
    t0 = R_LOG - 0.02
    wbox(bm, wall, gu0 - TRIM_W, gu0 + 0.005, t0, t0 + TRIM_T, gz0 - 0.28, gz1 + 0.005)
    wbox(bm, wall, gu1 - 0.005, gu1 + TRIM_W, t0, t0 + TRIM_T, gz0 - 0.28, gz1 + 0.005)
    wbox(bm, wall, gu0 - TRIM_W - 0.005, gu1 + TRIM_W + 0.005, t0 + 0.004, t0 + TRIM_T + 0.005, gz1, gz1 + 0.15)
    wbox(bm, wall, gu0 - TRIM_W - 0.06, gu1 + TRIM_W + 0.06, t0 + 0.008, t0 + 0.07, gz1 + 0.15, gz1 + 0.23)
    wbox(bm, wall, gu0 - TRIM_W - 0.04, gu1 + TRIM_W + 0.04, t0 - 0.005, t0 + 0.075, gz0 - 0.19, gz0 - 0.05)


def window_gable(bm, gu0, gu1, gz0, gz1):
    """Small window of the right gable, set in a hole of the gable boards."""
    wall = "right"
    wf = GABLE_W[1] - WALL[0] / 2 - 0.005
    wb = GABLE_W[0] - WALL[0] / 2 - 0.06
    ou0, ou1, oz0, oz1 = gu0 - 0.05, gu1 + 0.05, gz0 - 0.05, gz1 + 0.05
    wbox(bm, wall, ou0, gu0, wb, wf, oz0, oz1)
    wbox(bm, wall, gu1, ou1, wb, wf, oz0, oz1)
    wbox(bm, wall, gu0, gu1, wb + 0.005, wf - 0.005, gz1, oz1)
    wbox(bm, wall, gu0, gu1, wb + 0.005, wf - 0.005, oz0, gz0)
    wbox(bm, wall, gu0 - 0.02, gu1 + 0.02, wb + 0.02, wb + 0.028, gz0 - 0.02, gz1 + 0.02, 1)
    um, zm = (gu0 + gu1) / 2, (gz0 + gz1) / 2
    wbox(bm, wall, um - 0.02, um + 0.02, wb + 0.028, wb + 0.06, gz0 - 0.004, gz1 + 0.004)
    wbox(bm, wall, gu0 - 0.004, gu1 + 0.004, wb + 0.031, wb + 0.057, zm - 0.02, zm + 0.02)
    t0, tw = wf - 0.01, 0.10
    wbox(bm, wall, gu0 - tw, gu0 + 0.005, t0, t0 + 0.03, gz0 - 0.10, gz1 + 0.005)
    wbox(bm, wall, gu1 - 0.005, gu1 + tw, t0, t0 + 0.03, gz0 - 0.10, gz1 + 0.005)
    wbox(bm, wall, gu0 - tw - 0.005, gu1 + tw + 0.005, t0 + 0.004, t0 + 0.035, gz1, gz1 + 0.10)
    wbox(bm, wall, gu0 - tw - 0.04, gu1 + tw + 0.04, t0 + 0.008, t0 + 0.06, gz1 + 0.10, gz1 + 0.15)
    wbox(bm, wall, gu0 - tw - 0.03, gu1 + tw + 0.03, t0 - 0.005, t0 + 0.06, gz0 - 0.08, gz0 - 0.02)


def build_windows(col):
    bm = bmesh.new()
    for wall, wins in WINDOWS.items():
        gz = FRONT_GLASS_Z if wall in ("front", "back") else SIDE_GLASS_Z
        for gu0, gu1 in wins:
            oz0, oz1 = opening_z(wall, gu0 - JAMB, gu1 + JAMB)
            window(bm, wall, gu0, gu1, gz[0], gz[1], oz0, oz1)
    window_gable(bm, *GABLE_WIN)
    return obj_from_bm("PO_windows", bm, col, [M("frame"), M("glass")])


# ---------------------------------------------------------------- door and plaque
def build_door(col):
    bm = bmesh.new()
    u0, u1 = DOOR_X
    oz0, oz1 = opening_z("front", u0, u1)
    jw = 0.12
    wf = R_LOG + 0.02                      # frame slightly proud of the logs
    wbox(bm, "front", u0, u0 + jw, -0.12, wf, oz0, oz1)
    wbox(bm, "front", u1 - jw, u1, -0.12, wf, oz0, oz1)
    wbox(bm, "front", u0 + jw, u1 - jw, -0.115, wf - 0.005, DOOR_Z[1], oz1)
    wbox(bm, "front", u0 + jw, u1 - jw, -0.15, wf - 0.005, oz0, DOOR_Z[0])
    # leaf: vertical boards held by two cross battens on the face, across every board
    a, b = u0 + jw, u1 - jw
    y0 = -(WALL[1] / 2 + 0.02)
    boards(bm, [(a, y0, DOOR_Z[0]), (b, y0, DOOR_Z[0]), (b, y0, DOOR_Z[1]), (a, y0, DOOR_Z[1])],
           5, 0.006, 0.04, along="v", jitter=0.008, seed=7)
    wb0, wb1 = 0.058, 0.088                # batten on the board faces (boards end at w 0.06..0.068)
    for z0, z1 in DOOR_BATTENS:
        wbox(bm, "front", a + 0.005, b - 0.005, wb0, wb1, z0, z1)
    door = obj_from_bm("PO_door", bm, col, [M("board")])
    # iron, unbevelled: strap hinges on the battens at the right edge, latch handle at the left edge
    bm = bmesh.new()
    for z0, z1 in DOOR_BATTENS:
        zm = (z0 + z1) / 2
        wbox(bm, "front", b - 0.42, b + 0.015, wb1 - 0.002, wb1 + 0.006, zm - 0.025, zm + 0.025)
    ha, hb, hz = a + DOOR_HANDLE[0], a + DOOR_HANDLE[1], DOOR_HANDLE[2]
    wbox(bm, "front", ha - 0.02, ha + 0.035, 0.056, 0.072, hz - 0.17, hz + 0.04)   # latch plate
    for u in (ha + 0.005, hb - 0.025):                                            # two feet
        wbox(bm, "front", u, u + 0.02, 0.068, 0.115, hz - 0.012, hz + 0.012)
    wbox(bm, "front", ha, hb, 0.11, 0.13, hz - 0.014, hz + 0.014)                 # grip bar
    wbox(bm, "front", ha + 0.002, ha + 0.016, 0.07, 0.095, hz - 0.15, hz - 0.03)   # thumb lever
    iron = obj_from_bm("PO_door_iron", bm, col, [M("iron")])

    bm = bmesh.new()
    px, pz, pw, ph = PLAQUE
    hw, hh = pw / 2, ph / 2
    # the plate stands PLAQUE_STANDOFF proud of the log crowns on two wooden furring strips
    t0 = R_LOG * 1.05 + PLAQUE_STANDOFF    # crowns of the largest logs (radius +5%)
    t1 = t0 + PLAQUE_T
    wbox(bm, "front", px - hw, px + hw, t0, t1, pz - hh, pz + hh, 0)
    for a_, b_, c_, d_ in ((px - hw + 0.003, px + hw - 0.003, pz + hh - 0.02, pz + hh - 0.003),
                           (px - hw + 0.003, px + hw - 0.003, pz - hh + 0.003, pz - hh + 0.02),
                           (px - hw + 0.003, px - hw + 0.02, pz - hh + 0.02, pz + hh - 0.02),
                           (px + hw - 0.02, px + hw - 0.003, pz - hh + 0.02, pz + hh - 0.02)):
        wbox(bm, "front", a_, b_, t1 - 0.002, t1 + 0.004, c_, d_, 1)
    for u in (px - hw + 0.08, px + hw - 0.13):
        wbox(bm, "front", u, u + 0.05, R_LOG - 0.04, t0 + 0.002, pz - hh + 0.03, pz + hh - 0.03, 2)
    plaque = obj_from_bm("PO_plaque", bm, col, [M("plaque"), M("frame"), M("board")])
    return door, iron, plaque


# ---------------------------------------------------------------- roof
def build_roof(col):
    s = slope()
    th = math.atan(s)
    hx = ROOF[0]
    cx, cy, cs, _ = CHIMNEY
    bm = bmesh.new()
    nT = ROOF_T / math.cos(th)
    for sgn in (-1, 1):
        d = Vector((0, sgn * math.cos(th), -math.sin(th)))      # down the slope
        n = Vector((0, sgn * math.sin(th), math.cos(th)))       # out of the roof
        a = Vector((0, -sgn * 0.03, zu(0) + 0.03 * s))          # underside line, past the ridge
        b = Vector((0, sgn * ROOF[1], EAVE_Z))                  # underside at the eave edge
        c = (a + b) / 2 + n * (ROOF_T / 2)
        beam(bm, (-hx, c.y, c.z), (hx, c.y, c.z), (b - a).length, ROOF_T, side=d)
        for x in SEAM_X:
            p0 = Vector((x, 0, zu(0))) + n * (ROOF_T + 0.012)
            p1 = Vector((x, sgn * (ROOF[1] - 0.02), zu(ROOF[1] - 0.02))) + n * (ROOF_T + 0.012)
            beam(bm, p0, p1, 0.03, 0.035)
    # ridge cap: chevron of folded iron, stopped at the chimney
    zt = RIDGE_Z + nT
    zl = zt - CAP_HALF * s - 0.005
    prof = [(-CAP_HALF, zl), (0, zt + CAP_RISE - 0.02), (CAP_HALF, zl),
            (CAP_HALF, zl + 0.02), (0, zt + CAP_RISE), (-CAP_HALF, zl + 0.02)]
    for x0, x1 in ((-hx - 0.01, cx - cs / 2 + 0.01), (cx + cs / 2 - 0.01, hx + 0.01)):
        prism(prof, x1 - x0, plane="YZ", offset=(x0 + x1) / 2, bm=bm)
    tri_ngons(bm)
    return obj_from_bm("PO_roof", bm, col, [M("roof")])


def build_roof_frame(col):
    """Rafters on wall plates and a ridge beam, barge rafters and barge boards at the gable
    overhang, gable sills on the end walls."""
    s = slope()
    th = math.atan(s)
    cx, cy, cs, _ = CHIMNEY
    bm = bmesh.new()
    rw, rd = RAFTER
    hy = WALL[1] / 2
    top_log = BASE_Z + R_LOG + (N_COURSES - 1) * COURSE + R_LOG
    for sgn in (-1, 1):
        n = Vector((0, sgn * math.sin(th), math.cos(th)))
        for x in RAFTER_X + [-BARGE_X, BARGE_X]:
            p0 = Vector((x, 0, zu(0) + 0.005)) - n * (rd / 2)
            p1 = Vector((x, sgn * RAFTER_END, zu(RAFTER_END) + 0.005)) - n * (rd / 2)
            beam(bm, p0, p1, rw, rd)
        # wall plate on the top log, out to the barge rafters
        zp = zu(hy) - rd / math.cos(th) + 0.01
        aabb(bm, (-BARGE_X - 0.045, sgn * hy - 0.06, top_log - 0.03), (BARGE_X + 0.045, sgn * hy + 0.06, zp))
    # barge boards: one chevron board per gable, top flush with the roof top
    nT, bhv = ROOF_T / math.cos(th), 0.18 / math.cos(th)
    ze, zr = EAVE_Z + nT, RIDGE_Z + nT
    ry = ROOF[1]
    prof = [(-ry, ze), (0, zr), (ry, ze), (ry, ze - bhv), (0, zr - bhv), (-ry, ze - bhv)]
    for sx in (-1, 1):
        prism(prof, 0.03, plane="YZ", offset=sx * (ROOF[0] - 0.035), bm=bm)
    # ridge beam, interrupted at the chimney
    for x0, x1 in ((-BARGE_X - 0.045, cx - cs / 2 - 0.05), (cx + cs / 2 + 0.05, BARGE_X + 0.045)):
        aabb(bm, (x0, -0.07, zu(0) - 0.16), (x1, 0.07, zu(0) + 0.002))
    # gable sills on the end walls' top logs
    for sx in (-1, 1):
        x = sx * WALL[0] / 2
        aabb(bm, (x - 0.1, -hy - R_LOG, GABLE_SILL_Z[0]), (x + 0.1, hy + R_LOG, GABLE_SILL_Z[1]))
    tri_ngons(bm)
    return obj_from_bm("PO_roof_frame", bm, col, [M("board")])


def build_gables(col):
    """Vertical boards on both gables cut to the roof line; on the right gable the boards stop
    round the gable window."""
    bm = bmesh.new()
    hy = WALL[1] / 2 + 0.13
    bw, gap = 0.20, 0.01
    n = int((2 * hy + gap) / (bw + gap))
    bw = (2 * hy + gap) / n - gap
    z0 = GABLE_SILL_Z[1] - 0.015
    gu0, gu1, gz0, gz1 = GABLE_WIN
    hole = (gu0 - 0.05, gu1 + 0.05, gz0 - 0.05, gz1 + 0.05)
    t = GABLE_W[1] - GABLE_W[0]

    def top(y):
        return max(zu(y) + 0.01, z0 + 0.02)

    for sx in (-1, 1):
        xc = sx * (GABLE_W[0] + GABLE_W[1]) / 2
        for i in range(n):
            a = -hy + i * (bw + gap)
            b = a + bw
            upper = [(b, top(b))] + ([(0, top(0))] if a < 0 < b else []) + [(a, top(a))]
            if sx > 0 and b > hole[0] and a < hole[1]:
                prism([(a, z0), (b, z0), (b, hole[2] + 0.01), (a, hole[2] + 0.01)], t, plane="YZ", offset=xc, bm=bm)
                prism([(a, hole[3] - 0.01), (b, hole[3] - 0.01)] + upper, t, plane="YZ", offset=xc, bm=bm)
                continue
            prism([(a, z0), (b, z0)] + upper, t, plane="YZ", offset=xc, bm=bm)
    tri_ngons(bm)
    return obj_from_bm("PO_gables", bm, col, [M("board")])


# ---------------------------------------------------------------- chimney
def build_chimney(col):
    """Brick stack from the attic through the ridge, corbelled band, hollow top."""
    cx, cy, cs, ctop = CHIMNEY
    bm = bmesh.new()
    h = cs / 2
    aabb(bm, (cx - h, cy - h, 4.45), (cx + h, cy + h, ctop - 0.35))
    aabb(bm, (cx - h - 0.04, cy - h - 0.04, ctop - 0.36), (cx + h + 0.04, cy + h + 0.04, ctop - 0.24))
    t, ho, zb = 0.11, h - 0.02, ctop - 0.25
    aabb(bm, (cx - ho, cy - ho, zb), (cx + ho, cy - ho + t, ctop))
    aabb(bm, (cx - ho, cy + ho - t, zb), (cx + ho, cy + ho, ctop))
    aabb(bm, (cx - ho, cy - ho + t, zb), (cx - ho + t, cy + ho - t, ctop))
    aabb(bm, (cx + ho - t, cy - ho + t, zb), (cx + ho, cy + ho - t, ctop))
    aabb(bm, (cx - ho + t - 0.01, cy - ho + t - 0.01, ctop - 0.2), (cx + ho - t + 0.01, cy + ho - t + 0.01, ctop - 0.17), 1)
    return obj_from_bm("PO_chimney", bm, col, [M("brick"), M("dark")])


# ---------------------------------------------------------------- porch
def build_porch(col):
    px0, px1, depth, ctop = PORCH
    hy = WALL[1] / 2
    yf, yl = -(hy + depth), LANDING_Y
    bm = bmesh.new()
    # posts from the ground to the canopy header
    for x in POST_X:
        aabb(bm, (x - 0.06, POST_Y - 0.06, 0.0), (x + 0.06, POST_Y + 0.06, 2.93))
    # landing: front beam through the posts, joists into the plinth, deck boards along Y
    aabb(bm, (px0, yl + 0.004, BASE_Z - 0.18), (px1, yl + 0.064, BASE_Z - 0.04))
    for x in (px0 + 0.04, (px0 + px1) / 2, px1 - 0.04):
        aabb(bm, (x - 0.03, yl + 0.01, BASE_Z - 0.16), (x + 0.03, -(PLINTH[1] - STONE_PROUD) + 0.01, BASE_Z - 0.03))
    zd = BASE_Z - 0.04
    yk = -(PLINTH[1] - STONE_PROUD) + 0.005     # deck boards end in the mortar core
    boards(bm, [(px0, yl, zd), (px1, yl, zd), (px1, yk, zd), (px0, yk, zd)],
           12, 0.008, 0.04, along="v", seed=3)
    # steps: three saw-cut stringers, treads, risers
    rise, run = BASE_Z / 4, (yl - yf) / 3
    prof = [(yf, 0.0)]
    for i in range(3):
        z = (i + 1) * rise - 0.04
        prof += [(yf + i * run, z), (yf + (i + 1) * run, z)]
    prof[-1] = (yl + 0.03, prof[-1][1])
    prof += [(yl + 0.03, 0.20), (yf + 0.30, 0.0)]
    for x in (px0 + 0.025, (px0 + px1) / 2, px1 - 0.025):
        prism(prof, 0.04, plane="YZ", offset=x, bm=bm)
    for i in range(3):
        ya, z = yf + i * run, (i + 1) * rise
        aabb(bm, (px0 - 0.02, ya - 0.02, z - 0.042), (px1 + 0.02, ya + run + 0.01, z))
        aabb(bm, (px0, ya - 0.012, z - rise + (0.003 if i else 0.0)), (px1, ya + 0.013, z - 0.045))
    # canopy: header on the posts, knee braces, bearers from a wall ledger, iron sheet
    def zc(y):
        return ctop + (y - POST_Y) * CANOPY_K
    th = math.atan(CANOPY_K)
    cx0, cx1 = px0 - 0.02, px1 + 0.10
    yw = -(hy + R_LOG) + 0.10
    bh = 0.10 / math.cos(th)
    aabb(bm, (cx0 + 0.04, POST_Y - 0.06, 2.92), (cx1 - 0.04, POST_Y + 0.06, zc(POST_Y) - 0.016 - bh + 0.01))
    for x, sx in ((POST_X[0], 1), (POST_X[1], -1)):
        beam(bm, (x, POST_Y, 2.50), (x + sx * 0.40, POST_Y, 2.95), 0.06, 0.06)
    for x in (cx0 + 0.06, (cx0 + cx1) / 2, cx1 - 0.06):
        p0 = Vector((x, yw, zc(yw) - 0.016 - bh / 2))
        p1 = Vector((x, yf - 0.01, zc(yf - 0.01) - 0.016 - bh / 2))
        beam(bm, p0, p1, 0.06, 0.10)
    yl0 = -(hy + R_LOG)
    lz1 = zc(yl0) - 0.016 - bh + 0.01
    aabb(bm, (cx0, yl0 - 0.06, lz1 - 0.14), (cx1, yl0 + 0.04, lz1))
    d = Vector((0, -math.cos(th), -math.sin(th)))
    n = Vector((0, -math.sin(th), math.cos(th)))
    a = Vector((0, yw - 0.02, zc(yw - 0.02)))
    b = Vector((0, yf - 0.06, zc(yf - 0.06)))
    c = (a + b) / 2 - n * 0.01
    beam(bm, (cx0, c.y, c.z), (cx1, c.y, c.z), (b - a).length, 0.02, side=d, mat_index=1)
    tri_ngons(bm)
    return obj_from_bm("PO_porch", bm, col, [M("board"), M("roof")])


CEIL_Z = 3.05         # низ потолка комнаты: выше окон торцов (верх наличника 3,01), ниже верхних венцов
FLOOR_BOARD = 0.24    # ширина половой и потолочной доски
CURTAIN_W = -0.135    # занавеска: за коробкой окна (её задняя грань на −0,12)
CURTAIN_FOLD = (0.07, 0.016)   # шаг и глубина складок занавески


def boards_xy(bm, z0, z1):
    """Настил по всей комнате: доски вдоль X с зазором 4 мм, концами на 5 см в брёвнах стен."""
    hx, hy = WALL[0] / 2 - R_LOG + 0.05, WALL[1] / 2 - R_LOG + 0.05
    n = int(2 * hy / FLOOR_BOARD)
    w = 2 * hy / n
    for i in range(n):
        aabb(bm, (-hx, -hy + i * w + 0.002, z0), (hx, -hy + (i + 1) * w - 0.002, z1), 0)


def curtain(bm, wall, u0, u1, z0, z1):
    """Задергушка: льняное полотно в складках от низа окна до горбыльков переплёта, на тонком прутке; два слоя
    полотна в 3 мм, чтобы ткань была видна и снаружи, и из комнаты."""
    step, depth = CURTAIN_FOLD
    n = max(4, round((u1 - u0) / step * 2))
    for layer, dw in ((0, 0.0), (1, -0.003)):
        cols = []
        for i in range(n + 1):
            u = u0 + (u1 - u0) * i / n
            w = CURTAIN_W + dw + depth * 0.5 * (1 + math.sin(math.pi * i / n * (u1 - u0) / step * 2))
            cols.append([bm.verts.new(wall_pt(wall, u, w, z)) for z in (z0, z1)])
        for i in range(n):
            a, b = cols[i], cols[i + 1]
            quad = [a[0], b[0], b[1], a[1]] if layer == 0 else [a[1], b[1], b[0], a[0]]
            f = bm.faces.new(quad)
            f.material_index = 1
    # пруток концами в коробке окна (её задняя грань на −0,12), полотно висит на нём
    wbox(bm, wall, u0 - 0.06, u1 + 0.06, CURTAIN_W - 0.006, -0.119, z1 - 0.004, z1 + 0.008, 2)


def build_interior(col):
    """Комната за окнами: пол из досок на уровне порога, дощатый потолок, на каждом окне — занавеска на прутке. Без
    неё за стеклом видна пустая коробка сруба насквозь."""
    bm = bmesh.new()
    boards_xy(bm, DOOR_Z[0] - 0.04, DOOR_Z[0])
    boards_xy(bm, CEIL_Z, CEIL_Z + 0.04)
    for wall, wins in WINDOWS.items():
        gz0, gz1 = FRONT_GLASS_Z if wall in ("front", "back") else SIDE_GLASS_Z
        for gu0, gu1 in wins:
            curtain(bm, wall, gu0 - 0.02, gu1 + 0.02, gz0 - 0.01, gz0 + 0.64 * (gz1 - gz0))
    bmesh.ops.recalc_face_normals(bm, faces=[f for f in bm.faces if f.material_index != 1])
    return obj_from_bm("PO_interior", bm, col, materials=[M("board"), M("curtain"), M("iron")])


def build(sc, col):
    rnd = random.Random(1930)
    objs = list(build_plinth(col)) + [build_logs(col, rnd), build_windows(col)]
    objs += list(build_door(col))
    objs += [build_roof(col), build_roof_frame(col), build_gables(col), build_chimney(col), build_porch(col)]
    objs += [build_interior(col)]
    # bevels by material on the hard pieces; logs are round, boards of gables, roof frame and window
    # trim stay sharp to keep the triangle budget (12 000)
    for name, width, seg in (("PO_chimney", 0.012, 1), ("PO_door", 0.006, 1), ("PO_porch", 0.008, 1)):
        add_bevel(bpy.data.objects[name], width, seg, "ANGLE", 30)
    for o in objs:
        # stones are rounded pillows: smooth across their facets, sharp only at the back
        finish_shading(o, 75 if o.name == "PO_stones" else 40)
    stones = bpy.data.objects["PO_stones"]
    stones.modifiers.remove(stones.modifiers["WeightedNormal"])   # plain smooth: no flat facets
