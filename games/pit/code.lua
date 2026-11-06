local DT = 1 / 60
local PX, PY = 44.543 / 2117, 28 / 941
local LEFT = 0.2285
local SPAWN_EVERY = 3.5
local START_X, FLOOR_Y = 1184.4, 385.3
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
local START = LEFT + (START_X + ORIGIN_X) * PX
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
    position = { START - 0.3, BASE_Y - 0.3, 0 },
    size = { 0.6, 0.6 },
    height = HEIGHT,
    rotation = ROTATION,
    zombie_age = 0,
    zombie_stage = 0,
  })
end

function step_zombie(zombie)
  local age = zombie.zombie_age + DT
  zombie.zombie_age = age
  if zombie.zombie_stage == 0 then
    local x = START + age * WALK_SPEED
    place(zombie, math.min(x, BASE_X), BASE_Y, 0)
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
