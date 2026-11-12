-- Длина особого удара разбойника для третьего ролика. Разбойник крупно в позе взмаха понизу — кадр 20
-- листа платформера, дубина вытянута вперёд; клетка игры здесь — CELL клеток сцены. С начала он стоит
-- тёмным силуэтом: рисунка ещё нет. На «плане» от его середины на уровне дубины вырастает линия на
-- 2,3 клетки с подписью «по плану 2,3 клетки»; на «нарисовали» силуэт проступает рисунком, и видно, что
-- линия выходит за конец дубины; на «по воздуху» этот кусок краснеет; на «Теперь» линия укорачивается до
-- 2,1 клетки (длина взмаха в коде боя) и вместе с подписью «сейчас 2,1» загорается золотом.
-- Секунды — моменты слов закадрового голоса от начала фразы «Ещё длину ударов я сначала задал…». Голос
-- ещё не записан, поэтому они расчётные (135 слов в минуту, слово номер i — на i × 0,444 с):
--   plan — «плане», drawn — «нарисовали», air — «по» в «по воздуху», now — «Теперь».
-- После записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  plan = 3.111,
  drawn = 6.222,
  air = 12,
  now = 12.889,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

-- Клеток сцены в клетке игры: кадр 4×3 клетки игры стоит здесь 20×15.
local CELL = 5
-- Середина разбойника по ширине — середина кадра (картинка стоит низом посередине его прямоугольника
-- 0,8 клетки); уровень дубины на её конце; низ кадра — земля.
local CENTER_X = 6.6
local CLUB_Y = 10.2
-- Длина взмаха по плану и сейчас; где по рисунку кадра 20 кончается дубина — клеток игры от середины.
local PLAN, NOW = 2.3, 2.1
local TIP = 1.885

local THICK = 0.16
local TICK_HEIGHT = 1.2
local LABEL_GAP, LABEL_HEIGHT = 0.5, 1.8

local GROW_TIME = 0.6
local SHRINK_TIME = 0.45
local COLOR_TIME = 0.3
local REVEAL_TIME, REVEAL_BUMP = 0.35, 0.04
local LABEL_DELAY = 0.25
local FADE_TIME, FADE_STEPS = 0.25, 5
local RISE, RISE_TIME = 0.3, 0.25

local WHITE, RED, GOLD = "#e9ecf2", "#ff4d4d", "#ffd34d"

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
  if u < 0.12 then return lerp(0.3, 1.15, u / 0.12) end
  if u < 0.22 then return lerp(1.15, 1, (u - 0.12) / 0.1) end
  return 1
end

local function channel(hex, i)
  return tonumber(string.sub(hex, 2 * i, 2 * i + 1), 16)
end

local function mix(a, b, u)
  local c = {}
  for i = 1, 3 do
    c[i] = math.floor(lerp(channel(a, i), channel(b, i), u) + 0.5)
  end
  return string.format("#%02x%02x%02x", c[1], c[2], c[3])
end

-- Длина линии в клетках игры: растёт до плана, на «Теперь» укорачивается до нынешней.
local function reach(t)
  if t < T.now then
    return PLAN * ease_out(clamp01((t - T.plan) / GROW_TIME))
  end
  return lerp(PLAN, NOW, ease_in_out(clamp01((t - T.now) / SHRINK_TIME)))
end

local function line_color(t)
  return mix(WHITE, GOLD, clamp01((t - T.now) / COLOR_TIME))
end

-- Засечка на конце краснеет вместе с куском за дубиной, а на «Теперь» загорается золотом с линией.
local function end_color(t)
  if t < T.now then
    return mix(WHITE, RED, clamp01((t - T.air) / COLOR_TIME))
  end
  return mix(RED, GOLD, clamp01((t - T.now) / COLOR_TIME))
end

local function show_label(p, t, from, end_x)
  local u = t - from
  if u < 0 then
    p.position = HIDDEN
    return
  end
  p.tone = math.floor(clamp01(u / FADE_TIME) * FADE_STEPS + 0.5)
  p.position = { x = end_x + LABEL_GAP, y = CLUB_Y - LABEL_HEIGHT / 2 + RISE * (1 - ease_out(clamp01(u / RISE_TIME)))}
end

-- Где и какого размера часть была записана в сцене.
local home = {}

function direct(director)
  local t = CLOCK_START - director.clock
  local end_x = CENTER_X + reach(t) * CELL
  local tip_x = CENTER_X + TIP * CELL
  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = home[p.part]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[p.part] = h
    end

    if p.part == "figure" then
      -- Рисунок проступает поверх силуэта и чуть вздрагивает, стоя на месте.
      local u = clamp01((t - T.drawn) / REVEAL_TIME)
      local s = 1 + REVEAL_BUMP * math.sin(math.pi * u)
      local feet = { x = h.x + h.w / 2, y = h.y + h.hgt }
      p.opacity = t < T.drawn and 0 or u
      p.size = { x = h.w * s, y = h.hgt * s }
      p.position = { x = feet.x - h.w * s / 2, y = feet.y - h.hgt * s }
    elseif p.part == "line" or p.part == "tick" or p.part == "end" or p.part == "air" then
      if t < T.plan then
        p.position = HIDDEN
      elseif p.part == "line" then
        p.size = { x = math.max(end_x - CENTER_X, 0.01), y = THICK }
        p.position = { x = CENTER_X, y = CLUB_Y - THICK / 2 }
        p.color = line_color(t)
      elseif p.part == "tick" then
        local s = pop_scale(t - T.plan)
        p.size = { x = THICK, y = TICK_HEIGHT * s }
        p.position = { x = CENTER_X - THICK / 2, y = CLUB_Y - TICK_HEIGHT * s / 2 }
        p.color = line_color(t)
      elseif p.part == "end" then
        p.size = { x = THICK, y = TICK_HEIGHT }
        p.position = { x = end_x - THICK / 2, y = CLUB_Y - TICK_HEIGHT / 2 }
        p.color = end_color(t)
      elseif t < T.air or end_x <= tip_x then
        p.position = HIDDEN
      else
        -- Кусок линии за концом дубины: краснеет на «по воздуху», на «Теперь» гаснет в золото.
        p.size = { x = end_x - tip_x, y = THICK }
        p.position = { x = tip_x, y = CLUB_Y - THICK / 2 }
        local alpha = clamp01((t - T.air) / COLOR_TIME)
        if t >= T.now then
          alpha = 1 - clamp01((t - T.now) / COLOR_TIME)
        end
        p.color = string.format("%s%02x", RED, math.floor(alpha * 255 + 0.5))
      end
    elseif p.part == "plan" then
      if t >= T.now then
        p.position = HIDDEN
      else
        show_label(p, t, T.plan + LABEL_DELAY, end_x)
      end
    elseif p.part == "now" then
      show_label(p, t, T.now + LABEL_DELAY, end_x)
    end
  end
end
