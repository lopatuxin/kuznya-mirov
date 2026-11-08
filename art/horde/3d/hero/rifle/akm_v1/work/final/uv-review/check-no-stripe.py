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
for name, px, py, az, el, span in (
    ('roof-no-stripe-final', 1325, 150, 30, 65, .30),
    ('magazine-heel-no-stripe-final', 960, 650, 10, -10, .14),
):
    center = Vector((0, (px - ANCHOR_PX[0]) / PPM, (ANCHOR_PX[1] - py) / PPM))
    cam = aim(sc, [obj], _dirv(az, el), (1200, 1200), lens=0, center=center)
    cam.data.ortho_scale = span
    render_objs(sc, [obj], cam, (1200, 1200), str(OUT / (name + '.png')), mode='albedo')
