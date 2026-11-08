-- Rust и код движка для второго ролика: в панели снизу медленно прокручивается настоящий кусок кода
-- движка — шаг мира; на «Rust» над ним появляется логотип языка, на «язык» рядом подпись «язык, где
-- важна скорость». Секунды — моменты слов закадрового голоса; пока голос не записан, они расчётные
-- (130 слов в минуту), после записи сюда ставятся настоящие, и сцена записывается заново.
local T = {
  rust = 4.43,
  lang = 4.91,
  finish = 9.31,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

-- Окно панели кода по высоте, высота строки и скорость прокрутки: к концу сцены видны последние строки.
local PANEL_TOP, PANEL_BOTTOM = 8.35, 16.95
local LINE = 0.62
local LINES = 52
local SPEED = (LINES * LINE - (PANEL_BOTTOM - PANEL_TOP)) / T.finish
-- Строка у края панели проступает за три ступени цвета; номер цвета — тон строки × 4 + ступень.
local FADE_STEPS = 3

local CAPTION = "язык, где важна скорость"

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.4, 1.1, u / 0.12) end
  if u < 0.22 then return lerp(1.1, 1, (u - 0.12) / 0.1) end
  return 1
end

local home = {}

function direct(director)
  local t = CLOCK_START - director.clock
  local scroll = SPEED * t

  local lines = find({ has = { "code_line" } })
  for i = 1, #lines do
    local l = lines[i]
    local y = 8.5 + l.code_line * LINE - scroll
    local edge = math.min(y - PANEL_TOP, PANEL_BOTTOM - LINE - y)
    if edge < 0 then
      l.position = HIDDEN
    else
      local fade = math.floor(math.min(edge / LINE, 1) * FADE_STEPS + 0.5)
      l.shade = l.tone * 4 + fade
      l.position = { x = 3.4, y = y }
    end
  end

  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = home[p.part]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[p.part] = h
    end
    if p.part == "logo" or p.part == "logo_card" then
      if t < T.rust then
        p.position = HIDDEN
      else
        local s = pop_scale(t - T.rust)
        local w, hh = h.w * s, h.hgt * s
        p.size = { x = w, y = hh }
        p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
      end
    elseif p.part == "caption" then
      if t >= T.lang then
        p.caption = CAPTION
      end
    end
  end
end
