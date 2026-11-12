-- Прототип из квадратов для второго ролика: всё движение задано временем от начала партии.
-- Секунды — моменты слов закадрового голоса: «герой», «враг», «земля», «двигается», «сталкивается»,
-- «большие студии», «не понимаю». Пока голос не записан, они расчётные (130 слов в минуту);
-- после записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  hero = 4.64,
  enemy = 5.88,
  ground = 7.75,
  move = 11.64,
  crash = 12.26,
  studios = 14.34,
  puzzled = 19.38,
}

local CLOCK_START = 1000
local SIDE = 1.8
-- До своего слова фигура стоит за краем сцены: цвет объекта прозрачным не бывает.
local HIDDEN = { x = -20, y = -20 }

-- Середины площадок по x и их верх: квадрат стоит низом на верхе площадки.
local P1 = { cx = 7, top = 12 }
local P2 = { cx = 16, top = 9 }
local P3 = { top = 12 }
local ENEMY_X = 26.2
local CONTACT_X = ENEMY_X - SIDE

local JUMP_TIME = 0.5
local LUNGE_TIME = 0.45
local HOP_TIME = 0.35
local HOP_EVERY = 1.6

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

-- Появление «щелчком»: из маленького чуть больше нормы и обратно, всего за пятую долю секунды.
local function pop_scale(t, t0)
  local u = t - t0
  if u < 0 then return nil end
  if u < 0.12 then return lerp(0.2, 1.15, u / 0.12) end
  if u < 0.2 then return lerp(1.15, 1, (u - 0.12) / 0.08) end
  return 1
end

-- Высота подскока на месте: дуга, которая начинается в t0 и длится dur.
local function hop_lift(t, t0, dur, height)
  local u = (t - t0) / dur
  if u <= 0 or u >= 1 then return 0 end
  return 4 * height * u * (1 - u)
end

-- Сплющивание при приземлении: на долю секунды квадрат шире и ниже.
local function squash(t, t_land)
  local u = t - t_land
  if u < 0 or u > 0.1 then return 1, 1 end
  return 1.18, 0.82
end

-- Ставит квадрат низом в точку (cx, bottom) с масштабом и сплющиванием.
local function place(obj, cx, bottom, w, h)
  obj.size = { x = w, y = h }
  obj.position = { x = cx - w / 2, y = bottom - h }
end

-- Подскоки после столкновения: каждые HOP_EVERY секунд до «не понимаю», у врага — со сдвигом.
local function idle_hops(t, first, height)
  local lift = 0
  local t0 = first
  while t0 < T.puzzled - 0.4 do
    lift = lift + hop_lift(t, t0, HOP_TIME, height)
    t0 = t0 + HOP_EVERY
  end
  return lift + hop_lift(t, T.puzzled, 0.25, 0.5)
end

local function hero_state(t)
  if t < T.move then
    return P1.cx, P1.top, nil
  end
  if t < T.move + JUMP_TIME then
    local u = (t - T.move) / JUMP_TIME
    return lerp(P1.cx, P2.cx, u), lerp(P1.top, P2.top, u) - 4 * 3 * u * (1 - u), nil
  end
  local lunge_start = T.crash - LUNGE_TIME
  if t < lunge_start then
    return P2.cx, P2.top, T.move + JUMP_TIME
  end
  if t < T.crash then
    local u = (t - lunge_start) / LUNGE_TIME
    return lerp(P2.cx, CONTACT_X, u), lerp(P2.top, P3.top, u) - 4 * 2.5 * u * (1 - u), nil
  end
  local recoil = ease_out(clamp01((t - T.crash) / 0.15))
  local cx = lerp(CONTACT_X, CONTACT_X - 0.5, recoil)
  return cx, P3.top - idle_hops(t, T.studios + 0.3, 1), T.crash
end

local function enemy_state(t)
  if t < T.crash then
    return ENEMY_X, P3.top
  end
  local push = ease_out(clamp01((t - T.crash) / 0.18))
  local cx = lerp(ENEMY_X, ENEMY_X + 0.8, push)
  return cx, P3.top - hop_lift(t, T.crash, 0.22, 0.4) - idle_hops(t, T.studios + 0.9, 0.8)
end

-- Дрожь обоих квадратов на ударе.
local function shake(t)
  local u = t - T.crash
  if u < 0 or u > 0.2 then return 0 end
  return 0.08 * math.sin(u * 90)
end

local function draw_square(obj, t, t_appear, cx, bottom, t_land)
  local s = pop_scale(t, t_appear)
  if s == nil then
    obj.position = HIDDEN
    return
  end
  local sx, sy = 1, 1
  if t_land then sx, sy = squash(t, t_land) end
  place(obj, cx + shake(t), bottom, SIDE * s * sx, SIDE * s * sy)
end

local PLATFORMS = {
  p1 = { x = 3, y = 12, w = 8, delay = 0 },
  p2 = { x = 13.5, y = 9, w = 5, delay = 0.25 },
  p3 = { x = 21, y = 12, w = 8, delay = 0.5 },
}

function direct(director)
  local t = CLOCK_START - director.clock

  local hero = find({ has = { "hero" } })[1]
  local hx, hb, hland = hero_state(t)
  draw_square(hero, t, T.hero, hx, hb, hland)

  local enemy = find({ has = { "enemy" } })[1]
  local ex, eb = enemy_state(t)
  draw_square(enemy, t, T.enemy, ex, eb, nil)

  local platforms = find({ has = { "platform" } })
  for i = 1, #platforms do
    local p = platforms[i]
    local spec = PLATFORMS[p.name]
    local s = pop_scale(t, T.ground + spec.delay)
    if s == nil then
      p.position = HIDDEN
    else
      local w = spec.w * s
      p.size = { x = w, y = s }
      p.position = { x = spec.x + (spec.w - w) / 2, y = spec.y + (1 - s) }
    end
  end

  if t >= T.studios then
    local spot = find({ has = { "caption_spot" } })[1]
    spot.caption = "так делают даже большие студии"
  end
  if t >= T.puzzled then
    local spot = find({ has = { "mark_spot" } })[1]
    spot.mark = "?"
  end
end
