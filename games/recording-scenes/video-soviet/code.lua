-- Советский мультфильм на старой плёнке для второго ролика: проба во весь кадр с медленным наездом;
-- на «блёклая» картинку затягивает тёплая пелена и темнеют края, на «плёнка» поверх бегут царапины
-- и пыль, кадр подрагивает и мерцает, как в старом проекторе. Секунды — моменты слов закадрового
-- голоса; пока голос не записан, они расчётные (130 слов в минуту), после записи сюда ставятся
-- настоящие, и сцена записывается заново.
local T = {
  pale = 5.5,
  film = 6.36,
  finish = 8.82,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }
local SCREEN = { w = 32, h = 18 }

local ZOOM = 0.06
-- Пелена выцветания и затемнение краёв нарастают от «блёклая» до конца сцены.
local WASH_MAX = 0.42
local VIGNETTE_MAX = 0.8
local FADE_IN = 1.2
-- Плёнка меняет царапины, пыль и сдвиг кадра 15 раз в секунду, как проектор на двойках.
local FILM_RATE = 15
local WEAVE = 0.07
local FLICKER = 0.08
local VARIANTS = 4

local function clamp01(u)
  if u < 0 then return 0 end
  if u > 1 then return 1 end
  return u
end

-- Случайное число от 0 до 1 для кадра плёнки и соли: одно и то же при каждом запуске.
local function noise(frame, salt)
  local v = math.sin(frame * 12.9898 + salt * 78.233) * 43758.5453
  return v - math.floor(v)
end

local function full_frame(p, dx, dy, zoom)
  local w, h = SCREEN.w * zoom, SCREEN.h * zoom
  p.size = { x = w, y = h }
  p.position = { x = (SCREEN.w - w) / 2 + dx, y = (SCREEN.h - h) / 2 + dy }
end

function direct(director)
  local t = CLOCK_START - director.clock
  local film_on = t >= T.film
  local frame = math.floor(t * FILM_RATE)
  local fade = clamp01((t - T.pale) / FADE_IN)

  -- Какая царапина и какая пыль в этом кадре плёнки: иногда кадр чистый.
  local scratch = 0
  local dust = 0
  if film_on then
    if noise(frame, 1) < 0.8 then scratch = 1 + math.floor(noise(frame, 2) * VARIANTS) end
    if noise(frame, 3) < 0.9 then dust = 1 + math.floor(noise(frame, 4) * VARIANTS) end
  end

  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local name = p.part
    if name == "picture" then
      local dx, dy = 0, 0
      if film_on then
        dx = (noise(frame, 5) - 0.5) * 2 * WEAVE
        dy = (noise(frame, 6) - 0.5) * 2 * WEAVE
      end
      full_frame(p, dx, dy, 1 + ZOOM * clamp01(t / T.finish) + WEAVE / 8)
    elseif name == "wash" then
      p.opacity = WASH_MAX * fade
      full_frame(p, 0, 0, 1)
    elseif name == "vignette" then
      p.opacity = VIGNETTE_MAX * fade
      full_frame(p, 0, 0, 1)
    elseif name == "dark" then
      if film_on then
        p.opacity = FLICKER * noise(frame, 7)
        full_frame(p, 0, 0, 1)
      else
        p.position = HIDDEN
      end
    else
      local kind = string.sub(name, 1, -2)
      local number = tonumber(string.sub(name, -1))
      local chosen = (kind == "scratch" and number == scratch) or (kind == "dust" and number == dust)
      if chosen then
        -- Пыль каждый кадр ложится в другое место; царапина лишь чуть уходит вбок.
        local shift = kind == "dust" and 6 or 0.6
        full_frame(p, (noise(frame, 8 + number) - 0.5) * shift, (noise(frame, 12 + number) - 0.5) * shift, 1)
      else
        p.position = HIDDEN
      end
    end
  end
end
