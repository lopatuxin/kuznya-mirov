-- План выпуска для третьего ролика: кадр игры — Илья на поляне — медленно приближается, на «три вещи»
-- справа въезжает панель «План выпуска» в оформлении «бронза» с пунктами «Прыжок — Герой и уровень —
-- Бой», пункты загораются на свои слова и горят до конца. Запись одна на всю фразу: монтаж в середине уходит на другую запись и возвращается.
-- Секунды — моменты слов закадрового голоса от начала фразы «Чтобы герой бегал…»: three — «три»
-- («нужно три вещи»), jump — «прыжку» («научиться прыжку»), hero — «Во-вторых», fight — «в-третьих»,
-- finish — конец фразы после «пойдём». Пока голос не записан, они расчётные (135 слов в минуту, слово
-- номер i — на i × 0,444 с); после записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  three = 2.97,
  jump = 6.59,
  hero = 16.24,
  fight = 22.51,
  finish = 28.84,
}

local CLOCK_START = 1000

-- Медленный наезд на кадр: картинка за всю фразу вырастает на ZOOM вокруг Ильи, и он стоит на месте.
local ZOOM = 0.08
local FOCUS = { x = 5.2, y = 12 }

-- Карточка въезжает справа из-за края кадра.
local SLIDE = 16
local SLIDE_TIME = 0.4

local LIGHT = { badge1 = "jump", badge2 = "hero", badge3 = "fight",
                num1 = "jump", num2 = "hero", num3 = "fight",
                word1 = "jump", word2 = "hero", word3 = "fight" }

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
  local slide = SLIDE * (1 - ease_out(clamp01((t - T.three) / SLIDE_TIME)))

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
        p.image = "rhomb_lit"
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
