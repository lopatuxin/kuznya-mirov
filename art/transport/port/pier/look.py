# Отделка причала порта для art/transport/port/surface.py: роль материала частей → фото библиотеки
# art/transport/textures в настоящем размере (как в office/look.py). WET — ниже воды (dims.py WATER_Z) всё мокрое
# и тёмное с зеленцой, переход на 0,5 м выше воды.

NAME = "port_pier"

WET = (2.2, "#5a5c48", 0.5)

ROLES = {
    "log": {"tex": "Wood033", "meters": 1.6, "round": True, "end": "log_end", "tones": ("#7a6a58", "#6f604f", "#857462")},
    "log_end": {"tex": "TreeEnd005", "file": "color_aged.jpg", "disc": (0.501, 0.509, 0.42), "tones": ("#ffffff", "#ece6dc", "#f6f1ea")},
    "timber": {"tex": "Wood033", "meters": 1.6, "round": True, "end": "log_end", "tones": ("#8f8a82", "#857f77", "#9a948a")},
    "plank": {"tex": "Wood033", "meters": 1.6, "tones": ("#c4bcb0", "#b8b0a4", "#cdc6ba", "#b0a89c")},
    "stone": {"tex": "Rock047", "meters": 1.0, "tones": ("#ffffff", "#e6e2dc", "#f2efe8", "#d8d4cc")},
    "iron": {"tex": "Metal025", "arm": "arm_iron.jpg", "meters": 0.8, "tint": "#6a605a"},
    "chain": {"tex": "Metal025", "arm": "arm_iron.jpg", "meters": 0.4, "tint": "#a07e6a"},
}
