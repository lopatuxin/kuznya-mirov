-- «Квадрат толкает квадрат» для второго ролика: синий толкает красный, потом всё рассыпается.
-- Секунды — моменты слов закадрового голоса «толкает» и «квадратов» («Поэтому квадратов у меня
-- не будет»). Пока голос не записан, они расчётные (130 слов в минуту); после записи сюда ставятся
-- настоящие, и сцена записывается заново.
local T = {
  push = 2.96,
  crumble = 5.02,
}

local CLOCK_START = 1000
local SIDE = 1.8
local HIDDEN = { x = -20, y = -20 }
local GROUND = 12

local HERO_START = 22.3
local ENEMY_START = 26.0
local ENEMY_END = 27.9
local APPROACH_TIME = 0.5
local PUSH_TIME = 1.0

-- Куски разлетаются от середины своего тела и падают; площадки рассыпаются вслед за квадратами.
local GRAVITY = 34
local CRUMBLE_DELAY = { hero = 0, enemy = 0, p3 = 0.1, p2 = 0.2, p1 = 0.3 }

local function clamp01(u)
  if u < 0 then return 0 end
  if u > 1 then return 1 end
  return u
end

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function ease_in(u)
  return u * u
end

local function ease_out(u)
  return 1 - (1 - u) * (1 - u)
end

local function hop_lift(t, t0, dur, height)
  local u = (t - t0) / dur
  if u <= 0 or u >= 1 then return 0 end
  return 4 * height * u * (1 - u)
end

local function enemy_cx(t)
  return lerp(ENEMY_START, ENEMY_END, ease_out(clamp01((t - T.push) / PUSH_TIME)))
end

local function hero_cx(t)
  if t < T.push - APPROACH_TIME then
    return HERO_START
  end
  if t < T.push then
    return lerp(HERO_START, ENEMY_START - SIDE, ease_in((t - (T.push - APPROACH_TIME)) / APPROACH_TIME))
  end
  return enemy_cx(t) - SIDE
end

local PLATFORMS = {
  p1 = { x = 3, y = 12 },
  p2 = { x = 13.5, y = 9 },
  p3 = { x = 21, y = 12 },
}

-- Где стоит тело в момент t, пока оно цело: левый верхний угол.
local function body_position(name, t)
  if name == "hero" then
    local lift = hop_lift(t, 1.0, 0.35, 0.8) + hop_lift(t, 2.2, 0.35, 0.8)
    return hero_cx(t) - SIDE / 2, GROUND - SIDE - lift
  end
  if name == "enemy" then
    return enemy_cx(t) - SIDE / 2, GROUND - SIDE - hop_lift(t, 1.6, 0.35, 0.6)
  end
  local p = PLATFORMS[name]
  return p.x, p.y
end

-- Случайность куска без счётчика случайности: одно и то же число для одного и того же куска.
local function jitter(ox, oy, salt)
  local v = math.sin(ox * 12.9898 + oy * 78.233 + salt * 37.719) * 43758.5453
  return v - math.floor(v) - 0.5
end

function direct(director)
  local t = CLOCK_START - director.clock

  local bodies = {}
  local list = find({ has = { "body" } })
  for i = 1, #list do
    local b = list[i]
    local x, y = body_position(b.name, t)
    bodies[b.name] = { obj = b, x = x, y = y, w = b.size.x, h = b.size.y, broken = t >= T.crumble + CRUMBLE_DELAY[b.name] }
    if bodies[b.name].broken then
      b.position = HIDDEN
    else
      b.position = { x = x, y = y }
    end
  end

  local pieces = find({ has = { "piece" } })
  for i = 1, #pieces do
    local p = pieces[i]
    local body = bodies[p.owner]
    local t_break = T.crumble + CRUMBLE_DELAY[p.owner]
    if t < t_break then
      p.position = { x = body.x + p.ox, y = body.y + p.oy }
    else
      -- Кусок стартует оттуда, где стояло тело в миг разлома, и летит по дуге вниз.
      local bx, by = body_position(p.owner, t_break)
      local dx = (p.ox + p.size.x / 2) / body.w - 0.5
      local dy = (p.oy + p.size.y / 2) / body.h - 0.5
      local vx = dx * 7 + jitter(p.ox, p.oy, 1) * 3
      local vy = -5 - dy * 4 + jitter(p.ox, p.oy, 2) * 3
      local s = t - t_break
      p.position = { x = bx + p.ox + vx * s, y = by + p.oy + vy * s + GRAVITY * s * s / 2 }
    end
  end
end
