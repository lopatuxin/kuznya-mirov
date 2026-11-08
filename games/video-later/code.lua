-- «Остальное — потом» для второго ролика: список из главы «Прототип» — «бой» и «прыжки» с галочками;
-- на «боя» и «прыжков» строка загорается жёлтым, а галочка заново щёлкает; на «остальное» ниже серым
-- проступает «остальное — потом». Секунды — моменты слов закадрового голоса; пока голос не записан,
-- они расчётные (130 слов в минуту), после записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  list = 0.3,
  fight = 6,
  jumps = 6.32,
  rest = 7.36,
}

local CLOCK_START = 1000
local HIDDEN = { x = -40, y = -40 }
local ROW_GAP = 0.2
-- Серая строка проступает за FADE_TIME через столько ступеней цвета, сколько их в `screens.json`.
local FADE_TIME = 0.8
local FADE_STEPS = 5

-- У каждой части: строка списка и слово, на котором она загорается.
local ROW = { box1 = 1, hole1 = 1, tick1 = 1, text1 = 1, box2 = 2, hole2 = 2, tick2 = 2, text2 = 2 }
local LIGHT = { "fight", "jumps" }

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.3, 1.15, u / 0.12) end
  if u < 0.2 then return lerp(1.15, 1, (u - 0.12) / 0.08) end
  return 1
end

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

    local row = ROW[p.part]
    if row then
      local appear = T.list + (row - 1) * ROW_GAP
      local light = T[LIGHT[row]]
      local s = 1
      if t < appear then
        s = nil
      elseif p.part == "tick" .. row and t >= light then
        s = pop_scale(t - light)
      else
        s = pop_scale(t - appear)
      end
      if s == nil then
        p.position = HIDDEN
      else
        local w, hh = h.w * s, h.hgt * s
        p.size = { x = w, y = hh }
        p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
      end
      if p.lit ~= nil then
        p.lit = t >= light and 1 or 0
      end
    else
      local u = (t - T.rest) / FADE_TIME
      if u < 0 then u = 0 end
      if u > 1 then u = 1 end
      p.fade = math.floor(u * FADE_STEPS + 0.5)
    end
  end
end
