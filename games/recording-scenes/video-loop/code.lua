-- Круг каркаса для второго ролика: посередине идёт арканоид, ракетка сама ходит за мячом; вокруг поля
-- четыре шага каркаса загораются на свои слова: «читает», «сдвигает», «проверяет», «рисует»; на «много
-- раз в секунду» по стрелкам круга начинает бегать огонёк. Секунды — моменты слов закадрового голоса;
-- пока голос не записан, они расчётные (130 слов в минуту), после записи сюда ставятся настоящие,
-- и сцена записывается заново.
local T = {
  read = 2.28,
  fast = 3.67,
  move = 4.76,
  check = 6.29,
  draw = 8.52,
}
local LIGHT = { T.read, T.move, T.check, T.draw }

local CLOCK_START = 1000
local HIDDEN = { x = -100, y = -100 }

local DIM = "#3a3f48"
local LIT = "#ffd34d"

-- Путь огонька по кругу: середины стрелок, по часовой стрелке от первого шага.
local LOOP = { { x = 10, y = 5.5 }, { x = 54, y = 5.5 }, { x = 54, y = 30.5 }, { x = 10, y = 30.5 } }
local LAP_TIME = 1.0
local TRAIL_GAP = 0.04

-- Ракетка: стенки поля, скорость и сдвиг удара, чтобы мяч уходил под разными углами.
local FIELD_LEFT, FIELD_RIGHT = 21, 43
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
  if ball == nil then
    return
  end
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

-- Точка на круге: доля круга от 0 до 1, по отрезкам между углами.
local function loop_point(u)
  local lengths, total = {}, 0
  for i = 1, #LOOP do
    local a, b = LOOP[i], LOOP[i % #LOOP + 1]
    lengths[i] = math.abs(b.x - a.x) + math.abs(b.y - a.y)
    total = total + lengths[i]
  end
  local d = (u - math.floor(u)) * total
  for i = 1, #LOOP do
    if d <= lengths[i] then
      local a, b = LOOP[i], LOOP[i % #LOOP + 1]
      local k = d / lengths[i]
      return a.x + (b.x - a.x) * k, a.y + (b.y - a.y) * k
    end
    d = d - lengths[i]
  end
  return LOOP[1].x, LOOP[1].y
end

local function pop_scale(u)
  if u < 0.12 then return 0.6 + 0.6 * u / 0.12 end
  if u < 0.22 then return 1.2 - 0.2 * (u - 0.12) / 0.1 end
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

    local runner = tonumber(string.match(p.part, "^runner(%d)$"))
    if runner then
      if t < T.fast then
        p.position = HIDDEN
      else
        local x, y = loop_point((t - T.fast) / LAP_TIME - runner * TRAIL_GAP)
        p.position = { x = x - h.w / 2, y = y - h.hgt / 2 }
      end
    elseif p.step then
      local light = LIGHT[p.step]
      local on = t >= light
      if p.lit ~= nil then
        p.lit = on and 1 or 0
      elseif string.match(p.part, "^border") or string.match(p.part, "^badge") then
        p.color = on and LIT or DIM
        local s = on and pop_scale(t - light) or 1
        local w, hh = h.w * s, h.hgt * s
        p.size = { x = w, y = hh }
        p.position = { x = h.x + (h.w - w) / 2, y = h.y + (h.hgt - hh) / 2 }
      end
    end
  end
end
