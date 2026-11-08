-- Редактор с арканоидом для второго ролика: во весь кадр идёт запись редактора, где открыт арканоид и
-- курсор обходит сцену; на «называют» окно редактора отъезжает вниз и уменьшается, на «редактором» над
-- ним выскакивает подпись «редактор». Запись редактора — видео в картинке `editor`, его пишет программа
-- записи роликов по своему расписанию мыши. Секунды — моменты слов закадрового голоса; пока голос не
-- записан, они расчётные (130 слов в минуту), после записи сюда ставятся настоящие, и сцена
-- записывается заново.
local T = {
  called = 4.55,
  editor = 4.95,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }
local SCREEN = { w = 64, h = 36 }

-- Окно редактора уменьшается до этой доли кадра и встаёт по центру над нижним краем.
local SHRINK_TO = 0.84
local SHRINK_TIME = 0.6
local BOTTOM_GAP = 1.0
local BORDER = 0.15
local CAPTION_RISE, CAPTION_RISE_TIME = 0.8, 0.18

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

local caption_home = nil

function direct(director)
  local t = CLOCK_START - director.clock
  local u = ease_in_out(clamp01((t - T.called) / SHRINK_TIME))
  local s = lerp(1, SHRINK_TO, u)
  local w, h = SCREEN.w * s, SCREEN.h * s
  local x = (SCREEN.w - w) / 2
  local y = lerp(0, SCREEN.h - h - BOTTOM_GAP, u)

  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    if p.part == "screen" then
      p.size = { x = w, y = h }
      p.position = { x = x, y = y }
    elseif p.part == "border" then
      local b = BORDER * u
      p.size = { x = w + 2 * b, y = h + 2 * b }
      p.position = { x = x - b, y = y - b }
    elseif p.part == "caption" then
      if caption_home == nil then
        caption_home = { x = p.position.x, y = p.position.y }
      end
      if t < T.editor then
        p.position = HIDDEN
      else
        local k = clamp01((t - T.editor) / CAPTION_RISE_TIME)
        p.position = { x = caption_home.x, y = caption_home.y + CAPTION_RISE * (1 - k) }
      end
    end
  end
end
