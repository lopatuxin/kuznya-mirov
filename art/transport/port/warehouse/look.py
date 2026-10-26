# Отделка склада порта для art/transport/port/surface.py: роль материала частей → фото библиотеки
# art/transport/textures в настоящем размере (как в office/look.py). slope — у плоских деталей роли волокно
# идёт по вертикали грани: обшивка стен из вертикальных досок.

NAME = "port_warehouse"

ROLES = {
    "board": {"tex": "Wood033", "meters": 1.6, "slope": True, "tones": ("#b8b0a4", "#a9a195", "#c3bbaf", "#a19a8f")},
    "timber": {"tex": "Wood033", "meters": 1.6, "tones": ("#8f8a82", "#857f77", "#9a948a")},
    "stone": {"tex": "Rock047", "meters": 1.0, "tones": ("#ffffff", "#e6e2dc", "#f2efe8", "#d8d4cc")},
    "glass": {"tex": "WindowGlass", "file": "color.png", "alpha": True, "meters": 1.0},
    "dark": {"tex": "Bricks096", "meters": 2.0, "tint": "#8a8478"},
    "iron": {"tex": "Metal025", "arm": "arm_iron.jpg", "meters": 0.8, "tint": "#7a6e66"},
    "roof": {"tex": "Metal025", "arm": "arm_roof.jpg", "meters": 4.0, "tint": "#e0a688", "tones": ("#ffffff", "#f0e6e2", "#e8ddd8")},
}
