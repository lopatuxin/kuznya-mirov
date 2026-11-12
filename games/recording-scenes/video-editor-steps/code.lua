-- Лесенка редактора для второго ролика: четыре ступеньки поднимаются слева направо — «показывать»,
-- «выбирать», «двигать и править», «запускать»; над каждой окошко. На «небольшими» ступеньки
-- выскакивают по одной с пустыми окошками; на своё слово ступенька загорается жёлтым, и в её окошке
-- идёт кусок записи редактора; пройденная ступенька остаётся белой. Куски нарезаны из записи для
-- сцены «Что нужно в редакторе» и стоят на первом кадре до слова своей ступеньки — видео в игре
-- идут с её начала, поэтому задержка вшита в сам кусок. Секунды — моменты слов закадрового голоса;
-- пока голос не записан, они расчётные (130 слов в минуту), после записи сюда ставятся настоящие,
-- куски нарезаются заново с новыми задержками, и сцена записывается заново.
local T = {
  pieces = 1.61,
}
local LIGHT = { 4.25, 6.43, 8.29, 10.29 }

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

local STEP_GAP = 0.18
local WAITING, CURRENT, DONE = 0, 1, 2
local EDGE_COLOR = { [WAITING] = "#4a4f5a", [CURRENT] = "#ffd34d", [DONE] = "#e9ecf2" }

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.3, 1.1, u / 0.12) end
  if u < 0.22 then return lerp(1.1, 1, (u - 0.12) / 0.1) end
  return 1
end

local function state_of(step, t)
  local next_light = LIGHT[step + 1]
  if next_light ~= nil and t >= next_light then return DONE end
  if t >= LIGHT[step] then return CURRENT end
  return WAITING
end

local home = {}

local function place(p, h, s)
  local w, hh = h.w * s, h.hgt * s
  p.size = { x = w, y = hh }
  p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
end

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local key = p.part .. p.step
    local h = home[key]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[key] = h
    end
    local appear = T.pieces + (p.step - 1) * STEP_GAP
    local state = state_of(p.step, t)
    if t < appear then
      p.position = HIDDEN
    elseif p.part == "clip" then
      if state == WAITING then
        p.position = HIDDEN
      else
        place(p, h, pop_scale(t - LIGHT[p.step]))
      end
    elseif p.part == "text" then
      p.position = { x = h.x, y = h.y }
      p.tone = state
    else
      place(p, h, pop_scale(t - appear))
      if p.part == "edge" then
        p.color = EDGE_COLOR[state]
      end
    end
  end
end
