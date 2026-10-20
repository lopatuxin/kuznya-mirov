# Модели Транспортной компании: ГАЗель, частные дома, пятиэтажка секцией, дом старого центра, деревья,
# контора, хлебозавод, магазин, рынок, стройка, пристань. Каждая модель — свой файл .glb в games/transport/models.
# Запуск: "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe" --background --factory-startup --python art/transport/models/build.py
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import bakery  # noqa: E402
import gazel  # noqa: E402
import lib  # noqa: E402
from lib import Model  # noqa: E402

OUT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "games", "transport", "models"))
GLASS = "#25313b"
FRAME = "#f1ede2"


def save(name):
    os.makedirs(OUT, exist_ok=True)
    lib.export(os.path.join(OUT, name + ".glb"))
    print("written", name)


IZBA_COLORS = {"brown": "#7a5232", "green": "#6e9474", "blue": "#6a8db0", "ochre": "#c4a35e"}
ROOFS = {"green": "#55785c", "red": "#8a4030", "grey": "#6c757b"}


def izba(name, wall, roof):
    """Изба: 6,5 вдоль X, 8,5 вдоль Y, фронтон с тремя окнами — на улицу (−Y)."""
    lib.reset()
    m = Model(name)
    w = m.mat("wall", wall, 0.85)
    base = m.mat("base", "#7d7468", 0.9)
    r = m.mat("roof", roof, 0.55, 0.2)
    g = m.mat("glass", GLASS, 0.2)
    f = m.mat("frame", FRAME, 0.7)
    brick = m.mat("brick", "#8e4a36", 0.9)
    m.box(-3.25, -4.25, 0, 3.25, 4.25, 0.5, base)
    m.box(-3.2, -4.2, 0.5, 3.2, 4.2, 3.6, w)
    m.gable(-3.2, -4.2, 3.2, 4.2, 3.6, 6.1, r, w, over=0.45, along="y")
    for u in (-1.8, 0, 1.8):
        m.window(("front", -4.2), u, 1.5, 0.85, 1.2, g, f)
    for u in (-2.4, 0.2, 2.6):
        m.window(("left", -3.2), u, 1.5, 0.85, 1.2, g, f)
        m.window(("right", 3.2), u, 1.5, 0.85, 1.2, g, f)
    m.window(("front", -4.2), 0, 4.3, 0.6, 0.7, g, f)
    m.box(1.2, 1.0, 4.6, 1.8, 1.6, 7.0, brick)
    m.box(3.2, 1.5, 0, 4.4, 3.2, 0.5, base)
    m.box(3.2, 1.6, 0.5, 3.3, 2.5, 2.6, m.mat("door", "#5a3b24", 0.8))
    m.finish()
    save(name)


def brick_house(name, wall, roof):
    """Кирпичный одноэтажный дом 9 × 8 с вальмовой крышей."""
    lib.reset()
    m = Model(name)
    w = m.mat("wall", wall, 0.9)
    base = m.mat("base", "#6f6a64", 0.9)
    r = m.mat("roof", roof, 0.6, 0.15)
    g = m.mat("glass", GLASS, 0.2)
    f = m.mat("frame", FRAME, 0.7)
    m.box(-4.5, -4, 0, 4.5, 4, 0.4, base)
    m.box(-4.45, -3.95, 0.4, 4.45, 3.95, 3.3, w)
    m.hip(-4.45, -3.95, 4.45, 3.95, 3.3, 5.6, r, over=0.5)
    for u in (-3.0, -1.0, 1.0, 3.0):
        m.window(("front", -3.95), u, 1.3, 0.9, 1.3, g, f)
        m.window(("back", 3.95), u, 1.3, 0.9, 1.3, g, f)
    for u in (-1.6, 1.6):
        m.window(("left", -4.45), u, 1.3, 0.9, 1.3, g, f)
        m.window(("right", 4.45), u, 1.3, 0.9, 1.3, g, f)
    m.finish()
    save(name)


def two_storey(name, wall, roof):
    """Двухэтажный частный дом 7 × 7, двускатная крыша вдоль X."""
    lib.reset()
    m = Model(name)
    w = m.mat("wall", wall, 0.85)
    base = m.mat("base", "#6f6a64", 0.9)
    r = m.mat("roof", roof, 0.6, 0.15)
    g = m.mat("glass", GLASS, 0.2)
    f = m.mat("frame", FRAME, 0.7)
    m.box(-3.5, -3.5, 0, 3.5, 3.5, 0.4, base)
    m.box(-3.45, -3.45, 0.4, 3.45, 3.45, 6.0, w)
    m.gable(-3.45, -3.45, 3.45, 3.45, 6.0, 8.4, r, w, over=0.5, along="x")
    for z in (1.2, 4.0):
        for u in (-1.7, 0, 1.7):
            m.window(("front", -3.45), u, z, 0.85, 1.2, g, f)
            m.window(("back", 3.45), u, z, 0.85, 1.2, g, f)
        for u in (-1.3, 1.3):
            m.window(("left", -3.45), u, z, 0.85, 1.2, g, f)
            m.window(("right", 3.45), u, z, 0.85, 1.2, g, f)
    m.finish()
    save(name)


def khrushchevka(name, panel):
    """Секция пятиэтажки 15 вдоль X, 12 вдоль Y, 14,5 в высоту; секции ставятся в ряд вдоль X."""
    lib.reset()
    m = Model(name)
    p = m.mat("panel", panel, 0.85)
    seam = m.mat("seam", "#b9b3a7", 0.9)
    g = m.mat("glass", "#2a3540", 0.2)
    f = m.mat("frame", "#e9e5dc", 0.7)
    roof = m.mat("roof", "#5d5f61", 0.9)
    balc = m.mat("balcony", "#c8c1b3", 0.85)
    door = m.mat("door", "#5b4636", 0.8)
    m.box(-7.5, -6, 0, 7.5, 6, 0.6, seam)
    m.box(-7.5, -6, 0.6, 7.5, 6, 14.5, p)
    m.box(-7.5, -6, 14.5, 7.5, 6, 14.9, seam)
    m.box(-7.2, -5.7, 14.9, 7.2, 5.7, 14.95, roof)
    for floor in range(5):
        z = 1.3 + floor * 2.7
        m.box(-7.52, -6.02, z - 0.55, 7.52, 6.02, z - 0.5, seam)
        for u in (-6.0, -3.0, 0, 3.0, 6.0):
            if floor == 0 and abs(u) < 0.1:
                continue
            m.window(("front", -6.0), u, z, 1.4, 1.4, g, f, depth=0.04, trim=0.08)
            m.window(("back", 6.0), u, z, 1.4, 1.4, g, f, depth=0.04, trim=0.08)
        if floor >= 1:
            for u in (-4.5, 4.5):
                m.box(u - 1.4, -7.0, z - 0.45, u + 1.4, -6.0, z + 0.55, balc)
    m.box(-0.8, -6.06, 0.6, 0.8, -6.0, 2.6, door)
    m.box(-1.3, -7.3, 2.75, 1.3, -6.0, 2.95, seam)
    m.finish()
    save(name)


def old_town(name, wall, roof):
    """Дом старого центра: два этажа 14 × 10, белые пояса, вальмовая крыша."""
    lib.reset()
    m = Model(name)
    w = m.mat("wall", wall, 0.85)
    trim = m.mat("trim", "#f4efe4", 0.75)
    r = m.mat("roof", roof, 0.55, 0.25)
    g = m.mat("glass", GLASS, 0.2)
    f = m.mat("frame", FRAME, 0.7)
    m.box(-7, -5, 0, 7, 5, 0.6, m.mat("base", "#8a7d70", 0.9))
    m.box(-6.95, -4.95, 0.6, 6.95, 4.95, 8.0, w)
    m.box(-7.1, -5.1, 3.9, 7.1, 5.1, 4.2, trim)
    m.box(-7.15, -5.15, 7.7, 7.15, 5.15, 8.1, trim)
    for x in (-6.95, 6.95):
        m.box(x - 0.3, -5.0, 0.6, x + 0.3, -4.85, 8.0, trim)
    m.hip(-6.95, -4.95, 6.95, 4.95, 8.1, 10.6, r, over=0.4)
    for z in (1.4, 4.9):
        for u in (-5.0, -2.5, 0, 2.5, 5.0):
            m.window(("front", -4.95), u, z, 1.0, 1.7, g, f)
            m.window(("back", 4.95), u, z, 1.0, 1.7, g, f)
        for u in (-2.6, 0, 2.6):
            m.window(("left", -6.95), u, z, 1.0, 1.7, g, f)
            m.window(("right", 6.95), u, z, 1.0, 1.7, g, f)
    m.finish()
    save(name)


def birch(name, seed):
    lib.reset()
    m = Model(name)
    bark = m.mat("bark", "#e8e4da", 0.8)
    leaf = m.mat("leaf", "#6f9a45", 0.85)
    leaf2 = m.mat("leaf2", "#86ad52", 0.85)
    m.cylinder(0, 0, 0, 0.18, 6.0, bark, 7, r_top=0.08)
    m.blob(0, 0, 6.5, 2.2, 2.0, 3.4, leaf, 1, seed)
    m.blob(0.8, -0.5, 7.8, 1.4, 1.3, 1.8, leaf2, 1, seed + 1)
    m.blob(-0.9, 0.6, 5.6, 1.3, 1.3, 1.5, leaf2, 1, seed + 2)
    m.finish()
    save(name)


def linden(name, seed):
    lib.reset()
    m = Model(name)
    bark = m.mat("bark", "#5c4a3a", 0.9)
    leaf = m.mat("leaf", "#4e7a34", 0.85)
    leaf2 = m.mat("leaf2", "#5f8c3c", 0.85)
    m.cylinder(0, 0, 0, 0.3, 3.5, bark, 7, r_top=0.2)
    m.blob(0, 0, 5.6, 3.2, 3.1, 3.0, leaf, 1, seed)
    m.blob(1.2, 0.8, 6.6, 2.0, 2.0, 1.9, leaf2, 1, seed + 1)
    m.blob(-1.4, -0.6, 5.2, 1.9, 1.9, 1.7, leaf2, 1, seed + 2)
    m.finish()
    save(name)


def pine(name):
    lib.reset()
    m = Model(name)
    bark = m.mat("bark", "#6a4a32", 0.9)
    needle = m.mat("needle", "#3e6a3c", 0.85)
    m.cylinder(0, 0, 0, 0.22, 4.0, bark, 7, r_top=0.15)
    for z, r in ((3.0, 2.3), (5.2, 1.8), (7.2, 1.3), (8.9, 0.8)):
        m.cylinder(0, 0, z, r, 2.6, needle, 8, r_top=0.15)
    m.finish()
    save(name)


def office():
    """Контора фирмы: одноэтажное здание 12 × 8 с вывеской «ГРУЗОПЕРЕВОЗКИ», ворота во двор."""
    lib.reset()
    m = Model("office")
    w = m.mat("wall", "#efe9dc", 0.85)
    roof = m.mat("roof", "#5a5c5e", 0.9)
    g = m.mat("glass", GLASS, 0.2)
    f = m.mat("frame", "#e8782b", 0.6)
    sign = m.mat("sign", "#e8782b", 0.5)
    door = m.mat("door", "#3d4650", 0.6)
    m.box(-6, -4, 0, 6, 4, 0.4, m.mat("base", "#7a746d", 0.9))
    m.box(-5.95, -3.95, 0.4, 5.95, 3.95, 3.8, w)
    m.box(-6.1, -4.1, 3.8, 6.1, 4.1, 4.2, roof)
    for u in (-4.2, -2.2, 2.2, 4.2):
        m.window(("front", -3.95), u, 1.3, 1.3, 1.4, g, f)
    m.box(-0.8, -4.0, 0.4, 0.8, -3.95, 2.6, door)
    m.box(-4.8, -4.25, 3.0, 4.8, -4.0, 3.75, sign)
    sign_obj = m.finish()
    lib.text(sign_obj, "ГРУЗОПЕРЕВОЗКИ", 0.62, (0, -4.3, 3.36), 0, "#ffffff")
    save("office")


def shop():
    """Магазин: одноэтажный 10 × 8, витрины, вывеска «ПРОДУКТЫ», козырёк."""
    lib.reset()
    m = Model("shop")
    w = m.mat("wall", "#e6dcc6", 0.85)
    roof = m.mat("roof", "#6b4a3a", 0.6)
    g = m.mat("glass", "#33414d", 0.15)
    f = m.mat("frame", "#3d6b8a", 0.5)
    sign = m.mat("sign", "#2f6aa3", 0.5)
    canopy = m.mat("canopy", "#c94a3a", 0.7)
    m.box(-5, -4, 0, 5, 4, 0.3, m.mat("base", "#7a746d", 0.9))
    m.box(-4.95, -3.95, 0.3, 4.95, 3.95, 3.6, w)
    m.hip(-4.95, -3.95, 4.95, 3.95, 3.6, 5.2, roof, over=0.4)
    for u in (-3.0, 3.0):
        m.window(("front", -3.95), u, 0.8, 2.6, 1.9, g, f)
    m.box(-0.8, -4.0, 0.3, 0.8, -3.95, 2.6, m.mat("door", "#3d4650", 0.6))
    m.box(-4.6, -5.0, 2.8, 4.6, -3.95, 2.95, canopy)
    m.box(-4.2, -4.12, 3.0, 4.2, -3.95, 3.55, sign)
    o = m.finish()
    lib.text(o, "ПРОДУКТЫ", 0.45, (0, -4.17, 3.27), 0, "#ffffff")
    save("shop")


def market():
    """Рынок: три ряда прилавков под цветными навесами и павильон с вывеской «РЫНОК»."""
    lib.reset()
    m = Model("market")
    table = m.mat("table", "#8b6a48", 0.9)
    post = m.mat("post", "#6a6e72", 0.6, 0.4)
    colors = ["#c94a3a", "#2f6aa3", "#e0b33a", "#4c8a4e", "#d06a2a"]
    awn = [m.mat(f"awning{i}", c, 0.8) for i, c in enumerate(colors)]
    goods = [m.mat(f"goods{i}", c, 0.8) for i, c in enumerate(["#d24c3a", "#e8a33a", "#7fb04a", "#f0d880"])]
    i = 0
    for row in (-7.0, 0.0, 7.0):
        for col in range(-12, 13, 4):
            m.box(col - 1.7, row - 1.0, 0, col + 1.7, row + 1.0, 0.95, table)
            m.box(col - 1.5, row - 0.8, 0.95, col + 1.5, row + 0.8, 1.15, goods[i % len(goods)])
            for px in (col - 1.8, col + 1.8):
                m.box(px - 0.06, row - 1.3, 0, px + 0.06, row - 1.18, 2.6, post)
            m.quad([(col - 2.0, row - 1.6, 2.4), (col + 2.0, row - 1.6, 2.4), (col + 2.0, row + 1.4, 2.9), (col - 2.0, row + 1.4, 2.9)], awn[i % len(awn)])
            m.quad([(col - 2.0, row + 1.4, 2.9), (col + 2.0, row + 1.4, 2.9), (col + 2.0, row - 1.6, 2.4), (col - 2.0, row - 1.6, 2.4)], awn[i % len(awn)])
            i += 1
    w = m.mat("pavilion", "#e8dfcc", 0.85)
    m.box(-6, -14, 0, 6, -10, 3.4, w)
    m.box(-6.2, -14.2, 3.4, 6.2, -9.8, 3.7, m.mat("roof", "#4d6b55", 0.6))
    m.box(-3.5, -14.25, 2.2, 3.5, -14.0, 3.3, m.mat("sign", "#c94a3a", 0.6))
    o = m.finish()
    lib.text(o, "РЫНОК", 0.8, (0, -14.3, 2.75), 0, "#ffffff")
    save("market")


def site():
    """Стройка: забор, кран, каркас дома, поддоны кирпича."""
    lib.reset()
    m = Model("site")
    fence = m.mat("fence", "#5f7f9a", 0.7)
    crane = m.mat("crane", "#e6b52e", 0.5, 0.3)
    concrete = m.mat("concrete", "#a9a6a0", 0.9)
    brick = m.mat("brick", "#a3563d", 0.9)
    sand = m.mat("sand", "#c9b07a", 0.95)
    for x0, y0, x1, y1 in ((-16, -12, 16, -11.8), (-16, 11.8, 16, 12), (-16, -12, -15.8, 12), (15.8, -12, 16, 12)):
        m.box(x0, y0, 0, x1, y1, 2.0, fence)
    for z in range(0, 4):
        m.box(-9, -6, z * 3.0, 5, 6, z * 3.0 + 0.3, concrete)
        for x in (-9, -2, 5):
            for y in (-6, 6):
                m.box(x - 0.25, y - 0.25, z * 3.0, x + 0.25, y + 0.25, z * 3.0 + 3.0, concrete)
    m.box(-9, -6, 0, -8.6, 6, 6.0, brick)
    m.box(9.5, -1, 0, 11, 0.5, 26, crane)
    m.box(-14, -0.6, 25, 18, 0.1, 26.2, crane)
    m.box(14, -1.2, 22.5, 17.5, 0.7, 25, concrete)
    m.box(-12.5, -0.3, 14, -12.3, -0.2, 25, m.mat("cable", "#2b2b2e", 0.8))
    for x, y in ((-13, -9), (-11, -9), (-13, -6.5), (8, 8), (10, 8)):
        m.box(x - 0.8, y - 0.8, 0, x + 0.8, y + 0.8, 1.1, brick)
    m.blob(11, -7, 0, 3.5, 2.5, 1.6, sand, 1, 3)
    m.finish()
    save("site")


def pier():
    """Пристань: дебаркадер у воды — белый дом на понтоне — и сходни."""
    lib.reset()
    m = Model("pier")
    hull = m.mat("hull", "#3d4f5f", 0.6, 0.3)
    deck = m.mat("deck", "#8b6a48", 0.9)
    w = m.mat("wall", "#f2f0ea", 0.7)
    roof = m.mat("roof", "#2f6aa3", 0.5)
    g = m.mat("glass", GLASS, 0.2)
    f = m.mat("frame", "#2f6aa3", 0.5)
    m.box(-15, -5, 0, 15, 5, 1.0, hull)
    m.box(-15.2, -5.2, 1.0, 15.2, 5.2, 1.2, deck)
    m.box(-11, -3.8, 1.2, 11, 3.8, 4.4, w)
    m.box(-11.4, -4.2, 4.4, 11.4, 4.2, 4.7, roof)
    m.box(-4, -2.5, 4.7, 4, 2.5, 7.0, w)
    m.hip(-4, -2.5, 4, 2.5, 7.0, 8.6, roof, over=0.3)
    for u in range(-9, 10, 3):
        m.window(("front", -3.8), u, 2.2, 1.6, 1.3, g, f)
        m.window(("back", 3.8), u, 2.2, 1.6, 1.3, g, f)
    m.box(-1.5, 5.2, 1.0, 1.5, 14, 1.2, deck)
    o = m.finish()
    lib.text(o, "КИНЕШМА", 0.9, (0, -2.56, 5.85), 0, "#2f6aa3")
    save("pier")


def warehouse(name, wall, sign_text):
    """Склад 30 × 18 с тремя воротами и вывеской."""
    lib.reset()
    m = Model(name)
    w = m.mat("wall", wall, 0.6, 0.35)
    roof = m.mat("roof", "#6c7378", 0.6, 0.4)
    gate = m.mat("gate", "#3c4248", 0.6, 0.3)
    sign = m.mat("sign", "#f3efe4", 0.6)
    m.box(-15, -9, 0, 15, 9, 0.5, m.mat("base", "#7a746d", 0.9))
    m.box(-14.95, -8.95, 0.5, 14.95, 8.95, 7.0, w)
    m.gable(-14.95, -8.95, 14.95, 8.95, 7.0, 9.5, roof, w, over=0.4, along="x")
    for u in (-9, 0, 9):
        m.box(u - 2.0, -9.0, 0.5, u + 2.0, -8.95, 4.8, gate)
        m.box(u - 2.8, -11, 0, u + 2.8, -8.95, 1.1, m.mat("ramp", "#8f8a82", 0.9))
    m.box(-7, -9.1, 5.3, 7, -8.95, 6.7, sign)
    o = m.finish()
    lib.text(o, sign_text, 0.8, (0, -9.15, 6.0), 0, "#2b2b2e")
    save(name)


def main():
    gazel.build("gazel", "#2f6aa3")
    gazel.build("gazel_green", "#3f7a4a")
    izba("izba_brown_green", IZBA_COLORS["brown"], ROOFS["green"])
    izba("izba_green_red", IZBA_COLORS["green"], ROOFS["red"])
    izba("izba_blue_grey", IZBA_COLORS["blue"], ROOFS["grey"])
    izba("izba_ochre_green", IZBA_COLORS["ochre"], ROOFS["green"])
    brick_house("brick_house", "#9e5a42", "#6b4a3a")
    brick_house("white_house", "#e3ddd0", "#7a3e32")
    two_storey("two_storey_blue", "#9fb4c8", "#6c757b")
    two_storey("two_storey_cream", "#e1d3b0", "#8a4030")
    khrushchevka("khrush_grey", "#d4cfc4")
    khrushchevka("khrush_sand", "#dccbb0")
    old_town("old_yellow", "#e6cf98", "#4d6b55")
    old_town("old_pink", "#dcb4a6", "#5d6064")
    birch("birch", 1)
    linden("linden", 2)
    pine("pine")
    office()
    bakery.build()
    shop()
    market()
    site()
    pier()
    warehouse("wholesale", "#8fa3b3", "ОПТОВАЯ БАЗА")
    warehouse("materials", "#b3a68f", "СТРОЙМАТЕРИАЛЫ")


main()
