-- Заставка главы третьего ролика в оформлении «бронза»: размытый лес уровня с листопадом, «Часть N
-- из 3», знак и название главы (картинка card), под ними план выпуска строкой «Прыжок — Герой и
-- уровень — Бой»: текущий пункт загорается на своё слово, пройденные горят светлым. Заставки других
-- глав — копия этой игры с другими CURRENT, DONE, T и картинкой card.
-- Эта копия — глава «Герой и уровень»: «Прыжок» пройден, «Герой и уровень» загорается на «героя»
-- («Теперь про героя»). Секунда light — момент этого слова закадрового голоса, её ставит монтаж
-- (work/scenes.mjs apply).
local T = {
  light = 0.6,
}
local CURRENT = 2
local DONE = { [1] = true }

local CLOCK_START = 1000
-- Лес за всю заставку медленно вырастает на ZOOM, название в начале чуть опускается к своему месту.
local ZOOM = 0.05
local ZOOM_TIME = 2.5
local CARD_FROM = 1.04
local CARD_TIME = 0.7

-- Номер цвета надписи в таблице `screens.json`: тусклый, горит, пройдено.
local DIM, LIT, PASSED = 0, 1, 2
local RHOMB = { [DIM] = "rhomb_dim", [LIT] = "rhomb_lit", [PASSED] = "rhomb_done" }

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function clamp01(x)
  if x < 0 then return 0 end
  if x > 1 then return 1 end
  return x
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.6, 1.2, u / 0.12) end
  if u < 0.22 then return lerp(1.2, 1, (u - 0.12) / 0.1) end
  return 1
end

-- Где и какого размера часть была записана в сцене: от этого считаются наезд и щелчок.
local home = {}

local function remember(p)
  local h = home[p.part]
  if h == nil then
    h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
    home[p.part] = h
  end
  return h
end

-- Размер s от записанного, с тем же центром.
local function scale_about_center(p, h, s)
  local w, hh = h.w * s, h.hgt * s
  p.size = { x = w, y = hh }
  p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
end

function direct(director)
  local t = CLOCK_START - director.clock

  local back = find({ has = { "backdrop" } })
  for i = 1, #back do
    local p = back[i]
    local h = remember(p)
    if p.part == "picture" then
      scale_about_center(p, h, 1 + ZOOM * clamp01(t / ZOOM_TIME))
    else
      local u = clamp01(t / CARD_TIME)
      scale_about_center(p, h, lerp(CARD_FROM, 1, 1 - (1 - u) * (1 - u)))
    end
  end

  local parts = find({ has = { "item" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = remember(p)

    local state = DIM
    local s = 1
    if DONE[p.item] then
      state = PASSED
    elseif p.item == CURRENT and t >= T.light then
      state = LIT
      s = pop_scale(t - T.light)
    end

    if p.lit ~= nil then
      p.lit = state
    else
      p.image = RHOMB[state]
      scale_about_center(p, h, s)
    end
  end
end
