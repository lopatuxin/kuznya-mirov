# Проверка плагином blender-modeling готовой модели игры games/transport/models/garage.glb, которую строит
# art/transport/models/garage.py. Метры, Z вверх, улица — к −Y, верх земли — ноль. С образцом силуэт не сверяется:
# в образце свои ошибки, модель их исправила (решение хозяина 2026-10-26); ищутся огрехи формы и материалов.

SCENE = "EST_garage"
CAMERA = {"elev": 55, "az": -35, "px_per_m": 6.75}
LIGHT = {"sun_az": -35, "sun_el": 48, "sun": 0.6055, "sky": 0.55,
         "sun_color": "#fff3e0", "sky_color": "#cfe0f2", "ground": "#7d7a68"}

GLB = r"C:\projects\kuznya-mirov\games\transport\models\garage.glb"
PREFIX = "EG"


def M(role):
    """Материалы задаёт скрипт игры; роль здесь только для заглушек."""
    return mat(f"{SCENE}_{role}", "#808080", 0.85)


PARTS = {
    "yard": {
        "what": "весь гараж с двором одним объектом из games/transport/models/garage.glb: каретный сарай, ворота, щипец сеновала, мастерская, навес с бочками, колонка, пожарный щит, коновязь, забор, подъезды",
        "touches": [],
    },
}
