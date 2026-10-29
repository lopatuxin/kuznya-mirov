-- Герой бегает по земле и по балкону вышки. Стены, опоры, мебель и пол вышки движок берёт из её объёма
-- (модель с solid), а по лестнице герой лезет правилом climb по клавише E, поэтому здесь только бег.
local SCENE_WIDTH, SCENE_HEIGHT = 53, 36

-- Бег: скорость в клетках в секунду.
local RUN_SPEED = 4.5

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
  hero.position.x = math.max(0, math.min(hero.position.x, SCENE_WIDTH - hero.size.x))
  hero.position.y = math.max(0, math.min(hero.position.y, SCENE_HEIGHT - hero.size.y))
end
