# Отделка построек порта по способу моделей Транспортной компании (хлебозавод, гараж, двор): фото библиотеки
# art/transport/textures плиткой в настоящем размере — цвет, шероховатость с металлом, рельеф, затенение карт; своя
# развёртка у каждой детали: вдоль бревна, бруса и доски, по окружности бревна, срез с кольцами на торцах брёвен,
# по плоскости грани у камня, железа и листов; тон детали, затенение углов и потемнение к земле — в цвет вершин.
# Форма берётся из work/calib.blend постройки (части плагина blender-modeling), какой материал у какой роли — из
# look.py в папке постройки.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup
#     <постройка>\work\calib.blend --python art/transport/port/surface.py -- <папка постройки>
# Пишет games/transport/models/<имя>.glb (то, что читает игра) и <папка>\work\surface\<имя>.blend (то же с цветом
# вершин в материалах — для показа в Blender).
import math
import os
import random
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
TEXTURES = os.path.normpath(os.path.join(HERE, "..", "textures"))
OUT = os.path.normpath(os.path.join(HERE, "..", "..", "..", "games", "transport", "models"))
sys.path.insert(0, os.path.normpath(os.path.join(HERE, "..", "models")))

import lib  # noqa: E402

END_DISC = (0.5, 0.5, 0.42)    # середина и радиус среза на картинке торца по умолчанию (доли стороны); у роли — disc
GRIME = 1.4                    # до этой высоты над низом постройки стены темнеют к земле


def read(path, names):
    """Имена из файла настроек (dims.py, look.py); `mat` в dims.py не нужен — подменяется заглушкой."""
    g = {"mat": lambda *a, **k: None}
    exec(compile(open(path, encoding="utf-8").read(), path, "exec"), g)
    return {n: g[n] for n in names if n in g}


def gltf_output():
    group = bpy.data.node_groups.get("glTF Material Output")
    if group is None:
        group = bpy.data.node_groups.new("glTF Material Output", "ShaderNodeTree")
        group.interface.new_socket("Occlusion", in_out="INPUT", socket_type="NodeSocketFloat")
    return group


def material(name, spec):
    """Материал роли: фото `tex` (папка библиотеки) умножено на `tint`, или ровный цвет `color` без фото."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nodes, links = m.node_tree.nodes, m.node_tree.links
    bsdf = lib.bsdf(m)
    if "tex" not in spec:
        bsdf.inputs["Base Color"].default_value = (*lib.hex_color(spec["color"]), 1.0)
        bsdf.inputs["Roughness"].default_value = spec.get("rough", 0.85)
        bsdf.inputs["Metallic"].default_value = spec.get("metal", 0.0)
        return m

    def image(file, data):
        node = nodes.new("ShaderNodeTexImage")
        node.image = bpy.data.images.load(os.path.join(TEXTURES, spec["tex"], file), check_existing=True)
        if data:
            node.image.colorspace_settings.name = "Non-Color"
        return node

    color = image(spec.get("file", "color.jpg"), False)
    if spec.get("alpha"):
        # стекло: прозрачность из альфы картинки (налёт пыли плотнее), смешивание — как у прозрачных частей игры
        links.new(color.outputs["Alpha"], bsdf.inputs["Alpha"])
        m.surface_render_method = "BLENDED"
    mix = nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs[0].default_value = 1.0
    links.new(color.outputs["Color"], mix.inputs[6])
    mix.inputs[7].default_value = (*lib.hex_color(spec.get("tint", "#ffffff")), 1.0)
    links.new(mix.outputs[2], bsdf.inputs["Base Color"])
    # arm_matte.jpg — та же карта, сдвинутая в матовое: шероховатость старого дерева, камня, кирпича
    matte = os.path.exists(os.path.join(TEXTURES, spec["tex"], "arm_matte.jpg"))
    arm = image(spec.get("arm", "arm_matte.jpg" if matte else "arm.jpg"), True)
    channels = nodes.new("ShaderNodeSeparateColor")
    links.new(arm.outputs["Color"], channels.inputs["Color"])
    links.new(channels.outputs["Green"], bsdf.inputs["Roughness"])
    links.new(channels.outputs["Blue"], bsdf.inputs["Metallic"])
    relief = image("normal.jpg", True)
    normal_map = nodes.new("ShaderNodeNormalMap")
    normal_map.inputs["Strength"].default_value = spec.get("relief", 1.0)
    links.new(relief.outputs["Color"], normal_map.inputs["Color"])
    links.new(normal_map.outputs["Normal"], bsdf.inputs["Normal"])
    out = nodes.new("ShaderNodeGroup")
    out.node_tree = gltf_output()
    links.new(channels.outputs["Red"], out.inputs["Occlusion"])
    return m


def joined(sc, prefix):
    """Все видимые части постройки одной сеткой в осях мира, со всеми модификаторами (фаски)."""
    deps = bpy.context.evaluated_depsgraph_get()
    parts = []
    for c in sc.collection.children:
        if "_part_" in c.name and not c.hide_render:
            for o in c.all_objects:
                if o.type == 'MESH':
                    me = bpy.data.meshes.new_from_object(o.evaluated_get(deps))
                    me.transform(o.matrix_world)
                    copy = bpy.data.objects.new(o.name + "_s", me)
                    sc.collection.objects.link(copy)
                    parts.append(copy)
    bpy.context.window.scene = sc
    for o in sc.objects:
        o.select_set(False)
    for o in parts:
        o.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    obj.name = prefix
    obj.data.name = prefix
    return obj


def islands(bm):
    """Детали сетки: грани, связанные рёбрами."""
    seen, out = set(), []
    for f in bm.faces:
        if f.index in seen:
            continue
        stack, group = [f], []
        seen.add(f.index)
        while stack:
            g = stack.pop()
            group.append(g)
            for e in g.edges:
                for h in e.link_faces:
                    if h.index not in seen:
                        seen.add(h.index)
                        stack.append(h)
        out.append(group)
    return out


def frame(faces):
    """Середина, оси по убыванию разброса и разброс по ним у вершин детали."""
    pts = np.array(list({v.index: tuple(v.co) for f in faces for v in f.verts}.values()))
    c = pts.mean(0)
    vals, vecs = np.linalg.eigh(np.cov((pts - c).T) if len(pts) > 3 else np.eye(3))
    order = np.argsort(vals)[::-1]
    return Vector(c), [Vector(vecs[:, i]) for i in order], np.sqrt(np.maximum(vals[order], 0.0)), pts


def unwrap(bm, roles, spec, rnd):
    """Развёртка и материал торцов по деталям: координаты карты по углам каждой грани, роль каждой грани и тон
    детали для цвета вершин."""
    tone, uvs, mat = {}, {}, {}
    for faces in islands(bm):
        role = roles[faces[0].material_index]
        s = spec.get(role, {})
        meters = s.get("meters", 2.0)
        c, axes, spread, pts = frame(faces)
        a1, a2, a3 = axes
        long = spread[0] > 2.0 * max(spread[1], 1e-6)
        rel = pts - np.array(c)
        radius = float(np.sqrt((rel @ np.array(a2)) ** 2 + (rel @ np.array(a3)) ** 2).max()) if long else 0.0
        side = [f for f in faces if abs(f.normal.dot(a1)) < 0.3]
        round_ = long and s.get("round", False) and len(side) >= 8
        off = (rnd.random(), rnd.random())
        tint = s.get("tones", ["#ffffff"])
        t = lib.hex_color(rnd.choice(tint))
        # торец бревна — один срез на весь торец (он бывает из нескольких граней): середина, радиус, поворот
        caps = {}
        if round_ and "end" in s:
            for sign in (1, -1):
                cap = [f for f in faces if f.normal.dot(a1) * sign > 0.85]
                if not cap:
                    continue
                verts = {v.index: v.co for f in cap for v in f.verts}
                cc = sum(verts.values(), Vector()) / len(verts)
                ang = rnd.random() * 2 * math.pi
                e1 = a2 * math.cos(ang) + a3 * math.sin(ang)
                info = (cc, e1, a1.cross(e1), max((p - cc).length for p in verts.values()) or 1.0,
                        lib.hex_color(rnd.choice(spec[s["end"]].get("tones", ["#ffffff"]))))
                for f in cap:
                    caps[f.index] = info
        for f in faces:
            tone[f.index] = t
            n = f.normal
            if long and abs(n.dot(a1)) > 0.85:
                if f.index in caps:
                    cc, e1, e2, r, end_tone = caps[f.index]
                    mat[f.index] = roles.index(s["end"])
                    tone[f.index] = end_tone
                    ux, uy, ur = spec[s["end"]].get("disc", END_DISC)
                    uvs[f.index] = [(ux + ur * (lp.vert.co - cc).dot(e1) / r, uy + ur * (lp.vert.co - cc).dot(e2) / r)
                                    for lp in f.loops]
                    continue
                uvs[f.index] = [((lp.vert.co - c).dot(a2) / meters + off[0], (lp.vert.co - c).dot(a3) / meters + off[1])
                                for lp in f.loops]
                continue
            if round_:
                angles = []
                for loop in f.loops:
                    d = loop.vert.co - c
                    angles.append(math.atan2(d.dot(a3), d.dot(a2)))
                if max(angles) - min(angles) > math.pi:
                    angles = [x + 2 * math.pi if x < 0 else x for x in angles]
                uvs[f.index] = [((lp.vert.co - c).dot(a1) / meters + off[0], ang * radius / meters + off[1])
                                for lp, ang in zip(f.loops, angles)]
            elif long:
                tv = n.cross(a1)
                tv = tv.normalized() if tv.length > 1e-6 else a2
                uvs[f.index] = [((lp.vert.co - c).dot(a1) / meters + off[0], (lp.vert.co - c).dot(tv) / meters + off[1])
                                for lp in f.loops]
            else:
                t1 = Vector((0, 0, 1)).cross(n)
                t1 = t1.normalized() if t1.length > 0.1 else Vector((1, 0, 0))
                t2 = n.cross(t1)
                if s.get("slope"):
                    t1, t2 = t2, t1
                uvs[f.index] = [(lp.vert.co.dot(t1) / meters + off[0], lp.vert.co.dot(t2) / meters + off[1])
                                for lp in f.loops]
    return tone, uvs, mat


def occlusion(obj, tone, base_z, wet=None):
    """Затенение углов, запечённое Cycles в цвет вершин над плоскостью земли, потемнение к земле, тон детали;
    `wet` — (высота воды, цвет мокрого, ширина перехода): ниже воды всё умножено на цвет мокрого."""
    sc = bpy.context.scene
    sc.render.engine = "CYCLES"
    sc.cycles.device = "CPU"
    sc.cycles.samples = 64
    world = bpy.data.worlds.new("surface_bake")
    world.light_settings.distance = 2.0
    sc.world = world
    bpy.ops.mesh.primitive_plane_add(size=400.0, location=(0.0, 0.0, base_z - 0.002))
    plane = bpy.context.active_object
    attr = obj.data.color_attributes.new("occlusion", "BYTE_COLOR", "CORNER")
    obj.data.color_attributes.active_color = attr
    obj.data.color_attributes.render_color_index = obj.data.color_attributes.active_color_index
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    sc.render.bake.target = "VERTEX_COLORS"
    bpy.ops.object.bake(type="AO")
    mesh = obj.data
    for poly in mesh.polygons:
        t = tone.get(poly.index, (1.0, 1.0, 1.0))
        up = abs(poly.normal.z) > 0.5
        for li in poly.loop_indices:
            co = mesh.vertices[mesh.loops[li].vertex_index].co
            ao = attr.data[li].color[0]
            h = min(1.0, max(0.0, (co.z - base_z) / GRIME))
            v = (0.55 + 0.45 * ao) * (1.0 if up else 0.8 + 0.2 * h * h * (3 - 2 * h))
            w = (1.0, 1.0, 1.0)
            if wet:
                k = min(1.0, max(0.0, (wet[0] + wet[2] - co.z) / wet[2]))
                w = tuple(1.0 + (c - 1.0) * k for c in lib.hex_color(wet[1]))
            attr.data[li].color = (t[0] * v * w[0], t[1] * v * w[1], t[2] * v * w[2], 1.0)
    bpy.data.objects.remove(plane)


def preview(materials):
    """Для показа в Blender: цвет материала умножен на цвет вершин, как это делает игра (COLOR_0 по glTF)."""
    for m in materials:
        nodes, links = m.node_tree.nodes, m.node_tree.links
        bsdf = lib.bsdf(m)
        src = bsdf.inputs["Base Color"].links[0].from_socket if bsdf.inputs["Base Color"].links else None
        attr = nodes.new("ShaderNodeVertexColor")
        attr.layer_name = "occlusion"
        mul = nodes.new("ShaderNodeMix")
        mul.data_type = "RGBA"
        mul.blend_type = "MULTIPLY"
        mul.inputs[0].default_value = 1.0
        if src is None:
            mul.inputs[6].default_value = bsdf.inputs["Base Color"].default_value
        else:
            links.new(src, mul.inputs[6])
        links.new(attr.outputs["Color"], mul.inputs[7])
        links.new(mul.outputs[2], bsdf.inputs["Base Color"])


def main():
    asset = sys.argv[sys.argv.index("--") + 1]
    dims = read(os.path.join(asset, "dims.py"), ["SCENE"])
    look = read(os.path.join(asset, "look.py"), ["NAME", "ROLES", "SIGNS", "WET"])
    sc = bpy.data.scenes[dims["SCENE"]]
    # calib.blend — копия всего открытого файла: другие сцены (порт с прошлой отделкой) заняли бы имена
    bpy.context.window.scene = sc
    for other in [s for s in bpy.data.scenes if s is not sc]:
        bpy.data.scenes.remove(other)
    bpy.data.orphans_purge(do_local_ids=True, do_linked_ids=True, do_recursive=True)
    spec = look["ROLES"]
    obj = joined(sc, look["NAME"])
    slot_role = [s.material.name[len(dims["SCENE"]) + 1:] for s in obj.material_slots]
    roles = list(dict.fromkeys(slot_role + [s["end"] for s in spec.values() if "end" in s]))
    missing = [r for r in roles if r not in spec]
    if missing:
        raise ValueError(f"look.py has no ROLES for {missing}")
    rnd = random.Random(1930)
    mesh = obj.data
    face_role = [roles.index(slot_role[p.material_index]) for p in mesh.polygons]
    # сетку не пересобираем через bmesh: сглаживание граней (свои нормали частей) осталось бы потерянным
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bm.faces.ensure_lookup_table()
    for f in bm.faces:
        f.material_index = face_role[f.index]
    tone, uvs, ends = unwrap(bm, roles, spec, rnd)
    bm.free()
    for i, r in ends.items():
        face_role[i] = r
    while mesh.uv_layers:
        mesh.uv_layers.remove(mesh.uv_layers[0])
    layer = mesh.uv_layers.new(name="UVMap")
    mesh.uv_layers.active = layer
    for p in mesh.polygons:
        for k, li in enumerate(p.loop_indices):
            layer.data[li].uv = uvs[p.index][k]
    mesh.materials.clear()
    mats = [material(f"{look['NAME']}_{r}", spec[r]) for r in roles]
    for m in mats:
        mesh.materials.append(m)
    mesh.polygons.foreach_set("material_index", face_role)
    mesh.update()
    base_z = min(v.co.z for v in mesh.vertices)
    occlusion(obj, tone, base_z, look.get("WET"))
    for words, size, (x, _, z), rot, color, on in look.get("SIGNS", []):
        front = min(mesh.vertices[v].co.y for p in mesh.polygons if roles[p.material_index] == on for v in p.vertices)
        lib.text(obj, words, size, (x, front - 0.004, z), rot, color, depth=0.003)
    for o in list(bpy.data.objects):
        if o is not obj and o.parent is not obj:
            bpy.data.objects.remove(o, do_unlink=True)
    out_glb = os.path.join(OUT, look["NAME"] + ".glb")
    keep = bpy.context.window.scene
    for o in (obj, *obj.children):
        o.select_set(True)
    bpy.ops.export_scene.gltf(filepath=out_glb, export_format="GLB", export_yup=True, export_apply=True,
                              export_vertex_color="ACTIVE", use_selection=True)
    bpy.context.window.scene = keep
    preview(mats)
    work = os.path.join(asset, "work", "surface")
    os.makedirs(work, exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(work, look["NAME"] + ".blend"), copy=True)
    print("RESULT", {"glb": out_glb, "faces": len(obj.data.polygons), "materials": roles})


main()
