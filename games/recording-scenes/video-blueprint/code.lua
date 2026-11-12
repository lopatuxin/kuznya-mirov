-- Рама на чертеже для второго ролика: камера наезжает с кузни на чертёж велосипеда; с началом фразы
-- рама проявляется золотом, на «остальное» колёса, вилка, руль и седло — пунктиром. Золото и пунктир —
-- картинки размером с кузню, нарисованные точно по линиям чертежа, и едут вместе с ней.
-- Секунды — моменты слов закадрового голоса; пока голос не записан, они расчётные (130 слов
-- в минуту), после записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  frame = 0.405,
  rest = 3.085,
  finish = 5.535,
}

local CLOCK_START = 1000
local SCREEN = { w = 32, h = 18 }

-- Середина чертежа с подписью «движок» на кузне во весь кадр, в клетках, и наезд на него.
local BIKE = { x = 9.873, y = 5.3 }
local ZOOM_TO = 2.2
local ZOOM_TIME = 1.4
local DRIFT = 0.1
local FADE_TIME = 0.5

local function clamp(v, lo, hi)
  if v < lo then return lo end
  if v > hi then return hi end
  return v
end

local function ease_in_out(u)
  if u < 0.5 then return 2 * u * u end
  return 1 - 2 * (1 - u) * (1 - u)
end

local home = {}

function direct(director)
  local t = CLOCK_START - director.clock
  local u = ease_in_out(clamp(t / ZOOM_TIME, 0, 1))
  local z = 1 + (ZOOM_TO - 1) * u + DRIFT * clamp((t - ZOOM_TIME) / (T.finish - ZOOM_TIME), 0, 1)
  local fx = SCREEN.w / 2 + (BIKE.x - SCREEN.w / 2) * u
  local fy = SCREEN.h / 2 + (BIKE.y - SCREEN.h / 2) * u

  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = home[p.part]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[p.part] = h
    end
    -- Все три картинки одного размера и едут одной камерой; кузня всегда закрывает кадр.
    local w, hh = h.w * z, h.hgt * z
    local x = clamp(SCREEN.w / 2 + (h.x - fx) * z, SCREEN.w - w, 0)
    local y = clamp(SCREEN.h / 2 + (h.y - fy) * z, SCREEN.h - hh, 0)
    p.size = { x = w, y = hh }
    p.position = { x = x, y = y }
    if p.part == "gold" then
      p.opacity = clamp((t - T.frame) / FADE_TIME, 0, 1)
    elseif p.part == "dashed" then
      p.opacity = clamp((t - T.rest) / FADE_TIME, 0, 1)
    end
  end
end
