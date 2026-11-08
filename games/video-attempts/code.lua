-- Попытки героя для второго ролика: лесной уровень со старым героем; на «перерисовать» кадр темнеет
-- и на месте героя мигает пустая пунктирная рамка; потом три попытки во весь кадр с подписями
-- «попытка 1», «попытка 2» и «попытка 3» золотом; на «силач» все три на миг встают рядом.
-- Секунды — моменты слов закадрового голоса; пока голос не записан, они расчётные (130 слов
-- в минуту), после записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  redraw = 3.4,
  first = 6.18,
  second = 11.06,
  third = 14.2,
  side = 17.68,
  finish = 20.08,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }
local SCREEN = { w = 32, h = 18 }

-- Кадры по порядку: картинка, куда смотрит медленный наезд (в клетках кадра во весь экран), подпись.
local SHOTS = {
  { from = 0, image = "old_hero", focus = { x = 10.2, y = 5.8 } },
  { from = T.first, image = "armored", focus = { x = 9.9, y = 5.0 }, caption = "попытка 1", gold = 0 },
  { from = T.second, image = "peasant", focus = { x = 10.2, y = 5.2 }, caption = "попытка 2", gold = 0 },
  { from = T.third, image = "ilya", focus = { x = 10.6, y = 5.6 }, caption = "попытка 3", gold = 1 },
}
local ZOOM_FROM, ZOOM_TO = 1.0, 1.12
-- Место старого героя, где мигает пустая рамка.
local OUTLINE = { x = 7.9, y = 3.6, w = 4.7, h = 4.4 }
local DIM_MAX = 0.35
local HOLE_MAX = 0.85
local DIM_TIME = 0.6
local BLINK = 0.3
local SIDE_STEP = 0.12

local function clamp(v, lo, hi)
  if v < lo then return lo end
  if v > hi then return hi end
  return v
end

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.5, 1.08, u / 0.12) end
  if u < 0.22 then return lerp(1.08, 1, (u - 0.12) / 0.1) end
  return 1
end

local function current_shot(t)
  local index = 1
  for i = 1, #SHOTS do
    if t >= SHOTS[i].from then index = i end
  end
  return index
end

local function camera(index, t)
  local shot = SHOTS[index]
  local next_from = SHOTS[index + 1] and SHOTS[index + 1].from or T.side
  local z = lerp(ZOOM_FROM, ZOOM_TO, clamp((t - shot.from) / (next_from - shot.from), 0, 1))
  local x = clamp(SCREEN.w / 2 - shot.focus.x * z, SCREEN.w - SCREEN.w * z, 0)
  local y = clamp(SCREEN.h / 2 - shot.focus.y * z, SCREEN.h - SCREEN.h * z, 0)
  return { z = z, x = x, y = y }
end

local home = {}

local function place_home(p, s)
  local h = home[p.part]
  local w, hh = h.w * s, h.hgt * s
  p.size = { x = w, y = hh }
  p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
end

function direct(director)
  local t = CLOCK_START - director.clock
  local side = t >= T.side
  local index = current_shot(t)
  local shot = SHOTS[index]
  local cam = camera(index, t)

  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    if home[p.part] == nil then
      home[p.part] = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
    end
    local name = p.part
    local crop = tonumber(string.match(name, "^crop(%d)$"))
    local cap = tonumber(string.match(name, "^cap(%d)$"))

    if crop or cap then
      local n = crop or cap
      local appear = T.side + (n - 1) * SIDE_STEP
      if t < appear then
        p.position = HIDDEN
      else
        place_home(p, pop_scale(t - appear))
        if cap then p.caption = "попытка " .. n end
      end
    elseif side then
      p.position = HIDDEN
    elseif name == shot.image then
      p.size = { x = SCREEN.w * cam.z, y = SCREEN.h * cam.z }
      p.position = { x = cam.x, y = cam.y }
    elseif name == "dim" then
      if index == 1 and t >= T.redraw then
        p.opacity = DIM_MAX * clamp((t - T.redraw) / DIM_TIME, 0, 1)
        p.size = { x = SCREEN.w, y = SCREEN.h }
        p.position = { x = 0, y = 0 }
      else
        p.position = HIDDEN
      end
    elseif name == "hole" then
      -- Сам герой гаснет сильнее всего кадра: тёмное пятно на его месте под рамкой.
      if index == 1 and t >= T.redraw then
        p.opacity = HOLE_MAX * clamp((t - T.redraw) / DIM_TIME, 0, 1)
        p.size = { x = OUTLINE.w * cam.z, y = OUTLINE.h * cam.z }
        p.position = { x = cam.x + OUTLINE.x * cam.z, y = cam.y + OUTLINE.y * cam.z }
      else
        p.position = HIDDEN
      end
    elseif name == "dash_a" then
      local on = math.floor((t - T.redraw) / BLINK) % 2 == 0
      if index == 1 and t >= T.redraw and on then
        p.size = { x = OUTLINE.w * cam.z, y = OUTLINE.h * cam.z }
        p.position = { x = cam.x + OUTLINE.x * cam.z, y = cam.y + OUTLINE.y * cam.z }
      else
        p.position = HIDDEN
      end
    elseif name == "plaque" then
      if shot.caption then
        place_home(p, pop_scale(t - shot.from))
        p.caption = shot.caption
        p.gold = shot.gold
      else
        p.position = HIDDEN
      end
    else
      p.position = HIDDEN
    end
  end
end
