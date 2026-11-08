SCENE = "Horde_AKM_Reference_v1"
PREFIX = "HordeAKMv1"
REFERENCE = r"C:\projects\kuznya-mirov\art\horde\3d\hero\rifle\akm-reference-side-v1.png"
ASSET_FOLDER = r"C:\projects\kuznya-mirov\art\horde\3d\hero\rifle\akm_v1"
IMAGE_WIDTH = 2172
IMAGE_HEIGHT = 724
SILHOUETTE_BBOX = (12, 45, 2159, 706)
LENGTH = 0.88
PPM = (2159 - 12) / LENGTH
ANCHOR_PX = (1507, 706)
GRIP_PX = (1507, 385)
SUPPORT_PX = (740, 210)
EXPORT_SCALE = 0.8357751620933414 / 1.73
SIZE = {"y": LENGTH, "z": (706 - 45) / PPM}
CAMERA = {"elev": 30, "az": 50, "px_per_m": 500}
ROLE_COLOURS = {"shell": "#76716b"}
PARTS = {
    "envelope": {
        "what": "Rough closed variable-thickness extrusion of the approved alpha silhouette",
        "region": {"right": ((12 - 1507) / PPM, (2159 - 1507) / PPM, 0, (706 - 45) / PPM)},
        "size": SIZE,
        "touches": [],
        "need": 0.95,
    }
}


def M(role: str):
    return mat(f"{PREFIX}_{role}", ROLE_COLOURS[role], 1.0)
