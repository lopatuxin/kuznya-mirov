-- Обычный зомби вылезает из дыры посреди ямы и ковыляет к выходу справа. Земля — нарисованная плита
-- 44,543 × 28 клеток от x = 0,2285, картина ямы 2117 × 941 точек: точка картины по ширине — 0,02104
-- клетки, по глубине — 0,029756 (глубина под камерой 45° видна в 0,707 раза короче). Кадры зомби
-- лежат на земле плоскими картинками, как куски той же картины.
local DT = 1 / 60
local PX, PY = 44.543 / 2117, 28 / 941
local LEFT = 0.2285

-- Новый зомби появляется раз в столько секунд.
local SPAWN_EVERY = 3.5

-- Вылезание: 130 кадров по 1/12 с. Кадр — кусок картины от (1007; 374,5) размером 329 × 194 точки:
-- так напечатала нарезка листа по картинке дыры.
local CLIMB_FRAMES, CLIMB_FRAME_TIME = 130, 1 / 12
local CLIMB_POS = { LEFT + 1007 * PX, 374.5 * PY }
local CLIMB_SIZE = { 329 * PX, 194 * PY }

-- Ходьба: 36 кадров по 1/24 с. Первый кадр ходьбы стоит там, где кончилось вылезание: кадр листа
-- 252 × 256 в масштабе 0,602 от (1181,8; 379,3) точки картины.
local WALK_FRAMES, WALK_FRAME_TIME = 36, 1 / 24
local WALK_SCALE = 0.602
local WALK_POS = { LEFT + (1183 - 2 * WALK_SCALE) * PX, (389.5 - 17 * WALK_SCALE) * PY }
local WALK_SIZE = { 252 * WALK_SCALE * PX, 256 * WALK_SCALE * PY }
-- Скорость по ступням в ролике: 0,3 роста кадра ходьбы в секунду.
local WALK_SPEED = 0.3 * 256 * WALK_SCALE * PX

local CLIMB_TIME = CLIMB_FRAMES * CLIMB_FRAME_TIME

function spawn_zombies(spawner)
  if spawner.spawn_wait > 0 then return end
  spawner.spawn_wait = SPAWN_EVERY
  create({
    zombie = true,
    image = "zombie_climb",
    position = CLIMB_POS,
    size = CLIMB_SIZE,
    zombie_age = 0,
    zombie_frame = 0,
  })
end

function step_zombie(zombie)
  zombie.zombie_age = zombie.zombie_age + DT
  local age = zombie.zombie_age
  if age < CLIMB_TIME then
    zombie.zombie_frame = math.floor(age / CLIMB_FRAME_TIME)
    return
  end
  local walked = age - CLIMB_TIME
  if zombie.image ~= "zombie_walk" then
    zombie.image = "zombie_walk"
    zombie.size = { x = WALK_SIZE[1], y = WALK_SIZE[2] }
  end
  zombie.zombie_frame = math.floor(walked / WALK_FRAME_TIME) % WALK_FRAMES
  zombie.position = { x = WALK_POS[1] + walked * WALK_SPEED, y = WALK_POS[2] }
end
