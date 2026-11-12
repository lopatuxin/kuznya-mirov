-- Пробы стиля в ленте YouTube для второго ролика: три пробы крупно в ряд, на «читается» они
-- уменьшаются до обложек и встают в ленту среди серых заглушек, на «маленьком» обложку
-- современного мультфильма обводит жёлтая рамка. Секунды — моменты слов закадрового голоса;
-- пока голос не записан, они расчётные (130 слов в минуту), после записи сюда ставятся настоящие,
-- и сцена записывается заново.
local T = {
  reads = 1.78,
  small = 2.72,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

-- Три пробы крупно в ряд, как в конце сцены с десятью пробами: размер, левый край первой, шаг.
local BIG = { w = 9.6, h = 5.4, x = 1.0, y = 4.4, step = 10.2 }
local TRIES = { [5] = 0, [6] = 1, [7] = 2 }
local SHRINK_TIME = 0.6
local POP_STEP = 0.025

local function clamp01(u)
  if u < 0 then return 0 end
  if u > 1 then return 1 end
  return u
end

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function ease_in_out(u)
  if u < 0.5 then return 2 * u * u end
  return 1 - 2 * (1 - u) * (1 - u)
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.5, 1.08, u / 0.12) end
  if u < 0.2 then return lerp(1.08, 1, (u - 0.12) / 0.08) end
  return 1
end

local home = {}

local function place_scaled(p, h, s)
  local w, hh = h.w * s, h.hgt * s
  p.size = { x = w, y = hh }
  p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
end

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "slot" } })
  for i = 1, #parts do
    local p = parts[i]
    local key = p.slot .. p.role
    local h = home[key]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[key] = h
    end

    local big = TRIES[p.slot]
    if p.role == "thumb" and big ~= nil then
      local u = ease_in_out(clamp01((t - T.reads) / SHRINK_TIME))
      local bx = BIG.x + big * BIG.step
      p.size = { x = lerp(BIG.w, h.w, u), y = lerp(BIG.h, h.hgt, u) }
      p.position = { x = lerp(bx, h.x, u), y = lerp(BIG.y, h.y, u) }
    elseif p.role == "frame" then
      if t < T.small then
        p.position = HIDDEN
      else
        place_scaled(p, h, pop_scale(t - T.small))
      end
    else
      local appear = T.reads + (p.slot - 1) * POP_STEP
      if t < appear then
        p.position = HIDDEN
      else
        place_scaled(p, h, pop_scale(t - appear))
      end
    end
  end
end
