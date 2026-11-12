-- Папки игр и движок для второго ролика: слева папки змейки, арканоида и тетриса с их текстовыми
-- файлами, справа блок движка; от папок к движку бегут листки файлов, а внутри движка пусто — про
-- сами игры в нём нет ни строчки. На «сказочная» три папки поднимаются, снизу появляется папка
-- сказочной игры, на «файлов» листки в ней множатся всё быстрее. Секунды — моменты слов закадрового
-- голоса; пока голос не записан, они расчётные (130 слов в минуту), после записи сюда ставятся
-- настоящие, и сцена записывается заново.
local T = {
  collected = 0.685,
  engine = 2.905,
  fairy = 5.225,
  build = 6.225,
  files = 7.805,
  more = 9.125,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

-- Папки трёх игр выскакивают в начале по очереди, их файлы — на «собраны» по одному.
local FOLDER_AT = { 0.1, 0.25, 0.4, T.fairy + 0.25 }
local ROW_FILES_GAP = 0.25
local FILE_GAP = 0.08

-- На «сказочная» три верхние строки поднимаются, освобождая место четвёртой.
local ROW_RISE, ROW_RISE_TIME = 3.5, 0.5

-- Стрелки к движку вырастают после его появления, по ним бегут два листка за секунду.
local ARROW_AT = { T.engine + 0.3, T.engine + 0.42, T.engine + 0.54, T.build }
local ARROW_GROW = 0.35
local ARROW_FROM, ARROW_LENGTH = 36, 8.2
local PULSE_RUN = 7.9
local PULSE_TIME = 1.0

-- Листки сказочной игры: первые появляются по одному, дальше всё чаще — к «больше» папка полна.
local FAIRY_SHEETS = 69
local FAIRY_SPREAD = T.more - T.files + 0.4

local POP = 0.18
local LABEL_RISE = 0.4

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
  if u < 0.55 then return lerp(0.5, 1.08, u / 0.55) end
  return lerp(1.08, 1, (u - 0.55) / 0.45)
end

local function row_shift(row, t)
  if row == 4 then return 0 end
  return -ROW_RISE * ease_in_out(clamp01((t - T.fairy) / ROW_RISE_TIME))
end

local function appear_at(o)
  local part = o.part
  if part == "engine_border" or part == "engine_body" or part == "engine_title" then
    return T.engine
  end
  if part == "tab" or part == "body" or part == "flap" or part == "name" then
    return FOLDER_AT[o.row]
  end
  if o.row == 4 then
    return T.files + FAIRY_SPREAD * math.sqrt((o.idx - 1) / FAIRY_SHEETS)
  end
  return T.collected + (o.row - 1) * ROW_FILES_GAP + (o.idx - 1) * FILE_GAP
end

local home = {}

local function home_of(o)
  local h = home[o.uid]
  if h == nil then
    h = { x = o.position.x, y = o.position.y, w = o.size.x, hgt = o.size.y }
    home[o.uid] = h
  end
  return h
end

-- Предмет выскакивает из своей середины; надпись проступает чуть снизу вверх.
local function show(o, h, dy, since)
  if o.folder_name ~= nil or o.file_name ~= nil or o.title ~= nil then
    o.position = { x = h.x, y = h.y + dy + LABEL_RISE * (1 - clamp01(since / POP)) }
    return
  end
  local s = pop_scale(clamp01(since / POP))
  local w, hh = h.w * s, h.hgt * s
  o.size = { x = w, y = hh }
  o.position = { x = h.x + (h.w - w) / 2, y = h.y + dy + (h.hgt - hh) / 2 }
end

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local o = parts[i]
    local h = home_of(o)
    local dy = o.row and row_shift(o.row, t) or 0
    if o.part == "line" then
      local u = clamp01((t - ARROW_AT[o.row]) / ARROW_GROW)
      if u <= 0 then
        o.position = HIDDEN
      else
        o.size = { x = ARROW_LENGTH * ease_in_out(u), y = h.hgt }
        o.position = { x = h.x, y = h.y + dy }
      end
    elseif o.part == "head" then
      local at = ARROW_AT[o.row] + ARROW_GROW
      if t < at then
        o.position = HIDDEN
      else
        show(o, h, dy, t - at)
      end
    elseif o.part == "pulse" then
      local at = ARROW_AT[o.row] + ARROW_GROW
      if t < at then
        o.position = HIDDEN
      else
        local u = (t - at) / PULSE_TIME + (o.idx - 1) / 2
        u = u - math.floor(u)
        o.position = { x = ARROW_FROM + PULSE_RUN * u - h.w / 2, y = h.y + dy }
      end
    else
      local at = appear_at(o)
      if t < at then
        o.position = HIDDEN
      else
        show(o, h, dy, t - at)
      end
    end
  end
end
