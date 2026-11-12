-- «Ещё далеко» для второго ролика: кузня с рамой и рулём во весь кадр. На «каркас» железная рама на
-- козлах становится золотой, на «редактор» — руль; на «простые игры» камера подъезжает к раме, у которой
-- нет колёс, и стоит на ней «хотя до»; на «нормального» переезжает на чертёж на стене: там золотом то,
-- что уже выковано, — рама и руль, а на «далеко» синим пунктиром проступают колёса, вилка и седло,
-- которых ещё нет. Золото и пунктир — как в сцене с чертежом третьей главы; это картинки размером
-- с кузню, и они едут вместе с ней.
-- Секунды — моменты слов закадрового голоса; пока голос не записан, они расчётные (130 слов
-- в минуту), после записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  frame = 1.39,
  editor = 1.91,
  simple = 4.11,
  normal = 5.47,
  far = 7.23,
  finish = 8.29,
}

local CLOCK_START = 1000
local SCREEN = { w = 32, h = 18 }

local TO_FRAME_TIME = 0.9
local TO_PRINT_TIME = 1.2
local FADE_TIME = 0.4

-- Камера по моментам: во сколько раз приближена кузня и какая её точка (в клетках кузни во весь кадр)
-- посередине. К точке с `move` камера переезжает плавно, к остальным — медленно ровно подъезжает.
local SHOTS = {
  { t = 0, z = 1, x = 16, y = 9 },
  { t = T.simple, z = 1.06, x = 16, y = 9 },
  { t = T.simple + TO_FRAME_TIME, z = 1.6, x = 9.872, y = 10.715, move = true },   -- рама на козлах
  { t = T.normal, z = 1.63, x = 9.872, y = 10.715 },
  { t = T.normal + TO_PRINT_TIME, z = 1.9, x = 20.246, y = 5.513, move = true },   -- чертёж
  { t = T.finish, z = 1.97, x = 20.246, y = 5.513 },
}

-- Когда проступает каждая накладка; золото чертежа — к концу переезда камеры на него.
local APPEAR = {
  frame_gold = T.frame,
  bar_gold = T.editor,
  print_gold = T.normal + TO_PRINT_TIME - FADE_TIME,
  print_dashed = T.far,
}

local function clamp(v, lo, hi)
  if v < lo then return lo end
  if v > hi then return hi end
  return v
end

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function ease_in_out(u)
  if u < 0.5 then return 2 * u * u end
  return 1 - 2 * (1 - u) * (1 - u)
end

local function camera(t)
  for i = 2, #SHOTS do
    local a, b = SHOTS[i - 1], SHOTS[i]
    if t < b.t or i == #SHOTS then
      local u = clamp((t - a.t) / (b.t - a.t), 0, 1)
      if b.move then u = ease_in_out(u) end
      return { z = lerp(a.z, b.z, u), x = lerp(a.x, b.x, u), y = lerp(a.y, b.y, u) }
    end
  end
end

local home = {}

function direct(director)
  local t = CLOCK_START - director.clock
  local cam = camera(t)

  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = home[p.part]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[p.part] = h
    end
    -- Все картинки одного размера и едут одной камерой; кузня всегда закрывает кадр.
    local w, hh = h.w * cam.z, h.hgt * cam.z
    p.size = { x = w, y = hh }
    p.position = {
      x = clamp(SCREEN.w / 2 + (h.x - cam.x) * cam.z, SCREEN.w - w, 0),
      y = clamp(SCREEN.h / 2 + (h.y - cam.y) * cam.z, SCREEN.h - hh, 0),
    }
    local appear = APPEAR[p.part]
    if appear ~= nil then
      p.opacity = clamp((t - appear) / FADE_TIME, 0, 1)
    end
  end
end
