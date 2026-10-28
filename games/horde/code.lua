-- Герой бегает по земле и по балкону вышки. Стены, опоры, мебель и пол вышки движок берёт из её объёма
-- (модель с solid), поэтому здесь только бег и подъём по лестнице. Места лестницы — в клетках от середины
-- места вышки на земле, сняты со скрипта объёма art/horde/3d/tower/build_tower.py.
local SCENE_WIDTH, SCENE_HEIGHT = 53, 36

-- Бег: скорость в клетках в секунду.
local RUN_SPEED = 4.5

-- Лестница: где встать внизу, чтобы подняться, и куда герой встаёт наверху, на высоте настила.
local LADDER_FOOT = { x = 4.81, y = 3.25 }
local LADDER_TOP = { x = 2.90, y = 1.34 }
local DECK_TOP = 4.5
local REACH = 0.9

local function feet(hero)
  return hero.position.x + hero.size.x / 2, hero.position.y + hero.size.y / 2
end

local function put_feet(hero, x, y, z)
  hero.position = { x = x - hero.size.x / 2, y = y - hero.size.y / 2, z = z }
end

local function near(x, y, px, py)
  return math.abs(x - px) < REACH and math.abs(y - py) < REACH
end

local function climb(hero, tower)
  local x, y = feet(hero)
  local cx, cy = tower.position.x + tower.size.x / 2, tower.position.y + tower.size.y / 2
  local foot_x, foot_y = cx + LADDER_FOOT.x, cy + LADDER_FOOT.y
  local top_x, top_y = cx + LADDER_TOP.x, cy + LADDER_TOP.y
  -- Наверху герой — когда стоит выше половины высоты настила.
  if hero.position.z > DECK_TOP / 2 then
    if near(x, y, top_x, top_y) then
      put_feet(hero, foot_x, foot_y, 0)
    end
  elseif near(x, y, foot_x, foot_y) then
    put_feet(hero, top_x, top_y, DECK_TOP)
  end
end

-- Скорость по зажатым клавишам: две клавиши сразу ведут по диагонали с той же скоростью.
function steer(hero)
  local dx = (hero.go_right and 1 or 0) - (hero.go_left and 1 or 0)
  local dy = (hero.go_down and 1 or 0) - (hero.go_up and 1 or 0)
  local length = math.sqrt(dx * dx + dy * dy)
  if length > 0 then
    hero.velocity = { x = dx / length * RUN_SPEED, y = dy / length * RUN_SPEED }
  else
    hero.velocity = { x = 0, y = 0 }
  end
end

function keep_in_scene(hero)
  local tower = find{ has = { "tower" } }[1]
  -- Пока E зажата, нажатие действует один раз: иначе герой прыгал бы вверх и вниз каждый шаг.
  if hero.climb and not hero.climbed then
    climb(hero, tower)
  end
  hero.climbed = hero.climb
  hero.position.x = math.max(0, math.min(hero.position.x, SCENE_WIDTH - hero.size.x))
  hero.position.y = math.max(0, math.min(hero.position.y, SCENE_HEIGHT - hero.size.y))
end
