-- Список для второго ролика: «бой» и «прыжки» с галочками, сверху падает «трава на ветру»
-- с качающимся кустом и тут же перечёркивается. Секунды — моменты слов закадрового голоса
-- «рисовать», «бой», «прыжки», «травы», «качается». Пока голос не записан, они расчётные
-- (130 слов в минуту); после записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  list = 1.85,
  fight = 4.79,
  jumps = 5.13,
  grass = 8.27,
  cross = 9.03,
}

local CLOCK_START = 1000
-- До своего слова часть стоит за краем сцены: цвет объекта прозрачным не бывает.
local HIDDEN = { x = -40, y = -40 }

local ROW_GAP = 0.2
local FALL_HEIGHT = 15
local FALL_TIME = 0.35
local STRIKE_TIME = 0.25

local LABELS = { text1 = "бой", text2 = "прыжки", text3 = "трава на ветру" }

-- Когда часть появляется и как: щелчком, падением всей третьей строки или ростом черты слева.
local APPEAR = {
  box1 = { at = "list", delay = 0, how = "pop" },
  hole1 = { at = "list", delay = 0, how = "pop" },
  text1 = { at = "list", delay = 0, how = "show" },
  box2 = { at = "list", delay = ROW_GAP, how = "pop" },
  hole2 = { at = "list", delay = ROW_GAP, how = "pop" },
  text2 = { at = "list", delay = ROW_GAP, how = "show" },
  tick1 = { at = "fight", delay = 0, how = "pop" },
  tick2 = { at = "jumps", delay = 0, how = "pop" },
  box3 = { at = "grass", delay = 0, how = "fall" },
  hole3 = { at = "grass", delay = 0, how = "fall" },
  text3 = { at = "grass", delay = 0, how = "fall" },
  bush = { at = "grass", delay = 0, how = "fall" },
  strike = { at = "cross", delay = 0, how = "grow" },
}

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.2, 1.15, u / 0.12) end
  if u < 0.2 then return lerp(1.15, 1, (u - 0.12) / 0.08) end
  return 1
end

-- Падение строки: разгон вниз, удар о место и короткий отскок.
local function fall_offset(u)
  if u < FALL_TIME then
    local k = u / FALL_TIME
    return -FALL_HEIGHT * (1 - k * k)
  end
  local b = u - FALL_TIME
  if b < 0.2 then
    return -0.5 * 4 * (b / 0.2) * (1 - b / 0.2)
  end
  return 0
end

-- Где и какого размера часть была записана в сцене: от этого считаются все её движения.
local home = {}

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = home[p.part]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[p.part] = h
    end
    local a = APPEAR[p.part]
    local u = a and (t - T[a.at] - a.delay) or 0
    if a and u < 0 then
      p.position = HIDDEN
    elseif a and a.how == "pop" then
      local s = pop_scale(u)
      local w, hh = h.w * s, h.hgt * s
      p.size = { x = w, y = hh }
      p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
    elseif a and a.how == "fall" then
      p.position = { x = h.x, y = h.y + fall_offset(u) }
    elseif a and a.how == "grow" then
      local k = u / STRIKE_TIME
      if k > 1 then k = 1 end
      p.size = { x = math.max(h.w * k, 0.01), y = h.hgt }
      p.position = { x = h.x, y = h.y }
    else
      p.position = { x = h.x, y = h.y }
    end
    if LABELS[p.part] and (a == nil or u >= 0) then
      p.label = LABELS[p.part]
    end
  end
end
