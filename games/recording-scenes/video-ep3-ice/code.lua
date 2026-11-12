-- Квадрат на льду для третьего ролика: так вёл бы себя квадрат-герой из прототипа второго ролика,
-- если бы движок считал настоящую физику с трением и толчками. Пол и высокая стена справа — как
-- площадки прототипа, сверху подпись «физика как в жизни». На «скользит» квадрат разгоняется, на
-- черте «отпустил» клавишу отпустили, а он катится далеко за черту и медленно тормозит; на «залипает»
-- прыгает к стене, прижимается к ней и висит, еле сползая; на «почти не делают» всё чуть темнеет.
-- Физики здесь нет: всё движение задано временем от начала сцены. Оформление «бронза»: схема лежит
-- на панели тёмного дерева поверх размытого леса уровня.
-- Моменты расчётные, пока голос не записан: слово номер i (с нуля) фразы «Настоящую физику, как
-- в жизни, … почти не делают» начинается на i × 0,444 с (135 слов в минуту). slide — «скользит»
-- (слово 16), stick — «залипает» (слово 21), rarely — «почти» (слово 29). После записи сюда ставятся
-- настоящие, и сцена записывается заново.
local T = {
  slide = 5.02,
  stick = 6.38,
  rarely = 9.22,
}

local CLOCK_START = 1000
-- До своего часа часть стоит за краем сцены: цвет объекта прозрачным не бывает.
local HIDDEN = { x = -40, y = -40 }

local SIDE = 1.8
local GROUND = 12.8
-- Левая грань стены: к ней прижимается квадрат.
local WALL_X = 27

-- Разгон с нажатой клавишей: из START_X за PUSH.time на PUSH.dist клеток, ровно до черты «отпустил».
local START_X = 5.2
local PUSH = { dist = 3.2, time = 0.5 }
-- После черты квадрат катится ещё GLIDE клеток и тормозит равномерно, как на льду.
local GLIDE = 9.5
local MARK_X = START_X + PUSH.dist
local SPEED = 2 * PUSH.dist / PUSH.time
local GLIDE_TIME = 2 * GLIDE / SPEED
local RELEASE_AT = T.slide + PUSH.time
local STOP_X = MARK_X + GLIDE

-- Прыжок к стене: вершина дуги — у самой стены, низ квадрата там на STICK_BOTTOM.
local JUMP_TIME = 0.42
local STICK_BOTTOM = 8.8
local CONTACT_AT = T.stick + JUMP_TIME
-- На стене квадрат сползает на CREEP клеток в секунду.
local CREEP = 0.16
-- Ударившись о стену, квадрат на миг сплющивается о неё.
local PRESS = { w = 1.5, h = 2.1, time = 0.12 }

-- Начало сцены: пол выскакивает, стена вырастает из пола, за ними квадрат.
local INTRO = { floor = 0, wall = 0.15, hero = 0.35 }
local WALL_GROW = { time = 0.22, over = 1.06, settle = 0.1 }
local MARK_GROW_TIME = 0.15

-- Подписи въезжают снизу на RISE клетки за RISE_TIME.
local RISE, RISE_TIME = 0.6, 0.18
local LABEL_AT = { head = 0.2, slidecap = T.slide, note = RELEASE_AT, stickcap = CONTACT_AT }

-- Затемнение: чёрная картинка поверх всего до SHADE за DARKEN_TIME. Подписи рисуются поверх
-- картинок, поэтому темнеют своим цветом — ступенями таблиц в `screens.json`.
local SHADE = 0.35
local DARKEN_TIME = 0.6
local DIM_STEPS = 5

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

local function pop_scale(u)
  if u < 0.12 then return lerp(0.2, 1.15, u / 0.12) end
  if u < 0.2 then return lerp(1.15, 1, (u - 0.12) / 0.08) end
  return 1
end

-- Рост стены вверх от пола с небольшим перелётом.
local function grow_scale(u)
  if u < WALL_GROW.time then return WALL_GROW.over * ease_out(u / WALL_GROW.time) end
  return lerp(WALL_GROW.over, 1, clamp01((u - WALL_GROW.time) / WALL_GROW.settle))
end

-- Где квадрат в момент t: середина по x, низ, ширина и высота.
local function hero_state(t)
  if t < T.slide then
    return START_X, GROUND, SIDE, SIDE
  end
  if t < RELEASE_AT then
    local u = t - T.slide
    return START_X + SPEED / PUSH.time * u * u / 2, GROUND, SIDE, SIDE
  end
  if t < RELEASE_AT + GLIDE_TIME then
    local u = t - RELEASE_AT
    return MARK_X + SPEED * u - SPEED / GLIDE_TIME * u * u / 2, GROUND, SIDE, SIDE
  end
  if t < T.stick then
    return STOP_X, GROUND, SIDE, SIDE
  end
  local wall_cx = WALL_X - SIDE / 2
  if t < CONTACT_AT then
    local u = (t - T.stick) / JUMP_TIME
    return lerp(STOP_X, wall_cx, u), GROUND - (GROUND - STICK_BOTTOM) * ease_out(u), SIDE, SIDE
  end
  local bottom = STICK_BOTTOM + CREEP * (t - CONTACT_AT)
  if t < CONTACT_AT + PRESS.time then
    local mid = bottom - SIDE / 2
    return WALL_X - PRESS.w / 2, mid + PRESS.h / 2, PRESS.w, PRESS.h
  end
  return wall_cx, bottom, SIDE, SIDE
end

-- Где и какого размера часть была записана в сцене: от этого считаются все её движения.
local home = {}

function direct(director)
  local t = CLOCK_START - director.clock
  local dark = clamp01((t - T.rarely) / DARKEN_TIME)
  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = home[p.part]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[p.part] = h
    end

    if p.part == "floor" then
      local u = t - INTRO.floor
      if u < 0 then
        p.position = HIDDEN
      else
        local s = pop_scale(u)
        local w, hh = h.w * s, h.hgt * s
        p.size = { x = w, y = hh }
        p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) }
      end
    elseif p.part == "wall" then
      local u = t - INTRO.wall
      if u < 0 then
        p.position = HIDDEN
      else
        local hh = math.max(h.hgt * grow_scale(u), 0.01)
        p.size = { x = h.w, y = hh }
        p.position = { x = h.x, y = h.y + h.hgt - hh }
      end
    elseif p.part == "mark" then
      local u = t - RELEASE_AT
      if u < 0 then
        p.position = HIDDEN
      else
        local hh = math.max(h.hgt * ease_out(clamp01(u / MARK_GROW_TIME)), 0.01)
        p.size = { x = h.w, y = hh }
        p.position = { x = h.x, y = h.y + (h.hgt - hh) / 2 }
      end
    elseif p.part == "hero" then
      local u = t - INTRO.hero
      if u < 0 then
        p.position = HIDDEN
      else
        local s = pop_scale(u)
        local cx, bottom, w, hh = hero_state(t)
        w, hh = w * s, hh * s
        p.size = { x = w, y = hh }
        p.position = { x = cx - w / 2, y = bottom - hh }
      end
    elseif p.part == "shade" then
      p.opacity = SHADE * dark
    else
      local u = t - LABEL_AT[p.part]
      if u < 0 then
        p.position = HIDDEN
      else
        p.position = { x = h.x, y = h.y + RISE * (1 - ease_out(clamp01(u / RISE_TIME))) }
      end
      p.dim = math.floor(dark * DIM_STEPS + 0.5)
    end
  end
end
