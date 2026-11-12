-- Автор под горой листов для второго ролика: автор со спины за ноутбуком, сверху падают листы —
-- отрисовки моста, на каждой красный круг, — и копятся в горы у стола; на «сошёл с ума» картинка
-- сменяется той, где он схватился за голову, листы сыплются чаще, гора вздрагивает. Секунда — момент
-- слова «сошёл» закадрового голоса; пока голос не записан, она расчётная (130 слов в минуту), после
-- записи сюда ставится настоящая, и сцена записывается заново.
local T = {
  first = 0.3,
  mad = 4.37,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

-- Сколько листов падает до «сошёл с ума» (с разгоном), остальные — чаще после.
local BEFORE_MAD = 38
local RAMP = 0.75
local AFTER_STEP = 0.12

local FALL_FROM = -3
local FALL_TIME = 0.32
local BOUNCE = 0.08
local SHAKE_TIME = 0.5

-- Бумага листа и картинка на ней: картинка с полями по 0,1 клетки.
local MARGIN = 0.1
local PIC = { w = 3.2, h = 1.8 }
local RING = 1.4
-- Где на каждой отрисовке ошибка, которую обводит круг: доля ширины и высоты картинки.
local SPOT = {
  sheet_card = { x = 0.61, y = 0.37 },
  sheet_brush = { x = 0.59, y = 0.12 },
  sheet_channel = { x = 0.62, y = 0.35 },
  sheet_outline = { x = 0.62, y = 0.35 },
  sheet_realism = { x = 0.70, y = 0.36 },
}

local function drop_time(card)
  if card <= BEFORE_MAD then
    return T.first + (T.mad - T.first) * (card / BEFORE_MAD) ^ RAMP
  end
  return T.mad + (card - BEFORE_MAD) * AFTER_STEP
end

-- Сдвиг листа по высоте от места, где он ляжет: падение с разгоном и короткий отскок.
local function fall_offset(u, ty)
  if u < FALL_TIME then
    local k = u / FALL_TIME
    return (FALL_FROM - ty) * (1 - k * k)
  end
  local b = u - FALL_TIME
  if b < 0.12 then
    return -BOUNCE * 4 * (b / 0.12) * (1 - b / 0.12)
  end
  return 0
end

local function shake(t, card)
  local u = t - T.mad
  if u < 0 or u > SHAKE_TIME then return 0 end
  return 0.12 * math.sin(u * 70 + card) * (1 - u / SHAKE_TIME)
end

-- Какая картинка на листе: её имя записано у части «pic» того же листа.
local pic_of = {}

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "card" } })
  for i = 1, #parts do
    local p = parts[i]
    if p.role == "pic" and pic_of[p.card] == nil then
      pic_of[p.card] = p.image
    end
  end

  for i = 1, #parts do
    local p = parts[i]
    local u = t - drop_time(p.card)
    if u < 0 then
      p.position = HIDDEN
    else
      local x = p.tx + shake(t, p.card)
      local y = p.ty + fall_offset(u, p.ty)
      if p.role == "paper" then
        p.position = { x = x, y = y }
      elseif p.role == "pic" then
        p.position = { x = x + MARGIN, y = y + MARGIN }
      else
        local spot = SPOT[pic_of[p.card]]
        p.position = {
          x = x + MARGIN + spot.x * PIC.w - RING / 2,
          y = y + MARGIN + spot.y * PIC.h - RING / 2,
        }
      end
    end
  end

  local desk = find({ has = { "part" } })
  for i = 1, #desk do
    local d = desk[i]
    local shown = (d.part == "picture_desk") == (t < T.mad)
    if shown then
      d.position = { x = 0, y = 0 }
    else
      d.position = HIDDEN
    end
  end
end
