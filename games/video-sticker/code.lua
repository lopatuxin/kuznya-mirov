-- Герой-наклейка для второго ролика: лесной уровень во весь кадр, на «мхом» камера едет к мху на
-- уступе, на «герой» — к герою, на «плоский» вокруг него встаёт пунктирная рамка, как у наклейки,
-- и штрихи бегут по кругу. Секунды — моменты слов закадрового голоса; пока голос не записан, они
-- расчётные (130 слов в минуту), после записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  moss = 4.89,
  hero = 6.43,
  flat = 7.51,
  finish = 12.49,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }
local SCREEN = { w = 32, h = 18 }

-- Точки картинки в клетках кадра во весь экран: мох на левом уступе, середина героя и рамка вокруг него.
local MOSS = { x = 3.8, y = 9.0 }
local HERO = { x = 10.2, y = 5.8 }
local OUTLINE = { x = 7.9, y = 3.6, w = 4.7, h = 4.4 }

-- Куда смотрит камера: с какого времени, на что, с каким увеличением и за сколько доезжает.
local MOVES = {
  { at = 0, focus = { x = 16, y = 9 }, z = 1.0, time = 0.01 },
  { at = T.moss, focus = MOSS, z = 1.5, time = 1.0 },
  { at = T.hero, focus = HERO, z = 1.7, time = 0.9 },
}
local DRIFT = 0.05
local MARCH = 0.2

local function clamp(v, lo, hi)
  if v < lo then return lo end
  if v > hi then return hi end
  return v
end

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function ease_in_out(u)
  if u < 0.5 then return 2 * u * u end
  return 1 - 2 * (1 - u) * (1 - u)
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.7, 1.08, u / 0.12) end
  if u < 0.22 then return lerp(1.08, 1, (u - 0.12) / 0.1) end
  return 1
end

-- Камера в момент t: от прошлой цели к нынешней с плавным разгоном и торможением, потом лёгкий наезд.
local function camera(t)
  local fx, fy, z = MOVES[1].focus.x, MOVES[1].focus.y, MOVES[1].z
  for i = 2, #MOVES do
    local m = MOVES[i]
    if t >= m.at then
      local u = ease_in_out(clamp((t - m.at) / m.time, 0, 1))
      fx, fy, z = lerp(fx, m.focus.x, u), lerp(fy, m.focus.y, u), lerp(z, m.z, u)
    end
  end
  z = z + DRIFT * clamp(t / T.finish, 0, 1)
  local x = clamp(SCREEN.w / 2 - fx * z, SCREEN.w - SCREEN.w * z, 0)
  local y = clamp(SCREEN.h / 2 - fy * z, SCREEN.h - SCREEN.h * z, 0)
  return { z = z, x = x, y = y }
end

function direct(director)
  local t = CLOCK_START - director.clock
  local cam = camera(t)
  local march = math.floor(t / MARCH) % 2
  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    if p.part == "picture" then
      p.size = { x = SCREEN.w * cam.z, y = SCREEN.h * cam.z }
      p.position = { x = cam.x, y = cam.y }
    else
      local mine = (p.part == "dash_a") == (march == 0)
      if t < T.flat or not mine then
        p.position = HIDDEN
      else
        local s = pop_scale(t - T.flat)
        local w, h = OUTLINE.w * cam.z * s, OUTLINE.h * cam.z * s
        local cx = cam.x + (OUTLINE.x + OUTLINE.w / 2) * cam.z
        local cy = cam.y + (OUTLINE.y + OUTLINE.h / 2) * cam.z
        p.size = { x = w, y = h }
        p.position = { x = cx - w / 2, y = cy - h / 2 }
      end
    end
  end
end
