-- Три игры каркаса для второго ролика: змейка, арканоид и тетрис идут рядом и играют сами; над
-- каждой на её слово появляется подпись. Змейка идёт к еде в обход своего тела, ракетка арканоида
-- ходит под мяч, тетрис кладёт каждую фигуру туда, где ряды лучше всего собираются. Секунды — моменты
-- слов закадрового голоса; пока голос не записан, они расчётные (130 слов в минуту), после записи
-- сюда ставятся настоящие, и сцена записывается заново.
local T = {
  snake = 2.53,
  arkanoid = 3.25,
  tetris = 3.99,
}
local CAPTION_AT = { T.snake, T.arkanoid, T.tetris }

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

local function lerp(a, b, u)
  return a + (b - a) * u
end

-- Змейка ---------------------------------------------------------------------------------------

-- Поле змейки: левый верхний угол и размер вместе со стенами.
local SNAKE_X, SNAKE_TOP, SNAKE_SIZE = 4, 13, 24
-- Тело змейки — клетки поля, которые прошла голова, от шеи к хвосту; длина растёт с каждой едой.
local START_LENGTH = 5
local body = {}
local last_head

local function key(x, y)
  return x * 100 + y
end

local function cell_of(obj)
  return math.floor(obj.position.x - SNAKE_X + 0.5), math.floor(obj.position.y - SNAKE_TOP + 0.5)
end

local function blocked_cells()
  local blocked = {}
  for i = 1, #body do
    blocked[key(body[i][1], body[i][2])] = true
  end
  return blocked
end

local function inside(x, y)
  return x >= 1 and x <= SNAKE_SIZE - 2 and y >= 1 and y <= SNAKE_SIZE - 2
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
function snake_autopilot(head)
  local food = find({ has = { "food" } })[1]
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
  local cells = (SNAKE_SIZE - 2) * (SNAKE_SIZE - 2)
  for k = 0, cells - 1 do
    local index = (head.score * 137 + k * 53) % cells
    local x, y = 1 + index % (SNAKE_SIZE - 2), 1 + math.floor(index / (SNAKE_SIZE - 2))
    if not blocked[key(x, y)] then
      food.position = { x = SNAKE_X + x, y = SNAKE_TOP + y }
      return
    end
  end
end

-- Голова перешла в новую клетку — прежняя клетка становится шеей, хвост укорачивается до длины.
local function update_snake()
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
      seg.position = { x = SNAKE_X + cell[1], y = SNAKE_TOP + cell[2] }
    else
      seg.position = HIDDEN
    end
  end
end

-- Арканоид -------------------------------------------------------------------------------------

-- Внутренние края стенок поля арканоида; ракетка ходит под мяч со сдвигом, который меняется с каждым
-- разбитым кирпичом, чтобы мяч уходил под разными углами.
local FIELD_LEFT, FIELD_RIGHT = 35, 57
local PADDLE_SPEED = 0.5
local AIM = { -1.2, 0.4, 1.1, -0.5 }

-- Отскок мяча от ракетки: угол зависит только от места удара — как в арканоиде.
function paddle_bounce(ball, paddle)
  if ball.velocity.y < 0 then
    return
  end
  local ball_mid = ball.position.x + ball.size.x / 2
  local paddle_mid = paddle.position.x + paddle.size.x / 2
  local offset = (ball_mid - paddle_mid) / (paddle.size.x / 2)
  if offset > 1 then
    offset = 1
  elseif offset < -1 then
    offset = -1
  end
  local angle = offset * (60 * math.pi / 180)
  local speed = math.sqrt(ball.velocity.x * ball.velocity.x + ball.velocity.y * ball.velocity.y)
  ball.velocity.x = speed * math.sin(angle)
  ball.velocity.y = -speed * math.cos(angle)
  ball.position.y = paddle.position.y - ball.size.y
end

-- Прочный кирпич разбивается с третьего удара — как в арканоиде, только без звука.
function strong_brick_hit(ball, brick)
  brick.hits = brick.hits + 1
  if brick.hits >= 3 then
    local paddles = find({ has = { "broken" } })
    for i = 1, #paddles do
      paddles[i].broken = paddles[i].broken + 10
    end
    delete(brick)
  end
end

function paddle_autopilot(paddle)
  local ball = find({ has = { "ball" } })[1]
  local aim = AIM[math.floor(paddle.broken / 10) % #AIM + 1]
  local target = ball.position.x + ball.size.x / 2 - paddle.size.x / 2 + aim
  local x = paddle.position.x
  local step = target - x
  if step > PADDLE_SPEED then step = PADDLE_SPEED end
  if step < -PADDLE_SPEED then step = -PADDLE_SPEED end
  x = x + step
  if x < FIELD_LEFT then x = FIELD_LEFT end
  if x > FIELD_RIGHT - paddle.size.x then x = FIELD_RIGHT - paddle.size.x end
  paddle.position.x = x
end

-- Тетрис ---------------------------------------------------------------------------------------

-- Стакан: мировая клетка столбца 0 и ряда 0 (верхнего), размер в клетках.
local WELL_X, WELL_TOP = 65, 16
local COLS, ROWS = 10, 20

-- Фигуры — клетки каждого положения (столбец, ряд) внутри коробки 4×4, по часовой стрелке.
local SHAPES = {
  i = { { { 0, 1 }, { 1, 1 }, { 2, 1 }, { 3, 1 } }, { { 2, 0 }, { 2, 1 }, { 2, 2 }, { 2, 3 } } },
  o = { { { 1, 0 }, { 2, 0 }, { 1, 1 }, { 2, 1 } } },
  t = { { { 1, 0 }, { 0, 1 }, { 1, 1 }, { 2, 1 } }, { { 1, 0 }, { 1, 1 }, { 2, 1 }, { 1, 2 } },
        { { 0, 1 }, { 1, 1 }, { 2, 1 }, { 1, 2 } }, { { 1, 0 }, { 0, 1 }, { 1, 1 }, { 1, 2 } } },
  s = { { { 1, 0 }, { 2, 0 }, { 0, 1 }, { 1, 1 } }, { { 1, 0 }, { 1, 1 }, { 2, 1 }, { 2, 2 } } },
  z = { { { 0, 0 }, { 1, 0 }, { 1, 1 }, { 2, 1 } }, { { 2, 0 }, { 1, 1 }, { 2, 1 }, { 1, 2 } } },
  j = { { { 0, 0 }, { 0, 1 }, { 1, 1 }, { 2, 1 } }, { { 1, 0 }, { 2, 0 }, { 1, 1 }, { 1, 2 } },
        { { 0, 1 }, { 1, 1 }, { 2, 1 }, { 2, 2 } }, { { 1, 0 }, { 1, 1 }, { 0, 2 }, { 1, 2 } } },
  l = { { { 2, 0 }, { 0, 1 }, { 1, 1 }, { 2, 1 } }, { { 1, 0 }, { 1, 1 }, { 1, 2 }, { 2, 2 } },
        { { 0, 1 }, { 1, 1 }, { 2, 1 }, { 0, 2 } }, { { 0, 0 }, { 1, 0 }, { 1, 1 }, { 1, 2 } } },
}
local QUEUE = { "i", "t", "o", "l", "s", "j", "z", "t", "i", "o", "l", "j" }

-- Партия идёт с середины: внизу стакана уже лежат почти полные ряды; буква — цвет кубика.
local PREFILL = {
  [15] = "l.........",
  [16] = "ll..oo.s..",
  [17] = "ltjjoossz.",
  [18] = "ttjzzssiz.",
  [19] = "ljjzzoosi.",
}

-- Темп: появление первой фигуры, ход поворотом или сдвигом, падение на ряд, пауза перед следующей
-- фигурой, ступень сгорания ряда — пять ступеней от середины к краям, как в тетрисе NES.
local FIRST_PIECE = 0.3
local MOVE_STEP = 0.06
local DROP_STEP = 0.045
local SPAWN_PAUSE = 0.2
local CLEAR_STEP = 0.07
local CLEAR_COLUMNS = { { 4, 5 }, { 3, 6 }, { 2, 7 }, { 1, 8 }, { 0, 9 } }
local SPAWN_COL = 3

-- Веса оценки положения (Yiyuan Lee, «Tetris AI — The (Near) Perfect Bot»): высота стопки, собранные
-- ряды, дыры, неровность верха.
local W_HEIGHT, W_LINES, W_HOLES, W_BUMPS = -0.510066, 0.760666, -0.35663, -0.184483

local grid = {}
local free_cubes = {}
local piece = nil
local queue_at = 0
local state = "wait"
local state_at = FIRST_PIECE
local clear_rows = {}
local clear_step = 0
local tetris_ready = false

local function cell_world(col, row)
  return { x = WELL_X + col, y = WELL_TOP + row }
end

local function take_cube(kind)
  local c = table.remove(free_cubes)
  c.image = "cube_" .. kind
  return c
end

local function fits(kind, rot, col, row)
  local cells = SHAPES[kind][rot]
  for i = 1, 4 do
    local c, r = col + cells[i][1], row + cells[i][2]
    if c < 0 or c >= COLS or r >= ROWS then return false end
    if r >= 0 and grid[r][c] then return false end
  end
  return true
end

-- Оценка: фигура падает в столбец отвесно; считается стакан после сгоревших рядов.
local function evaluate(kind, rot, col)
  if not fits(kind, rot, col, 0) then return nil end
  local row = 0
  while fits(kind, rot, col, row + 1) do row = row + 1 end
  local filled = {}
  for r = 0, ROWS - 1 do
    filled[r] = {}
    for c = 0, COLS - 1 do filled[r][c] = grid[r][c] ~= nil end
  end
  local cells = SHAPES[kind][rot]
  for i = 1, 4 do filled[row + cells[i][2]][col + cells[i][1]] = true end
  local kept, lines = {}, 0
  for r = 0, ROWS - 1 do
    local full = true
    for c = 0, COLS - 1 do
      if not filled[r][c] then full = false end
    end
    if full then lines = lines + 1 else kept[#kept + 1] = filled[r] end
  end
  local heights, holes = {}, 0
  for c = 0, COLS - 1 do
    heights[c] = 0
    local roof = false
    for i = 1, #kept do
      if kept[i][c] then
        if not roof then heights[c] = #kept - i + 1 end
        roof = true
      elseif roof then
        holes = holes + 1
      end
    end
  end
  local total, bumps = 0, 0
  for c = 0, COLS - 1 do
    total = total + heights[c]
    if c > 0 then bumps = bumps + math.abs(heights[c] - heights[c - 1]) end
  end
  return W_HEIGHT * total + W_LINES * lines + W_HOLES * holes + W_BUMPS * bumps
end

local function spawn_piece()
  queue_at = queue_at % #QUEUE + 1
  local kind = QUEUE[queue_at]
  local best, best_rot, best_col
  for rot = 1, #SHAPES[kind] do
    for col = -2, COLS - 1 do
      local score = evaluate(kind, rot, col)
      if score ~= nil and (best == nil or score > best) then
        best, best_rot, best_col = score, rot, col
      end
    end
  end
  piece = { kind = kind, rot = 1, col = SPAWN_COL, row = 0, target_rot = best_rot, target_col = best_col, cubes = {} }
  for i = 1, 4 do piece.cubes[i] = take_cube(kind) end
end

local function place_piece()
  local cells = SHAPES[piece.kind][piece.rot]
  for i = 1, 4 do
    piece.cubes[i].position = cell_world(piece.col + cells[i][1], piece.row + cells[i][2])
  end
end

local function lock_piece()
  local cells = SHAPES[piece.kind][piece.rot]
  for i = 1, 4 do
    grid[piece.row + cells[i][2]][piece.col + cells[i][1]] = piece.cubes[i]
  end
  piece = nil
  clear_rows = {}
  for r = 0, ROWS - 1 do
    local full = true
    for c = 0, COLS - 1 do
      if grid[r][c] == nil then full = false end
    end
    if full then clear_rows[#clear_rows + 1] = r end
  end
end

-- Сгоревшие ряды убраны — ряды над ними опускаются на столько, сколько сгорело ниже.
local function drop_rows()
  local rows_left = {}
  local cleared = {}
  for i = 1, #clear_rows do cleared[clear_rows[i]] = true end
  for r = ROWS - 1, 0, -1 do
    if not cleared[r] then rows_left[#rows_left + 1] = grid[r] end
  end
  for i = 1, ROWS do
    local r = ROWS - i
    grid[r] = rows_left[i] or {}
    for c = 0, COLS - 1 do
      if grid[r][c] then grid[r][c].position = cell_world(c, r) end
    end
  end
end

local function setup_tetris()
  local cubes = find({ has = { "cube" } })
  for i = 1, #cubes do
    cubes[i].position = HIDDEN
    free_cubes[#free_cubes + 1] = cubes[i]
  end
  for r = 0, ROWS - 1 do grid[r] = {} end
  for r, line in pairs(PREFILL) do
    for c = 0, COLS - 1 do
      local kind = string.sub(line, c + 1, c + 1)
      if kind ~= "." then
        local cube = take_cube(kind)
        cube.position = cell_world(c, r)
        grid[r][c] = cube
      end
    end
  end
end

-- Тетрис за шаг: фигура поворачивается и сдвигается к выбранному месту, падает, ложится; полные ряды
-- гаснут от середины к краям, ряды над ними опускаются, появляется следующая фигура.
local function update_tetris(t)
  if not tetris_ready then
    setup_tetris()
    tetris_ready = true
  end
  if state == "wait" then
    if t >= state_at then
      spawn_piece()
      state, state_at = "move", t + MOVE_STEP
    end
  elseif state == "move" then
    while t >= state_at and state == "move" do
      state_at = state_at + MOVE_STEP
      if piece.rot ~= piece.target_rot then
        local next_rot = piece.rot % #SHAPES[piece.kind] + 1
        if fits(piece.kind, next_rot, piece.col, piece.row) then piece.rot = next_rot end
      elseif piece.col ~= piece.target_col then
        local step = piece.target_col > piece.col and 1 or -1
        if fits(piece.kind, piece.rot, piece.col + step, piece.row) then piece.col = piece.col + step end
      else
        state = "drop"
      end
    end
  end
  if state == "drop" then
    while t >= state_at and state == "drop" do
      state_at = state_at + DROP_STEP
      if fits(piece.kind, piece.rot, piece.col, piece.row + 1) then
        piece.row = piece.row + 1
      else
        place_piece()
        lock_piece()
        if #clear_rows > 0 then
          state, clear_step = "clear", 0
        else
          state = "wait"
          state_at = t + SPAWN_PAUSE
        end
      end
    end
  end
  if state == "clear" then
    while t >= state_at and state == "clear" do
      state_at = state_at + CLEAR_STEP
      clear_step = clear_step + 1
      local pair = CLEAR_COLUMNS[clear_step]
      for i = 1, #clear_rows do
        local row = grid[clear_rows[i]]
        for j = 1, 2 do
          local cube = row[pair[j]]
          cube.position = HIDDEN
          free_cubes[#free_cubes + 1] = cube
          row[pair[j]] = nil
        end
      end
      if clear_step == #CLEAR_COLUMNS then
        drop_rows()
        state = "wait"
        state_at = t + SPAWN_PAUSE
      end
    end
  end
  if piece ~= nil then place_piece() end
end

-- Подписи --------------------------------------------------------------------------------------

-- Подпись выскакивает на своё слово, чуть снизу вверх; горит жёлтым, пока не назовут следующую игру.
local CAPTION_RISE, CAPTION_RISE_TIME = 0.6, 0.15
local caption_home = {}

local function update_captions(t)
  local captions = find({ has = { "caption" } })
  for i = 1, #captions do
    local c = captions[i]
    local home = caption_home[c.slot]
    if home == nil then
      home = { x = c.position.x, y = c.position.y }
      caption_home[c.slot] = home
    end
    local at = CAPTION_AT[c.slot]
    if t < at then
      c.position = HIDDEN
    else
      local u = math.min((t - at) / CAPTION_RISE_TIME, 1)
      c.position = { x = home.x, y = home.y + lerp(CAPTION_RISE, 0, u) }
      local next_at = CAPTION_AT[c.slot + 1]
      c.tone = (next_at ~= nil and t >= next_at) and 0 or 1
    end
  end
end

function direct(director)
  local t = CLOCK_START - director.clock
  update_snake()
  update_tetris(t)
  update_captions(t)
end
