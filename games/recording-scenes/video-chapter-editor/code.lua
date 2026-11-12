-- Карточка главы второго ролика: план выпуска «Стиль — Каркас — Редактор» по центру, текущий пункт
-- загорается на своё слово. Карточки других глав — копия этой игры с другими CURRENT, DONE и T.
-- Эта копия — глава «Редактор»: «Стиль» и «Каркас» пройдены, «Редактор» загорается в начале фразы
-- «Каркас есть, но управлять им пока нечем» — своего слова у него в ней нет, поэтому он горит с «есть».
-- Секунда — момент этого слова закадрового голоса. Пока голос не записан,
-- она расчётная (130 слов в минуту); после записи сюда ставится настоящая, и сцена записывается заново.
local T = {
  light = 0.99,
}
local CURRENT = 3
local DONE = { [1] = true, [2] = true }

local CLOCK_START = 1000

local DIM_BADGE = "#3a3f48"
local LIT_BADGE = "#ffd34d"
local DONE_BADGE = "#e9ecf2"
-- Номер цвета надписи в таблице `screens.json`: серый, горит, пройдено.
local DIM, LIT, PASSED = 0, 1, 2

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.6, 1.2, u / 0.12) end
  if u < 0.22 then return lerp(1.2, 1, (u - 0.12) / 0.1) end
  return 1
end

-- Где и какого размера часть была записана в сцене: от этого считается её щелчок.
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
    if DONE[p.item] then
      state = PASSED
    elseif p.item == CURRENT and t >= T.light then
      state = LIT
      s = pop_scale(t - T.light)
    end

    if p.lit ~= nil then
      p.lit = state
    elseif state == LIT then
      p.color = LIT_BADGE
    elseif state == PASSED then
      p.color = DONE_BADGE
    else
      p.color = DIM_BADGE
    end

    if p.lit == nil then
      local w, hh = h.w * s, h.hgt * s
      p.size = { x = w, y = hh }
      p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
    end
  end
end
