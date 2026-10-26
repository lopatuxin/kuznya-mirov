# Фургон целиком: его строит скрипт игры art/transport/models/gaz_aa.py (make), здесь объект только переносится
# в коллекцию части. Править форму — в скрипте игры, не здесь.
import importlib
import sys


def build(sc, col):
    if MODELS not in sys.path:
        sys.path.insert(0, MODELS)
    import lib, bakery, gazel, gaz_aa
    for mod in (lib, bakery, gazel, gaz_aa):
        importlib.reload(mod)
    old = bpy.data.objects.get("GV_van")
    if old:
        bpy.data.objects.remove(old)
    obj = gaz_aa.make("GV_van", "van")
    for c in list(obj.users_collection):
        c.objects.unlink(obj)
    col.objects.link(obj)
