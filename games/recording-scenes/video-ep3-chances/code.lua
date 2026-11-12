-- Шансы защиты Ильи для третьего ролика: три пустые дорожки, на «шансы» над ними подпись «шансы
-- постоянные», на «отбивает», «пропускает», «уворачивается» дорожка заполняется полосой до своего числа из
-- ста — 35, 35 и 30, как в коде боя платформера, — а подпись над ней считает до этого числа. Названная
-- полоса горит золотом, пока не назовут следующую, потом остаётся своего цвета. Оформление «бронза»:
-- панель тёмного дерева на размытом лесу уровня, дорожки в бронзовом ободе, как полоски здоровья
-- заставки первого ролика: отбил — сталь, пропустил — красная, увернулся — зелёная.
-- Секунды — моменты слов закадрового голоса от начала фразы «Характеристик у Ильи пока нет…». Голос ещё
-- не записан, поэтому они расчётные (135 слов в минуту, слово номер i — на i × 0,444 с):
--   chances — «шансы», parry — «отбивает», hurt — «пропускает», dodge — «уворачивается».
-- После записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  chances = 2.84,
  parry = 6.28,
  hurt = 7.38,
  dodge = 9.3,
}

local CLOCK_START = 1000
-- До своего слова часть стоит за краем сцены.
local HIDDEN = { x = -100, y = -100 }

-- Ряды сверху вниз: на какое слово, до скольких из ста, подпись и картинка полосы после своей очереди.
local ROWS = {
  { cue = "parry", value = 35, text = "отбил: %d из 100", image = "bar_steel" },
  { cue = "hurt", value = 35, text = "пропустил: %d", image = "bar_red" },
  { cue = "dodge", value = 30, text = "увернулся: %d", image = "bar_green" },
}
local GOLD = "bar_gold"

-- Дорожки выскакивают в начале сцены одна за другой.
local TRACK_AT, TRACK_STAGGER = 0.05, 0.12
-- Полоса растёт столько секунд с небольшим перелётом; подпись проявляется за FADE_TIME.
local GROW_TIME = 0.55
local OVERSHOOT = 0.9
local FADE_TIME = 0.25
local FADE_STEPS = 5
local DONE_TONE = 6
local RISE, RISE_TIME = 0.35, 0.25

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

-- Разгон с перелётом за конец и возвратом: в конце ровно 1.
local function ease_out_back(u)
  local v = u - 1
  return 1 + (OVERSHOOT + 1) * v * v * v + OVERSHOOT * v * v
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.3, 1.08, u / 0.12) end
  if u < 0.22 then return lerp(1.08, 1, (u - 0.12) / 0.1) end
  return 1
end

local function fade_tone(u)
  return math.floor(clamp01(u / FADE_TIME) * FADE_STEPS + 0.5)
end

local function next_cue(row)
  local after = ROWS[row + 1]
  return after and T[after.cue] or nil
end

-- Где и какого размера часть была записана в сцене: от этого считаются все её движения.
local home = {}

local function key_of(p)
  return p.part .. (p.row or "")
end

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local key = key_of(p)
    local h = home[key]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[key] = h
    end

    if p.part == "heading" then
      local u = t - T.chances
      if u < 0 then
        p.position = HIDDEN
      else
        p.position = { x = h.x, y = h.y + RISE * (1 - ease_out(clamp01(u / RISE_TIME))) }
        p.tone = fade_tone(u)
      end
    elseif p.part == "track" then
      local u = t - TRACK_AT - (p.row - 1) * TRACK_STAGGER
      if u < 0 then
        p.position = HIDDEN
      else
        local s = pop_scale(u)
        local w, hh = h.w * s, h.hgt * s
        p.size = { x = w, y = hh }
        p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
      end
    else
      local row = ROWS[p.row]
      local u = t - T[row.cue]
      local after = next_cue(p.row)
      local current = after == nil or t < after
      if u < 0 then
        p.position = HIDDEN
      elseif p.part == "bar" then
        local k = ease_out_back(clamp01(u / GROW_TIME))
        p.size = { x = math.max(h.w * k, 0.01), y = h.hgt }
        p.position = { x = h.x, y = h.y }
        p.image = current and GOLD or row.image
      else
        local counted = math.floor(row.value * math.min(1, ease_out(clamp01(u / GROW_TIME))) + 0.5)
        p.label = string.format(row.text, counted)
        p.position = { x = h.x, y = h.y + RISE * (1 - ease_out(clamp01(u / RISE_TIME))) }
        p.tone = current and fade_tone(u) or DONE_TONE
      end
    end
  end
end
