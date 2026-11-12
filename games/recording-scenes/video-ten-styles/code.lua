-- Десять проб стиля для второго ролика: пробы выкладываются сеткой 5×2 по одной, к слову «десяти»
-- сетка полная; на «трёх» семь гаснут, а реализм, советский и современный мультфильм выезжают
-- вперёд крупно, и под каждым на своё слово появляется подпись. Секунды — моменты слов
-- закадрового голоса; пока голос не записан, они расчётные (130 слов в минуту), после записи
-- сюда ставятся настоящие, и сцена записывается заново.
local T = {
  lay = 1.57,
  ten = 7.85,
  three = 10.35,
  realism = 10.75,
  soviet = 11.79,
  modern = 12.99,
}

local CLOCK_START = 1000
local TILES = 10
local CHOSEN = 3

-- Пока сетка стоит, она медленно приближается к зрителю на GRID_ZOOM вокруг середины кадра.
local GRID_ZOOM = 0.04
local CENTER = { x = 16, y = 9 }

-- Три выбранные пробы в ряд: размер и левый верхний угол первой, шаг между ними.
local BIG = { w = 9.6, h = 5.4, x = 1.0, y = 4.4, step = 10.2 }
local MOVE_TIME = 0.5
local FADE_TIME = 0.4

local CAPTIONS = { "реализм", "советский мультфильм", "современный мультфильм" }
local CAPTION_CUE = { "realism", "soviet", "modern" }

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

-- Где проба стоит в сетке: место из сцены, приближенное вокруг середины кадра.
local function grid_rect(h, t)
  local zoom = 1 + GRID_ZOOM * clamp01((t - T.lay) / (T.three - T.lay))
  return {
    x = CENTER.x + (h.x - CENTER.x) * zoom,
    y = CENTER.y + (h.y - CENTER.y) * zoom,
    w = h.w * zoom,
    h = h.hgt * zoom,
  }
end

local home = {}

local function place(p, x, y, w, h)
  p.size = { x = w, y = h }
  p.position = { x = x, y = y }
end

function direct(director)
  local t = CLOCK_START - director.clock

  local tiles = find({ has = { "tile" } })
  for i = 1, #tiles do
    local p = tiles[i]
    local h = home[p.tile]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[p.tile] = h
    end

    local appear = T.lay + (p.tile - 1) * (T.ten - T.lay) / (TILES - 1)
    local g = grid_rect(h, math.min(t, T.three))
    if t < appear then
      p.opacity = 0
      place(p, g.x, g.y, g.w, g.h)
    elseif t < T.three or p.tile > CHOSEN then
      local s = pop_scale(t - appear)
      local w, hh = g.w * s, g.h * s
      place(p, g.x + (g.w - w) / 2, g.y + (g.h - hh) / 2, w, hh)
      if p.tile > CHOSEN then
        p.opacity = 1 - clamp01((t - T.three) / FADE_TIME)
      else
        p.opacity = 1
      end
    else
      local u = ease_in_out(clamp01((t - T.three) / MOVE_TIME))
      local bx = BIG.x + (p.tile - 1) * BIG.step
      place(p, lerp(g.x, bx, u), lerp(g.y, BIG.y, u), lerp(g.w, BIG.w, u), lerp(g.h, BIG.h, u))
      p.opacity = 1
    end
  end

  local captions = find({ has = { "caption_of" } })
  for i = 1, #captions do
    local c = captions[i]
    if t >= T[CAPTION_CUE[c.caption_of]] then
      c.caption = CAPTIONS[c.caption_of]
    end
  end
end
