-- Остановка бойцов в миг попадания — схема для третьего ролика. Лента из четырёх шагов игры: в каждом
-- один и тот же кадр попадания из замедленной записи боя, обрезанный до Ильи и разбойника; над лентой
-- скобка «пауза». На «сдвинуть» бойцы в шаге 1 съезжают вправо (лист кадров с окном обрезки, сдвинутым
-- от кадра к кадру), рамка шага краснеет, под ним красная стрелка «ещё шаг»; на «прыжок» внизу выскакивает
-- клавиша «прыжок», на «пропадал» её перечёркивает красная черта. На «Пришлось» шаг 1 встаёт ровно, как
-- остальные, стрелка и красная рамка гаснут; на «выводить» черта уходит и клавиша загорается золотом.
-- Оформление «бронза»: панель тёмного дерева на размытом лесу уровня, кадры в бронзовых рамках, красная
-- рамка шага 1 и горящая клавиша — свои картинки поверх, проступают прозрачностью.
-- Секунды — моменты слов закадрового голоса от начала фразы «Но просто остановить обоих не вышло…».
-- Голос ещё не записан, поэтому они расчётные (135 слов в минуту, слово номер i — на i × 0,444 с):
--   shift — «сдвинуть», jump — «прыжок», lost — «пропадал», fix — «Пришлось», out — «выводить».
-- После записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  shift = 3.71,
  jump = 5.61,
  lost = 7.67,
  fix = 8.83,
  out = 10.47,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

-- Лист кадров попадания: кадр 0 — ровно, последний — бойцы сдвинуты на шаг.
local LAST_SHOT = 9
local SHIFT_TIME, BACK_TIME = 0.35, 0.4

-- Шаги выскакивают в начале один за другим, подписи над ними проступают чуть позже; скобка растёт
-- от середины, когда шаги уже стоят.
local STRIP_STAGGER, STEP_DELAY = 0.1, 0.12
local BRACKET_AT, BRACKET_TIME = 0.35, 0.35
local ARROW_DELAY, ARROW_TIME = 0.1, 0.3
local HEAD_OVERLAP = 0.15
local STRIKE_TIME = 0.25
local LIGHT_TIME = 0.2
local FADE_TIME, FADE_STEPS = 0.25, 5
local RISE, RISE_TIME = 0.3, 0.25

local RED = "#ff4d4d"
local KEY_TONE = { normal = 0, lost = 1, lit = 2 }
local KEY_LABEL_DELAY = 0.09

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

local function ease_in_out(u)
  if u < 0.5 then return 2 * u * u end
  return 1 - 2 * (1 - u) * (1 - u)
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.3, 1.08, u / 0.12) end
  if u < 0.22 then return lerp(1.08, 1, (u - 0.12) / 0.1) end
  return 1
end

local function with_alpha(hex, u)
  return string.format("%s%02x", hex, math.floor(clamp01(u) * 255 + 0.5))
end

local function fade_tone(u)
  return math.floor(clamp01(u / FADE_TIME) * FADE_STEPS + 0.5)
end

-- Насколько сдвинут шаг 1: 0 — ровно, 1 — на целый шаг.
local function shifted(t)
  if t < T.fix then
    return ease_in_out(clamp01((t - T.shift) / SHIFT_TIME))
  end
  return 1 - ease_in_out(clamp01((t - T.fix) / BACK_TIME))
end

-- Стрелка «ещё шаг»: доля её длины — растёт после «сдвинуть», уходит на «Пришлось».
local function arrow_share(t)
  if t < T.fix then
    return ease_out(clamp01((t - T.shift - ARROW_DELAY) / ARROW_TIME))
  end
  return 1 - ease_out(clamp01((t - T.fix) / ARROW_TIME))
end

-- Насколько горит клавиша: с «выводить» за LIGHT_TIME.
local function key_light(t)
  return clamp01((t - T.out) / LIGHT_TIME)
end

local function put_scaled(p, h, s)
  local w, hh = h.w * s, h.hgt * s
  p.size = { x = w, y = hh }
  p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
end

local function rise_in(p, h, u)
  if u < 0 then
    p.position = HIDDEN
    return
  end
  p.position = { x = h.x, y = h.y + RISE * (1 - ease_out(clamp01(u / RISE_TIME))) }
  p.tone = fade_tone(u)
end

-- Где и какого размера часть была записана в сцене: от этого считаются все её движения.
local home = {}

local function key_of(p)
  return p.part .. (p.slot or "")
end

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    if home[key_of(p)] == nil then
      home[key_of(p)] = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
    end
  end
  for i = 1, #parts do
    local p = parts[i]
    local h = home[key_of(p)]

    local part = p.part
    if part == "shot" or part == "edge" or part == "alarm" then
      local u = t - (p.slot - 1) * STRIP_STAGGER
      if u < 0 then
        p.position = HIDDEN
      else
        put_scaled(p, h, pop_scale(u))
      end
      if p.slot == 1 and part == "shot" then
        p.shot = math.floor(shifted(t) * LAST_SHOT + 0.5)
      elseif part == "alarm" then
        p.opacity = shifted(t)
      end
    elseif part == "step" then
      rise_in(p, h, t - (p.slot - 1) * STRIP_STAGGER - STEP_DELAY)
    elseif part == "title" then
      local u = t - BRACKET_AT
      if u < 0 then p.position = HIDDEN else rise_in(p, h, u) end
    elseif part == "bracket" then
      local k = ease_out(clamp01((t - BRACKET_AT) / BRACKET_TIME))
      local w = math.max(h.w * k, 0.01)
      p.size = { x = w, y = h.hgt }
      p.position = { x = h.x + (h.w - w) / 2, y = h.y }
    elseif part == "bracket_left" or part == "bracket_right" then
      p.position = t < BRACKET_AT + BRACKET_TIME and HIDDEN or { x = h.x, y = h.y }
    elseif part == "note" then
      local share = arrow_share(t)
      if t < T.shift + ARROW_DELAY or share <= 0 then
        p.position = HIDDEN
      elseif t < T.fix then
        rise_in(p, h, t - T.shift - ARROW_DELAY)
      else
        p.position = { x = h.x, y = h.y }
        p.tone = math.floor(share * FADE_STEPS + 0.5)
      end
    elseif part == "shaft" or part == "head" then
      local share = arrow_share(t)
      local w = h.w * share
      if t < T.shift + ARROW_DELAY or share <= 0 then
        p.position = HIDDEN
      elseif part == "shaft" then
        p.size = { x = math.max(w, 0.01), y = h.hgt }
        p.position = { x = h.x, y = h.y }
      else
        local shaft = home["shaft"]
        p.position = { x = shaft.x + shaft.w * share - HEAD_OVERLAP, y = h.y }
        p.opacity = share
      end
    elseif part == "key_on" or part == "key_off" or part == "key" then
      local u = t - T.jump
      -- Надпись не растёт вместе с клавишей: она появляется, когда клавиша почти выросла.
      if u < 0 or (part == "key" and u < KEY_LABEL_DELAY) then
        p.position = HIDDEN
      else
        local light = key_light(t)
        put_scaled(p, h, pop_scale(u) * (1 + 0.06 * math.sin(math.pi * light)))
        if part == "key_on" then
          p.opacity = light
        elseif part == "key_off" then
          p.opacity = 1
        elseif light >= 0.5 then
          p.tone = KEY_TONE.lit
        elseif t >= T.lost then
          p.tone = KEY_TONE.lost
        else
          p.tone = KEY_TONE.normal
        end
      end
    elseif part == "strike" then
      local u = t - T.lost
      local gone = clamp01((t - T.out) / LIGHT_TIME)
      if u < 0 or gone >= 1 then
        p.position = HIDDEN
      else
        p.size = { x = math.max(h.w * ease_out(clamp01(u / STRIKE_TIME)), 0.01), y = h.hgt }
        p.position = { x = h.x, y = h.y }
        p.color = with_alpha(RED, 1 - gone)
      end
    end
  end
end
