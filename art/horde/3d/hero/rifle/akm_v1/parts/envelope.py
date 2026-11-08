from collections import Counter, defaultdict, deque

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.geometry import delaunay_2d_cdt
from mathutils.kdtree import KDTree

TRACE_TOLERANCE_PX = 1.0


def main_component(mask: np.ndarray) -> np.ndarray:
    first = mask[:-1, :-1] & mask[1:, 1:] & ~mask[:-1, 1:] & ~mask[1:, :-1]
    second = mask[:-1, 1:] & mask[1:, :-1] & ~mask[:-1, :-1] & ~mask[1:, 1:]
    mask = mask.copy()
    mask[:-1, 1:] |= first
    mask[:-1, :-1] |= second
    height, width = mask.shape
    found = np.zeros_like(mask)
    pending = deque([(1300, 150)])
    found[150, 1300] = True
    while pending:
        x, y = pending.popleft()
        for xx, yy in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
            if 0 <= xx < width and 0 <= yy < height and mask[yy, xx] and not found[yy, xx]:
                found[yy, xx] = True
                pending.append((xx, yy))
    return found


def boundary_edges(mask: np.ndarray) -> dict:
    padded = np.pad(mask, 1)
    sides = (padded[:-2, 1:-1], padded[1:-1, 2:],
             padded[2:, 1:-1], padded[1:-1, :-2])
    offsets = (((0, 0), (1, 0)), ((1, 0), (1, 1)),
               ((1, 1), (0, 1)), ((0, 1), (0, 0)))
    edges = defaultdict(list)
    for neighbor, (a, b) in zip(sides, offsets):
        ys, xs = np.nonzero(mask & ~neighbor)
        for x, y in zip(xs.tolist(), ys.tolist()):
            edges[(x + a[0], y + a[1])].append((x + b[0], y + b[1]))
    return edges


def trace_loops(mask: np.ndarray) -> list:
    edges = boundary_edges(mask)
    loops = []
    while edges:
        start = next(iter(edges))
        point = start
        loop = []
        while True:
            loop.append(point)
            targets = edges[point]
            target = targets.pop()
            if not targets:
                del edges[point]
            point = target
            if point == start:
                break
        area = sum(a[0] * b[1] - a[1] * b[0]
                   for a, b in zip(loop, loop[1:] + loop[:1])) / 2
        if abs(area) >= 12:
            loops.append(loop)
    return loops


def simplify_line(points: np.ndarray, epsilon: float) -> np.ndarray:
    if len(points) <= 2:
        return points
    edge = points[-1] - points[0]
    norm = np.linalg.norm(edge)
    if norm < 1e-9:
        distance = np.linalg.norm(points - points[0], axis=1)
    else:
        delta = points - points[0]
        distance = np.abs(delta[:, 0] * edge[1] - delta[:, 1] * edge[0]) / norm
    split = int(distance.argmax())
    if distance[split] <= epsilon:
        return points[[0, -1]]
    left = simplify_line(points[:split + 1], epsilon)
    right = simplify_line(points[split:], epsilon)
    return np.concatenate((left[:-1], right))


def simplify_loop(loop: list) -> list:
    points = np.asarray(loop, dtype=float)
    split = int(np.linalg.norm(points - points[0], axis=1).argmax())
    first = simplify_line(points[:split + 1], TRACE_TOLERANCE_PX)
    second = simplify_line(np.concatenate((points[split:], points[:1])), TRACE_TOLERANCE_PX)
    return np.concatenate((first[:-1], second[:-1])).tolist()


def inside_pixel(mask: np.ndarray, x: float, y: float) -> bool:
    height, width = mask.shape
    return 0 <= int(y) < height and 0 <= int(x) < width and bool(mask[int(y), int(x)])


def contour_samples(contour: list) -> list:
    samples = []
    for a, b in zip(contour, contour[1:] + contour[:1]):
        start, end = np.asarray(a), np.asarray(b)
        delta = end - start
        length = np.linalg.norm(delta)
        count = max(1, int(np.ceil(length / 14)))
        normal = np.array((-delta[1], delta[0])) / length
        for i in range(count):
            point = start + delta * (i + 0.5) / count
            for distance in (1, 3, 6, 10, 16, 24, 34):
                samples.append(point + normal * distance)
    return samples


def contour_tree(contours: list) -> KDTree:
    samples = []
    for contour in contours:
        for a, b in zip(contour, contour[1:] + contour[:1]):
            start, end = np.asarray(a), np.asarray(b)
            count = max(1, int(np.ceil(np.linalg.norm(end - start))))
            samples.extend(start + (end - start) * i / count for i in range(count))
    tree = KDTree(len(samples))
    for i, point in enumerate(samples):
        tree.insert((point[0], point[1], 0), i)
    tree.balance()
    return tree


def outline_membership(points: np.ndarray, contours: list) -> np.ndarray:
    x, y = points[:, 0], points[:, 1]
    inside = np.zeros(len(points), dtype=bool)
    for contour in contours:
        for (ax, ay), (bx, by) in zip(contour, contour[1:] + contour[:1]):
            if abs(by - ay) < 1e-12:
                continue
            crossing = ((ay > y) != (by > y)) & (x < (bx - ax) * (y - ay) / (by - ay) + ax)
            inside ^= crossing
    return inside


def planar_mesh(mask: np.ndarray) -> tuple:
    points, edges = [], []
    contours = [simplify_loop(loop) for loop in trace_loops(mask)]
    for contour in contours:
        offset = len(points)
        points.extend(contour)
        edges.extend((offset + i, offset + (i + 1) % len(contour))
                     for i in range(len(contour)))
    for contour in contours:
        points.extend(p for p in contour_samples(contour) if inside_pixel(mask, *p))
    for y in range(65, IMAGE_HEIGHT - 20, 30):
        for x in range(25, IMAGE_WIDTH - 20, 35):
            if mask[y - 3:y + 4, x - 3:x + 4].all():
                points.append((x, y))
    verts, _, triangles, *_ = delaunay_2d_cdt(
        [Vector(p) for p in points], edges, [], 0, 0.03, False)
    centers = np.asarray([tuple(sum((verts[i] for i in t), Vector((0, 0))) / len(t))
                          for t in triangles])
    membership = outline_membership(centers, contours)
    faces = [t for t, inside in zip(triangles, membership) if inside]
    used = sorted({i for t in faces for i in t})
    mapping = {old: new for new, old in enumerate(used)}
    return ([verts[i] for i in used], [tuple(mapping[i] for i in t) for t in faces],
            contour_tree(contours))


def blend(a: float, b: float, t: float) -> float:
    weight = min(1.0, max(0.0, t))
    weight = weight * weight * (3 - 2 * weight)
    return a * (1 - weight) + b * weight


def half_width(x: float, y: float) -> float:
    front = blend(0.006, 0.017, (x - 580) / 60)
    body = blend(front, 0.018, (x - 970) / 80)
    base = blend(body, 0.016, (x - 1570) / 80)
    guard = blend(0.010, 0.004, (x - 1240) / 45)
    grip = blend(guard, 0.014, (x - 1380) / 60)
    lower = blend(grip, 0.016, (x - 1580) / 60)
    return blend(base, lower, (y - 255) / 50)


def shoulder_radius(x: float, y: float) -> float:
    if x < 580:
        return 0.006
    if 1000 < x < 1600 and y < 170:
        return 0.014
    if y > 300 and x < 1250:
        return 0.005
    return 0.008


def rounded_width(point, tree: KDTree, boundary: bool) -> float:
    if boundary:
        return 0.001
    distance = tree.find((point.x, point.y, 0))[2] / PPM
    fraction = min(1.0, distance / shoulder_radius(point.x, point.y))
    curve = np.sqrt(max(0.0, 2 * fraction - fraction * fraction))
    return 0.001 + (half_width(point.x, point.y) - 0.001) * curve


def bake_finish(obj) -> None:
    evaluated = obj.evaluated_get(depsgraph_of([obj]))
    old_mesh = obj.data
    mesh = bpy.data.meshes.new_from_object(evaluated)
    obj.modifiers.clear()
    obj.data = mesh
    if old_mesh.users == 0:
        bpy.data.meshes.remove(old_mesh)
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.remove_doubles(bm, verts=list(bm.verts), dist=1e-8)
    bmesh.ops.dissolve_degenerate(bm, edges=list(bm.edges), dist=1e-8)
    bmesh.ops.triangulate(bm, faces=list(bm.faces))
    bmesh.ops.dissolve_degenerate(bm, edges=list(bm.edges), dist=1e-8)
    bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
    bm.to_mesh(mesh)
    bm.free()
    mesh.update()


def extruded_mesh(points: list, triangles: list, tree: KDTree) -> tuple:
    edge_count = Counter(tuple(sorted((a, b))) for t in triangles
                         for a, b in zip(t, t[1:] + t[:1]))
    boundary = {i for edge, amount in edge_count.items() if amount == 1 for i in edge}
    widths = [rounded_width(p, tree, i in boundary) for i, p in enumerate(points)]
    vertices = []
    for sign in (-1, 1):
        vertices.extend((sign * width,
                         (p.x - ANCHOR_PX[0]) / PPM,
                         (ANCHOR_PX[1] - p.y) / PPM) for p, width in zip(points, widths))
    count = len(points)
    faces = [tuple(reversed(t)) for t in triangles]
    faces.extend(tuple(i + count for i in t) for t in triangles)
    for (a, b), amount in edge_count.items():
        if amount == 1:
            faces.append((a, b, b + count, a + count))
    return vertices, faces


def build(sc, col) -> None:
    pixels = load_rgba(REFERENCE)
    mask = main_component(pixels[:, :, 3] > 0.5)
    points, triangles, tree = planar_mesh(mask)
    vertices, faces = extruded_mesh(points, triangles, tree)
    mesh = bpy.data.meshes.new(PREFIX + "_envelope_mesh")
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(PREFIX + "_envelope", mesh)
    col.objects.link(obj)
    obj.data.materials.append(M("shell"))
    obj["source_opaque_pixels"] = int((pixels[:, :, 3] > 0.5).sum())
    obj["used_opaque_pixels"] = int(mask.sum())
    finish_shading(obj, 40)
    bake_finish(obj)
