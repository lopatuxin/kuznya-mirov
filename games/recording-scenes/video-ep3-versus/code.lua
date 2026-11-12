-- Два подхода к защите для конца третьего ролика: с начала посередине карточка «У нас — защищается
-- сам» в золотом ободе (оформление «бронза»: панели тёмного дерева на размытом лесу уровня); на «Dark
-- Souls» она отъезжает вправо, а слева въезжает карточка «Dark Souls», обод загорается уже на ней; на «блок» в ней проступает «блок и перекат жмёте сами»; на «Напишите»
-- ободы гаснут и внизу появляется жёлтая подпись «пишите в комментариях», как в конце второго ролика.
-- Секунды — моменты слов закадрового голоса от начала фразы «Но многим ближе, как в Dark Souls…». Голос
-- ещё не записан, поэтому они расчётные (135 слов в минуту, слово номер i — на i × 0,444 с):
--   dark — «Dark», block — «блок», write — «Напишите».
-- После записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  dark = 1.71,
  block = 2.89,
  write = 4.67,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

-- Пока карточка одна, она стоит посередине кадра; на «Dark Souls» обе едут на свои места.
local START_SHIFT = { left = -15.5, right = -7.5 }
local SLIDE_TIME = 0.5
local FADE_TIME, FADE_STEPS = 0.25, 5
local RISE, RISE_TIME = 0.3, 0.25
local CAPTION_RISE, CAPTION_RISE_TIME = 0.4, 0.18

local function clamp01(u)
  if u < 0 then return 0 end
  if u > 1 then return 1 end
  return u
end

local function ease_out(u)
  return 1 - (1 - u) * (1 - u)
end

local function ease_in_out(u)
  if u < 0.5 then return 2 * u * u end
  return 1 - 2 * (1 - u) * (1 - u)
end

-- Какая карточка сейчас в золотом ободе: своя до «Dark Souls», та — до «Напишите», потом никакая.
local function lit_side(t)
  if t < T.dark then return "right" end
  if t < T.write then return "left" end
  return nil
end

-- Где часть была записана в сцене: от этого считаются все её движения.
local home = {}

function direct(director)
  local t = CLOCK_START - director.clock
  local slide = 1 - ease_in_out(clamp01((t - T.dark) / SLIDE_TIME))
  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local key = p.part .. (p.side or "")
    local h = home[key]
    if h == nil then
      h = { x = p.position.x, y = p.position.y }
      home[key] = h
    end

    if p.part == "caption" then
      if t < T.write then
        p.position = HIDDEN
      else
        local k = clamp01((t - T.write) / CAPTION_RISE_TIME)
        p.position = { x = h.x, y = h.y + CAPTION_RISE * (1 - k) }
      end
    else
      local dx = START_SHIFT[p.side] * slide
      local dy = 0
      if p.side == "left" and string.sub(p.part, 1, 4) == "desc" then
        local u = t - T.block
        if u < 0 then
          dx = HIDDEN.x - h.x
        else
          dy = RISE * (1 - ease_out(clamp01(u / RISE_TIME)))
          p.tone = math.floor(clamp01(u / FADE_TIME) * FADE_STEPS + 0.5)
        end
      end
      if p.part == "edge" then
        p.opacity = lit_side(t) == p.side and 1 or 0
      end
      p.position = { x = h.x + dx, y = h.y + dy }
    end
  end
end
