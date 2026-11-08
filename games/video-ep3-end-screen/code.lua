-- Конечная заставка третьего ролика: кадр боя — Илья и разбойник друг против друга — во весь кадр
-- за первую секунду темнеет, сверху выплывает название ролика золотыми буквами (картинка title, оформление «бронза»), дальше кадр 15 секунд медленно
-- приближается. Рамок под кнопки нет: кнопку подписки и следующий ролик хозяин ставит поверх
-- в Творческой студии. Затемнение — чёрная картинка поверх кадра. Моментов слов нет: заставка
-- идёт 15 секунд после последней фразы.
local DURATION = 15

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

local SHADE = 0.6
local DARKEN_TIME = 1.0
local TITLE_AT = 0.8
local TITLE_RISE, TITLE_RISE_TIME = 0.8, 0.25
-- Кадр растёт от бойцов: они стоят на месте и не уезжают под название.
local ZOOM_TO = 1.08
local ZOOM_ORIGIN = { x = 33, y = 20 }

local function clamp01(u)
  if u < 0 then return 0 end
  if u > 1 then return 1 end
  return u
end

local home = {}

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = home[p.part]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[p.part] = h
    end
    if p.part == "picture" then
      local z = 1 + (ZOOM_TO - 1) * clamp01(t / DURATION)
      p.size = { x = h.w * z, y = h.hgt * z }
      p.position = { x = ZOOM_ORIGIN.x + (h.x - ZOOM_ORIGIN.x) * z, y = ZOOM_ORIGIN.y + (h.y - ZOOM_ORIGIN.y) * z }
    elseif p.part == "shade" then
      p.opacity = SHADE * clamp01(t / DARKEN_TIME)
    elseif p.part == "title" then
      if t < TITLE_AT then
        p.position = HIDDEN
      else
        local k = clamp01((t - TITLE_AT) / TITLE_RISE_TIME)
        p.position = { x = h.x, y = h.y + TITLE_RISE * (1 - k) }
      end
    end
  end
end
