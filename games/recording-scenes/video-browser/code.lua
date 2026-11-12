-- Змейка: из браузера в программу — для второго ролика. Змейка сама играет в окне браузера; на
-- «соберётся» окно браузера отъезжает влево, справа появляется окно обычной программы, и игра
-- переезжает в него, не переставая играть. Секунда — момент слова «соберётся» закадрового голоса;
-- пока голос не записан, она расчётная (130 слов в минуту), после записи сюда ставится настоящая,
-- и сцена записывается заново.
local T = {
  exe = 4,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

-- Где левый край окна браузера и поля игры в каждый момент: браузер сначала по центру, потом
-- отъезжает влево; поле едет с ним, потом переезжает в окно программы справа.
local BROWSER_CENTER, BROWSER_LEFT = 18, 2
local FIELD_IN_BROWSER = 2
local FIELD_IN_APP = 36
local FIELD_TOP = 7.6
local SHIFT_TIME = 0.6
local MOVE_FROM = 0.9
local MOVE_TIME = 1.0
local SIZE = 24

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
  if u < 0.12 then return lerp(0.7, 1.04, u / 0.12) end
  if u < 0.22 then return lerp(1.04, 1, (u - 0.12) / 0.1) end
  return 1
end

local function browser_left(t)
  return lerp(BROWSER_CENTER, BROWSER_LEFT, ease_in_out(clamp01((t - T.exe) / SHIFT_TIME)))
end

local function field_left(t)
  local in_browser = browser_left(t) + FIELD_IN_BROWSER
  local u = ease_in_out(clamp01((t - T.exe - MOVE_FROM) / MOVE_TIME))
  return lerp(in_browser, FIELD_IN_APP, u)
end

-- Где поле стоит сейчас: его двигает direct, автопилот и еда считают клетки от него.
local field_x = BROWSER_CENTER + FIELD_IN_BROWSER
-- Тело змейки — клетки поля, которые прошла голова, от шеи к хвосту; длина растёт с каждой едой.
local START_LENGTH = 5
local body = {}
local last_head

local function key(x, y)
  return x * 100 + y
end

local function cell_of(obj)
  return math.floor(obj.position.x - field_x + 0.5), math.floor(obj.position.y - FIELD_TOP + 0.5)
end

-- Занятые клетки поля: тело змейки; стены — край поля, его проверяет inside.
local function blocked_cells()
  local blocked = {}
  for i = 1, #body do
    blocked[key(body[i][1], body[i][2])] = true
  end
  return blocked
end

local function inside(x, y)
  return x >= 1 and x <= SIZE - 2 and y >= 1 and y <= SIZE - 2
end

-- Сколько клеток достижимо из (x, y): змейка не заходит туда, где ей не развернуться.
local function room(x, y, blocked, limit)
  local seen = { [key(x, y)] = true }
  local queue = { { x, y } }
  local count = 0
  local head = 1
  while head <= #queue and count < limit do
    local c = queue[head]
    head = head + 1
    count = count + 1
    local steps = { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } }
    for i = 1, 4 do
      local nx, ny = c[1] + steps[i][1], c[2] + steps[i][2]
      local k = key(nx, ny)
      if inside(nx, ny) and not blocked[k] and not seen[k] then
        seen[k] = true
        queue[#queue + 1] = { nx, ny }
      end
    end
  end
  return count
end

local ROTATION = { [key(1, 0)] = 0, [key(0, 1)] = 90, [key(-1, 0)] = 180, [key(0, -1)] = 270 }

-- Змейка сама идёт к еде: из трёх направлений без разворота берёт ближайшее к еде, где ей хватит места.
function autopilot(head)
  local food = find({ has = { "food" } })[1]
  if food == nil then return end
  local hx, hy = cell_of(head)
  local fx, fy = cell_of(food)
  local blocked = blocked_cells()
  local need = #body + 4
  local vx, vy = head.velocity.x, head.velocity.y
  local best, best_score
  local options = { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } }
  for i = 1, 4 do
    local dx, dy = options[i][1], options[i][2]
    local nx, ny = hx + dx, hy + dy
    if not (dx == -vx and dy == -vy) and inside(nx, ny) and not blocked[key(nx, ny)] then
      local space = room(nx, ny, blocked, need)
      local distance = math.abs(fx - nx) + math.abs(fy - ny)
      local score = distance + (space < need and 1000 - space or 0)
      if best_score == nil or score < best_score then
        best, best_score = options[i], score
      end
    end
  end
  if best then
    head.velocity = { x = best[1], y = best[2] }
    head.rotation = ROTATION[key(best[1], best[2])]
  end
end

-- Съеденная еда переезжает в свободную клетку поля; порядок клеток зависит только от счёта.
function move_food(food, head)
  local blocked = blocked_cells()
  local hx, hy = cell_of(head)
  blocked[key(hx, hy)] = true
  local cells = (SIZE - 2) * (SIZE - 2)
  for k = 0, cells - 1 do
    local index = (head.score * 137 + k * 53) % cells
    local x, y = 1 + index % (SIZE - 2), 1 + math.floor(index / (SIZE - 2))
    if not blocked[key(x, y)] then
      food.position = { x = field_x + x, y = FIELD_TOP + y }
      return
    end
  end
end

local home = {}

function direct(director)
  local t = CLOCK_START - director.clock

  -- Игра едет целиком: поле, стены, голова и еда сдвигаются вслед за окном.
  local target = field_left(t)
  local shift = target - field_x
  local game = find({ has = { "game" } })
  for i = 1, #game do
    local o = game[i]
    o.position = { x = o.position.x + shift, y = o.position.y }
  end
  field_x = target

  -- Голова перешла в новую клетку — прежняя клетка становится шеей, хвост укорачивается до длины.
  local head = find({ has = { "controlled" } })[1]
  local hx, hy = cell_of(head)
  if last_head == nil then
    for k = 1, START_LENGTH do body[k] = { hx - k, hy } end
  elseif last_head[1] ~= hx or last_head[2] ~= hy then
    table.insert(body, 1, last_head)
  end
  last_head = { hx, hy }
  local length = START_LENGTH + head.score
  while #body > length do table.remove(body) end

  local segments = find({ has = { "segment" } })
  for i = 1, #segments do
    local seg = segments[i]
    local cell = body[seg.segment]
    if cell then
      seg.position = { x = field_x + cell[1], y = FIELD_TOP + cell[2] }
    else
      seg.position = HIDDEN
    end
  end

  local bx = browser_left(t)
  local parts = find({ has = { "window" } })
  for i = 1, #parts do
    local p = parts[i]
    local h = home[p.part]
    if h == nil then
      h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
      home[p.part] = h
    end
    if p.window == "browser" then
      p.position = { x = h.x + (bx - BROWSER_CENTER), y = h.y }
    elseif t < T.exe then
      p.position = HIDDEN
    else
      local s = pop_scale(t - T.exe)
      local cx, cy = 48, 19.4
      p.size = { x = h.w * s, y = h.hgt * s }
      p.position = { x = cx + (h.x - cx) * s, y = cy + (h.y - cy) * s }
    end
  end
end
