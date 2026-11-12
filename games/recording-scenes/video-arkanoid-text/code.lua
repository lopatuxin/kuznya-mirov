-- Описание арканоида для второго ролика: слева идёт копия арканоида, ракетка сама ходит под мяч;
-- справа две панели с текстом его описания — предметы сцены и правила. На свои слова строки
-- загораются золотом, а в игре обводятся предметы, о которых они; на «сталкиваются» мяч бьёт кирпич,
-- на «менять» скорость мяча в тексте переписывается на вдвое большую, и мяч летит вдвое быстрее.
-- Секунды — моменты слов закадрового голоса; пока голос не записан, они расчётные (130 слов в минуту),
-- после записи сюда ставятся настоящие, и сцена записывается заново. Удар по кирпичу к слову код
-- подгоняет сам: на отскоке от ракетки выбирает угол, при котором мяч долетит до кирпича вовремя.
local T = {
  files = 1.845,
  second = 2.625,
  objects = 3.785,
  position = 5.065,
  size = 6.205,
  velocity = 8.065,
  rule = 9.745,
  hit = 11.245,
  change = 12.635,
  edit = 13.625,
}

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }
local DT = 1 / 60

-- Поле арканоида: внутренние края стенок, нижний край кирпичей и высота, на которой мяч касается
-- ракетки. Ракетка ходит под мяч со сдвигом, который меняется с каждым разбитым кирпичом.
local FIELD_LEFT, FIELD_RIGHT, FIELD_TOP = 4, 26, 7
local BRICKS_BOTTOM = 13
local LAND_Y = 27
local PADDLE_SPEED = 0.5
local AIM = { -1.2, 0.4, 1.1, -0.5 }

-- Подгонка удара к слову. Углы отскока от ракетки перебираются через градус. Полёт вверх до кирпича
-- длится от 1,1 до 2,2 с, круг до ракетки и обратно — от 2,2 до 4,3 с; поэтому перед последним
-- отскоком до удара должно оставаться от 1,2 до 1,9 с, а раньше — не меньше 3,5 с: из промежутка
-- между ними к слову не успеть ни одним полётом, ни двумя.
local MAX_ANGLE = 58
local HIT_TOLERANCE = 0.03
local LAST_LEG_FROM, LAST_LEG_TO = 1.2, 1.9
local FAR_ENOUGH = 3.5
local FLY_LIMIT = 6

-- Панели текста: левый край строк, высота строки, верх каждой панели, когда панель появляется.
local TEXT_X = 30.8
local LINE = 1.0
local PANEL_PAD = 0.3
local PANEL_TOP = { 5.6, 20.2 }
local APPEAR = { T.files, T.second }
local LINES_AFTER = 0.12
local LINE_GAP = 0.035

-- Цвета строк — номера в таблице надписи: обычный, две ступени к нижнему краю панели, золото, вспышка.
local NORMAL, FADE1, FADE2, GOLD, FLASH = 0, 1, 2, 3, 4
local FADED = { { [11] = FADE1, [12] = FADE2 }, { [7] = FADE1, [8] = FADE2 } }

-- Какие строки горят на какие слова и какие предметы в игре тогда обведены.
local LIT = {
  { from = T.objects, to = T.position, panel = 1, rows = { 1, 4, 8 } },
  { from = T.position, to = T.size, panel = 1, rows = { 2, 5, 9 } },
  { from = T.size, to = T.velocity, panel = 1, rows = { 3, 6, 10 } },
  { from = T.velocity, to = T.rule, panel = 1, rows = { 7 } },
  { from = T.rule, to = T.change, panel = 2, rows = { 2, 3, 4, 5, 6 } },
  { from = T.change, to = math.huge, panel = 1, rows = { 7 } },
}
local FRAMED = {
  { from = T.objects, to = T.velocity, targets = { paddle = true, ball = true, brick1 = true } },
  { from = T.velocity, to = T.rule, targets = { ball = true } },
}
local BRICK1 = { x = 4, y = 8, w = 2, h = 1 }
local FRAME_THICK = 0.12
local FRAME_MARGIN = 0.25

-- Вспышка вокруг разбитого кирпича и строка «effects», которая вспыхивает вместе с ней.
local FLASH_TIME = 0.4
local EFFECTS_ROW = 5

-- Правка скорости мяча: старое число стирается по знаку, новое печатается; курсор мигает, пока стоит.
local VELOCITY_ROW = 7
local VELOCITY_PREFIX = '    "velocity": ['
local VELOCITY_SUFFIX = '] },'
local OLD_VALUE, NEW_VALUE = "7, -11", "14, -22"
local ERASE_STEP, TYPE_PAUSE, TYPE_STEP = 0.055, 0.12, 0.075
local EDIT_DONE = T.edit + #OLD_VALUE * ERASE_STEP + TYPE_PAUSE + #NEW_VALUE * TYPE_STEP
local CHAR_W = 0.45
local CARET_BLINK = 0.4

local function clamp01(u)
  if u < 0 then return 0 end
  if u > 1 then return 1 end
  return u
end

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function pop_scale(u)
  if u < 0.1 then return lerp(0.85, 1.02, u / 0.1) end
  if u < 0.2 then return lerp(1.02, 1, (u - 0.1) / 0.1) end
  return 1
end

local function now()
  return CLOCK_START - find({ has = { "clock" } })[1].clock
end

local function bricks_now()
  local list = {}
  local bricks = find({ has = { "brick" } })
  for i = 1, #bricks do
    local b = bricks[i]
    list[i] = { x = b.position.x, y = b.position.y, w = b.size.x, h = b.size.y, strong = b.hits ~= nil }
  end
  return list
end

-- Полёт мяча вперёд по шагам движка: стенки и кирпичи отражают его так же, как в движке, — мяч
-- выталкивается вплотную к краю по оси, где выталкивать меньше. Отдаёт время первого удара по
-- кирпичу, прочный ли тот кирпич, и время, когда мяч вернётся к ракетке.
local function fly(x, y, vx, vy, bricks)
  local alive = {}
  for i = 1, #bricks do alive[i] = true end
  local hit, hit_strong
  for n = 1, math.floor(FLY_LIMIT / DT) do
    x = x + vx * DT
    y = y + vy * DT
    if x < FIELD_LEFT then x = FIELD_LEFT; vx = -vx end
    if x > FIELD_RIGHT - 1 then x = FIELD_RIGHT - 1; vx = -vx end
    if y < FIELD_TOP then y = FIELD_TOP; vy = -vy end
    if y < BRICKS_BOTTOM then
      for i = 1, #bricks do
        local b = bricks[i]
        if alive[i] and x < b.x + b.w and x + 1 > b.x and y < b.y + b.h and y + 1 > b.y then
          if hit == nil then
            hit, hit_strong = n * DT, b.strong
          end
          local fx = b.x + b.w
          if x + 0.5 < b.x + b.w / 2 then fx = b.x - 1 end
          local fy = b.y + b.h
          if y + 0.5 < b.y + b.h / 2 then fy = b.y - 1 end
          if math.abs(fx - x) <= math.abs(fy - y) then
            x, vx = fx, -vx
          else
            y, vy = fy, -vy
          end
          if not b.strong then alive[i] = false end
        end
      end
    end
    if vy > 0 and y > LAND_Y then
      return hit, hit_strong, n * DT
    end
  end
  return hit, hit_strong, nil
end

-- Угол отскока, при котором мяч разобьёт кирпич ровно на слове, а если до слова больше одного полёта —
-- угол ближе всего к обычному, после которого до слова останется время, откуда к нему можно успеть.
local function plan(x, speed, natural, left)
  local bricks = bricks_now()
  local final, final_err = nil, HIT_TOLERANCE
  local pass, pass_cost = nil, math.huge
  for deg = -MAX_ANGLE, MAX_ANGLE do
    local a = deg * math.pi / 180
    local hit, strong, back = fly(x, LAND_Y, speed * math.sin(a), -speed * math.cos(a), bricks)
    if hit ~= nil and not strong and math.abs(hit - left) <= final_err then
      final, final_err = a, math.abs(hit - left)
    end
    if back ~= nil then
      local rest = left - back
      if (rest >= LAST_LEG_FROM and rest <= LAST_LEG_TO) or rest >= FAR_ENOUGH then
        local cost = math.abs(a - natural)
        if cost < pass_cost then
          pass, pass_cost = a, cost
        end
      end
    end
  end
  if final ~= nil then return final, true end
  if pass ~= nil then return pass, false end
  return natural, false
end

local hit_planned = false

-- Отскок мяча от ракетки: угол зависит от места удара — как в арканоиде, — пока код не подгоняет
-- удар по кирпичу к слову.
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
  local t = now()
  if not hit_planned and t < T.hit then
    angle, hit_planned = plan(ball.position.x, speed, angle, T.hit - t)
  end
  ball.velocity.x = speed * math.sin(angle)
  ball.velocity.y = -speed * math.cos(angle)
  ball.position.y = paddle.position.y - ball.size.y
end

-- Прочный кирпич разбивается с третьего удара — как в арканоиде, только без звука.
function strong_brick_hit(ball, brick)
  brick.hits = brick.hits + 1
  if brick.hits >= 3 then
    local scorers = find({ has = { "score" } })
    for i = 1, #scorers do
      scorers[i].score = scorers[i].score + 10
    end
    delete(brick)
  end
end

-- Ракетка сама идёт под мяч с небольшим сдвигом, который меняется с каждым разбитым кирпичом.
function autopilot(paddle)
  local ball = find({ has = { "ball" } })[1]
  local aim = AIM[math.floor(paddle.score / 10) % #AIM + 1]
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

local function is_lit(panel, row, t)
  for i = 1, #LIT do
    local l = LIT[i]
    if l.panel == panel and t >= l.from and t < l.to then
      for j = 1, #l.rows do
        if l.rows[j] == row then return true end
      end
    end
  end
  return false
end

local function is_framed(target, t)
  for i = 1, #FRAMED do
    local f = FRAMED[i]
    if t >= f.from and t < f.to and f.targets[target] then return true end
  end
  return false
end

local function frame_side(side, x, y, w, h, m)
  if side == 1 then return x - m, y - m, w + 2 * m, FRAME_THICK end
  if side == 2 then return x - m, y + h + m - FRAME_THICK, w + 2 * m, FRAME_THICK end
  if side == 3 then return x - m, y - m, FRAME_THICK, h + 2 * m end
  return x + w + m - FRAME_THICK, y - m, FRAME_THICK, h + 2 * m
end

local function target_rect(target)
  if target == "brick1" then
    return BRICK1.x, BRICK1.y, BRICK1.w, BRICK1.h
  end
  local o = find({ has = { target } })[1]
  return o.position.x, o.position.y, o.size.x, o.size.y
end

local function velocity_value(t)
  if t < T.edit then return OLD_VALUE end
  local u = t - T.edit
  local erased = math.floor(u / ERASE_STEP)
  if erased < #OLD_VALUE then
    return string.sub(OLD_VALUE, 1, #OLD_VALUE - erased)
  end
  u = u - #OLD_VALUE * ERASE_STEP - TYPE_PAUSE
  if u < 0 then return "" end
  return string.sub(NEW_VALUE, 1, math.min(math.floor(u / TYPE_STEP) + 1, #NEW_VALUE))
end

local function row_y(panel, row)
  return PANEL_TOP[panel] + PANEL_PAD + row * LINE
end

local home = {}
local framed_since = {}
local bricks_seen = nil
local flash = nil
local sped_up = false

-- Разбитый кирпич после начала слова — тот, по которому бьёт мяч: вокруг него вспыхивает рамка.
local function watch_bricks(t)
  local seen = {}
  local bricks = bricks_now()
  for i = 1, #bricks do
    local b = bricks[i]
    seen[b.x .. "," .. b.y] = b
  end
  if bricks_seen ~= nil and flash == nil and t >= T.hit - 0.2 then
    for key, b in pairs(bricks_seen) do
      if seen[key] == nil then
        flash = { x = b.x, y = b.y, w = b.w, h = b.h, at = t }
      end
    end
  end
  bricks_seen = seen
end

function direct(director)
  local t = CLOCK_START - director.clock
  watch_bricks(t)

  if not sped_up and t >= EDIT_DONE then
    sped_up = true
    local ball = find({ has = { "ball" } })[1]
    ball.velocity = { x = ball.velocity.x * 2, y = ball.velocity.y * 2 }
  end

  local flashing = flash ~= nil and t - flash.at < FLASH_TIME
  local value = velocity_value(t)

  local lines = find({ has = { "code" } })
  for i = 1, #lines do
    local l = lines[i]
    if t < APPEAR[l.panel] + LINES_AFTER + l.row * LINE_GAP then
      l.position = HIDDEN
    else
      l.position = { x = TEXT_X, y = row_y(l.panel, l.row) }
      if is_lit(l.panel, l.row, t) then
        if flashing and l.panel == 2 and l.row == EFFECTS_ROW then
          l.shade = FLASH
        else
          l.shade = GOLD
        end
      else
        l.shade = FADED[l.panel][l.row] or NORMAL
      end
      if l.panel == 1 and l.row == VELOCITY_ROW then
        l.code = VELOCITY_PREFIX .. value .. VELOCITY_SUFFIX
      end
    end
  end

  local parts = find({ has = { "part" } })
  for i = 1, #parts do
    local p = parts[i]
    if p.part == "bar" then
      if is_lit(p.panel, p.row, t) then
        p.position = { x = 30.3, y = row_y(p.panel, p.row) }
      else
        p.position = HIDDEN
      end
    elseif p.part == "frame" then
      if is_framed(p.target, t) then
        framed_since[p.target] = framed_since[p.target] or t
        local m = lerp(0.9, FRAME_MARGIN, clamp01((t - framed_since[p.target]) / 0.18))
        local x, y, w, h = target_rect(p.target)
        local fx, fy, fw, fh = frame_side(p.side, x, y, w, h, m)
        p.position = { x = fx, y = fy }
        p.size = { x = fw, y = fh }
      else
        framed_since[p.target] = nil
        p.position = HIDDEN
      end
    elseif p.part == "flash" then
      if flashing then
        local u = (t - flash.at) / FLASH_TIME
        local fx, fy, fw, fh = frame_side(p.side, flash.x, flash.y, flash.w, flash.h, lerp(0.1, 0.8, u))
        p.position = { x = fx, y = fy }
        p.size = { x = fw, y = fh }
      else
        p.position = HIDDEN
      end
    elseif p.part == "caret" then
      local typing = t >= T.edit and t < EDIT_DONE
      local blink_on = math.floor((t - T.change) / CARET_BLINK) % 2 == 0
      if t >= T.change and (typing or blink_on) then
        local chars = #VELOCITY_PREFIX + #value
        p.position = { x = TEXT_X + chars * CHAR_W, y = row_y(1, VELOCITY_ROW) + 0.1 }
      else
        p.position = HIDDEN
      end
    else
      local panel = tonumber(string.match(p.part, "(%d)$"))
      local h = home[p.part]
      if h == nil then
        h = { x = p.position.x, y = p.position.y, w = p.size.x, hgt = p.size.y }
        home[p.part] = h
      end
      if t < APPEAR[panel] then
        p.position = HIDDEN
      else
        local s = pop_scale(t - APPEAR[panel])
        local w, hh = h.w * s, h.hgt * s
        p.size = { x = w, y = hh }
        p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
      end
    end
  end
end
