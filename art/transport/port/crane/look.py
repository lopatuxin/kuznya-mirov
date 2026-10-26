# Отделка крана причала для art/transport/port/surface.py: роль материала частей → фото библиотеки
# art/transport/textures в настоящем размере (как в office/look.py).

NAME = "port_crane"

ROLES = {
    "timber": {"tex": "Wood033", "meters": 1.6, "tones": ("#9a8c7c", "#8f8272", "#a49684", "#887a6a")},
    "iron": {"tex": "Metal025", "arm": "arm_iron.jpg", "meters": 0.5, "tint": "#6a5e56"},
    "rust": {"tex": "Metal025", "arm": "arm_roof.jpg", "meters": 0.5, "tint": "#c09078"},
    "rope": {"tex": "Rope001", "meters": 0.3, "round": True, "tint": "#8a7a62"},
    "handle": {"tex": "Wood086", "meters": 0.6, "tint": "#c09060"},
}
