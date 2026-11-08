-- Руль на раме для второго ролика: кузнец ставит на раму руль с красной биркой «ИНТЕРФЕЙС» — бирку
-- нарисовала нейросеть, — а камера медленно наезжает с кузни на руль и бирку, не теряя лица кузнеца.
-- Сцена идёт под «…но управлять им пока нечем» сразу после карточки главы «Редактор»; своих слов у неё
-- нет, наезд идёт всю сцену.
local DURATION = 5

local CLOCK_START = 1000
local SCREEN = { w = 32, h = 18 }

-- Середина руля с биркой на кузне во весь кадр, в клетках, и наезд на неё.
local HANDLEBAR = { x = 13.23, y = 7.3 }
local ZOOM_TO = 1.45

local function clamp(v, lo, hi)
  if v < lo then return lo end
  if v > hi then return hi end
  return v
end

local function ease_in_out(u)
  if u < 0.5 then return 2 * u * u end
  return 1 - 2 * (1 - u) * (1 - u)
end

local home = nil

function direct(director)
  local t = CLOCK_START - director.clock
  local u = ease_in_out(clamp(t / DURATION, 0, 1))
  local z = 1 + (ZOOM_TO - 1) * u
  local fx = SCREEN.w / 2 + (HANDLEBAR.x - SCREEN.w / 2) * u
  local fy = SCREEN.h / 2 + (HANDLEBAR.y - SCREEN.h / 2) * u

  local p = find({ has = { "part" } })[1]
  if home == nil then
    home = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
  end
  -- Кузня всегда закрывает кадр целиком.
  local w, hh = home.w * z, home.hgt * z
  p.size = { x = w, y = hh }
  p.position = {
    x = clamp(SCREEN.w / 2 + (home.x - fx) * z, SCREEN.w - w, 0),
    y = clamp(SCREEN.h / 2 + (home.y - fy) * z, SCREEN.h - hh, 0),
  }
end
