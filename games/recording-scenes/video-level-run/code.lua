-- Первый уровень: кадры Ильи по его движению и возврат в начало после падения в ущелье.

local DT = 1 / 60
local START_X = 3
local START_Y = 7.1

-- Кадры листа hero.png: стойка 0–3, бег 4–11, прыжок 12–14 (подъём, верх, падение).
local IDLE_FIRST, IDLE_COUNT = 0, 4
local RUN_FIRST, RUN_COUNT = 4, 8
local JUMP_UP, JUMP_TOP, JUMP_DOWN = 12, 13, 14

-- Круг дыхания в секундах и путь в клетках за круг бега: ноги не скользят по земле.
local IDLE_CYCLE = 1.2
local STRIDE = 2.8
-- Медленнее этого по ширине — стоит; ближе к верхней точке этой доли скорости отрыва — кадр «верх».
local STILL_SPEED = 0.5
local TOP_SHARE = 0.3

function animate(hero)
  local v = hero.velocity
  if v.x > STILL_SPEED then
    hero.flip_x = false
  elseif v.x < -STILL_SPEED then
    hero.flip_x = true
  end

  if not hero.on_ground then
    local takeoff = 2 * hero.jump_height / hero.jump_time
    if v.y < -takeoff * TOP_SHARE then
      hero.hero_frame = JUMP_UP
    elseif v.y < takeoff * TOP_SHARE then
      hero.hero_frame = JUMP_TOP
    else
      hero.hero_frame = JUMP_DOWN
    end
    return
  end

  if math.abs(v.x) > STILL_SPEED then
    hero.run_dist = (hero.run_dist + math.abs(v.x) * DT) % STRIDE
    hero.hero_frame = RUN_FIRST + math.floor(hero.run_dist / STRIDE * RUN_COUNT)
  else
    hero.idle_time = (hero.idle_time + DT) % IDLE_CYCLE
    hero.hero_frame = IDLE_FIRST + math.floor(hero.idle_time / IDLE_CYCLE * IDLE_COUNT)
  end
end

function respawn(hero)
  -- Упал — дальше Илья стоит в начале: ролик показывает возврат, а не новый забег.
  hero.pilot_off = true
  hero.hold_right = false
  hero.hold_jump = false
  hero.position = { x = START_X, y = START_Y }
  hero.velocity = { x = 0, y = 0 }
end

-- Запись ролика: Ильёй ведёт код. Сначала он стоит, потом бежит вправо и прыгает с земли,
-- когда левый край героя дошёл до места прыжка; прыжок держится полсекунды — высокий.
local WAIT_STEPS = 72
local JUMPS = { 19.5, 32.6, 40.3, 48.9, 54.6, 60.6, 79.3, 83.6, 110.9 }
local USED_JUMPS = 9
local HOLD_STEPS = 30

function pilot(hero)
  if hero.pilot_off then
    return
  end
  hero.pilot_step = hero.pilot_step + 1
  if hero.pilot_step <= WAIT_STEPS then
    return
  end
  hero.hold_right = true
  if hero.pilot_hold > 0 then
    hero.pilot_hold = hero.pilot_hold - 1
    if hero.pilot_hold == 0 then
      hero.hold_jump = false
    end
    return
  end
  local next = hero.pilot_next
  if next <= USED_JUMPS and hero.on_ground and hero.position.x >= JUMPS[next] then
    hero.hold_jump = true
    hero.pilot_hold = HOLD_STEPS
    hero.pilot_next = next + 1
  end
end
