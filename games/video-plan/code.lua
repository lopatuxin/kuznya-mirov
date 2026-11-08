-- План выпуска для второго ролика: кузня с чертежом, справа въезжает карточка «Стиль — Каркас —
-- Редактор», пункты загораются на свои слова и остаются гореть. Секунды — моменты слов
-- закадрового голоса «план», «стиль», «каркас», «делаю». Пока голос не записан, они расчётные
-- (130 слов в минуту); после записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  plan = 0.8,
  style = 3.3,
  frame = 4.93,
  editor = 10.01,
  finish = 13.09,
}

local CLOCK_START = 1000

-- Медленный наезд на чертёж: картинка за всю сцену вырастает на ZOOM вокруг середины чертежа.
local ZOOM = 0.05
local FOCUS = { x = 9.8, y = 5.2 }

-- Карточка въезжает справа из-за края кадра.
local SLIDE = 12
local SLIDE_TIME = 0.35

local LIT_BADGE = "#ffd34d"
local LIGHT = { badge1 = "style", badge2 = "frame", badge3 = "editor",
                num1 = "style", num2 = "frame", num3 = "editor",
                word1 = "style", word2 = "frame", word3 = "editor" }

local function clamp01(u)
  if u < 0 then return 0 end
  if u > 1 then return 1 end
  return u
end

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function ease_out(u)
  return 1 - (1 - u) * (1 - u)
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.6, 1.2, u / 0.12) end
  if u < 0.22 then return lerp(1.2, 1, (u - 0.12) / 0.1) end
  return 1
end

-- Где и какого размера часть была записана в сцене: от этого считаются все её движения.
local home = {}

function direct(director)
  local t = CLOCK_START - director.clock
  local slide = SLIDE * (1 - ease_out(clamp01((t - T.plan) / SLIDE_TIME)))

  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = home[p.part]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[p.part] = h
    end

    if p.part == "picture" then
      local s = 1 + ZOOM * clamp01(t / T.finish)
      p.size = { x = h.w * s, y = h.hgt * s }
      p.position = { x = FOCUS.x - (FOCUS.x - h.x) * s, y = FOCUS.y - (FOCUS.y - h.y) * s }
    else
      local cue = LIGHT[p.part]
      local lit = cue ~= nil and t >= T[cue]
      local s = 1
      if lit and string.sub(p.part, 1, 5) == "badge" then
        s = pop_scale(t - T[cue])
        p.color = LIT_BADGE
      end
      if lit and p.lit ~= nil then
        p.lit = 1
      end
      local w, hh = h.w * s, h.hgt * s
      p.size = { x = w, y = hh }
      p.position = { x = h.x + slide + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
    end
  end
end
