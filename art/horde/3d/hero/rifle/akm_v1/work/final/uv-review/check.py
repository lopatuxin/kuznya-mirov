import hashlib
import json
import struct
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(r'C:/projects/kuznya-mirov/art/horde/3d/hero/rifle/akm_v1')
OUT = ROOT / 'work/final/uv-review'
BM = r'C:/Users/lopat/.codex/plugins/cache/anton-toolkit-marketplace/blender-modeling/0.2.3-codex.ha8b4c4edceb3/scripts'
exec(Path(BM, 'bm.py').read_text(encoding='utf-8'))
load_dims(str(ROOT))
sc = bpy.data.scenes[SCENE]
bpy.context.window.scene = sc
obj = bpy.data.objects[PREFIX + '_envelope']
mesh = obj.data
raw = (ROOT / 'akm-v1.glb').read_bytes()
chunk_size = struct.unpack_from('<I', raw, 12)[0]
glb = json.loads(raw[20:20 + chunk_size])
primitives = [p for m in glb['meshes'] for p in m['primitives']]
result = {
    'blend': str(bpy.data.filepath),
    'coordinate_hash': hashlib.sha256(json.dumps([tuple(v.co) for v in mesh.vertices]).encode()).hexdigest(),
    'vertices': len(mesh.vertices),
    'triangles': len(mesh.polygons),
    'uv_layers': [layer.name for layer in mesh.uv_layers],
    'color_attributes': [attr.name for attr in mesh.color_attributes],
    'glb': {
        'materials': len(glb.get('materials', [])),
        'images': glb.get('images', []),
        'attributes': [list(p['attributes']) for p in primitives],
        'triangles': sum(glb['accessors'][p['indices']]['count'] // 3 for p in primitives),
        'nodes': [n.get('name') for n in glb['nodes']],
    },
}
for name, px, py, az, el, span in (
    ('roof-final', 1325, 150, 30, 65, .30),
    ('magazine-final', 1070, 480, 10, 15, .19),
    ('magazine-heel-final', 960, 650, 10, -10, .14),
):
    center = Vector((0, (px - ANCHOR_PX[0]) / PPM, (ANCHOR_PX[1] - py) / PPM))
    cam = aim(sc, [obj], _dirv(az, el), (1200, 1200), lens=0, center=center)
    cam.data.ortho_scale = span
    render_objs(sc, [obj], cam, (1200, 1200), str(OUT / (name + '.png')), mode='albedo')
source = ROOT.parent / 'akm-reference-side-v1.png'
result['reference_sha256'] = hashlib.sha256(source.read_bytes()).hexdigest()
bpy.ops.wm.open_mainfile(filepath=str(ROOT / 'work/before-uv-fix.blend'))
result['before_packed_images'] = [
    {'name': i.name, 'hash': hashlib.sha256(bytes(i.packed_file.data)).hexdigest()}
    for i in bpy.data.images if i.packed_file
]
result['basecolor_sha256'] = hashlib.sha256((ROOT / 'akm-basecolor.png').read_bytes()).hexdigest()
result['basecolor_identical_to_before_packed'] = any(
    image['hash'] == result['basecolor_sha256'] for image in result['before_packed_images']
)
(OUT / 'independent-check.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
print(json.dumps(result), flush=True)



