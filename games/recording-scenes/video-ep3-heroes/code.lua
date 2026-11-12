-- Герои игры для третьего ролика: ряд из трёх карточек, как карточки противников второго ролика.
-- Оформление «бронза»: карточки — панели тёмного дерева в бронзе на размытом лесу уровня, у Ильи
-- горит золотой обод. В первой — Илья в стойке (кадр 0 листа кадров платформера) с подписью «Илья Муромец», во второй и
-- третьей — безликие тёмные фигуры с жёлтым «?», без имён. На «Героев» посередине кадра выскакивает
-- карточка Ильи; на «несколько» она отъезжает на своё место слева, и вслед одна за другой выскакивают
-- две карточки с силуэтами; на «очередь» силуэты чуть вспыхивают.
-- Моменты расчётные, пока голос не записан: слово номер i (с нуля) фразы «Героев в игре будет
-- несколько, про остальных расскажу, когда до них дойдёт очередь» начинается на i × 0,444 с
-- (135 слов в минуту). heroes — «Героев» (слово 0), several — «несколько» (слово 4), turn — «очередь»
-- (слово 12). После записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  heroes = 0,
  several = 1.78,
  turn = 5.33,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

-- Пока Илья один, его карточка стоит посередине кадра; на «несколько» она отъезжает на место.
local SLIDE = 15
local SLIDE_TIME = 0.45
-- Вторая карточка выскакивает, когда первая уже почти отъехала, чтобы рамки не наехали; третья —
-- вслед за ней. Вспыхивают они тоже по очереди.
local SECOND_DELAY = 0.3
local STAGGER = 0.18
local FLASH_STAGGER = 0.12
-- Подпись и «?» появляются, когда карточка уже почти выскочила.
local LABEL_DELAY = 0.12
local BOB = { height = 0.18, rate = 0.8 }
-- Вспышка: светлый силуэт проступает поверх тёмного до FLASH.glow и гаснет за FLASH.time,
-- фигура чуть подрастает от ступней, золотой обод проступает до FLASH.rim, «?» подскакивает.
local FLASH = { time = 0.6, glow = 0.55, bump = 0.05, hop = 0.5, rim = 0.7 }

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
  if slot == 1 then return T.heroes end
  return T.several + SECOND_DELAY + (slot - 2) * STAGGER
end

local function row_shift(slot, t)
  if slot ~= 1 then return 0 end
  return SLIDE * (1 - ease_in_out(clamp01((t - T.several) / SLIDE_TIME)))
end

-- Сила вспышки силуэта от 0 до 1 и обратно; у Ильи вспышки нет.
local function flash_of(slot, t)
  if slot == 1 then return 0 end
  local u = (t - T.turn - (slot - 2) * FLASH_STAGGER) / FLASH.time
  if u <= 0 or u >= 1 then return 0 end
  return math.sin(math.pi * u)
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
  return p.part .. p.slot
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
    local slot = p.slot
    local appear = appear_at(slot)
    local card = home["card" .. slot]
    local card_center = { x = card.x + card.w / 2, y = card.y + card.h / 2 }
    local flash = flash_of(slot, t)
    local dx = row_shift(slot, t)
    if t < appear then
      p.position = HIDDEN
    elseif p.part == "title" then
      if t < appear + LABEL_DELAY then
        p.position = HIDDEN
      else
        p.position = { x = h.x + dx, y = h.y }
      end
    elseif p.part == "mark" then
      if t < appear + LABEL_DELAY then
        p.position = HIDDEN
      else
        local bob = BOB.height * math.sin(2 * math.pi * (t * BOB.rate + slot * 0.3))
        p.position = { x = h.x, y = h.y + bob - FLASH.hop * flash }
      end
    elseif p.part == "dark" or p.part == "lit" or p.part == "figure" then
      local feet = { x = h.x + h.w / 2, y = h.y + h.h }
      local r = scaled(h, feet, 1 + FLASH.bump * flash)
      put(p, scaled(r, card_center, pop_scale(t - appear)), dx)
      if p.part == "lit" then
        p.opacity = FLASH.glow * flash
      end
    else
      put(p, scaled(h, card_center, pop_scale(t - appear)), dx)
      if p.part == "edge" and slot ~= 1 then
        p.opacity = FLASH.rim * flash
      end
    end
  end
end
