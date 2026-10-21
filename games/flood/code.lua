-- Потоп («Фаза-01-прототип» игры): мир ушёл под воду, люди живут на плотах и судах из досок. Судно — клетки
-- по сетке: настил, парус, сеть, таран, броня и кормило с рулевым. Движок рисует клетки фигурами, ловит щелчки
-- и находит наложившиеся клетки; всё остальное здесь: ход и повороты судов, удары и разлом на обломки, обломки
-- на воде, крюк и сеть, поведение чужих судов, стройка плота. Суда и обломки игра держит в своих таблицах, а
-- движку пишет места клеток. Каждая партия перечитывает этот файл, поэтому всё ниже начинает партию заново.
--
-- Сетка судна: x — вправо, y — к корме; нос смотрит в −y. Курс `h` — градусы по часовой стрелке, если смотреть
-- сверху: 0 — нос вглубь сцены (вверх окна), 90 — вправо. Так же поворачивает свои прямоугольники движок.

local DT = 1 / 60
local SEA = 256                 -- сторона сцены, клеток
local EDGE = 10                 -- ближе к краю сцены середина судна не заходит
local START = { x = 128, y = 128 }

local BASE_Z = -0.12            -- основание настила: доски сидят в воде
local DECK_H = 0.3
local TOP_Z = BASE_Z + DECK_H   -- верх настила: на нём стоят парус, таран, сеть и рулевой
local CELL = 0.96               -- сторона клетки: соседние клетки одного судна не налегают друг на друга

-- Ход. Судно — одно твёрдое тело: куда смотрит нос и куда оно движется — разное. Вёсла и паруса толкают вдоль
-- носа, к скорости v = SPEED_K · тяга / (тяга + DRAG · вес). Плот из четырёх досок без паруса:
-- 8 · 1,5 / (1,5 + 0,6 · 4) = 3,1 клетки в секунду.
local SPEED_K = 8
local PADDLE = 1.5              -- тяга вёсел, которая есть у любого судна
local DRAG = 0.6
local REVERSE = 0.4             -- задним ходом — эта доля скорости
local ACCEL_TAU = 0.8           -- секунд на разгон у невесомого судна
local ACCEL_TAU_MASS = 0.08     -- и прибавка за каждую единицу веса
-- Без тяги судно катится по инерции: плот (вес 4) сбавляет ход за 1,4 секунды, баржа (33,5) — за 5,8,
-- и с хода 2,6 проходит ещё клеток 15.
local COAST_TAU = 0.8
local COAST_TAU_MASS = 0.15
-- Вода поперёк корпуса держит сильно: боковой ход гаснет за SIDE_TAU, поэтому после поворота судно ещё немного
-- скользит по-старому, а потом выравнивается.
local SIDE_TAU = 0.25
local SIDE_TAU_MASS = 0.01
-- Руль поворачивает, только пока его обтекает вода. На полном руле судно идёт по кругу шириной TURN_CIRCLE своих
-- длин, поэтому чем медленнее ход, тем медленнее поворот, а без хода судно не поворачивает. На полном руле судно
-- теряет TURN_LOSS хода: плот длиной 2 идёт по кругу в 6 клеток на ходу 3,1 · 0,75 = 2,3 и поворачивает на
-- 2,3 / 3 = 0,78 радиана, 44° в секунду; баржа длиной 6 на ходу 2,6 · 0,75 = 1,9 — на 12° в секунду.
local TURN_CIRCLE = 3
local TURN_LOSS = 0.25
-- Поворот разгоняется и гаснет не сразу, тем дольше, чем больше у судна момент инерции клеток:
-- у плота (2,7) — 0,27 секунды, у баржи (около 170) — 1,6.
local ROT_TAU = 0.25
local ROT_TAU_INERTIA = 0.008
local SPIN = 0.5                -- такая доля удара не по середине судна закручивает его

-- Удар. Урон клетке: DMG_K · скорость сближения · таран бьющей клетки · 2 · вес чужого / (оба веса) · стойкость.
-- Налётчик (вес 10,5) таранит плот (вес 4) на сближении 3: 0,8 · 3 · 3 · 2 · 10,5 / 14,5 = 10,4 — настил (5)
-- разбит с одного удара, кормило (14) — с двух.
local DMG_K = 0.8
local MIN_HIT = 1.2             -- медленнее этого суда только толкаются
local HIT_COOLDOWN = 36         -- шагов между ударами одной пары судов
local PUSH = 0.06               -- на сколько клеток за шаг касания суда расходятся

-- Обломки.
local DRIFT = 0.15              -- обломок дрейфует сам по себе не быстрее этого, клеток в секунду
local DEBRIS_NEAR = 40          -- в этом круге вокруг плота игра держит DEBRIS_TARGET обломков
local DEBRIS_TARGET = 70
local DEBRIS_SPAWN_MIN = 22     -- новые появляются не ближе: за краем окна
local DEBRIS_FAR = 80           -- дальше от плота обломки тонут
local KILL_CREDIT = 20          -- удар не раньше стольких секунд до гибели судна засчитывает его ударившему
local SETTLE = 0.8              -- секунд, пока обломки разбитой клетки падают в воду и их не подобрать
local HOOK_RANGE = 10           -- до какого обломка докидывает крюк, клеток от рулевого
local HOOK_SPEED = 6
local HOOK_DROP = 14            -- обломок дальше этого срывается с крюка
local NET_SPEED = 2.5

-- Чужие суда.
local THINK_EVERY = 15          -- шагов между решениями судна
local SIGHT_DEBRIS = 18
local SIGHT_PREY = 24
local FLEE_RANGE = 15
local PREY_RATIO = 0.85         -- охотник гонится за теми, кто не тяжелее этой доли его веса
local THREAT_RATIO = 1.15       -- и бежит от охотников тяжелее себя на эту долю
local CHASE_GIVE_UP = 30        -- секунд погони, после которых охотник бросает добычу
local CHARGE_RANGE = 8          -- с этого расстояния охотник идёт на таран
local CHARGE_BOOST = 1.6        -- во столько раз быстрее он идёт на таран
local CHARGE_TIME = 1.8         -- секунд разгона
local CHARGE_EVERY = 5          -- секунд между заходами
local KEEP_CLEAR = 5            -- ближе этого суда обходят друг друга, если не охотятся
local STUCK_SPEED = 0.3         -- медленнее этого вперёд дольше STUCK_TIME секунд плюс время разгона — застряло
local STUCK_TIME = 1.5
local BACK_TIME = 2             -- столько секунд плюс время разгона застрявшее судно отходит задним ходом
local SPAWN_MIN, SPAWN_MAX = 30, 55
local SPAWN_GAP = 12            -- секунд между появлениями судов одного вида
-- Сколько судов каждого вида держит море с этой секунды партии.
local SCHEDULE = {
  { t = 0, gather = 8, raid = 0, hunt = 0 },
  { t = 75, gather = 8, raid = 2, hunt = 0 },
  { t = 200, gather = 8, raid = 3, hunt = 1 },
  { t = 400, gather = 9, raid = 4, hunt = 2 },
}
local TUNE_SAFE = 16            -- охотник ближе этого не даёт открыть стройку
local FURLED = 0.12             -- высота свёрнутого паруса: на время стройки паруса плота свёрнуты
local REPAIR_HP = 3             -- столько прочности чинит одна доска

local RES = { "wood", "cloth", "metal" }
local RES_NAME = { wood = "доски", cloth = "ткань", metal = "железо" }
local START_RES = { wood = 4, cloth = 0, metal = 0 }

local state = {
  game = nil,
  started = false,
  tuning = false,
  step = 0,
  t = 0,
  vessels = {},       -- vid → судно
  order = {},         -- суда по порядку появления: обход в одном и том же порядке в каждой партии
  next_vid = 1,
  player = nil,
  debris = {},        -- обломки списком: обход в одном порядке
  debris_by_id = {},  -- did → обломок
  next_did = 1,
  contacts = {},      -- касания клеток разных судов за прошлый шаг
  pair_cd = {},       -- пара судов → шаг, до которого удар не повторяется
  hook = nil,         -- обломок на крюке
  rope = nil,         -- объект верёвки
  chosen = "deck",
  res = {},
  kills = 0,
  biggest = 0,
  next_spawn = { gather = 0, raid = 0, hunt = 0 },
  notice_until = 0,
}

-- Числа и слова ---------------------------------------------------------------------------------

local function clamp(v, lo, hi)
  if v < lo then return lo end
  if v > hi then return hi end
  return v
end

-- Числа из таблиц данных и из свойств приходят в код дробными: 1.0, а не 1. Номер клетки и подписи
-- поэтому пишутся через целое.
local function int(n)
  return math.floor(n + 0.5)
end

local function key(gx, gy)
  return string.format("%d,%d", int(gx), int(gy))
end

local function angle_diff(a, b)
  local d = (b - a) % 360
  if d > 180 then d = d - 360 end
  return d
end

local function heading_to(dx, dy)
  return math.deg(math.atan(dx, -dy)) % 360
end

local function dist(ax, ay, bx, by)
  local dx, dy = bx - ax, by - ay
  return math.sqrt(dx * dx + dy * dy)
end

local function plural(n, one, few, many)
  local m10, m100 = n % 10, n % 100
  if m10 == 1 and m100 ~= 11 then return one end
  if m10 >= 2 and m10 <= 4 and (m100 < 12 or m100 > 14) then return few end
  return many
end

local function fmt_time(seconds)
  local s = math.floor(seconds)
  return string.format("%d:%02d", s // 60, s % 60)
end

local function fmt_cost(cost)
  local parts = {}
  for _, r in ipairs(RES) do
    if cost[r] then parts[#parts + 1] = string.format("%s %d", RES_NAME[r], int(cost[r])) end
  end
  return table.concat(parts, " · ")
end

local function notice(text)
  local g = state.game
  g.notice_text = text
  g.notice = true
  state.notice_until = state.t + 4
end

-- Судно ------------------------------------------------------------------------------------------

local function part_of(rec)
  return tables.parts[rec.part]
end

-- Где на воде середина клетки: поворот смещения от середины веса судна на курс.
local function cell_center(v, gx, gy)
  local r = math.rad(v.h)
  local c, s = math.cos(r), math.sin(r)
  local lx, ly = gx - v.px, gy - v.py
  return v.cx + lx * c - ly * s, v.cy + lx * s + ly * c
end

local function forward(v)
  local r = math.rad(v.h)
  return math.sin(r), -math.cos(r)
end

local function helm_of(v)
  return v.grid[key(0, 0)]
end

local function color_of(v, name)
  local look = tables.vessels[v.look]
  return look[name] or name
end

-- Середина верхней части клетки: таран выступает вперёд за нос клетки.
local function top_center(v, rec, x, y)
  local ahead = part_of(rec).top.forward or 0
  local r = math.rad(v.h)
  return x + ahead * math.sin(r), y - ahead * math.cos(r)
end

-- Объект верхней части клетки: рулевой, парус, таран, сеть.
local function top_fields(v, rec, x, y)
  local top = part_of(rec).top
  if top == nil then return nil end
  local tx, ty = top_center(v, rec, x, y)
  local height = top.height
  if state.tuning and v == state.player and rec.part == "sail" then height = FURLED end
  return {
    top = true, fresh = true, vid = v.vid, gx = rec.gx, gy = rec.gy, part = rec.part,
    position = { tx - top.size[1] / 2, ty - top.size[2] / 2, TOP_Z }, size = { top.size[1], top.size[2] },
    rotation = v.h, shape = top.shape, height = height, color = color_of(v, top.color),
  }
end

local function base_color(v, rec)
  return part_of(rec).color or color_of(v, "deck")
end

-- Заявка движку на объекты клетки; ручки объектов придут шагом позже, через `fresh`.
local function create_cell_objects(v, rec)
  local x, y = cell_center(v, rec.gx, rec.gy)
  local base = {
    cell = true, collides = true, fresh = true, vid = v.vid, gx = rec.gx, gy = rec.gy,
    part = rec.part, hp = rec.hp, max_hp = rec.max_hp,
    position = { x - CELL / 2, y - CELL / 2, BASE_Z }, size = { CELL, CELL }, rotation = v.h,
    shape = "box", height = DECK_H, color = base_color(v, rec),
  }
  if rec.hp < rec.max_hp then base.damaged = true end
  if v.kind == "player" then
    base.on_click = { { "clicked", true } }
    if rec.part == "helm" then base.camera_follows = true end
  elseif rec.part == "helm" then
    base.helm_label = true
    base.name = v.name
  end
  create(base)
  local top = top_fields(v, rec, x, y)
  if top then create(top) end
  rec.drawn = false
end

-- Середина веса, вес, тяга, длина и момент инерции, а из них — ход и поворот. Середина веса сдвинулась —
-- середина судна на воде сдвигается так, чтобы клетки остались на своих местах.
local function restat(v)
  local mass, power, sx, sy = 0, PADDLE, 0, 0
  v.ram = false
  v.nets = {}
  for _, rec in ipairs(v.cells) do
    local p = part_of(rec)
    mass = mass + p.mass
    power = power + (p.power or 0)
    sx = sx + rec.gx * p.mass
    sy = sy + rec.gy * p.mass
    if p.ram then v.ram = true end
    if p.pull then v.nets[#v.nets + 1] = rec end
  end
  local px, py = sx / mass, sy / mass
  if v.px then
    local r = math.rad(v.h)
    local c, s = math.cos(r), math.sin(r)
    local dx, dy = px - v.px, py - v.py
    v.cx = v.cx + dx * c - dy * s
    v.cy = v.cy + dx * s + dy * c
  end
  v.px, v.py = px, py
  -- Момент инерции: каждая клетка — квадрат со стороной 1, у него свой момент m / 6 и ещё m · r² от середины веса.
  local inertia, bow, stern = 0, math.huge, -math.huge
  for _, rec in ipairs(v.cells) do
    local m = part_of(rec).mass
    local dx, dy = rec.gx - px, rec.gy - py
    inertia = inertia + m * (dx * dx + dy * dy + 1 / 6)
    bow, stern = math.min(bow, rec.gy), math.max(stern, rec.gy)
  end
  v.mass = mass
  v.power = power
  v.vmax = SPEED_K * power / (power + DRAG * mass)
  v.inertia = inertia
  v.radius = TURN_CIRCLE * (stern - bow + 1) / 2
  v.tau = ACCEL_TAU + ACCEL_TAU_MASS * mass
  v.coast_tau = COAST_TAU + COAST_TAU_MASS * mass
  v.side_tau = SIDE_TAU + SIDE_TAU_MASS * mass
  v.rot_tau = ROT_TAU + ROT_TAU_INERTIA * inertia
  v.size = #v.cells
end

local function add_cell(v, gx, gy, part)
  local p = tables.parts[part]
  local rec = { gx = int(gx), gy = int(gy), part = part, hp = p.hp, max_hp = p.hp }
  v.cells[#v.cells + 1] = rec
  v.grid[key(gx, gy)] = rec
  return rec
end

local function delete_cell_objects(rec)
  if rec.base then delete(rec.base) end
  if rec.top then delete(rec.top) end
  rec.base, rec.top = nil, nil
end

local function remove_cell(v, rec)
  rec.dead = true
  delete_cell_objects(rec)
  v.grid[key(rec.gx, rec.gy)] = nil
  for i, r in ipairs(v.cells) do
    if r == rec then
      table.remove(v.cells, i)
      break
    end
  end
end

-- Судно по образцу из таблицы `vessels`; `kind` — чьё оно: `player` или вид чужого судна.
local function new_vessel(look_name, kind, x, y, h)
  local look = tables.vessels[look_name]
  local v = {
    vid = state.next_vid, look = look_name, kind = kind, name = look.name, cx = x, cy = y, h = h, w = 0,
    vx = 0, vy = 0, thrust = 0, steer = 0, cells = {}, grid = {}, cargo = {}, alive = true,
    ai = { mode = "wander", think = state.next_vid % THINK_EVERY },
  }
  state.next_vid = state.next_vid + 1
  for _, c in ipairs(look.cells) do
    add_cell(v, c[1], c[2], c[3])
  end
  for _, r in ipairs(RES) do
    v.cargo[r] = (look.cargo and look.cargo[r]) or 0
  end
  restat(v)
  v.cx, v.cy = x, y
  for _, rec in ipairs(v.cells) do
    create_cell_objects(v, rec)
  end
  state.vessels[v.vid] = v
  state.order[#state.order + 1] = v
  return v
end

-- Связные клетки: от кормила по соседям. Кормила нет — связных нет.
local function connected(v, skip)
  local seen = {}
  local helm = helm_of(v)
  if helm == nil or helm == skip then return seen end
  local queue = { helm }
  seen[helm] = true
  local i = 1
  while i <= #queue do
    local rec = queue[i]
    i = i + 1
    for _, d in ipairs({ { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } }) do
      local n = v.grid[key(rec.gx + d[1], rec.gy + d[2])]
      if n and n ~= skip and not seen[n] then
        seen[n] = true
        queue[#queue + 1] = n
      end
    end
  end
  return seen
end

-- Обломки ----------------------------------------------------------------------------------------

local function pick_debris_kind()
  local total = 0
  for _, name in ipairs({ "plank", "tarp", "scrap", "crate", "barrel" }) do
    total = total + tables.debris[name].weight
  end
  local roll = math.random() * total
  for _, name in ipairs({ "plank", "tarp", "scrap", "crate", "barrel" }) do
    roll = roll - tables.debris[name].weight
    if roll <= 0 then return name end
  end
  return "plank"
end

local function spawn_debris(kind, x, y, vx, vy, settle)
  local d = tables.debris[kind]
  local rec = {
    did = state.next_did, kind = kind, x = clamp(x, EDGE, SEA - EDGE), y = clamp(y, EDGE, SEA - EDGE),
    vx = vx or 0, vy = vy or 0, w = d.size[1], l = d.size[2], z = d.z, ready_at = state.t + (settle or 0),
  }
  local drift_a, drift_s = math.random() * 2 * math.pi, math.random() * DRIFT
  rec.dx, rec.dy = math.cos(drift_a) * drift_s, math.sin(drift_a) * drift_s
  state.next_did = state.next_did + 1
  state.debris[#state.debris + 1] = rec
  state.debris_by_id[rec.did] = rec
  create({
    debris = true, collides = true, fresh = true, did = rec.did,
    position = { rec.x - rec.w / 2, rec.y - rec.l / 2, rec.z }, size = { rec.w, rec.l },
    rotation = math.random() * 360, shape = d.shape, height = d.height, color = d.color,
    on_click = { { "clicked", true } },
  })
  return rec
end

local function drop_rope()
  if state.rope then delete(state.rope) end
  state.rope = nil
  if state.hook then state.hook.hooked = false end
  state.hook = nil
end

local function remove_debris(rec)
  rec.taken = true
  if rec.obj then delete(rec.obj) end
  rec.obj = nil
  state.debris_by_id[rec.did] = nil
  for i, r in ipairs(state.debris) do
    if r == rec then
      table.remove(state.debris, i)
      break
    end
  end
  if state.hook == rec then drop_rope() end
end

-- Обломки клетки падают в воду рядом с ней, дальше от середины судна, и разлетаются. Пока они падают,
-- их не подобрать: иначе их сразу уносит в трюм соседняя клетка или тот, кто ударил.
local function scatter(kinds, x, y, speed, from_x, from_y)
  local ox, oy = x - from_x, y - from_y
  local len = math.sqrt(ox * ox + oy * oy)
  if len < 0.01 then
    local a = math.random() * 2 * math.pi
    ox, oy, len = math.cos(a), math.sin(a), 1
  end
  ox, oy = ox / len, oy / len
  for _, kind in ipairs(kinds) do
    local a = math.atan(oy, ox) + (math.random() - 0.5) * 2
    local sp = speed * (0.5 + math.random())
    spawn_debris(kind, x + ox * 0.8, y + oy * 0.8, math.cos(a) * sp, math.sin(a) * sp, SETTLE)
  end
end

-- Груз потопленного судна всплывает ящиками, свёртками ткани и железом.
local function scatter_cargo(v)
  local kinds = {}
  local wood, cloth, metal = v.cargo.wood, v.cargo.cloth, v.cargo.metal
  while wood >= 2 and cloth >= 1 do
    kinds[#kinds + 1] = "crate"
    wood, cloth = wood - 2, cloth - 1
  end
  for _ = 1, wood do kinds[#kinds + 1] = "plank" end
  for _ = 1, cloth do kinds[#kinds + 1] = "tarp" end
  for _ = 1, metal do kinds[#kinds + 1] = "scrap" end
  scatter(kinds, v.cx, v.cy, 1.6, v.cx, v.cy)
end

-- Подсказки и строки -----------------------------------------------------------------------------

local function refresh_res()
  local g = state.game
  g.res_text = string.format("Доски %d    Ткань %d    Железо %d", int(state.res.wood), int(state.res.cloth),
    int(state.res.metal))
end

local function refresh_ship()
  local g, v = state.game, state.player
  if v == nil then return end
  local sails = 0
  for _, rec in ipairs(v.cells) do
    if rec.part == "sail" then sails = sails + 1 end
  end
  g.ship_text = string.format("Плот: %d %s · парусов %d · ход до %d км/ч", v.size,
    plural(v.size, "клетка", "клетки", "клеток"), sails, math.floor(v.vmax * 3.6 + 0.5))
end

local function refresh_stats()
  local g = state.game
  g.stats_text = string.format("Время %s    Потоплено %d", fmt_time(state.t), state.kills)
end

local function can_pay(cost)
  for _, r in ipairs(RES) do
    if (cost[r] or 0) > state.res[r] then return false end
  end
  return true
end

local function lacking(cost)
  local parts = {}
  for _, r in ipairs(RES) do
    local need = (cost[r] or 0) - state.res[r]
    if need > 0 then parts[#parts + 1] = string.format("%s %d", RES_NAME[r], int(need)) end
  end
  return "Не хватает: " .. table.concat(parts, ", ")
end

local function pay(cost, sign)
  for _, r in ipairs(RES) do
    state.res[r] = state.res[r] - sign * (cost[r] or 0)
  end
  refresh_res()
end

local function half(cost)
  local out = {}
  for _, r in ipairs(RES) do
    if cost[r] then out[r] = int(cost[r]) // 2 end
  end
  return out
end

-- Цена новой клетки с деталью: настил плюс сама деталь.
local function new_cell_cost(pk)
  local cost = {}
  for _, r in ipairs(RES) do
    local n = tables.parts.deck.cost[r] or 0
    if pk ~= "deck" then n = n + (tables.parts[pk].cost[r] or 0) end
    if n > 0 then cost[r] = n end
  end
  return cost
end

local function refresh_part_rows()
  for _, row in ipairs(find({ has = { "part_kind" } })) do
    local pk = row.pk
    row.listed = state.tuning
    row.mark = (pk == state.chosen) and "»" or ""
    if pk == "remove" then
      row.cost_text = "вернёт половину цены"
      row.effect_text = "снимает деталь, а с голого настила — саму клетку"
    else
      local p = tables.parts[pk]
      if pk == "deck" then
        row.cost_text = fmt_cost(p.cost)
      else
        row.cost_text = fmt_cost(p.cost) .. "  ·  новой клеткой " .. fmt_cost(new_cell_cost(pk))
      end
      row.effect_text = p.effect
    end
  end
end

-- Начало партии -----------------------------------------------------------------------------------

local function spawn_ring(count, rmin, rmax)
  for _ = 1, count do
    local a = math.random() * 2 * math.pi
    local r = rmin + math.random() * (rmax - rmin)
    spawn_debris(pick_debris_kind(), START.x + math.cos(a) * r, START.y + math.sin(a) * r)
  end
end

local function start_game(g)
  state.started = true
  for _, r in ipairs(RES) do state.res[r] = START_RES[r] end
  local v = new_vessel("raft", "player", START.x, START.y, 0)
  state.player = v
  state.biggest = v.size
  spawn_ring(18, 4, 18)
  spawn_ring(44, 18, DEBRIS_NEAR)
  refresh_res()
  refresh_ship()
  refresh_stats()
  refresh_part_rows()
  g.tune_text = ""
  notice("Плыви к обломкам: что коснулось плота — твоё. Щёлкни по обломку — крюк подтянет его.")
end

-- Ручки объектов, которые движок создал на прошлом шаге.
local function register_fresh()
  for _, obj in ipairs(find({ has = { "fresh" } })) do
    obj.fresh = false
    if obj.debris then
      local rec = state.debris_by_id[obj.did]
      if rec and not rec.taken then rec.obj = obj else delete(obj) end
    elseif obj.rope then
      if state.hook then
        if state.rope then delete(state.rope) end
        state.rope = obj
      else
        delete(obj)
      end
    else
      local v = state.vessels[obj.vid]
      local rec = v and v.grid[key(obj.gx, obj.gy)]
      -- Клетку перестроили, пока её объект создавался: этот объект — от прежней детали.
      if rec == nil or obj.part ~= rec.part then
        delete(obj)
      else
        if obj.top then
          if rec.top then delete(rec.top) end
          rec.top = obj
        else
          if rec.base then delete(rec.base) end
          rec.base = obj
        end
        rec.drawn = false
      end
    end
  end
end

-- Поломка ----------------------------------------------------------------------------------------

local sink

-- Клетка разбита: её обломки падают в воду, а клетки, которые держались только на ней, отваливаются.
local function break_cell(v, rec)
  local x, y = cell_center(v, rec.gx, rec.gy)
  scatter(part_of(rec).drops, x, y, 1.2, v.cx, v.cy)
  remove_cell(v, rec)
  local alive = connected(v)
  local loose = {}
  for _, r in ipairs(v.cells) do
    if not alive[r] then loose[#loose + 1] = r end
  end
  for _, r in ipairs(loose) do
    local lx, ly = cell_center(v, r.gx, r.gy)
    scatter(part_of(r).drops, lx, ly, 0.8, v.cx, v.cy)
    remove_cell(v, r)
  end
  restat(v)
  if v == state.player then refresh_ship() end
end

local function damage(v, rec, amount, by)
  if rec.dead or not v.alive then return end
  rec.hp = rec.hp - amount * (part_of(rec).takes or 1)
  if by then v.last_hit_by, v.last_hit_t = by, state.t end
  if rec.hp <= 0 then
    if rec.part == "helm" then sink(v) else break_cell(v, rec) end
    return
  end
  if rec.base then
    rec.base.hp = rec.hp
    rec.base.damaged = true
  end
end

sink = function(v)
  v.alive = false
  for _, rec in ipairs(v.cells) do
    local x, y = cell_center(v, rec.gx, rec.gy)
    scatter(part_of(rec).drops, x, y, 1.4, v.cx, v.cy)
    rec.dead = true
    delete_cell_objects(rec)
  end
  v.cells, v.grid = {}, {}
  scatter_cargo(v)
  state.vessels[v.vid] = nil
  for i, u in ipairs(state.order) do
    if u == v then
      table.remove(state.order, i)
      break
    end
  end
  local g = state.game
  if v == state.player then
    drop_rope()
    g.result_text = string.format("Продержался %s, потопил %d %s. Самый большой плот — %d %s.",
      fmt_time(state.t), state.kills, plural(state.kills, "судно", "судна", "судов"), state.biggest,
      plural(state.biggest, "клетка", "клетки", "клеток"))
    g.sunk = true
  elseif state.player and v.last_hit_by == state.player.vid and state.t - v.last_hit_t < KILL_CREDIT then
    state.kills = state.kills + 1
    notice("Ты потопил: " .. v.name .. ". Собирай обломки.")
  end
end

-- Касания клеток ---------------------------------------------------------------------------------

local function contact_strength(rec)
  return rec and not rec.dead and (part_of(rec).ram or 1) or 0
end

-- Толчок в точке (x, y): меняет ход судна, а если пришёлся не в середину веса — ещё и закручивает его.
local function push_vessel(v, dvx, dvy, x, y)
  v.vx, v.vy = v.vx + dvx, v.vy + dvy
  local rx, ry = x - v.cx, y - v.cy
  v.w = v.w + SPIN * v.mass * (rx * dvy - ry * dvx) / v.inertia
end

-- Удар двух судов: расходятся, отскакивают, и если сближались быстро — ломают друг другу клетки. Толчок делится
-- по весу: лёгкое судно отлетает дальше тяжёлого.
local function hit(a, ra, b, rb)
  local ax, ay = cell_center(a, ra.gx, ra.gy)
  local bx, by = cell_center(b, rb.gx, rb.gy)
  local nx, ny = bx - ax, by - ay
  local len = math.sqrt(nx * nx + ny * ny)
  if len < 1e-6 then
    nx, ny = b.cx - a.cx, b.cy - a.cy
    len = math.sqrt(nx * nx + ny * ny)
    if len < 1e-6 then nx, ny, len = 1, 0, 1 end
  end
  nx, ny = nx / len, ny / len
  local share_a = b.mass / (a.mass + b.mass)
  local share_b = 1 - share_a
  a.cx, a.cy = a.cx - nx * PUSH * share_a, a.cy - ny * PUSH * share_a
  b.cx, b.cy = b.cx + nx * PUSH * share_b, b.cy + ny * PUSH * share_b
  local closing = (a.vx - b.vx) * nx + (a.vy - b.vy) * ny
  if closing <= 0 then return end
  local j = closing * 1.3
  local mx, my = (ax + bx) / 2, (ay + by) / 2
  push_vessel(a, -nx * j * share_a, -ny * j * share_a, mx, my)
  push_vessel(b, nx * j * share_b, ny * j * share_b, mx, my)
  if closing < MIN_HIT then return end
  a.ai.charge_until, b.ai.charge_until = 0, 0
  local pair = math.min(a.vid, b.vid) .. ":" .. math.max(a.vid, b.vid)
  if (state.pair_cd[pair] or 0) > state.step then return end
  state.pair_cd[pair] = state.step + HIT_COOLDOWN
  local to_b = DMG_K * closing * contact_strength(ra) * 2 * a.mass / (a.mass + b.mass)
  local to_a = DMG_K * closing * contact_strength(rb) * 2 * b.mass / (a.mass + b.mass)
  damage(b, rb, to_b, a.vid)
  damage(a, ra, to_a, b.vid)
end

-- Паузы ударов, которые уже прошли: порядок обхода тут не важен, остаются только будущие.
local function forget_cooldowns()
  local kept = {}
  for pair, until_step in pairs(state.pair_cd) do
    if until_step > state.step then kept[pair] = until_step end
  end
  state.pair_cd = kept
end

-- Касания прошлого шага: на пару судов — одно, где бьёт таран, если он коснулся.
local function resolve_contacts()
  local best, pairs_order = {}, {}
  for _, c in ipairs(state.contacts) do
    local a, b = state.vessels[c.va], state.vessels[c.vb]
    if a and b then
      local ra, rb = a.grid[c.ka], b.grid[c.kb]
      if ra and rb then
        local pair = math.min(a.vid, b.vid) .. ":" .. math.max(a.vid, b.vid)
        local power = contact_strength(ra) + contact_strength(rb)
        local cur = best[pair]
        if cur == nil then pairs_order[#pairs_order + 1] = pair end
        if cur == nil or power > cur.power then
          best[pair] = { a = a, ra = ra, b = b, rb = rb, power = power }
        end
      end
    end
  end
  state.contacts = {}
  for _, pair in ipairs(pairs_order) do
    local c = best[pair]
    if c.a.alive and c.b.alive then hit(c.a, c.ra, c.b, c.rb) end
  end
end

-- Чужие суда -------------------------------------------------------------------------------------

local function nearest_vessel(v, range, accept)
  local found, best = nil, range
  for _, u in ipairs(state.order) do
    if u ~= v and u.alive and accept(u) then
      local d = dist(v.cx, v.cy, u.cx, u.cy)
      if d < best then found, best = u, d end
    end
  end
  return found, best
end

local function set_hunting(v, on)
  local helm = helm_of(v)
  if helm and helm.base and helm.base.hunting ~= on then helm.base.hunting = on end
end

-- Сколько судну нужно места у края моря, чтобы развернуться: край, запас и ширина круга разворота.
local function edge_room(v)
  return EDGE + 4 + 2 * v.radius
end

local function inside_room(v, x, y)
  local room = edge_room(v)
  return x >= room and x <= SEA - room and y >= room and y <= SEA - room
end

local function think(v)
  local ai = v.ai
  -- Сборщик боится любого, у кого таран; охотник — только тех, кто заметно тяжелее его.
  local threat = nearest_vessel(v, FLEE_RANGE, function(u)
    if not u.ram then return false end
    if v.kind == "gather" then return true end
    return u.mass > v.mass * THREAT_RATIO
  end)
  if threat then
    ai.mode, ai.prey = "flee", nil
    ai.tx, ai.ty = v.cx + (v.cx - threat.cx), v.cy + (v.cy - threat.cy)
    set_hunting(v, false)
    return
  end
  if v.ram then
    local prey = ai.prey
    if prey and (not prey.alive or state.t - ai.chase_from > CHASE_GIVE_UP
        or dist(v.cx, v.cy, prey.cx, prey.cy) > SIGHT_PREY * 1.4) then
      prey = nil
      ai.rest_until = state.t + 8
    end
    if prey == nil and (ai.rest_until or 0) <= state.t then
      prey = nearest_vessel(v, SIGHT_PREY, function(u) return u.mass <= v.mass * PREY_RATIO end)
      if prey then ai.chase_from = state.t end
    end
    ai.prey = prey
    if prey then
      ai.mode = "chase"
      ai.tx, ai.ty = prey.cx + prey.vx * 0.6, prey.cy + prey.vy * 0.6
      set_hunting(v, prey == state.player)
      return
    end
  end
  set_hunting(v, false)
  -- Обломок у самого края судно не ловит: развернуться там негде.
  local found, best = nil, SIGHT_DEBRIS
  for _, d in ipairs(state.debris) do
    if not d.hooked and inside_room(v, d.x, d.y) then
      local dd = dist(v.cx, v.cy, d.x, d.y)
      if dd < best then found, best = d, dd end
    end
  end
  if found then
    ai.mode = "gather"
    ai.tx, ai.ty = found.x, found.y
    return
  end
  if ai.mode ~= "wander" or ai.tx == nil or dist(v.cx, v.cy, ai.tx, ai.ty) < 4 then
    local a = math.random() * 2 * math.pi
    local r = 15 + math.random() * 25
    local room = edge_room(v)
    ai.tx = clamp(v.cx + math.cos(a) * r, room, SEA - room)
    ai.ty = clamp(v.cy + math.sin(a) * r, room, SEA - room)
  end
  ai.mode = "wander"
end

-- Руль и тяга к цели решения. Судну нужно место на разворот у края моря: цель ближе к краю придвигается
-- внутрь, а судно, которое туда всё же идёт, правит к середине моря. Охотник в погоне идёт за добычей до края.
local function steer_ai(v)
  local ai = v.ai
  ai.think = ai.think + 1
  if ai.prey and not ai.prey.alive then ai.think = THINK_EVERY end
  if ai.think >= THINK_EVERY then
    ai.think = 0
    think(v)
  end
  local room = edge_room(v)
  local near_edge = (ai.mode == "chase") and EDGE + 2 or room
  local tx = clamp(ai.tx or v.cx, near_edge, SEA - near_edge)
  local ty = clamp(ai.ty or v.cy, near_edge, SEA - near_edge)
  local fx, fy = forward(v)
  local to_edge = (v.cx < room and fx < -0.3) or (v.cx > SEA - room and fx > 0.3)
      or (v.cy < room and fy < -0.3) or (v.cy > SEA - room and fy > 0.3)
  if to_edge and ai.mode ~= "chase" then
    tx, ty = SEA / 2, SEA / 2
  end
  -- Таран: подойдя к добыче, охотник разгоняется, бьёт и заходит снова.
  v.boost = 1
  if ai.mode == "chase" and ai.prey then
    if dist(v.cx, v.cy, ai.prey.cx, ai.prey.cy) < CHARGE_RANGE and state.t >= (ai.charge_ready or 0) then
      ai.charge_until = state.t + CHARGE_TIME
      ai.charge_ready = state.t + CHARGE_EVERY
    end
    if state.t < (ai.charge_until or 0) then v.boost = CHARGE_BOOST end
  end
  local dx, dy = tx - v.cx, ty - v.cy
  local len = math.sqrt(dx * dx + dy * dy)
  if len > 0.01 then dx, dy = dx / len, dy / len end
  -- Кроме добычи, суда обходят тех, кто рядом: сборщики не толкаются у одного обломка.
  local near, d = nearest_vessel(v, KEEP_CLEAR, function(u) return u ~= ai.prey end)
  if near then
    local ax, ay = v.cx - near.cx, v.cy - near.cy
    local alen = math.max(0.01, math.sqrt(ax * ax + ay * ay))
    local push = 1.5 * (1 - d / KEEP_CLEAR)
    dx, dy = dx + ax / alen * push, dy + ay / alen * push
  end
  local diff = angle_diff(v.h, heading_to(dx, dy))
  -- Цель внутри круга разворота с той стороны, куда поворачивать: до неё не довернуть, судно ходило бы вокруг.
  -- Тогда прямо, пока цель не выйдет из круга, а там уже разворот.
  local side = diff >= 0 and 1 or -1
  local ox, oy = v.cx - fy * side * v.radius, v.cy + fx * side * v.radius
  if dist(ox, oy, tx, ty) < v.radius then
    v.steer = 0
  else
    -- Курс доворачивается за 0,5 секунды плюс двойное запаздывание поворота: тяжёлое судно доворачивает плавнее
    -- и не проскакивает курс. Быстрее, чем позволяет ход, не повернуть — тогда руль до упора. Судно ещё катится
    -- назад — руль перекладывается в другую сторону, как на заднем ходу.
    local u = v.vx * fx + v.vy * fy
    if math.abs(u) < 0.3 then u = 0.3 end
    local w_want = math.rad(diff) / (0.5 + 2 * v.rot_tau)
    v.steer = clamp(w_want * v.radius / u, -1, 1)
  end
  if ai.mode == "gather" and dist(v.cx, v.cy, tx, ty) < 3 then
    v.thrust = 0.5
  else
    v.thrust = 1
  end
  -- Застряло носом в край моря или в другое судно: вперёд хода нет, а без хода не повернуть. Тогда задний ход
  -- с рулём до упора в другую сторону — так нос отходит туда, куда надо, — и снова вперёд. Тяжёлое судно
  -- отходит дольше: заднего хода оно набирает медленнее.
  if state.t < (ai.back_until or 0) then
    v.thrust, v.steer = -1, -side
    return
  end
  if v.vx * fx + v.vy * fy < STUCK_SPEED then
    ai.slow = (ai.slow or 0) + DT
  else
    ai.slow = 0
  end
  if ai.slow > STUCK_TIME + v.tau then
    ai.slow = 0
    ai.back_until = state.t + BACK_TIME + v.tau
  end
end

local function count_kind(kind)
  local n = 0
  for _, v in ipairs(state.order) do
    if v.kind == kind then n = n + 1 end
  end
  return n
end

local function wanted(kind)
  local target = 0
  for _, row in ipairs(SCHEDULE) do
    if state.t >= row.t then target = row[kind] end
  end
  return target
end

local function spawn_ai(kind)
  local p = state.player
  for _ = 1, 24 do
    local a = math.random() * 2 * math.pi
    local r = SPAWN_MIN + math.random() * (SPAWN_MAX - SPAWN_MIN)
    local x, y = p.cx + math.cos(a) * r, p.cy + math.sin(a) * r
    if x > EDGE + 6 and x < SEA - EDGE - 6 and y > EDGE + 6 and y < SEA - EDGE - 6 then
      local free = true
      for _, u in ipairs(state.order) do
        if dist(x, y, u.cx, u.cy) < 15 then free = false break end
      end
      if free then
        new_vessel(kind, kind, x, y, math.random() * 360)
        return true
      end
    end
  end
  return false
end

local function populate()
  for _, kind in ipairs({ "gather", "raid", "hunt" }) do
    if count_kind(kind) < wanted(kind) and state.t >= state.next_spawn[kind] then
      if spawn_ai(kind) then state.next_spawn[kind] = state.t + SPAWN_GAP end
    end
  end
end

-- Ход ---------------------------------------------------------------------------------------------

-- Шаг судна. Сначала поворот: руль тянет его к ходу вдоль носа, делённому на радиус круга разворота, — без хода
-- к нулю, задним ходом в обратную сторону. Потом ход раскладывается по новому курсу: вдоль носа его ведут вёсла
-- и паруса, поперёк гасит вода. Сам ход остаётся прежним по миру, поэтому в повороте судно скользит боком.
local function move_vessel(v)
  local fx, fy = forward(v)
  local w_target = v.steer * (v.vx * fx + v.vy * fy) / v.radius
  v.w = v.w + (w_target - v.w) * math.min(1, DT / v.rot_tau)
  v.h = (v.h + math.deg(v.w) * DT) % 360
  fx, fy = forward(v)
  local rx, ry = -fy, fx
  local u = v.vx * fx + v.vy * fy
  local side = v.vx * rx + v.vy * ry
  local target = 0
  if v.thrust > 0 then
    target = v.vmax * v.thrust * (v.boost or 1)
  elseif v.thrust < 0 then
    target = v.vmax * REVERSE * v.thrust
  end
  target = target * (1 - TURN_LOSS * math.abs(v.steer))
  local tau = (v.thrust ~= 0) and v.tau or v.coast_tau
  u = u + (target - u) * math.min(1, DT / tau)
  side = side * math.exp(-DT / v.side_tau)
  v.vx, v.vy = fx * u + rx * side, fy * u + ry * side
  v.cx = v.cx + v.vx * DT
  v.cy = v.cy + v.vy * DT
  if v.cx < EDGE or v.cx > SEA - EDGE then
    v.cx, v.vx = clamp(v.cx, EDGE, SEA - EDGE), 0
  end
  if v.cy < EDGE or v.cy > SEA - EDGE then
    v.cy, v.vy = clamp(v.cy, EDGE, SEA - EDGE), 0
  end
end

-- Места клеток судна для движка. Поворот пишется, только когда сменился, и раньше места: запись поворота
-- ставит объект на поверхность под ним, то есть на дно, а место с высотой поднимает его обратно на воду.
local function draw_vessel(v)
  local turned = v.drawn_h ~= v.h
  local r = math.rad(v.h)
  local c, s = math.cos(r), math.sin(r)
  for _, rec in ipairs(v.cells) do
    local lx, ly = rec.gx - v.px, rec.gy - v.py
    local x, y = v.cx + lx * c - ly * s, v.cy + lx * s + ly * c
    local fresh = not rec.drawn
    if rec.base then
      if turned or fresh then rec.base.rotation = v.h end
      rec.base.position = { x = x - CELL / 2, y = y - CELL / 2, z = BASE_Z }
    end
    if rec.top then
      local top = part_of(rec).top
      local tx, ty = top_center(v, rec, x, y)
      if turned or fresh then rec.top.rotation = v.h end
      rec.top.position = { x = tx - top.size[1] / 2, y = ty - top.size[2] / 2, z = TOP_Z }
    end
    if rec.base and (rec.top or part_of(rec).top == nil) then rec.drawn = true end
  end
  v.drawn_h = v.h
end

local function control_player(g)
  local v = state.player
  v.thrust = (g.up and 1 or 0) - (g.down and 1 or 0)
  v.steer = (g.right and 1 or 0) - (g.left and 1 or 0)
  -- Запись снова привязывает камеру к рулевому, если игрок утащил вид правой кнопкой мыши.
  local helm = helm_of(v)
  if helm and helm.base then helm.base.camera_follows = true end
end

-- Обломки дрейфуют каждый в свою сторону; тот, что на крюке, тянется к рулевому, а рядом с сетью — к сети.
local function move_debris()
  local p = state.player
  local hx, hy = cell_center(p, 0, 0)
  local nets = {}
  for _, rec in ipairs(p.nets) do
    local x, y = cell_center(p, rec.gx, rec.gy)
    nets[#nets + 1] = { x = x, y = y, range = part_of(rec).pull }
  end
  local decay = math.exp(-1.5 * DT)
  for _, d in ipairs(state.debris) do
    local vx, vy = d.dx + d.vx, d.dy + d.vy
    d.vx, d.vy = d.vx * decay, d.vy * decay
    if d.hooked then
      local dx, dy = hx - d.x, hy - d.y
      local len = math.sqrt(dx * dx + dy * dy)
      if len > 0.01 then
        vx, vy = p.vx + dx / len * HOOK_SPEED, p.vy + dy / len * HOOK_SPEED
      end
    else
      for _, n in ipairs(nets) do
        local dx, dy = n.x - d.x, n.y - d.y
        local len = math.sqrt(dx * dx + dy * dy)
        if len < n.range and len > 0.01 then
          vx, vy = p.vx + dx / len * NET_SPEED, p.vy + dy / len * NET_SPEED
          break
        end
      end
    end
    d.x = d.x + vx * DT
    d.y = d.y + vy * DT
    if d.obj then d.obj.position = { x = d.x - d.w / 2, y = d.y - d.l / 2, z = d.z } end
  end
end

-- Верёвка крюка — тонкая доска от рулевого до обломка.
local function draw_rope()
  local d = state.hook
  if d == nil then return end
  local p = state.player
  local hx, hy = cell_center(p, 0, 0)
  if dist(hx, hy, d.x, d.y) > HOOK_DROP then
    drop_rope()
    return
  end
  if state.rope == nil then return end
  local dx, dy = d.x - hx, d.y - hy
  local len = math.max(0.05, math.sqrt(dx * dx + dy * dy))
  local mx, my = (hx + d.x) / 2, (hy + d.y) / 2
  state.rope.rotation = math.deg(math.atan(-dx, dy))
  state.rope.size = { x = 0.08, y = len }
  state.rope.position = { x = mx - 0.04, y = my - len / 2, z = TOP_Z + 0.15 }
end

local function keep_debris()
  if state.step % 60 ~= 0 then return end
  local p = state.player
  local near = 0
  local far = {}
  for _, d in ipairs(state.debris) do
    local dd = dist(p.cx, p.cy, d.x, d.y)
    if dd < DEBRIS_NEAR then near = near + 1 end
    -- Уплывшее за край, куда судам не доплыть, и слишком далёкое от плота тонет.
    local gone = d.x < EDGE - 3 or d.x > SEA - EDGE + 3 or d.y < EDGE - 3 or d.y > SEA - EDGE + 3
    if (dd > DEBRIS_FAR or gone) and not d.hooked then far[#far + 1] = d end
  end
  for _, d in ipairs(far) do
    remove_debris(d)
  end
  -- Новое больше впереди по ходу плота: туда он и плывёт.
  local fx, fy = forward(p)
  local ahead = heading_to(fx, fy)
  for _ = 1, math.min(5, DEBRIS_TARGET - near) do
    local spread = (math.random() < 0.7) and 240 or 360
    local a = math.rad(ahead - 90 + (math.random() - 0.5) * spread)
    local r = DEBRIS_SPAWN_MIN + math.random() * (DEBRIS_NEAR - DEBRIS_SPAWN_MIN)
    spawn_debris(pick_debris_kind(), p.cx + math.cos(a) * r, p.cy + math.sin(a) * r)
  end
end

-- Стройка -----------------------------------------------------------------------------------------

local function clear_ghosts()
  for _, obj in ipairs(find({ has = { "ghost" } })) do
    delete(obj)
  end
end

-- Пустые клетки рядом с плотом: щелчок по ним ставит выбранную деталь.
local function make_ghosts()
  clear_ghosts()
  local v = state.player
  local seen = {}
  for _, rec in ipairs(v.cells) do
    for _, d in ipairs({ { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } }) do
      local gx, gy = rec.gx + d[1], rec.gy + d[2]
      local k = key(gx, gy)
      if v.grid[k] == nil and not seen[k] then
        seen[k] = true
        local x, y = cell_center(v, gx, gy)
        create({
          ghost = true, gx = gx, gy = gy,
          position = { x - 0.43, y - 0.43, 0.02 }, size = { 0.86, 0.86 }, rotation = v.h,
          shape = "box", height = 0.05, color = "#a9dbe6", on_click = { { "clicked", true } },
        })
      end
    end
  end
end

-- Во время стройки паруса плота свёрнуты: высокое полотно не закрывает пустые клетки от щелчка.
local function furl_sails(furled)
  for _, rec in ipairs(state.player.cells) do
    if rec.top and rec.part == "sail" then
      rec.top.height = furled and FURLED or part_of(rec).top.height
    end
  end
end

-- Деталь клетки сменилась: прочность остаётся в той же доле, иначе «поставил и снял» чинило бы без досок.
local function change_part(rec, part)
  local share = rec.hp / rec.max_hp
  rec.part = part
  rec.max_hp = tables.parts[part].hp
  rec.hp = rec.max_hp * share
end

local function rebuild_cell(v, rec)
  delete_cell_objects(rec)
  create_cell_objects(v, rec)
end

local function build_at(gx, gy)
  local g, v = state.game, state.player
  local pk = state.chosen
  if pk == "remove" then
    g.tune_text = "«Снять» работает по клеткам плота, а не по пустым"
    return
  end
  local cost = new_cell_cost(pk)
  if not can_pay(cost) then
    g.tune_text = lacking(cost)
    return
  end
  pay(cost, 1)
  local rec = add_cell(v, gx, gy, pk)
  restat(v)
  create_cell_objects(v, rec)
  make_ghosts()
  refresh_ship()
  state.biggest = math.max(state.biggest, v.size)
  g.tune_text = "Поставлено: " .. tables.parts[pk].name
end

local function modify_cell(gx, gy)
  local g, v = state.game, state.player
  local rec = v.grid[key(gx, gy)]
  if rec == nil then return end
  local pk = state.chosen
  if pk == "remove" then
    if rec.part == "helm" then
      g.tune_text = "Кормило не снять: на нём стоишь ты"
      return
    end
    if rec.part ~= "deck" then
      pay(half(tables.parts[rec.part].cost), -1)
      local name = tables.parts[rec.part].name
      change_part(rec, "deck")
      rebuild_cell(v, rec)
      g.tune_text = "Снято: " .. name .. ", клетка осталась настилом"
    else
      local alive = connected(v, rec)
      for _, r in ipairs(v.cells) do
        if r ~= rec and not alive[r] then
          g.tune_text = "Эту клетку не снять: без неё плот развалится"
          return
        end
      end
      pay(half(tables.parts.deck.cost), -1)
      remove_cell(v, rec)
      g.tune_text = "Клетка снята"
    end
    restat(v)
    make_ghosts()
    refresh_ship()
    return
  end
  if pk == "deck" then
    g.tune_text = "Здесь уже есть настил: настил ставят на пустые клетки"
    return
  end
  if rec.part == pk then
    g.tune_text = "Здесь уже стоит деталь «" .. tables.parts[pk].name .. "»"
    return
  end
  if rec.part == "helm" then
    g.tune_text = "На кормиле стоит рулевой: сюда деталь не поставить"
    return
  end
  if rec.part ~= "deck" then
    g.tune_text = "Здесь уже деталь «" .. tables.parts[rec.part].name .. "»: сначала убери её кнопкой «Снять»"
    return
  end
  local cost = tables.parts[pk].cost
  if not can_pay(cost) then
    g.tune_text = lacking(cost)
    return
  end
  pay(cost, 1)
  change_part(rec, pk)
  rebuild_cell(v, rec)
  restat(v)
  refresh_ship()
  g.tune_text = "Поставлено: " .. tables.parts[pk].name
end

local function threat_near()
  local p = state.player
  for _, u in ipairs(state.order) do
    if u ~= p and u.ram and dist(p.cx, p.cy, u.cx, u.cy) < TUNE_SAFE then return u end
  end
  return nil
end

local function open_tuning()
  local g, p = state.game, state.player
  local u = threat_near()
  if u then
    notice("Рядом " .. u.name .. " — не до стройки. Уйди подальше.")
    return
  end
  drop_rope()
  state.tuning = true
  g.tuning = true
  p.vx, p.vy, p.w = 0, 0, 0
  refresh_part_rows()
  make_ghosts()
  furl_sails(true)
  g.tune_text = "Выбери деталь и щёлкни по пустой клетке у плота или по клетке самого плота"
end

local function close_tuning()
  local g = state.game
  state.tuning = false
  g.tuning = false
  clear_ghosts()
  refresh_part_rows()
  furl_sails(false)
end

local function hook(did)
  local d = state.debris_by_id[did]
  if d == nil or d.taken then return end
  local p = state.player
  local hx, hy = cell_center(p, 0, 0)
  if dist(hx, hy, d.x, d.y) > HOOK_RANGE then
    notice(string.format("Далеко: крюк летит на %d клеток", HOOK_RANGE))
    return
  end
  drop_rope()
  d.hooked = true
  state.hook = d
  create({
    rope = true, fresh = true, position = { hx, hy, TOP_Z + 0.15 }, size = { 0.08, 0.08 },
    shape = "box", height = 0.06, color = "#e6d8ae",
  })
end

-- Функции правил и кнопок -------------------------------------------------------------------------

function tick(g)
  state.game = g
  register_fresh()
  if not state.started then
    start_game(g)
    return
  end
  if g.sunk then return end
  if state.tuning then
    for _, v in ipairs(state.order) do draw_vessel(v) end
    return
  end
  state.step = state.step + 1
  state.t = state.t + DT
  resolve_contacts()
  if g.sunk then return end
  control_player(g)
  for _, v in ipairs(state.order) do
    if v ~= state.player then steer_ai(v) end
  end
  for _, v in ipairs(state.order) do
    move_vessel(v)
    draw_vessel(v)
  end
  move_debris()
  draw_rope()
  keep_debris()
  if state.step % 180 == 0 then populate() end
  if state.step % 600 == 0 then forget_cooldowns() end
  if state.step % 10 == 0 then refresh_stats() end
  if g.notice and state.t > state.notice_until then g.notice = false end
end

-- Клетка коснулась обломка: обломок — в трюм её судна.
function touch_debris(cell, obj)
  if state.tuning then return end
  local v = state.vessels[cell.vid]
  local d = state.debris_by_id[obj.did]
  if v == nil or not v.alive or d == nil or d.taken or state.t < d.ready_at then return end
  local rec = v.grid[key(cell.gx, cell.gy)]
  if rec == nil or rec.dead then return end
  local gives = tables.debris[d.kind].gives
  if v == state.player then
    for _, r in ipairs(RES) do state.res[r] = state.res[r] + (gives[r] or 0) end
    refresh_res()
  else
    for _, r in ipairs(RES) do v.cargo[r] = v.cargo[r] + (gives[r] or 0) end
  end
  remove_debris(d)
end

-- Клетки двух судов наложились: удар разбирается на следующем шаге, по одному на пару судов.
function touch_cell(a, b)
  if state.tuning or a.vid == b.vid then return end
  state.contacts[#state.contacts + 1] = { va = a.vid, ka = key(a.gx, a.gy), vb = b.vid, kb = key(b.gx, b.gy) }
end

function on_clicked(obj)
  obj.clicked = false
  if not state.started or state.game.sunk then return end
  if state.tuning then
    if obj.ghost then
      build_at(obj.gx, obj.gy)
    elseif obj.cell and obj.vid == state.player.vid then
      modify_cell(obj.gx, obj.gy)
    end
  elseif obj.debris then
    hook(obj.did)
  end
end

function toggle_tuning()
  if not state.started or state.game.sunk then return end
  if state.tuning then close_tuning() else open_tuning() end
end

function finish_tuning()
  if state.tuning then close_tuning() end
end

function pick_part(row)
  state.chosen = row.pk
  refresh_part_rows()
  local g = state.game
  if row.pk == "remove" then
    g.tune_text = "Щёлкни по клетке плота: снимется деталь, а голый настил — целиком"
  else
    g.tune_text = "Выбрано: " .. row.name .. ". Щёлкни по клетке"
  end
end

function repair()
  local g, v = state.game, state.player
  if not state.tuning then return end
  local missing = 0
  for _, rec in ipairs(v.cells) do missing = missing + (rec.max_hp - rec.hp) end
  if missing <= 0.01 then
    g.tune_text = "Чинить нечего: плот цел"
    return
  end
  if state.res.wood <= 0 then
    g.tune_text = "Чинить нечем: нужны доски"
    return
  end
  local boards = int(math.min(state.res.wood, math.ceil(missing / REPAIR_HP)))
  local hp = boards * REPAIR_HP
  for _, rec in ipairs(v.cells) do
    local add = math.min(hp, rec.max_hp - rec.hp)
    rec.hp = rec.hp + add
    hp = hp - add
    if rec.base then
      rec.base.hp = rec.hp
      rec.base.damaged = rec.hp < rec.max_hp
    end
  end
  pay({ wood = boards }, 1)
  g.tune_text = string.format("Починено, ушло досок: %d", boards)
end
