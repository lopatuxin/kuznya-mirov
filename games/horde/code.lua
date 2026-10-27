-- Герой ходит по земле и по балкону вышки. Размер сцены и картинки вышки код не видит: эти числа записаны здесь.
local SCENE_WIDTH = 53
local SCENE_HEIGHT = 18
-- Шаг партии движка — всегда 1/60 секунды.
local STEP = 1 / 60

-- Картинка вышки: её размер в клетках из game.json, низ стоит серединой на нижнем крае опор; клеток на точку.
local IMAGE_W, IMAGE_H, CELL = 4.955, 8.5, 0.0085

-- Настил балкона — ромб на картинке: передний угол и шаги к левому и правому углам, в точках картинки.
local DECK_FRONT = { x = 295, y = 535 }
local TO_LEFT = { x = -280, y = -140 }
local TO_RIGHT = { x = 280, y = -110 }
-- Будка занимает настил от этой доли до её дополнения по обеим осям; у перил герой не стоит ближе EDGE.
local HUT, EDGE = 0.16, 0.03
-- Допуск у стены будки: точку, вытолкнутую ровно на стену, не уносит за будку.
local WALL = 0.001

-- Лестница: где встать внизу (от левого верхнего угла опор), чтобы подняться, и где герой оказывается наверху.
local LADDER_FOOT = { x = 3.9, y = 2.6 }
local LADDER_TOP = { a = 0.06, b = 0.48 }
local REACH = 0.9

-- Бег: скорость в клетках в секунду и путь за один круг кадров бега — ноги не скользят по земле.
local RUN_SPEED, STRIDE = 4.5, 3.6
-- Кадров в наборе бега каждой позы.
local RUN_FRAMES = { hero_run_down_right = 20 }

-- Слой героя на передних сторонах балкона — над вышкой, на задних — под ней.
local LAYER_FRONT, LAYER_BACK, LAYER_GROUND = 11, 9, 10

local function feet(hero)
  return hero.position.x + hero.size.x / 2, hero.position.y + hero.size.y
end

local function put_feet(hero, x, y)
  hero.position.x = x - hero.size.x / 2
  hero.position.y = y - hero.size.y
end

local function image_corner(tower)
  return tower.position.x + tower.size.x / 2 - IMAGE_W / 2, tower.position.y + tower.size.y - IMAGE_H
end

local function deck_to_scene(tower, a, b)
  local image_x, image_y = image_corner(tower)
  local px = DECK_FRONT.x + a * TO_LEFT.x + b * TO_RIGHT.x
  local py = DECK_FRONT.y + a * TO_LEFT.y + b * TO_RIGHT.y
  return image_x + px * CELL, image_y + py * CELL
end

local function scene_to_deck(tower, x, y)
  local image_x, image_y = image_corner(tower)
  local px = (x - image_x) / CELL - DECK_FRONT.x
  local py = (y - image_y) / CELL - DECK_FRONT.y
  local det = TO_LEFT.x * TO_RIGHT.y - TO_RIGHT.x * TO_LEFT.y
  return (px * TO_RIGHT.y - py * TO_RIGHT.x) / det, (TO_LEFT.x * py - TO_LEFT.y * px) / det
end

-- Держит точку на кольце настила вокруг будки: из-под будки выталкивает к ближней стене.
local function keep_on_deck(a, b)
  a = math.max(EDGE, math.min(a, 1 - EDGE))
  b = math.max(EDGE, math.min(b, 1 - EDGE))
  if a > HUT and a < 1 - HUT and b > HUT and b < 1 - HUT then
    local to_a_low, to_a_high = a - HUT, 1 - HUT - a
    local to_b_low, to_b_high = b - HUT, 1 - HUT - b
    local least = math.min(to_a_low, to_a_high, to_b_low, to_b_high)
    if least == to_a_low then
      a = HUT
    elseif least == to_a_high then
      a = 1 - HUT
    elseif least == to_b_low then
      b = HUT
    else
      b = 1 - HUT
    end
  end
  return a, b
end

local function push_out_of(hero, box)
  local left = hero.position.x + hero.size.x - box.position.x
  local right = box.position.x + box.size.x - hero.position.x
  local up = hero.position.y + hero.size.y - box.position.y
  local down = box.position.y + box.size.y - hero.position.y
  if left <= 0 or right <= 0 or up <= 0 or down <= 0 then
    return
  end
  local least = math.min(left, right, up, down)
  if least == left then
    hero.position.x = hero.position.x - left
  elseif least == right then
    hero.position.x = hero.position.x + right
  elseif least == up then
    hero.position.y = hero.position.y - up
  else
    hero.position.y = hero.position.y + down
  end
end

local function climb(hero, tower)
  local x, y = feet(hero)
  local foot_x, foot_y = tower.position.x + LADDER_FOOT.x, tower.position.y + LADDER_FOOT.y
  if hero.on_deck then
    local top_x, top_y = deck_to_scene(tower, LADDER_TOP.a, LADDER_TOP.b)
    if math.abs(x - top_x) < REACH and math.abs(y - top_y) < REACH then
      hero.on_deck = false
      put_feet(hero, foot_x, foot_y)
    end
  elseif math.abs(x - foot_x) < REACH and math.abs(y - foot_y) < REACH then
    hero.on_deck = true
    put_feet(hero, deck_to_scene(tower, LADDER_TOP.a, LADDER_TOP.b))
  end
end

-- Скорость по зажатым клавишам: две клавиши сразу ведут по диагонали с той же скоростью.
function steer(hero)
  local dx = (hero.go_right and 1 or 0) - (hero.go_left and 1 or 0)
  local dy = (hero.go_down and 1 or 0) - (hero.go_up and 1 or 0)
  local length = math.sqrt(dx * dx + dy * dy)
  if length > 0 then
    hero.velocity = {x = dx / length * RUN_SPEED, y = dy / length * RUN_SPEED}
  else
    hero.velocity = {x = 0, y = 0}
  end
end

-- Восемь направлений по ходу часовой стрелки от «вправо»; левые — отражения правых.
-- Набор кадров бега есть не у каждой позы: без него герой показывает позу стоя.
local HEADINGS = {
  { pose = "right", flip = false },
  { pose = "down_right", flip = false },
  { pose = "down", flip = false },
  { pose = "down_right", flip = true },
  { pose = "right", flip = true },
  { pose = "up_right", flip = true },
  { pose = "up", flip = false },
  { pose = "up_right", flip = false },
}
local RUN_POSES = { down_right = true }

-- Бежит — кадры бега своего направления по пройденному пути, стоит — поза стоя туда, куда бежал.
local function animate(hero, dt)
  local speed = math.sqrt(hero.velocity.x ^ 2 + hero.velocity.y ^ 2)
  if speed > 0 then
    local turn = math.atan(hero.velocity.y, hero.velocity.x) / (2 * math.pi) * 8
    hero.heading = math.floor(turn + 0.5) % 8
    hero.run_dist = (hero.run_dist or 0) + speed * dt
  else
    hero.run_dist = 0
  end
  local heading = HEADINGS[hero.heading + 1]
  hero.flip_x = heading.flip
  if speed > 0 and RUN_POSES[heading.pose] then
    local run = "hero_run_" .. heading.pose
    hero.image = run
    hero.hero_frame = math.floor(hero.run_dist / STRIDE * RUN_FRAMES[run]) % RUN_FRAMES[run]
  else
    hero.image = "hero_" .. heading.pose
  end
end

function keep_in_scene(hero)
  local tower = find{has = {"tower"}}[1]
  -- Пока E зажата, нажатие действует один раз: иначе герой прыгал бы вверх и вниз каждый шаг.
  if hero.climb and not hero.climbed then
    climb(hero, tower)
  end
  hero.climbed = hero.climb
  if hero.on_deck then
    local a, b = keep_on_deck(scene_to_deck(tower, feet(hero)))
    put_feet(hero, deck_to_scene(tower, a, b))
    -- Перед будкой — передние стороны и боковые углы; у самой стены будки герой тоже перед ней.
    hero.layer = (a <= HUT + WALL or b <= HUT + WALL) and LAYER_FRONT or LAYER_BACK
  else
    hero.position.x = math.max(0, math.min(hero.position.x, SCENE_WIDTH - hero.size.x))
    hero.position.y = math.max(0, math.min(hero.position.y, SCENE_HEIGHT - hero.size.y))
    push_out_of(hero, tower)
    hero.layer = LAYER_GROUND
  end
  animate(hero, STEP)
end
