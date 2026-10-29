-- Обычный зомби вылезает из дыры посреди ямы, идёт к правому пологому склону, карабкается по нему на
-- четвереньках и ковыляет по траве дальше вправо. Земля — нарисованная плита 44,543 × 28 клеток от
-- x = 0,2285, картина ямы 2117 × 941 точек: точка картины по ширине — 0,02104 клетки, по глубине — 0,029756
-- (глубина под камерой 45° видна в 0,707 раза короче). Кадры зомби лежат на земле плоскими картинками, как
-- куски той же картины.
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

-- До склона зомби идёт два круга ходьбы: ролик подъёма начинается с первого кадра ходьбы на 138,7 точки
-- правее, у подножия склона.
local FLOOR_WALK_TIME = 2 * WALK_FRAMES * WALK_FRAME_TIME

-- Подъём по склону: 105 кадров по 1/12 с. Кадр — кусок картины от (1319,5; 200) размером 570,5 × 329,5
-- точки: нарезка листа по куску картины со склоном (от (1250; 200), увеличен вдвое).
local SLOPE_FRAMES, SLOPE_FRAME_TIME = 105, 1 / 12
local SLOPE_POS = { LEFT + 1319.5 * PX, 200 * PY }
local SLOPE_SIZE = { 570.5 * PX, 329.5 * PY }

-- Наверху ходьба продолжается с кадра 17: он ближе всех по силуэту к последнему кадру подъёма и
-- стоит на его месте от (1651,5; 255,5) точки картины.
local TOP_WALK_FRAME = 17
local TOP_WALK_POS = { LEFT + 1651.5 * PX, 255.5 * PY }

local CLIMB_TIME = CLIMB_FRAMES * CLIMB_FRAME_TIME
local SLOPE_START = CLIMB_TIME + FLOOR_WALK_TIME
local TOP_START = SLOPE_START + SLOPE_FRAMES * SLOPE_FRAME_TIME

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

local function show(zombie, image, size)
  if zombie.image == image then return end
  zombie.image = image
  zombie.size = { x = size[1], y = size[2] }
end

local function walk(zombie, from, walked, first_frame)
  show(zombie, "zombie_walk", WALK_SIZE)
  zombie.zombie_frame = (first_frame + math.floor(walked / WALK_FRAME_TIME)) % WALK_FRAMES
  zombie.position = { x = from[1] + walked * WALK_SPEED, y = from[2] }
end

function step_zombie(zombie)
  zombie.zombie_age = zombie.zombie_age + DT
  local age = zombie.zombie_age
  if age < CLIMB_TIME then
    zombie.zombie_frame = math.floor(age / CLIMB_FRAME_TIME)
  elseif age < SLOPE_START then
    walk(zombie, WALK_POS, age - CLIMB_TIME, 0)
  elseif age < TOP_START then
    show(zombie, "zombie_slope", SLOPE_SIZE)
    zombie.zombie_frame = math.floor((age - SLOPE_START) / SLOPE_FRAME_TIME)
    zombie.position = { x = SLOPE_POS[1], y = SLOPE_POS[2] }
  else
    walk(zombie, TOP_WALK_POS, age - TOP_START, TOP_WALK_FRAME)
  end
end
