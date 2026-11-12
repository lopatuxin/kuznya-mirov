-- Выбор противника для начала третьего ролика. С первого кадра посередине стоят три карточки из конца
-- второго ролика — волк, леший, разбойник — уже в цвете; на «Чаще всего» ряд уменьшается и отъезжает
-- влево, а справа один за другим выезжают два комментария зрителей (настоящий текст, сказочные ники);
-- на «взял» карточка разбойника загорается золотым ободом и чуть растёт, волк и леший гаснут до серого.
-- Оформление «бронза»: карточки и комментарии — панели тёмного дерева в бронзе на размытом лесу уровня.
-- Серые противники — те же картинки без цвета и темнее, они лежат под цветными: цветная просто
-- становится прозрачной. Секунды — моменты слов закадрового голоса от начала фразы «Всем привет…»:
-- often — «Чаще» («Чаще всего называли разбойника»), took — «взял» («его я и взял»). Пока голос не
-- записан, они расчётные (135 слов в минуту, слово номер i — на i × 0,444 с); после записи сюда
-- ставятся настоящие, и сцена записывается заново.
local T = {
  often = 5.49,
  took = 8.25,
}

local CLOCK_START = 1000

-- Ряд противников: сначала посередине кадра, на «Чаще всего» уменьшается и встаёт у левого края.
local ROW_FROM = { x = 10.1, y = 4.35 }
local ROW_TO = { x = 2.5, y = 8.7 }
local ROW_SCALE = 0.72
local ROW_TIME = 0.5

-- Комментарии выезжают из-за правого края кадра с небольшим перелётом.
local COMMENT_SLIDE = 30
local COMMENT_DELAY = { 0.25, 0.55 }
local COMMENT_TIME = 0.55
local COMMENT_BACK = 0.7

-- На «взял» выбранный противник чуть вырастает с выскоком, поверх его карточки проступает золотой
-- светящийся обод, остальные гаснут.
local CHOSEN = 3
local CHOSEN_SCALE = 1.08
local CHOOSE_TIME = 0.35
local CHOSEN_BACK = 1.7

-- Номер цвета номера карточки в таблице `screens.json`: серый, золотой, белый.
local TONE_DIM, TONE_GOLD, TONE_WHITE = 0, 1, 2

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

-- Доезжает с перелётом: чем больше back, тем дальше проскакивает цель перед тем, как встать.
local function ease_out_back(u, back)
  local w = u - 1
  return 1 + (back + 1) * w * w * w + back * w * w
end

local function scaled(r, c, s)
  return { x = c.x + (r.x - c.x) * s, y = c.y + (r.y - c.y) * s, w = r.w * s, h = r.h * s }
end

local function put(p, r)
  p.size = { x = r.w, y = r.h }
  p.position = { x = r.x, y = r.y }
end

-- Место части в ряду противников в момент t: ряд целиком уменьшается к левому краю.
local function in_row(h, t)
  local u = ease_in_out(clamp01((t - T.often) / ROW_TIME))
  local s = lerp(1, ROW_SCALE, u)
  local ax, ay = lerp(ROW_FROM.x, ROW_TO.x, u), lerp(ROW_FROM.y, ROW_TO.y, u)
  return { x = ax + (h.x - ROW_FROM.x) * s, y = ay + (h.y - ROW_FROM.y) * s, w = h.w * s, h = h.h * s }
end

local home = nil

local function key_of(p)
  return p.part .. (p.slot or "") .. (p.comment or "")
end

local function place_enemy(p, h, t)
  local r = in_row(h, t)
  local choose = clamp01((t - T.took) / CHOOSE_TIME)
  local chosen = p.slot == CHOSEN
  if chosen then
    local card = in_row(home["card" .. p.slot], t)
    local center = { x = card.x + card.w / 2, y = card.y + card.h / 2 }
    r = scaled(r, center, lerp(1, CHOSEN_SCALE, ease_out_back(choose, CHOSEN_BACK)))
  end
  put(p, r)

  if p.part == "edge" then
    p.opacity = chosen and choose or 0
  elseif p.part == "figure" and not chosen then
    p.opacity = 1 - choose
  elseif p.part == "num" then
    if t < T.took then
      p.tone = TONE_WHITE
    elseif chosen then
      p.tone = TONE_GOLD
    elseif choose >= 0.5 then
      p.tone = TONE_DIM
    end
  end
end

local function place_comment(p, h, t)
  local u = clamp01((t - T.often - COMMENT_DELAY[p.comment]) / COMMENT_TIME)
  local dx = COMMENT_SLIDE * (1 - ease_out_back(u, COMMENT_BACK))
  put(p, { x = h.x + dx, y = h.y, w = h.w, h = h.h })
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
    if p.comment ~= nil then
      place_comment(p, h, t)
    else
      place_enemy(p, h, t)
    end
  end
end
