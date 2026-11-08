# Модель целиком такой, какой её получает игра: импорт glb. Цвет вершин (запечённое затенение) умножается
# на цвет материала, как в движке. Править форму и материалы — в скрипте игры art/transport/models/, не здесь.


def build(sc, col):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=GLB)
    new = [o for o in bpy.data.objects if o not in before]
    for o in new:
        for c in list(o.users_collection):
            c.objects.unlink(o)
        col.objects.link(o)
        o.name = f"{PREFIX}_{o.name}"
        if o.type != "MESH" or not o.data.color_attributes:
            continue
        attr = o.data.color_attributes[0].name
        for slot in o.material_slots:
            m = slot.material
            if not m or not m.use_nodes or m.get("bm_vc"):
                continue
            nt = m.node_tree
            b = next((n for n in nt.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled"), None)
            if b is None:
                continue
            sock = b.inputs["Base Color"]
            vc = nt.nodes.new("ShaderNodeVertexColor")
            vc.layer_name = attr
            mix = nt.nodes.new("ShaderNodeMix")
            mix.data_type = "RGBA"
            mix.blend_type = "MULTIPLY"
            mix.inputs["Factor"].default_value = 1.0
            if sock.links:
                nt.links.new(sock.links[0].from_socket, mix.inputs["A"])
            else:
                mix.inputs["A"].default_value = sock.default_value
            nt.links.new(vc.outputs["Color"], mix.inputs["B"])
            nt.links.new(mix.outputs["Result"], sock)
            m["bm_vc"] = 1
