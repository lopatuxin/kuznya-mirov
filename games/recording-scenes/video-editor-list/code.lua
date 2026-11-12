-- Что нужно в редакторе — для второго ролика: идёт запись настоящего редактора с арканоидом, на
-- «прикинул» она отъезжает влево, справа появляется список из четырёх пустых пунктов. Пока голос
-- говорит о пункте, он горит жёлтым, а в конце его фразы в квадратик встаёт галочка. Поверх записи
-- на свои слова жёлтая рамка обводит положение, размер и скорость мяча, потом кнопки запуска, паузы и
-- шага; на «разобраться» вокруг мяча, вставшего на паузе у кирпича, появляется красный круг.
-- Запись редактора — видео в картинке `editor`, его пишет программа записи роликов по расписанию мыши
-- `ep02\motion\editor\list.json`, по тем же моментам слов. Секунды — моменты слов закадрового голоса;
-- пока голос не записан, они расчётные (130 слов в минуту), после записи сюда и в расписание ставятся
-- настоящие, и обе записи делаются заново.
local T = {
  plan = 0.96,
  open = 3.47,
  whole = 5.76,
  click = 6.96,
  where = 10.07,
  size = 11.2,
  speed = 12.62,
  flies = 13.1,
  drag = 14.16,
  undo = 17.82,
  run = 19.1,
  pause = 21.88,
  steps = 22.46,
  step = 23.38,
  find_out = 24.2,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

-- Запись редактора: 1350 × 990 точек, по 30 точек в клетке; сначала по центру, на «прикинул» отъезжает
-- влево, освобождая место списку.
local PX = 1 / 30
local EDITOR_FROM, EDITOR_TO, EDITOR_TOP = 9.5, 1, 1.5
local SLIDE_TIME = 0.6
local BORDER = 0.15

-- Пункты списка: появляются по одному после «прикинул»; когда пункт горит и когда получает галочку.
local ITEMS_AFTER, ITEM_GAP = 0.3, 0.15
local ACTIVE = { T.open, T.click, T.drag, T.run }
local CHECKED = { T.whole + 0.2, T.flies + 0.2, T.undo + 0.45, T.step + 0.25 }
local WAITING, CURRENT, DONE = 0, 1, 2
local BOX_COLOR = { [WAITING] = "#6b717c", [CURRENT] = "#ffd34d", [DONE] = "#e9ecf2" }

-- Жёлтая рамка поверх записи: где она стоит в точках записи и с какого по какое время.
local HIGHLIGHTS = {
  { from = T.where, to = T.size, x = 1035, y = 194, w = 311, h = 25 },
  { from = T.size, to = T.speed, x = 1035, y = 223, w = 311, h = 25 },
  { from = T.speed, to = T.drag - 0.2, x = 1035, y = 252, w = 311, h = 25 },
  { from = T.run, to = T.run + 0.45, x = 857, y = 6, w = 36, h = 36 },
  { from = T.pause, to = T.pause + 0.55, x = 768, y = 6, w = 34, h = 36 },
  { from = T.steps, to = T.step + 0.25, x = 799, y = 6, w = 34, h = 36 },
}
local FRAME_THICK = 0.1

-- Красный круг вокруг мяча, вставшего на паузе у кирпича: середина и размер в точках записи.
local RING = { x = 874, y = 355, size = 120 }

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
  if u < 0.12 then return lerp(0.3, 1.15, u / 0.12) end
  if u < 0.2 then return lerp(1.15, 1, (u - 0.12) / 0.08) end
  return 1
end

local function editor_x(t)
  return lerp(EDITOR_FROM, EDITOR_TO, ease_in_out(clamp01((t - T.plan) / SLIDE_TIME)))
end

local function item_state(item, t)
  if t >= CHECKED[item] then return DONE end
  if t >= ACTIVE[item] then return CURRENT end
  return WAITING
end

local function frame_side(side, x, y, w, h)
  if side == 1 then return x, y, w, FRAME_THICK end
  if side == 2 then return x, y + h - FRAME_THICK, w, FRAME_THICK end
  if side == 3 then return x, y, FRAME_THICK, h end
  return x + w - FRAME_THICK, y, FRAME_THICK, h
end

local function place(p, h, s)
  local w, hh = h.w * s, h.hgt * s
  p.size = { x = w, y = hh }
  p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
end

local home = {}

local function home_of(p)
  local key = p.part .. (p.item or "") .. (p.side or "")
  local h = home[key]
  if h == nil then
    h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
    home[key] = h
  end
  return h
end

function direct(director)
  local t = CLOCK_START - director.clock
  local ex = editor_x(t)
  local highlight = nil
  for i = 1, #HIGHLIGHTS do
    local hl = HIGHLIGHTS[i]
    if t >= hl.from and t < hl.to then highlight = hl end
  end

  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = home_of(p)
    if p.part == "editor" then
      p.position = { x = ex, y = EDITOR_TOP }
    elseif p.part == "border" then
      p.position = { x = ex - BORDER, y = EDITOR_TOP - BORDER }
    elseif p.part == "frame" then
      if highlight == nil then
        p.position = HIDDEN
      else
        local fx, fy, fw, fh = frame_side(p.side, ex + highlight.x * PX, EDITOR_TOP + highlight.y * PX,
          highlight.w * PX, highlight.h * PX)
        p.position = { x = fx, y = fy }
        p.size = { x = fw, y = fh }
      end
    elseif p.part == "ring" then
      if t < T.find_out then
        p.position = HIDDEN
      else
        local size = RING.size * PX * pop_scale(t - T.find_out)
        p.size = { x = size, y = size }
        p.position = { x = ex + RING.x * PX - size / 2, y = EDITOR_TOP + RING.y * PX - size / 2 }
      end
    else
      local appear = T.plan + ITEMS_AFTER + (p.item - 1) * ITEM_GAP
      local state = item_state(p.item, t)
      if t < appear then
        p.position = HIDDEN
      elseif p.part == "tick" then
        if state == DONE then
          place(p, h, pop_scale(t - CHECKED[p.item]))
        else
          p.position = HIDDEN
        end
      elseif p.part == "text" then
        p.position = { x = h.x, y = h.y }
        p.tone = state
      else
        place(p, h, pop_scale(t - appear))
        if p.part == "box" then
          p.color = BOX_COLOR[state]
        end
      end
    end
  end
end
