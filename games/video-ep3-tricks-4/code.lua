-- Список хитростей прыжка для третьего ролика в оформлении «бронза»: панель на размытом лесу уровня,
-- четыре хитрости из Celeste, текущая загорается золотом с выскоком ромба-номера.
-- Записей четыре, по 2 секунды, — копии этой игры video-ep3-tricks-1 … -4, у каждой свой CURRENT;
-- монтаж ставит каждую перед клипом своей хитрости. Пункты до текущего уже пройдены и светлые,
-- после него — тусклые.
-- Момент light — когда текущий пункт загорается, от начала записи. К слову голоса он не привязан:
-- запись стоит перед клипом хитрости, а не на слове, и после записи голоса не меняется.
local T = {
  light = 0.3,
}
local CURRENT = 4

local CLOCK_START = 1000

-- Номер цвета надписи в таблицах `screens.json`: тусклый, горит, пройдено; ромб номера — своя картинка.
local DIM, LIT, PASSED = 0, 1, 2
local RHOMB = { [DIM] = "rhomb_dim", [LIT] = "rhomb_lit", [PASSED] = "rhomb_done" }

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.6, 1.2, u / 0.12) end
  if u < 0.22 then return lerp(1.2, 1, (u - 0.12) / 0.1) end
  return 1
end

-- Где и какого размера часть была записана в сцене: от этого считается её выскок.
local home = {}

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "item" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = home[p.part]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[p.part] = h
    end

    local state = DIM
    local s = 1
    if p.item < CURRENT then
      state = PASSED
    elseif p.item == CURRENT and t >= T.light then
      state = LIT
      s = pop_scale(t - T.light)
    end

    if p.lit ~= nil then
      p.lit = state
    else
      p.image = RHOMB[state]
      local w, hh = h.w * s, h.hgt * s
      p.size = { x = w, y = hh }
      p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
    end
  end
end
