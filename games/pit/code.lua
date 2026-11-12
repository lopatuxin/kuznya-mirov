local DT = 1 / 60
local PX, PY = 44.543 / 2117, 28 / 941
local LEFT = 0.2285
local SPAWN_EVERY = 10
local FLOOR_Y = 385.3
-- Нора посреди дна: середина на картине и радиус в клетках — как у земли с норой (art/pit/ground/build_ground_hole.mjs).
local HOLE_X, HOLE_Y, HOLE_RADIUS = 1068, 490, 1.45
local EMERGE_RIM, EMERGE_TRAVEL, EMERGE_TIME, EMERGE_SETTLE = 0.24, 0.7, 8.5, 1
local SLOPE_START_X, SLOPE_END_X = 1447, 1615
local TOP_Y = 274.4
local WALK_SPEED = 144.5 * PX / (3 * 2.71)
local ORIGIN_X, ORIGIN_Y = 102.5 * 0.54, 217.06 * 0.54
local CLIMB_TIME, FADE_TIME = 6.98 / 0.8, 12 / 60
local RAMP_RUN, RAMP_RISE, ROOT_TRAVEL = 0.8735, 0.8137, 1.3835
local RUN = (SLOPE_END_X - SLOPE_START_X) * PX
local RISE = (FLOOR_Y - TOP_Y) * PY
local A, B = RAMP_RUN * RAMP_RUN - RAMP_RISE * RAMP_RISE, 2 * RAMP_RISE * RISE
local HEIGHT = (-B + math.sqrt(B * B + 4 * A * (RUN * RUN + RISE * RISE))) / (2 * A)
local SIN = RUN / (HEIGHT * RAMP_RUN)
local COS = (HEIGHT * RAMP_RISE - RISE) / (HEIGHT * RAMP_RUN)
local ROTATION = -math.deg(math.atan(SIN, COS))
-- Зомби вылезает из норы посреди дна: кромка, за которую он хватается в движении «подъём_зомби_из_ямы»,
-- приходится на правый край норы, после движения таз стоит на EMERGE_TRAVEL роста впереди.
local HOLE = LEFT + HOLE_X * PX + HOLE_RADIUS - EMERGE_RIM * HEIGHT * SIN
local HOLE_ROW = HOLE_Y * PY
local START = HOLE + EMERGE_TRAVEL * HEIGHT * SIN
local BASE_X = LEFT + (SLOPE_START_X + ORIGIN_X) * PX - 0.26 * HEIGHT * SIN
local BASE_Y = (FLOOR_Y + ORIGIN_Y) * PY - 0.26 * HEIGHT * COS
local EXIT_X, EXIT_Y, EXIT_Z = ROOT_TRAVEL * HEIGHT * SIN, ROOT_TRAVEL * HEIGHT * COS, RAMP_RISE * HEIGHT

local function place(zombie, x, y, z)
  zombie.position = { x = x - 0.3, y = y - 0.3, z = z }
end

local function leave_slope(zombie, age)
  local fade = math.min(1, (age + DT) / FADE_TIME)
  place(zombie, BASE_X + EXIT_X * fade + age * WALK_SPEED, BASE_Y + EXIT_Y * fade, EXIT_Z * fade)
end

function spawn_zombies(spawner)
  if spawner.spawn_wait > 0 then return end
  spawner.spawn_wait = SPAWN_EVERY
  create({
    zombie = true,
    model = "zombie",
    position = { HOLE - 0.3, HOLE_ROW - 0.3, 0 },
    size = { 0.6, 0.6 },
    height = HEIGHT,
    rotation = ROTATION,
    animation = "подъём_зомби_из_ямы",
    zombie_age = 0,
    zombie_stage = 3,
  })
end

function step_zombie(zombie)
  local age = zombie.zombie_age + DT
  zombie.zombie_age = age
  if zombie.zombie_stage == 3 then
    if age + 0.000001 < EMERGE_TIME then return end
    zombie.zombie_stage = 0
    zombie.zombie_age = 0
    zombie.animation = ""
    place(zombie, HOLE + (START - HOLE) * math.min(1, DT / FADE_TIME), HOLE_ROW, 0)
    return
  end
  if zombie.zombie_stage == 0 then
    local fade = math.min(1, (age + DT) / FADE_TIME)
    local x = HOLE + (START - HOLE) * fade + age * WALK_SPEED
    local settle = math.min(1, age / EMERGE_SETTLE)
    place(zombie, math.min(x, BASE_X), HOLE_ROW + (BASE_Y - HOLE_ROW) * settle * settle * (3 - 2 * settle), 0)
    if x >= BASE_X then
      zombie.zombie_stage = 1
      zombie.zombie_age = 0
      zombie.animation = "подъём_зомби_по_склону"
    end
    return
  end
  if zombie.zombie_stage == 1 then
    if age + 0.000001 < CLIMB_TIME then return end
    zombie.zombie_stage = 2
    zombie.zombie_age = 0
    zombie.animation = ""
    leave_slope(zombie, 0)
    return
  end
  leave_slope(zombie, age)
end
