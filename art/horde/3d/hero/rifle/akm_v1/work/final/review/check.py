import bpy, json, os, struct
from pathlib import Path
import numpy as np
BM = r'C:\Users\lopat\.codex\plugins\cache\anton-toolkit-marketplace\blender-modeling\0.2.3-codex.ha8b4c4edceb3\scripts'
exec(open(BM + '/bm.py', encoding='utf-8').read())
ASSET = r'C:\projects\kuznya-mirov\art\horde\3d\hero\rifle\akm_v1'
OUT = ASSET + r'\work\final\review'
load_dims(ASSET)
sc = bpy.data.scenes[SCENE]
names = model_names(sc)
gates = whole_gates(sc, OUT)
raw_mesh = load_rgba(OUT + r'\sil_right.png')[:, :, 3] > .5
reference = load_rgba(REFERENCE)[:, :, 3] > .5
reference = resize_nearest(reference, raw_mesh.shape[1], raw_mesh.shape[0])
holes = fill_holes(reference) & ~reference
raw_iou = (raw_mesh & reference).sum() / (raw_mesh | reference).sum()
mesh = bpy.data.objects[names[0]].data
uv = mesh.uv_layers.active
uvs = np.array([loop.uv[:] for loop in uv.data])
image_nodes = [n for m in mesh.materials for n in m.node_tree.nodes if n.type == 'TEX_IMAGE']
images = [{'name':n.image.name,'size':list(n.image.size),'packed':bool(n.image.packed_file)} for n in image_nodes]
raw = Path(ASSET, 'akm-v1.glb').read_bytes()
size = struct.unpack_from('<I',raw,12)[0]
glb = json.loads(raw[20:20+size])
report = {
    'gates':gates,
    'raw_silhouette_iou':float(raw_iou),
    'reference_opening_pixels':int(holes.sum()),
    'filled_reference_opening_pixels':int((raw_mesh & holes).sum()),
    'vertices':len(mesh.vertices),'edges':len(mesh.edges),'faces':len(mesh.polygons),
    'euler_characteristic':len(mesh.vertices)-len(mesh.edges)+len(mesh.polygons),
    'uv_finite':bool(np.isfinite(uvs).all()),'uv_min':uvs.min(0).tolist(),'uv_max':uvs.max(0).tolist(),
    'image_nodes':images,
    'glb':glb_report(str(Path(ASSET,'akm-v1.glb'))),
    'glb_nodes':[{'name':n.get('name'),'translation':n.get('translation')} for n in glb['nodes']],
    'embedded_images':glb.get('images'),
}
Path(OUT,'independent-check.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
print('REVIEW ' + json.dumps(report))
os._exit(0)
