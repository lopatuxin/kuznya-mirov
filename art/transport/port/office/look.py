# Отделка конторы порта для art/transport/port/surface.py: роль материала частей → фото библиотеки
# art/transport/textures в настоящем размере. tex — папка библиотеки, meters — сколько метров покрывает одна
# картинка, arm — своя карта шероховатости и металла папки (arm_roof.jpg — почти без металла), tint — подкраска фото, tones — тона деталей роли (каждая деталь берёт один, он идёт в цвет вершин),
# round — круглые брёвна: развёртка по окружности, торцы — роль end, срез с кольцами; file — своя картинка цвета
# папки (color_grey.jpg — выцветший срез), alpha — прозрачность из альфы картинки (стекло).

NAME = "port_office"

LOGS = ("#8f7c66", "#83715d", "#9a8771", "#7a6a59")  # тона брёвен двора деревни (yard.py)

ROLES = {
    "log": {"tex": "Wood033", "meters": 1.6, "round": True, "end": "log_end", "tones": LOGS},
    "log_end": {"tex": "TreeEnd005", "file": "color_aged.jpg", "disc": (0.501, 0.509, 0.42), "tones": ("#ffffff", "#ece6dc", "#f6f1ea")},
    "board": {"tex": "Wood086", "meters": 1.6, "tones": ("#d0c4b4", "#c2b5a4", "#dad0c2")},
    "stone": {"tex": "Rock047", "meters": 1.0, "tones": ("#ffffff", "#e6e2dc", "#f2efe8", "#d8d4cc")},
    "dark": {"tex": "Bricks096", "meters": 2.0, "tint": "#8a8478"},
    "frame": {"tex": "PaintedWood009C", "meters": 1.2, "tint": "#fbf9f4"},
    "glass": {"tex": "WindowGlass", "file": "color.png", "alpha": True, "meters": 1.0},
    "curtain": {"tex": "Linen", "meters": 0.6, "tint": "#f4efe6", "tones": ("#ffffff", "#f6f2ea", "#efe9de")},
    "plaque": {"color": "#1f3f8f", "rough": 0.25},
    "iron": {"tex": "Metal025", "arm": "arm_iron.jpg", "meters": 0.8, "tint": "#7a6e66"},
    "roof": {"tex": "Metal025", "arm": "arm_roof.jpg", "meters": 4.0, "tint": "#e0a688", "tones": ("#ffffff", "#f0e6e2", "#e8ddd8")},
    "brick": {"tex": "Bricks085", "meters": 2.7, "tint": "#e0c4b8", "tones": ("#ffffff",)},
}

# надпись: слова, высота букв, (x, —, z) середины (y — перед гранью роли on), поворот, цвет, роль-основа
SIGNS = [("ПОРТ", 0.26, (0.41, None, 2.33), 0.0, "#f4f2ec", "plaque")]
