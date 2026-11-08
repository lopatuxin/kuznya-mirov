-- Ошибки реализма для второго ролика: реализм во весь кадр, потом камера крупно показывает ошибки
-- нейросети — четвёртую голову под крылом, голову над остальными, палицу-фонарь, каждую обводит
-- красный круг, — и снова реализм: дракон и замок на горе с подписями. Секунды — моменты слов
-- закадрового голоса; пока голос не записан, они расчётные (130 слов в минуту), после записи
-- сюда ставятся настоящие, и сцена записывается заново.
local T = {
  head1 = 5.58,
  head2 = 6.54,
  mace = 7.42,
  lantern = 9.32,
  dragon = 11.62,
  dragonword = 12.12,
  forest = 13.18,
  castle = 13.88,
  finish = 15.05,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }
local SCREEN = { w = 32, h = 18 }

-- Места на картинке в клетках картинки во весь кадр (точка исходника, делённая на 40).
local HEAD1 = { x = 19.6, y = 6.6, r = 1.6 }
local HEAD2 = { x = 18.8, y = 1.0, r = 1.1 }
local MACE = { x = 4.05, y = 7.3, r = 1.9 }
local DRAGON = { x = 22.5, y = 6.5 }
local CASTLE = { x = 17.2, y = 2.8 }
local DRAGON_LABEL = { x = 22.5, y = 10.8 }
local CASTLE_LABEL = { x = 17.2, y = 5.0 }
local PAN_TIME = 0.8

-- Кадры сцены по порядку: какая картинка, куда смотрит камера и как медленно наезжает.
local SHOTS = {
  { from = 0, image = "realism", z0 = 1.0, z1 = 1.08 },
  { from = T.head1, image = "bridge_card", focus = HEAD1, z0 = 2.0, z1 = 2.08 },
  { from = T.head2, image = "bridge_brush", focus = HEAD2, z0 = 1.9, z1 = 1.98 },
  { from = T.mace, image = "bridge_card", focus = MACE, z0 = 2.0, z1 = 2.3 },
  { from = T.dragon, image = "realism", z0 = 1.7, z1 = 1.8 },
}

-- Красный круг: на какой картинке, вокруг чего и с какого слова.
local RINGS = {
  ring_head1 = { shot = 2, target = HEAD1, from = T.head1 + 0.25 },
  ring_head2 = { shot = 3, target = HEAD2, from = T.head2 + 0.2 },
  ring_mace = { shot = 4, target = MACE, from = T.lantern },
}
-- В картинке круга его линия идёт на 0,8 половины стороны; круг берётся с запасом в 1,1 цели.
local RING_REACH = 0.8 * 1.1

local PLAQUES = {
  plaque_dragon = { at = DRAGON_LABEL, from = T.dragonword, text = "дракон" },
  plaque_castle = { at = CASTLE_LABEL, from = T.castle, text = "замок" },
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

local function pop_scale(u)
  if u < 0.12 then return lerp(0.4, 1.12, u / 0.12) end
  if u < 0.22 then return lerp(1.12, 1, (u - 0.12) / 0.1) end
  return 1
end

local function current_shot(t)
  local index = 1
  for i = 1, #SHOTS do
    if t >= SHOTS[i].from then index = i end
  end
  return index
end

-- Камера кадра: увеличение и левый верхний угол картинки на экране. Картинка всегда закрывает кадр.
local function camera(index, t)
  local shot = SHOTS[index]
  local next_from = SHOTS[index + 1] and SHOTS[index + 1].from or T.finish
  local z = lerp(shot.z0, shot.z1, clamp((t - shot.from) / (next_from - shot.from), 0, 1))
  local focus = shot.focus or { x = SCREEN.w / 2, y = SCREEN.h / 2 }
  if index == #SHOTS then
    local u = ease_in_out(clamp((t - T.forest) / PAN_TIME, 0, 1))
    focus = { x = lerp(DRAGON.x, CASTLE.x, u), y = lerp(DRAGON.y, CASTLE.y, u) }
  end
  local x = clamp(SCREEN.w / 2 - focus.x * z, SCREEN.w - SCREEN.w * z, 0)
  local y = clamp(SCREEN.h / 2 - focus.y * z, SCREEN.h - SCREEN.h * z, 0)
  return { z = z, x = x, y = y }
end

local function on_screen(cam, p)
  return cam.x + p.x * cam.z, cam.y + p.y * cam.z
end

function direct(director)
  local t = CLOCK_START - director.clock
  local index = current_shot(t)
  local cam = camera(index, t)

  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local ring = RINGS[p.part]
    local plaque = PLAQUES[p.part]
    if ring then
      if index == ring.shot and t >= ring.from then
        local half = ring.target.r * cam.z / RING_REACH * pop_scale(t - ring.from)
        local cx, cy = on_screen(cam, ring.target)
        p.size = { x = 2 * half, y = 2 * half }
        p.position = { x = cx - half, y = cy - half }
      else
        p.position = HIDDEN
      end
    elseif plaque then
      if index == #SHOTS and t >= plaque.from then
        local s = pop_scale(t - plaque.from)
        local w, h = 5 * s, 1.6 * s
        local cx, cy = on_screen(cam, plaque.at)
        p.size = { x = w, y = h }
        p.position = { x = cx - w / 2, y = cy - h / 2 }
        p.caption = plaque.text
      else
        p.position = HIDDEN
      end
    elseif p.part == SHOTS[index].image then
      p.size = { x = SCREEN.w * cam.z, y = SCREEN.h * cam.z }
      p.position = { x = cam.x, y = cam.y }
    else
      p.position = HIDDEN
    end
  end
end
