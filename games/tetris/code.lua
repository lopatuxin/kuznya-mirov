-- Тетрис как на NES — «Правила игры» выражают форму фигур, сдвиг, поворот и упор; здесь —
-- то, чего правила не выражают (требование 49): поиск полных рядов, анимация гаснущих рядов,
-- опускание верхних рядов, пауза перед появлением по высоте, автоповтор клавиш, мягкое падение,
-- уровень и очки. Каждая партия собирает этот файл заново, так что локальные переменные ниже
-- живут ровно одну партию — как объявленные здесь, playing state, не мировые свойства.

local frame = 0
local known_pivot = nil
local gravity_counter = 0
local das_dir = nil
-- Кадров до следующего сдвига зажатой клавишей: 16 − autorepeatX из NES, который в начале
-- партии равен нулю.
local das_timer = 16
local das_repeat_attempt = false
local shift_blocked = false
local turn_blocked = false
local soft_drop_rows = 0
local down_armed = true
local down_held_prev = false

local clearing = false
local clear_rows = {}
local clear_step = 0
local pending_pause_frames = 10
local pause_timer = 0

local booted = false

-- «Случайность как в NES», требование 39: очередь фигур решает код, а не pick_one сам по
-- себе — семь `spawn` в rules.json гейтуются каждый своим next_<фигура>, выставленным здесь
-- до этапа 7 того же шага.
local KIND_FIELDS = { "next_i", "next_o", "next_t", "next_s", "next_z", "next_j", "next_l" }
local last_kind = nil

local function roll_kind()
  local first = math.random(0, 7)
  if first == 7 or (last_kind and first == last_kind - 1) then
    last_kind = math.random(0, 6) + 1
  else
    last_kind = first + 1
  end
  return last_kind
end

local function queue_next_kind(game)
  for i = 1, #KIND_FIELDS do
    game[KIND_FIELDS[i]] = false
  end
  game[KIND_FIELDS[roll_kind()]] = true
end

-- «Появление», требование 47, и место появления по meatfighter.com/nintendotetrisai (раздел
-- «Spawning Tetriminos», $98BA: Tetrimino X = 5, Y = 0 у всех фигур): опорный кубик любой
-- фигуры встаёт в стакане на клетку (6, 6) — колонка 5 NES плюс стена слева (мировая колонка 1
-- = колонка 0 NES), ряд 0 NES плюс шесть буферных рядов над стаканом (мировой ряд 6 — верхний
-- видимый). rules.json ставит опорный кубик каждой фигуры в окошке в одну и ту же клетку (14,
-- 9) — «where» (13, 8) плюс (1, 1), — так что сдвиг к месту появления общий для всех фигур.
local WINDOW_TO_WELL_DX = 6 - 14
local WINDOW_TO_WELL_DY = 6 - 9
local function spawn_from_window(game)
  local cubes = find({ has = { "in_window" } })
  for i = 1, #cubes do
    local c = cubes[i]
    c.position.x = c.position.x + WINDOW_TO_WELL_DX
    c.position.y = c.position.y + WINDOW_TO_WELL_DY
    c.in_window = false
    c.falling = true
    c.collides = true
    if c.window_pivot then
      c.window_pivot = false
      c.pivot = true
    end
  end
  queue_next_kind(game)
end

-- «Падение», требование 40: кадров на ряд по уровню 0–19; 2 кадра с уровня 19, 1 — с 29-го.
local FALL_FRAMES = {
  48, 43, 38, 33, 28, 23, 18, 13, 8, 6,
  5, 5, 5, 4, 4, 4, 3, 3, 3, 2,
}
local function fall_frames(level)
  if level < 20 then
    return FALL_FRAMES[level + 1]
  elseif level < 29 then
    return 2
  end
  return 1
end

local function delete_cell_at(row, col)
  local landed = find({ has = { "landed" } })
  for i = 1, #landed do
    local p = landed[i].position
    if math.floor(p.y + 0.5) == row and math.floor(p.x + 0.5) == col then
      delete(landed[i])
      return
    end
  end
end

-- «Сжигание», требование 43: полные ряды среди лежащих кубиков, по возрастанию номера ряда.
local function clear_full_rows()
  local landed = find({ has = { "landed" } })
  local counts = {}
  for i = 1, #landed do
    local y = math.floor(landed[i].position.y + 0.5)
    counts[y] = (counts[y] or 0) + 1
  end
  local rows = {}
  for y, count in pairs(counts) do
    if count >= 10 then
      rows[#rows + 1] = y
    end
  end
  table.sort(rows)
  return rows
end

-- Очки за ряды, уровень, опускание верхних рядов — требования 44–45. tetris.wiki, "Scoring":
-- у NES "level multiplier is based on the level after the line clear, not before" — ряды и
-- уровень считаются раньше очков, множитель берёт уже новый уровень, если переход случился на
-- этом же сжигании.
local function finish_clear(game)
  local n = #clear_rows
  local landed = find({ has = { "landed" } })
  for i = 1, #landed do
    local y = landed[i].position.y
    local shift = 0
    for j = 1, #clear_rows do
      if clear_rows[j] > y then
        shift = shift + 1
      end
    end
    if shift > 0 then
      landed[i].position.y = y + shift
    end
  end

  game.rows = game.rows + n
  game.rows_to_level = game.rows_to_level - n
  if game.rows_to_level <= 0 then
    game.level = game.level + 1
    game.mult = game.level + 1
    game.rows_to_level = game.rows_to_level + 10
    play_sound("level")
  end

  local base = { 40, 100, 300, 1200 }
  local points = (base[n] or 0) * (game.level + 1)
  local total = game.score + points
  if total > 999999 then
    total = 999999
  end
  game.score = total
  if n >= 4 then
    play_sound("tetris")
  else
    play_sound("clear")
  end

  local flash_objs = find({ has = { "flash_overlay" } })
  if flash_objs[1] then
    flash_objs[1].flash_on = 1
  end

  clearing = false
  clear_rows = {}
  clear_step = 0
  pause_timer = pending_pause_frames
end

-- Одна ступень анимации сжигания — требование 43: столбцы 4–5, 3–6, 2–7, 1–8, 0–9.
local COLUMN_STEPS = { { 5, 6 }, { 4, 7 }, { 3, 8 }, { 2, 9 }, { 1, 10 } }
local function advance_clear_step(game)
  clear_step = clear_step + 1
  local pair = COLUMN_STEPS[clear_step]
  if pair then
    for i = 1, #clear_rows do
      delete_cell_at(clear_rows[i], pair[1])
      delete_cell_at(clear_rows[i], pair[2])
    end
  end
  if #clear_rows >= 4 then
    local flash_objs = find({ has = { "flash_overlay" } })
    if flash_objs[1] then
      flash_objs[1].flash_on = (clear_step % 2 == 0) and 1 or 0
    end
  end
  if clear_step >= 5 then
    finish_clear(game)
  end
end

-- Нижний видимый ряд стакана — «Свойства сцены»: сцена шире стакана на буферные ряды сверху
-- (требование 47) и полосу счёта справа, стакан сам занимает мировые ряды 6..25.
local WELL_BOTTOM_ROW = 25

-- Пауза перед появлением по высоте — требование 46; высота считается по опорному кубику
-- (tetriminoY, $0041 у meatfighter.com/nintendotetrisai — та же переменная, которой движок
-- ROM меряет положение фигуры при падении и проверке границ), не по самому нижнему кубику
-- фигуры. Двум нижним рядам отвечает depth 0 или 1 (база 10 кадров); дальше каждая новая
-- группа из 4 рядов выше добавляет свои 2 кадра начиная с её первого ряда, поэтому граница
-- группы округляется вверх (math.ceil).
local function appearance_pause(pivot_row)
  local depth = WELL_BOTTOM_ROW - pivot_row
  if depth < 0 then
    depth = 0
  end
  if depth <= 1 then
    return 10
  end
  local groups = math.ceil((depth - 1) / 4)
  local frames = 10 + 2 * groups
  if frames > 18 then
    frames = 18
  end
  return frames
end

-- Фигура легла — требование 46–47: считает лежащие кубики решёнными, запускает сжигание или
-- сразу паузу появления.
function on_landed(pivot)
  play_sound("land")
  -- Имя "current_piece" — только у текущей фигуры: снимается здесь же, пока она ещё falling,
  -- чтобы лежащий кубик не остался под ним для следующих проверок записанного ввода.
  local pivot_row = pivot.position.y
  pivot.name = ""
  local game = find({ has = { "level" } })[1]
  local cubes = find({ has = { "falling" } })
  for i = 1, #cubes do
    cubes[i].solid = true
    cubes[i].landed = true
    -- «Правила игры»: писать false, не nil — у стороннего исполнителя Lua (luars 0.26.3)
    -- `obj.prop = nil` на признаке, которого объект ещё не касался, не вызывает __newindex
    -- вовсе (короткий путь «новый ключ, значение nil — уже готово», ставящий число значения
    -- впереди самой проверки метаметода); false проходит той же дорогой без обхода.
    cubes[i].falling = false
    cubes[i].pivot = false
  end

  if soft_drop_rows > 0 then
    local total = game.score + soft_drop_rows
    if total > 999999 then
      total = 999999
    end
    game.score = total
  end
  soft_drop_rows = 0

  pending_pause_frames = appearance_pause(pivot_row)

  local rows = clear_full_rows()
  if #rows > 0 then
    clearing = true
    clear_rows = rows
    clear_step = 0
  else
    pause_timer = pending_pause_frames
  end
end

-- Отзывы `want_left`/`want_right` о том, отменился ли сдвиг — требование 42 («упор держит
-- автоповтор заряженным») и звук только на удавшемся сдвиге: `on_shift_blocked` — `if_blocked`
-- того же shift, `on_shift_result` — следующее действие того же `do`, так что оба всегда
-- вызываются в паре, в порядке блокировка-потом-результат, на одном и том же попытке.
function on_shift_blocked(pivot)
  shift_blocked = true
end

function on_shift_result(pivot)
  local blocked = shift_blocked
  shift_blocked = false
  if blocked then
    -- tetris.wiki, "Tetris (NES)": "the DAS counter is instantly set to 16 [готова к повтору]
    -- if a tap shift is blocked" — упёршееся нажатие заряжает автоповтор сразу, как упёршийся
    -- повтор (das_timer уже <= 0 у него), а не ждёт обычные 16 кадров до первого повтора.
    das_timer = 0
    return
  end
  play_sound("move")
  if das_repeat_attempt then
    das_timer = 6
  end
end

-- Во время паузы появления и сжигания активного кадра нет — требование 42 и tetris.wiki,
-- "Tetris (NES)": "DAS charging is completely dead during ARE and line clear... any DAS charge
-- left over from the previous piece can be redirected during ARE". Направление сдвига можно
-- перенаправить (das_dir меняется), сам счётчик das_timer не трогается — заряд не теряется.
-- Свежее нажатие в паузе тоже не считается нажатием: в NES `shift_tetrimino` в паузе не
-- вызывается, и «только что нажата» пропадает (meatfighter, «Shifting Tetriminos»). После паузы
-- клавиша считается зажатой: без сдвига сразу, счётчик идёт с того значения, где стоял.
-- Поворот в это время не запоминается: NES реагирует на него только в активном кадре, нажатие
-- в паузе просто пропадает.

-- Какая стрелка ведёт сдвиг: NES проверяет правую первой, так что при обеих зажатых — вправо.
local function held_direction(game)
  if game.right_key then
    return "right"
  end
  if game.left_key then
    return "left"
  end
  return nil
end

-- Кадр без сдвига (пауза или зажатая «вниз»): направление следует за зажатой клавишей, а
-- отпущенная клавиша его снимает, чтобы нажатие в следующем активном кадре было свежим.
local function track_direction(game)
  das_dir = held_direction(game)
end

local function track_input_while_idle(game)
  game.cw_key = false
  game.ccw_key = false
  track_direction(game)
end

-- «Автоповтор» — требование 42: нажатие сдвигает сразу, дальше через 16, потом каждые 6 кадров;
-- удаётся сдвиг или нет, узнаётся уже после (`on_shift_result`), не здесь.
local function handle_das(pivot, dir_name, want_field)
  if das_dir ~= dir_name then
    das_dir = dir_name
    das_timer = 16
    das_repeat_attempt = false
    pivot[want_field] = true
  else
    das_timer = das_timer - 1
    if das_timer <= 0 then
      das_repeat_attempt = true
      pivot[want_field] = true
    end
  end
end

-- Звук поворота — только когда он на самом деле состоялся, как на NES: `on_turn_blocked` —
-- `if_blocked` у want_cw/want_ccw, `on_turn_result` — действие сразу после turn в том же do.
function on_turn_blocked(pivot)
  turn_blocked = true
end

function on_turn_result(pivot)
  if turn_blocked then
    turn_blocked = false
    return
  end
  play_sound("rotate")
end

-- Мягкое падение засчитывает ряд, когда shift опустил фигуру, требование 41. Тот же счётчик
-- прибавляется и когда фигура упёрлась в дно (`on_landed` перед этим, тем же do), но это
-- безвредно: `on_landed` уже обнулил soft_drop_rows и посчитал очки сам, а следующая фигура
-- обнулит его снова раньше, чем он где-то прочитается.
function on_fall_result(pivot)
  local game = find({ has = { "level" } })[1]
  if game.down_key and down_armed then
    soft_drop_rows = soft_drop_rows + 1
  end
end

-- Один шаг партии — требования 40–42, 46: правила уже подвинули/повернули/уронили фигуру
-- прошлым шагом; здесь решается, чего они попробуют сделать на этом.
function tick(game)
  frame = frame + 1
  -- Фронт нажатия «вниз» проверяется каждый кадр, паузы не исключение (meatfighter.com/
  -- nintendotetrisai, «Dropping Tetriminos»): «вниз» вооружает мягкое падение только тогда,
  -- когда она нажата именно на этом кадре, а не держится ещё с прошлого.
  local down_was_held = down_held_prev
  down_held_prev = game.down_key

  if frame == 1 then
    queue_next_kind(game)
  end

  if clearing then
    track_input_while_idle(game)
    if frame % 4 == 0 then
      advance_clear_step(game)
    end
    return
  end

  if pause_timer > 0 then
    track_input_while_idle(game)
    pause_timer = pause_timer - 1
    if pause_timer <= 0 then
      spawn_from_window(game)
    end
    return
  end

  if not booted then
    local window_piece = find({ has = { "in_window" } })
    if #window_piece < 4 then
      return
    end
    spawn_from_window(game)
    booted = true
  end

  local pivots = find({ has = { "pivot" } })
  local pivot = pivots[1]
  if not pivot then
    return
  end

  if not (known_pivot and known_pivot == pivot) then
    known_pivot = pivot
    gravity_counter = 0
    soft_drop_rows = 0
    -- Автоповтор здесь НЕ сбрасывается — требование 42: во время паузы появления и анимации
    -- сжигания счётчик стоит (tick выше выходит раньше, до этого места), а заряд, набранный
    -- зажатой клавишей до того, как прошлая фигура легла, переходит на эту же, как на NES.
    -- «Тесты по записанному вводу»: имя даёт проверкам способ адресовать текущую фигуру по
    -- имени объекта, как остальные проверки уже умеют; прежние фигуры имя не хранят дальше
    -- (снимается в on_landed).
    pivot.name = "current_piece"
    -- meatfighter.com/nintendotetrisai, «Dropping Tetriminos»: autorepeatY сбрасывается новой
    -- фигурой, но взводится только настоящим «только что нажал» — «вниз», уже зажатая с прошлой
    -- фигуры (или с паузы перед этой), новую не роняет, пока игрок её не отпустит и не нажмёт
    -- снова; «вниз», нажатая этим же кадром, роняет фигуру сразу, как обычное нажатие.
    down_armed = not (game.down_key and down_was_held)
  end

  local is_o = pivot.image == "cube_o"
  local is_i = pivot.image == "cube_i"
  local is_sz = pivot.image == "cube_s" or pivot.image == "cube_z"
  if (game.cw_key or game.ccw_key) and not is_o then
    if is_i or is_sz then
      -- I, Z и S поворачиваются только между двумя положениями («Правила игры», требование 38):
      -- направление берётся из фактического pivot.rotation, а не из своего состояния — так
      -- отменённый у стенки поворот не сбивает следующее нажатие в третье положение. Правая
      -- NRS (meatfighter.com/nintendotetrisai, таблица поворотов $8A9C: Ih→Iv поворотом по
      -- часовой, Sh→Sv и Zh→Zv — против) сводит I на положение 0 по часовой, S и Z — против.
      local to_vertical = pivot.rotation == nil or pivot.rotation == 0
      local cw_from_here = is_i and to_vertical or (is_sz and not to_vertical)
      if cw_from_here then
        pivot.want_cw = true
      else
        pivot.want_ccw = true
      end
    elseif game.cw_key then
      pivot.want_cw = true
    else
      pivot.want_ccw = true
    end
  end
  game.cw_key = false
  game.ccw_key = false

  -- Счётчик при отпускании не обнуляется: в NES autorepeatX сбрасывает только новое нажатие
  -- в активном кадре, а нажатие в паузе продолжает с этого значения.
  local dir = held_direction(game)
  if game.down_key then
    -- meatfighter, «Shifting Tetriminos» ($89B2): при зажатой «вниз» сдвига нет, счётчик стоит,
    -- а нажатие стрелки в это время теряется, как в паузе.
    track_direction(game)
  elseif dir == "right" then
    handle_das(pivot, "right", "want_right")
  elseif dir == "left" then
    handle_das(pivot, "left", "want_left")
  else
    das_dir = nil
  end

  local interval = fall_frames(game.level)
  if game.down_key and down_armed and interval > 2 then
    interval = 2
  end
  gravity_counter = gravity_counter + 1
  if gravity_counter >= interval then
    gravity_counter = 0
    pivot.want_fall = true
  end
  if not game.down_key then
    soft_drop_rows = 0
    down_armed = true
  end
end
