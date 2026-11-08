-- С кем драться герою — для конца второго ролика. На «вопрос» посередине выскакивают три карточки с
-- тёмными силуэтами противников, на каждом жёлтый знак вопроса; на «волком», «лешим», «разбойником»
-- силуэт своей карточки проявляется в цвете, рамка и номер горят жёлтым, пока не назовут следующего;
-- на «кем-то» ряд сдвигается влево и выскакивает четвёртая карточка с большим «?»; на «Напишите» внизу
-- появляется подпись «пишите в комментариях». Противники — картинки нейросети без фона, силуэты — те же
-- картинки, залитые тёмным. Секунды — моменты слов закадрового голоса; пока голос не записан, они
-- расчётные (130 слов в минуту), после записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  question = 0.66,
  wolf = 3.94,
  leshy = 4.57,
  robber = 5.8,
  someone = 6.62,
  write = 8.3,
}
local LIGHT = { T.wolf, T.leshy, T.robber, T.someone }

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

local STAGGER = 0.15
-- Пока карточек три, ряд стоит посередине кадра; на «кем-то» он отъезжает на место.
local ROW_SHIFT = 7.5
local SLIDE_TIME = 0.45
-- Четвёртая карточка выскакивает, когда третья уже почти отъехала, чтобы рамки не наехали.
local FOURTH_DELAY = 0.27
local REVEAL_TIME = 0.25
local REVEAL_BUMP = 0.06
local LABEL_DELAY = 0.12
local BOB = { height = 0.18, rate = 0.8 }
local CAPTION_RISE, CAPTION_RISE_TIME = 0.8, 0.18

local WAITING, CURRENT, DONE = 0, 1, 2
local EDGE_COLOR = { [WAITING] = "#4a4f5a", [CURRENT] = "#ffd34d", [DONE] = "#e9ecf2" }

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

local function pop_scale(u)
  if u < 0.12 then return lerp(0.3, 1.1, u / 0.12) end
  if u < 0.22 then return lerp(1.1, 1, (u - 0.12) / 0.1) end
  return 1
end

local function appear_at(slot)
  if slot == 4 then return T.someone + FOURTH_DELAY end
  return T.question + (slot - 1) * STAGGER
end

local function state_of(slot, t)
  local done_at = LIGHT[slot + 1] or T.write
  if t >= done_at then return DONE end
  if t >= LIGHT[slot] then return CURRENT end
  return WAITING
end

local function row_shift(slot, t)
  if slot == 4 then return 0 end
  return ROW_SHIFT * (1 - ease_in_out(clamp01((t - T.someone) / SLIDE_TIME)))
end

local function scaled(r, c, s)
  return { x = c.x + (r.x - c.x) * s, y = c.y + (r.y - c.y) * s, w = r.w * s, h = r.h * s }
end

local function put(p, r, dx)
  p.size = { x = r.w, y = r.h }
  p.position = { x = r.x + dx, y = r.y }
end

local home = nil

local function key_of(p)
  return p.part .. (p.slot or "")
end

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "part" } })
  if home == nil then
    home = {}
    for i = 1, #parts do
      local p = parts[i]
      home[key_of(p)] = { x = p.position.x, y = p.position.y, w = p.size.x, h = p.size.y }
    end
  end

  for i = 1, #parts do
    local p = parts[i]
    local h = home[key_of(p)]
    if p.part == "caption" then
      if t < T.write then
        p.position = HIDDEN
      else
        local k = clamp01((t - T.write) / CAPTION_RISE_TIME)
        p.position = { x = h.x, y = h.y + CAPTION_RISE * (1 - k) }
      end
    else
      local slot = p.slot
      local appear = appear_at(slot)
      local dx = row_shift(slot, t)
      local state = state_of(slot, t)
      local card = home["card" .. slot]
      local card_center = { x = card.x + card.w / 2, y = card.y + card.h / 2 }
      local pop = pop_scale(t - appear)
      if t < appear then
        p.position = HIDDEN
      elseif p.part == "mark" then
        if t < appear + LABEL_DELAY or state ~= WAITING then
          p.position = HIDDEN
        else
          local bob = BOB.height * math.sin(2 * math.pi * (t * BOB.rate + slot * 0.3))
          p.position = { x = h.x + dx, y = h.y + bob }
        end
      elseif p.part == "num" or p.part == "big" then
        if t < appear + LABEL_DELAY then
          p.position = HIDDEN
        else
          p.position = { x = h.x + dx, y = h.y }
          if p.part == "num" then p.tone = state end
        end
      elseif p.part == "dark" or p.part == "figure" then
        -- На своё слово противник чуть вздрагивает, стоя на месте, и проступает в цвете.
        local u = clamp01((t - LIGHT[slot]) / (REVEAL_TIME * 1.6))
        local feet = { x = h.x + h.w / 2, y = h.y + h.h }
        local r = scaled(h, feet, 1 + REVEAL_BUMP * math.sin(math.pi * u))
        put(p, scaled(r, card_center, pop), dx)
        if p.part == "figure" then
          p.opacity = clamp01((t - LIGHT[slot]) / REVEAL_TIME)
        end
      else
        put(p, scaled(h, card_center, pop), dx)
        if p.part == "edge" then p.color = EDGE_COLOR[state] end
      end
    end
  end
end
