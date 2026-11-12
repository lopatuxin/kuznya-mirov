local DT = 1 / 60
local PX, PY = 44.543 / 2117, 28 / 941
local LEFT = 0.2285
local SPAWN_JITTER = 0.4
local FLOOR_Y = 385.3
-- Нора посреди дна: середина на картине и радиус в клетках — как у земли с норой (art/pit/ground/build_ground_hole.mjs).
local HOLE_X, HOLE_Y, HOLE_RADIUS = 1068, 490, 1.45
local EMERGE_RIM, EMERGE_TRAVEL, EMERGE_TIME = 0.24, 0.7, 8.5
local TURN_TIME = 1
local SLOPE_START_X, SLOPE_END_X = 1447, 1615
-- На картине начало склона сдвигается вправо ближе к камере.
local SLOPE_X_PER_ROW = 0.18
local TOP_Y = 274.4
local WALK_SPEED = 144.5 * PX / (3 * 2.71)
local WALK_ROTATION = -90
local ZOMBIE_SIZE = 0.31
local ORIGIN_X, ORIGIN_Y = 102.5 * 0.54, 217.06 * 0.54
local CLIMB_TIME, FADE_TIME = 6.98 / 0.8, 12 / 60
local RAMP_RUN, RAMP_RISE, ROOT_TRAVEL = 1.683368, 1.568124, 2.193368
local RUN = (SLOPE_END_X - SLOPE_START_X) * PX
local RISE = (FLOOR_Y - TOP_Y) * PY
local A, B = RAMP_RUN * RAMP_RUN - RAMP_RISE * RAMP_RISE, 2 * RAMP_RISE * RISE
local HEIGHT = (-B + math.sqrt(B * B + 4 * A * (RUN * RUN + RISE * RISE))) / (2 * A)
local SIN = RUN / (HEIGHT * RAMP_RUN)
local COS = (HEIGHT * RAMP_RISE - RISE) / (HEIGHT * RAMP_RUN)
local BASE_X = LEFT + (SLOPE_START_X + ORIGIN_X) * PX - 0.26 * HEIGHT * SIN
local BASE_Y = (FLOOR_Y + ORIGIN_Y) * PY - 0.26 * HEIGHT * COS
local EXIT_X, EXIT_Z = ROOT_TRAVEL * HEIGHT, RAMP_RISE * HEIGHT

local function hole_route(dx, dy, spawn_gap, emerge_motion, emerge_time)
  -- В клипе зомби хватается за кромку на расстоянии EMERGE_RIM от начала движения, в долях роста.
  local rim_offset = HOLE_RADIUS - EMERGE_RIM * HEIGHT
  local x = LEFT + HOLE_X * PX + dx * rim_offset
  local y = HOLE_Y * PY + dy * rim_offset
  local emerge_x, emerge_y = dx * EMERGE_TRAVEL * HEIGHT, dy * EMERGE_TRAVEL * HEIGHT
  local lane_y = y + emerge_y
  return {
    x = x, y = y,
    emerge_x = emerge_x, emerge_y = emerge_y,
    lane_y = lane_y,
    slope_x = BASE_X + (lane_y - BASE_Y) * SLOPE_X_PER_ROW,
    rotation = -math.deg(math.atan(dx, dy)),
    spawn_gap = spawn_gap,
    emerge_motion = emerge_motion or "подъём_зомби_из_ямы",
    emerge_time = emerge_time or EMERGE_TIME,
  }
end

local HOLE_ROUTES = {
  hole_route(1, 0, 2.2),
  hole_route(0, -1, 3.3, "подъём_зомби_из_ямы_к_забору", 10.6),
  hole_route(0, 1, 4.5),
}

local function place(zombie, x, y, z)
  zombie.position = { x = x - ZOMBIE_SIZE / 2, y = y - ZOMBIE_SIZE / 2, z = z }
end

local function turn(from, to, seconds)
  local t = math.min(1, seconds / TURN_TIME)
  local angle = (to - from + 180) % 360 - 180
  return from + angle * t * t * (3 - 2 * t)
end

local function walk_to_slope(zombie, route, age)
  local fade = math.min(1, (age + DT) / FADE_TIME)
  local walk_age = math.max(0, age - FADE_TIME)
  local x = route.x + route.emerge_x * fade + walk_age * WALK_SPEED
  local y = route.y + route.emerge_y * fade
  place(zombie, math.min(x, route.slope_x), y, 0)
  if x >= route.slope_x then
    zombie.rotation = WALK_ROTATION
    zombie.zombie_stage = 1
    zombie.zombie_age = 0
    zombie.animation = "подъём_зомби_по_склону"
    return
  end
  zombie.rotation = turn(route.rotation, WALK_ROTATION, walk_age)
end

local function leave_slope(zombie, route, age)
  local fade = math.min(1, (age + DT) / FADE_TIME)
  place(zombie, route.slope_x + EXIT_X * fade + age * WALK_SPEED, route.lane_y, EXIT_Z * fade)
end

function spawn_zombies(spawner)
  if spawner.spawn_wait > 0 then return end
  local side = spawner.hole_side or 1
  local route = HOLE_ROUTES[side]
  spawner.spawn_wait = route.spawn_gap + (math.random() * 2 - 1) * SPAWN_JITTER
  spawner.hole_side = side % #HOLE_ROUTES + 1
  create({
    zombie = true,
    model = "zombie",
    position = { route.x - ZOMBIE_SIZE / 2, route.y - ZOMBIE_SIZE / 2, 0 },
    size = { ZOMBIE_SIZE, ZOMBIE_SIZE },
    height = HEIGHT,
    rotation = route.rotation,
    animation = route.emerge_motion,
    hole_side = side,
    zombie_age = 0,
    zombie_stage = 3,
  })
end

function step_zombie(zombie)
  local age = zombie.zombie_age + DT
  zombie.zombie_age = age
  local route = HOLE_ROUTES[zombie.hole_side or 1]
  if zombie.zombie_stage == 3 then
    if age + 0.000001 < route.emerge_time then return end
    zombie.zombie_stage = 0
    zombie.zombie_age = 0
    zombie.animation = ""
    walk_to_slope(zombie, route, 0)
    return
  end
  if zombie.zombie_stage == 0 then
    walk_to_slope(zombie, route, age)
    return
  end
  if zombie.zombie_stage == 1 then
    if age + 0.000001 < CLIMB_TIME then return end
    zombie.zombie_stage = 2
    zombie.zombie_age = 0
    zombie.animation = ""
    leave_slope(zombie, route, 0)
    return
  end
  leave_slope(zombie, route, age)
end
