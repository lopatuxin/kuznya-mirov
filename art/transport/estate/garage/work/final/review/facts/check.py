import bpy, json, struct, numpy as np
from mathutils import Vector
res = {}
o = bpy.data.objects["EG_garage"]
me = o.data
mw = o.matrix_world
co = np.array([ (mw @ v.co)[:] for v in me.vertices])
i = int(co[:,2].argmin())
res["minz"] = co[i].round(4).tolist()
# faces using that vertex
mats = set()
for p in me.polygons:
    if i in p.vertices:
        mats.add(me.materials[p.material_index].name)
res["minz_mats"] = sorted(mats)
low = np.where(co[:,2] < -0.0005)[0]
res["below_count"] = int(len(low))
res["below_pts"] = co[low][:20].round(3).tolist()
# materials
mi = {}
for m in me.materials:
    d = {"blend": m.surface_render_method if hasattr(m,'surface_render_method') else None}
    imgs = [n.image for n in m.node_tree.nodes if n.bl_idname=="ShaderNodeTexImage" and n.image]
    d["imgs"] = [(im.name, im.size[0], im.size[1]) for im in imgs]
    b = next((n for n in m.node_tree.nodes if n.bl_idname=="ShaderNodeBsdfPrincipled"), None)
    if b:
        d["alpha_linked"] = b.inputs["Alpha"].is_linked
        d["alpha"] = b.inputs["Alpha"].default_value
        d["metal_linked"] = b.inputs["Metallic"].is_linked
        d["metal"] = b.inputs["Metallic"].default_value
        d["rough"] = b.inputs["Roughness"].default_value
        d["rough_linked"] = b.inputs["Roughness"].is_linked
        if not b.inputs["Base Color"].is_linked:
            d["color"] = list(b.inputs["Base Color"].default_value)[:3]
    mi[m.name] = d
res["materials"] = mi
# vertex color stats per material
ca = me.color_attributes[0]
res["color_attr"] = (ca.name, ca.domain, ca.data_type)
cols = np.array([c.color[:] for c in ca.data])
per = {}
for p in me.polygons:
    mn = me.materials[p.material_index].name
    for li in p.loop_indices:
        idx = li if ca.domain=='CORNER' else me.loops[li].vertex_index
        per.setdefault(mn, []).append(cols[idx])
res["vc"] = {k: {"min": np.array(v).min(0).round(3).tolist(), "mean": np.array(v).mean(0).round(3).tolist(), "max": np.array(v).max(0).round(3).tolist(), "n": len(v)} for k, v in per.items()}
# glb json
data = open(GLB, "rb").read()
ln = struct.unpack("<I", data[12:16])[0]
js = json.loads(data[20:20+ln])
res["glb_materials"] = [{k: m.get(k) for k in ("name","alphaMode","doubleSided")} | {"pbr": {k: v for k, v in m.get("pbrMetallicRoughness", {}).items() if k != "baseColorTexture" and k!="metallicRoughnessTexture"}, "has_mr_tex": "metallicRoughnessTexture" in m.get("pbrMetallicRoughness", {})} for m in js["materials"]]
res["glb_attrs"] = [list(p["attributes"].keys()) for msh in js["meshes"] for p in msh["primitives"]][:3]
acc = js["accessors"]
c0 = [acc[p["attributes"]["COLOR_0"]]["type"] for msh in js["meshes"] for p in msh["primitives"] if "COLOR_0" in p["attributes"]]
res["color0_types"] = sorted(set(c0))
res["images"] = [im.get("name") or im.get("uri") for im in js.get("images", [])]
res["sign_bbox"] = [ (bpy.data.objects["EG_garage_sign"].matrix_world @ Vector(b))[:] for b in bpy.data.objects["EG_garage_sign"].bound_box][::7]
result = res
