-- Ход к выбранному врагу, раздача каталога врагам и пошаговый бой без картинок («Фаза-15»,
-- «Фаза-02-враг-на-пути», требования 19–26): движок знает только `walk` и признаки
-- `hero`/`enemy_unit`/`obstacle`/`target` — очередь ударов, урон, гибель и возвращение героя к
-- входу целиком здесь. Каждая партия перечитывает этот файл заново, поэтому все переменные ниже
-- начинают партию с этих значений.

local TURN_STEPS = 60
local HIT_STEP = 14

local COMBAT_DISTANCE = 0.3

local state = {
  last_x = nil, last_y = nil,
  entrance = nil,
  enemies_ready = false,
  combat_active = false, combat_enemy = nil, combat_turn = "enemy",
  combat_cycle_step = 0,
}

local function center(obj)
  return obj.position.x + obj.size.x / 2, obj.position.y + obj.size.y / 2
end

-- Расстояние между прямоугольниками — обычное, по диагонали (гипотенуза зазоров по осям), 0 когда
-- они соприкасаются или перекрываются на этой оси.
local function rect_gap(a, b)
  local ax0, ay0 = a.position.x, a.position.y
  local ax1, ay1 = ax0 + a.size.x, ay0 + a.size.y
  local bx0, by0 = b.position.x, b.position.y
  local bx1, by1 = bx0 + b.size.x, by0 + b.size.y
  local dx = math.max(bx0 - ax1, ax0 - bx1, 0)
  local dy = math.max(by0 - ay1, ay0 - by1, 0)
  return math.sqrt(dx * dx + dy * dy)
end

local function find_enemy_by_name(name)
  local enemies = find({ has = { "enemy_unit" } })
  for i = 1, #enemies do
    if enemies[i].name == name then
      return enemies[i]
    end
  end
  return nil
end

local function find_target()
  return find({ has = { "enemy_unit", "target" } })[1]
end

-- Строка каталога у врага, чьё имя понятно называет ошибку, а не «attempt to index a nil value»,
-- когда в сцене опечатка в `enemy`.
local function enemy_catalog(unit)
  local catalog = tables.enemies[unit.enemy]
  if catalog == nil then
    error(
      string.format(
        "враг \"%s\": в enemies.json нет записи \"%s\"",
        tostring(unit.name),
        tostring(unit.enemy)
      )
    )
  end
  return catalog
end

-- Раздаёт каждому врагу из каталога здоровье, урон и имя.
local function init_enemies()
  local enemies = find({ has = { "enemy_unit" } })
  for i = 1, #enemies do
    local unit = enemies[i]
    local catalog = enemy_catalog(unit)
    unit.health = catalog.health
    unit.max_health = catalog.health
    unit.damage = catalog.damage
    unit.title = catalog.name
  end
end

local function end_combat()
  state.combat_active = false
  state.combat_enemy = nil
end

-- Герой пал: на том же шаге встаёт у входа с полным здоровьем, `walk_to` и выбор врага сняты.
local function revive_hero(hero)
  hero.position = { x = state.entrance.x, y = state.entrance.y }
  hero.health = hero.max_health
  hero.walk_to = nil
  local enemies = find({ has = { "enemy_unit" } })
  for i = 1, #enemies do
    enemies[i].target = false
  end
end

local function try_start_combat(hero)
  if hero.walk_to ~= nil then
    return
  end
  local target = find_target()
  if target == nil or target.health <= 0 then
    return
  end
  if rect_gap(hero, target) > COMBAT_DISTANCE then
    return
  end
  state.combat_active = true
  state.combat_enemy = target.name
  state.combat_turn = "enemy"
  state.combat_cycle_step = 0
end

-- Удары каждые 60 шагов по очереди, первым бьёт враг, урон засчитывается на 15-м шаге удара.
-- Цель падает на 0 здоровья или ниже — бой сразу заканчивается, враг исчезает на том же шаге.
local function run_combat_tick(hero, enemy)
  local attacker, defender = enemy, hero
  if state.combat_turn ~= "enemy" then
    attacker, defender = hero, enemy
  end

  local step = state.combat_cycle_step
  if step == HIT_STEP then
    defender.health = defender.health - attacker.damage
    if defender.health <= 0 then
      end_combat()
      if defender == enemy then
        delete(enemy)
      else
        revive_hero(hero)
      end
      return
    end
  end

  state.combat_cycle_step = step + 1
  if state.combat_cycle_step >= TURN_STEPS then
    state.combat_cycle_step = 0
    state.combat_turn = (state.combat_turn == "enemy") and "hero" or "enemy"
  end
end

-- Точка у середины врага, сдвинутая на 0,05 к герою по большей из осей: цель внутри раздутого
-- врага движок выносит к ближайшему краю, и сдвиг делает этим краем тот, что смотрит на героя —
-- герой встаёт вплотную к врагу с той стороны, откуда пришёл.
local function approach_point(hero, target)
  local ex, ey = center(target)
  local hx, hy = center(hero)
  local dx, dy = hx - ex, hy - ey
  if math.abs(dx) >= math.abs(dy) then
    ex = ex + (dx >= 0 and 0.05 or -0.05)
  else
    ey = ey + (dy >= 0 and 0.05 or -0.05)
  end
  return ex, ey
end

-- Стоит первым в rules.json, перед `walk`: запоминает вход, а щелчок по врагу превращает в путь к
-- выбранному врагу — герой встаёт вплотную к нему, откуда бы ни пришёлся щелчок по телу (точка
-- земли под курсором у капсулы лежит за ней). Сторону подхода считает от места героя каждый шаг:
-- издали это сторона, откуда он идёт, а у самого врага — та, к которой путь и правда выводит.
-- Щелчок по земле снимает выбор у врагов раньше, чем сюда доходит очередь, и герой идёт куда
-- указали.
function hero_pre_walk(hero)
  if state.entrance == nil then
    state.entrance = { x = hero.position.x, y = hero.position.y }
  end
  if hero.walk_to == nil then
    return
  end
  local target = find_target()
  if target == nil then
    return
  end
  local x, y = approach_point(hero, target)
  hero.walk_to = { x = x, y = y }
end

function update_game(hero)
  if not state.enemies_ready then
    init_enemies()
    state.enemies_ready = true
  end

  local moved = false
  if state.last_x ~= nil then
    moved = hero.position.x ~= state.last_x or hero.position.y ~= state.last_y
  end

  -- Бой прерывается и когда герой сдвинулся, и когда у выбранного врага пропал `target` (щелчок,
  -- снявший выбор, без движения героя) — повторный щелчок по тому же врагу возвращает ему
  -- `target` тем же ходом и хода ударов не сбивает.
  if state.combat_active then
    local enemy = find_enemy_by_name(state.combat_enemy)
    if moved or enemy == nil or not enemy.target then
      end_combat()
    end
  end

  if not state.combat_active then
    try_start_combat(hero)
  end

  if state.combat_active then
    local enemy = find_enemy_by_name(state.combat_enemy)
    if enemy == nil then
      end_combat()
    else
      run_combat_tick(hero, enemy)
    end
  end

  state.last_x, state.last_y = hero.position.x, hero.position.y
end
