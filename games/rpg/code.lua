-- Кадр героя: сторона по большему сдвигу за шаг, фаза шага по кругу 1, 0, 2, 0, каждая держится
-- 9 шагов (0.15 c). Прошлое положение и счётчик живут в таблице, заведённой при загрузке
-- («Код игры» → «Память кода»): герой на сцене один.
local ANIMATION_PHASES = { 1, 0, 2, 0 }
local STEPS_PER_PHASE = 9

local state = { last_x = nil, last_y = nil, last_side = 0, walk_frames = 0 }

function animate_hero(hero)
  local dx, dy = 0, 0
  local moved = false
  if state.last_x ~= nil then
    dx = hero.position.x - state.last_x
    dy = hero.position.y - state.last_y
    moved = dx ~= 0 or dy ~= 0
  end

  local phase
  if moved then
    if math.abs(dx) >= math.abs(dy) then
      state.last_side = dx >= 0 and 2 or 1
    else
      state.last_side = dy >= 0 and 0 or 3
    end
    phase = ANIMATION_PHASES[math.floor(state.walk_frames / STEPS_PER_PHASE) % 4 + 1]
    state.walk_frames = state.walk_frames + 1
  else
    state.walk_frames = 0
    phase = 0
  end

  hero.hero_frame = 3 * state.last_side + phase

  state.last_x = hero.position.x
  state.last_y = hero.position.y
end
