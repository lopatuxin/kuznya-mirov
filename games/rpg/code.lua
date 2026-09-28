-- Ходьба героя, раздача каталога врагам и пошаговый бой («Фаза-02-враг-на-пути», требования
-- 19–26): движок знает только `walk` и признаки `hero`/`enemy_unit`/`obstacle`/`target` — очередь
-- ударов, урон, падение и полоски целиком здесь. У каждого вида — два листа картинок: основной
-- (9 столбцов × 5 строк 64×64: строки 0–3 — ходьба вверх/влево/вниз/вправо по 9 кадров, кадр 0 —
-- стойка; строка 4 — падение, 6 кадров) и удара (6 столбцов × 4 строки той же стороны, свой файл).
-- Каждая партия перечитывает этот файл заново, поэтому все переменные ниже начинают партию с этих
-- значений.

local MAIN_COLUMNS = 9
local FALL_ROW = 4
local WALK_STEP_FRAMES = 5
local FALL_STEP_FRAMES = 5
local FALL_TOTAL_STEPS = 30

local ATTACK_COLUMNS = 6
local ATTACK_STEP_FRAMES = 5
local ATTACK_TOTAL_STEPS = 30
local TURN_STEPS = 60
local HIT_STEP = 14

local COMBAT_DISTANCE = 0.3

local ROW = { up = 0, left = 1, down = 2, right = 3 }

-- Требование 16, «Проверка перед запуском»: без этой таблицы имена листов удара нигде не стояли
-- бы в тексте кода буквально (код читает их по имени персонажа, не по строке), и картинка
-- считалась бы объявленной и нигде не названной.
local ATTACK_IMAGE_BY_BASE = {
  hero = "hero_attack",
  goblin = "goblin_attack",
  orc = "orc_attack",
}

local state = {
  last_x = nil, last_y = nil, last_side = "down", walk_phase = 0,
  entrance = nil,
  enemies_ready = false,
  hero_falling = false, hero_fall_step = 0,
  enemy_falling = nil, enemy_fall_step = 0,
  combat_active = false, combat_enemy = nil, combat_turn = "enemy",
  combat_cycle_step = 0,
}

local function main_frame(row, col)
  return MAIN_COLUMNS * row + col
end

local function attack_frame(row, col)
  return ATTACK_COLUMNS * row + col
end

local function idle_frame(side)
  return main_frame(ROW[side], 0)
end

local function side_from_delta(dx, dy)
  if math.abs(dx) >= math.abs(dy) then
    return dx >= 0 and "right" or "left"
  end
  return dy >= 0 and "down" or "up"
end

local function center(obj)
  return obj.position.x + obj.size.x / 2, obj.position.y + obj.size.y / 2
end

-- Требование 23: расстояние между прямоугольниками — обычное, по диагонали (гипотенуза зазоров
-- по осям), 0 когда они соприкасаются или перекрываются на этой оси.
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

-- Требование 20: строка каталога у врага, чьё имя понятно называет ошибку, а не «attempt to
-- index a nil value», когда в сцене опечатка в `enemy`.
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

local function base_image_name(unit)
  if unit.hero then
    return "hero"
  end
  return enemy_catalog(unit).image
end

-- Требование 22: раздаёт каждому врагу из каталога здоровье, урон и имя, лицом вниз.
local function init_enemies()
  local enemies = find({ has = { "enemy_unit" } })
  for i = 1, #enemies do
    local unit = enemies[i]
    local catalog = enemy_catalog(unit)
    unit.health = catalog.health
    unit.max_health = catalog.health
    unit.damage = catalog.damage
    unit.title = catalog.name
    unit.frame = idle_frame("down")
  end
end

local function progress_enemy_fall()
  if state.enemy_falling == nil then
    return
  end
  local enemy = find_enemy_by_name(state.enemy_falling)
  if enemy == nil then
    state.enemy_falling = nil
    return
  end
  local step = state.enemy_fall_step
  enemy.frame = main_frame(FALL_ROW, math.min(5, math.floor(step / FALL_STEP_FRAMES)))
  state.enemy_fall_step = step + 1
  if state.enemy_fall_step >= FALL_TOTAL_STEPS then
    delete(enemy)
    state.enemy_falling = nil
    state.enemy_fall_step = 0
  end
end

-- Требование 26: герой падает 30 шагов, затем встаёт у входа с полным здоровьем, лицом вниз, без
-- цели.
local function progress_hero_fall(hero)
  local step = state.hero_fall_step
  hero.frame = main_frame(FALL_ROW, math.min(5, math.floor(step / FALL_STEP_FRAMES)))
  state.hero_fall_step = step + 1
  if state.hero_fall_step >= FALL_TOTAL_STEPS then
    hero.position = { x = state.entrance.x, y = state.entrance.y }
    hero.health = hero.max_health
    hero.walk_to = nil
    hero.frame = idle_frame("down")
    state.hero_falling = false
    state.hero_fall_step = 0
    state.last_side = "down"
    state.walk_phase = 0
    local enemies = find({ has = { "enemy_unit" } })
    for i = 1, #enemies do
      enemies[i].target = false
    end
  end
end

local function animate_hero_walk(hero, dx, dy, moved)
  if moved then
    state.last_side = side_from_delta(dx, dy)
    local col = 1 + math.floor(state.walk_phase / WALK_STEP_FRAMES) % 8
    hero.frame = main_frame(ROW[state.last_side], col)
    state.walk_phase = state.walk_phase + 1
  else
    state.walk_phase = 0
    hero.frame = idle_frame(state.last_side)
  end
end

-- Требование 25: возвращает и бьющего, и цель на основной лист, лицом вниз у врага — годится и
-- для ушедшего героя, и для снятого щелчком выбора, пока герой не сдвинулся.
local function interrupt_combat(hero, enemy)
  if enemy ~= nil then
    enemy.image = base_image_name(enemy)
    enemy.frame = idle_frame("down")
  end
  hero.image = base_image_name(hero)
  state.combat_active = false
  state.combat_enemy = nil
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

-- Требование 24: удары каждые 60 шагов по очереди, взмах — 6 кадров по 5 шагов на листе удара,
-- урон на 15-м шаге (индекс 14), затем снова основной лист. Требование 26: цель падает на 0
-- здоровья или ниже, бой сразу заканчивается, уцелевший поворачивается лицом вниз.
local function run_combat_tick(hero, enemy)
  local hx, hy = center(hero)
  local ex, ey = center(enemy)
  local hero_side = side_from_delta(ex - hx, ey - hy)
  local enemy_side = side_from_delta(hx - ex, hy - ey)

  local attacker, defender, attacker_side, defender_side
  if state.combat_turn == "enemy" then
    attacker, defender, attacker_side, defender_side = enemy, hero, enemy_side, hero_side
  else
    attacker, defender, attacker_side, defender_side = hero, enemy, hero_side, enemy_side
  end

  local step = state.combat_cycle_step
  if step < ATTACK_TOTAL_STEPS then
    if step == 0 then
      attacker.image = ATTACK_IMAGE_BY_BASE[base_image_name(attacker)]
    end
    attacker.frame = attack_frame(ROW[attacker_side], math.floor(step / ATTACK_STEP_FRAMES))
  else
    if step == ATTACK_TOTAL_STEPS then
      attacker.image = base_image_name(attacker)
    end
    attacker.frame = idle_frame(attacker_side)
  end
  defender.frame = idle_frame(defender_side)

  if step == HIT_STEP then
    defender.health = defender.health - attacker.damage
    if defender.health <= 0 then
      attacker.image = base_image_name(attacker)
      if defender == enemy then
        -- Требование 26: герой остаётся лицом туда, где стоял погибший враг — не «вниз» и не
        -- прежней стороной ходьбы, которую следом переписал бы `animate_hero_walk` этим же
        -- шагом (`state.last_side` держит именно то, что читает `animate_hero_walk`).
        attacker.frame = idle_frame(attacker_side)
        state.last_side = attacker_side
        state.enemy_falling = enemy.name
        state.enemy_fall_step = 0
      else
        attacker.frame = idle_frame("down")
        state.hero_falling = true
        state.hero_fall_step = 0
      end
      state.combat_active = false
      state.combat_enemy = nil
      return
    end
  end

  state.combat_cycle_step = step + 1
  if state.combat_cycle_step >= TURN_STEPS then
    state.combat_cycle_step = 0
    state.combat_turn = (state.combat_turn == "enemy") and "hero" or "enemy"
  end
end

-- Стоит первым в rules.json, перед `walk`: ловит клик героя до того, как тот успеет сдвинуть его
-- на этом же шаге (требование 26: «пока герой падает, щелчки его не двигают»).
function hero_pre_walk(hero)
  if state.entrance == nil then
    state.entrance = { x = hero.position.x, y = hero.position.y }
  end
  if state.hero_falling then
    hero.walk_to = nil
  end
end

function update_game(hero)
  if not state.enemies_ready then
    init_enemies()
    state.enemies_ready = true
  end

  progress_enemy_fall()

  local moved, dx, dy = false, 0, 0
  if state.last_x ~= nil then
    dx = hero.position.x - state.last_x
    dy = hero.position.y - state.last_y
    moved = dx ~= 0 or dy ~= 0
  end

  -- Требования 21, 23: бой прерывается и когда герой сдвинулся, и когда у выбранного врага
  -- пропал `target` (щелчок, снявший выбор, без движения героя) — повторный щелчок по тому же
  -- врагу возвращает ему `target` тем же ходом и хода ударов не сбивает.
  if state.combat_active then
    local enemy = find_enemy_by_name(state.combat_enemy)
    if moved or enemy == nil or not enemy.target then
      interrupt_combat(hero, enemy)
    end
  end

  if state.hero_falling then
    progress_hero_fall(hero)
    state.last_x, state.last_y = hero.position.x, hero.position.y
    return
  end

  if not state.combat_active then
    try_start_combat(hero)
  end

  if state.combat_active then
    local enemy = find_enemy_by_name(state.combat_enemy)
    if enemy == nil then
      state.combat_active = false
    else
      run_combat_tick(hero, enemy)
    end
  end

  if not state.combat_active then
    animate_hero_walk(hero, dx, dy, moved)
  end

  state.last_x, state.last_y = hero.position.x, hero.position.y
end
