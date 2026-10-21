-- Первый уровень: кадры Ильи, бой с разбойниками и возврат в начало после падения в ущелье.
-- Здоровья и урона нет: от удара разбойник и Илья только отшатываются.

local DT = 1 / 60
local START_X = 3
local START_Y = 7.1

-- Кадры листа hero.png, по 6 в строке: стойка, бег, прыжок (подъём, верх, падение), затем бой —
-- у каждого движения кадры на земле и следом такие же в прыжке.
local IDLE_FIRST, IDLE_COUNT = 0, 4
local RUN_FIRST, RUN_COUNT = 4, 8
local JUMP_UP, JUMP_TOP, JUMP_DOWN = 12, 13, 14
local CHOP_GROUND, CHOP_AIR = 15, 20 -- замах 2 кадра, удар 2, возврат 1
local DEFENCE_FRAMES = {
  parry = { ground = 25, air = 27 },
  dodge = { ground = 29, air = 31 },
  hurt = { ground = 33, air = 35 },
}

-- Круг дыхания в секундах и путь в клетках за круг бега: ноги не скользят по земле.
local IDLE_CYCLE = 1.2
local STRIDE = 2.8
-- Медленнее этого по ширине — стоит; ближе к верхней точке этой доли скорости отрыва — кадр «верх».
local STILL_SPEED = 0.5
local TOP_SHARE = 0.3
local HERO_RUN_SPEED = 7

-- Удар Ильи: секунды замаха, удара и возврата; место удара — клеток вперёд от середины и выше макушки.
local CHOP_WINDUP, CHOP_STRIKE, CHOP_RECOVER = 0.18, 0.08, 0.22
local CHOP_REACH, CHOP_ABOVE = 1.5, 0.4

-- Защита Ильи: секунды каждой и шансы из ста; остальное — пропущенный удар.
local DEFENCE_TIME = { parry = 0.3, dodge = 0.35, hurt = 0.35 }
local PARRY_CHANCE, DODGE_CHANCE = 0.35, 0.30

-- Отлёт от удара: на сколько клеток и за сколько секунд.
local PUSH_TIME = 0.12
local HURT_PUSH, SWEEP_PUSH, STAGGER_PUSH = 0.33, 0.5, 0.33
-- Остановка обоих в миг попадания и парирования: шагов (0,05 с).
local HOLD_STEPS = 3

-- Разбойник: где замечает Илью и откуда бьёт — по ширине между серединами и по высоте между низами.
local SIGHT, SIGHT_HEIGHT = 7, 2
local ATTACK_RANGE, ATTACK_HEIGHT = 1.2, 1.2
local PATROL_SPEED, CHASE_SPEED, PATROL_REST = 1.5, 3.5, 1
-- Не подходит к другому разбойнику, стоящему между ним и Ильёй, ближе этого.
local ROBBER_GAP = 1
-- Обычный удар: секунды замаха, удара, передышки; место удара — вперёд от середины и выше макушки.
local BLOW = { windup = 0.45, strike = 0.1, rest = 0.7, reach = 1.7, above = 0.6 }
-- Особый удар понизу, каждый третий: место взмаха — вперёд от середины и вверх от низа.
local SWEEP = { windup = 0.75, strike = 0.15, rest = 0.9, reach = 2.1, height = 0.7 }
local SPECIAL_EVERY = 3
-- После передышки или отшатывания — столько секунд до нового замаха.
local BLOW_PAUSE = 0.4
-- Отшатнулся от удара Ильи и после парирования.
local STAGGER_TIME, RECOIL_TIME = 0.35, 0.4

-- Кадры листа robber.png, по 5 в строке.
local R_IDLE_FIRST, R_IDLE_COUNT, R_IDLE_CYCLE = 0, 4, 1.4
local R_WALK_FIRST, R_WALK_COUNT, R_STRIDE = 4, 8, 1.6
local R_BLOW_FIRST, R_SWEEP_FIRST, R_STAGGER_FIRST = 12, 17, 22

-- Отсвет: белая вспышка от удара Ильи и красный ореол особого замаха.
local FLASH_TIME, FLASH_OPACITY = 0.1, 0.6
local AURA_RISE, AURA_OPACITY = 0.2, 0.9

-- Вспышки боя живут столько секунд; кадры по прожитому.
local FX_LIFE = 0.2
local FX_FRAMES = { clash = 4, impact = 3 }

local HERO_DEFAULTS = { act = "", act_time = 0, swing = 0, hold_until = 0, push_time = 0, push_speed = 0 }
local ROBBER_DEFAULTS = {
  mood = "patrol", act_time = 0, robber_frame = 0, walk_dist = 0, idle_time = 0, attacks = 0, cooldown = 0,
  hold_until = 0, flash_time = 0, push_time = 0, push_speed = 0, last_swing = 0, patrol_dir = 1,
}

local function ensure(obj, defaults)
  for key, value in pairs(defaults) do
    if obj[key] == nil then
      obj[key] = value
    end
  end
end

local function center_x(obj)
  return obj.position.x + obj.size.x / 2
end

local function bottom(obj)
  return obj.position.y + obj.size.y
end

local function facing(obj)
  return obj.flip_x and -1 or 1
end

local function face_towards(obj, x)
  local dx = x - center_x(obj)
  if dx > 0 then
    obj.flip_x = false
  elseif dx < 0 then
    obj.flip_x = true
  end
end

-- Наложились ли прямоугольник объекта и место [x0, x1] × [y0, y1]; касание краями — не наложение.
local function touches(obj, x0, x1, y0, y1)
  local p, s = obj.position, obj.size
  return p.x < x1 and x0 < p.x + s.x and p.y < y1 and y0 < p.y + s.y
end

-- Место впереди объекта на `reach` клеток от его середины.
local function ahead(obj, reach)
  local cx = center_x(obj)
  if facing(obj) > 0 then
    return cx, cx + reach
  end
  return cx - reach, cx
end

local function spawn_fx(kind, x, y)
  create({
    name = kind,
    fx = true,
    image = kind,
    fx_frames = FX_FRAMES[kind],
    fx_frame = 0,
    position = { x - 0.1, y - 0.1 },
    size = { 0.2, 0.2 },
    layer = 55,
    lifetime = FX_LIFE,
  })
end

-- Номер шага партии; ведёт его fight_hero — правило Ильи идёт первым среди правил боя.
local step_no = 0

-- Остановка в миг попадания: признак `held` выводит объект из правила бега и прыжка, и движок его не
-- двигает следующие HOLD_STEPS шагов — скорость, прыжок и нажатия остаются как были.
local function hold(obj)
  obj.held = true
  obj.hold_until = step_no + HOLD_STEPS
end

-- Идёт ли остановка; в её последний шаг признак снимается, и со следующего шага движок снова ведёт объект.
local function held(obj)
  if not obj.held then
    return false
  end
  if step_no >= obj.hold_until then
    obj.held = false
  end
  return true
end

local function push(obj, away, distance)
  obj.push_time = PUSH_TIME
  obj.push_speed = away * distance / PUSH_TIME
end

local function the_hero()
  return find({ has = { "hero" } })[1]
end

-- Илья

local function begin_chop(hero, unstoppable)
  hero.act = "chop"
  hero.act_time = 0
  hero.attack_queued = false
  hero.unstoppable = unstoppable
  hero.swing = hero.swing + 1
end

local function act_length(act)
  if act == "chop" then
    return CHOP_WINDUP + CHOP_STRIKE + CHOP_RECOVER
  end
  return DEFENCE_TIME[act]
end

local function finish_act(hero)
  local after_defence = hero.act ~= "chop"
  if hero.attack_queued then
    begin_chop(hero, after_defence)
  else
    hero.act = ""
    hero.act_time = 0
    hero.unstoppable = false
  end
end

local function stagger(robber, hero)
  robber.mood = "stagger"
  robber.act_time = 0
  face_towards(robber, center_x(hero))
  push(robber, -facing(robber), STAGGER_PUSH)
end

-- Илья попал по разбойнику. Отшатывается разбойник только в передышке и пока Илью не заметил: иначе
-- Илья, часто нажимая, не давал бы ему замахнуться.
local function robber_hit(robber, hero)
  robber.flash_time = FLASH_TIME
  hold(robber)
  hold(hero)
  spawn_fx("impact", center_x(robber), robber.position.y + 0.6)
  local mood = robber.mood
  if mood == "rest" or mood == "patrol" or mood == "watch" then
    stagger(robber, hero)
  elseif mood == "chase" then
    push(robber, center_x(robber) < center_x(hero) and -1 or 1, STAGGER_PUSH)
  end
end

local function chop_hits(hero)
  local t = hero.act_time
  if t < CHOP_WINDUP or t >= CHOP_WINDUP + CHOP_STRIKE then
    return
  end
  local x0, x1 = ahead(hero, CHOP_REACH)
  local y0, y1 = hero.position.y - CHOP_ABOVE, bottom(hero)
  for _, robber in ipairs(find({ has = { "robber" } })) do
    if robber.last_swing ~= hero.swing and touches(robber, x0, x1, y0, y1) then
      robber.last_swing = hero.swing
      robber_hit(robber, hero)
    end
  end
end

-- На земле в бою Илья стоит; отлёт от удара ведёт его сам. В прыжке бег не меняется.
local function hold_ground(hero)
  local busy = hero.act ~= "" and hero.on_ground
  if hero.push_time > 0 then
    hero.push_time = math.max(0, hero.push_time - DT)
    hero.velocity.x = hero.push_speed
  elseif busy then
    hero.velocity.x = 0
  end
  hero.run_speed = busy and 0 or HERO_RUN_SPEED
end

local function animate(hero)
  local v = hero.velocity
  local air = not hero.on_ground
  local act = hero.act
  if act == "chop" then
    local t = hero.act_time
    local k = 4
    if t < CHOP_WINDUP then
      k = t < CHOP_WINDUP / 2 and 0 or 1
    elseif t < CHOP_WINDUP + CHOP_STRIKE then
      k = t < CHOP_WINDUP + CHOP_STRIKE / 2 and 2 or 3
    end
    hero.hero_frame = (air and CHOP_AIR or CHOP_GROUND) + k
    return
  end
  if act ~= "" then
    local frames = DEFENCE_FRAMES[act]
    local k = hero.act_time < DEFENCE_TIME[act] / 2 and 0 or 1
    hero.hero_frame = (air and frames.air or frames.ground) + k
    return
  end

  if v.x > STILL_SPEED then
    hero.flip_x = false
  elseif v.x < -STILL_SPEED then
    hero.flip_x = true
  end

  if air then
    local takeoff = 2 * hero.jump_height / hero.jump_time
    if v.y < -takeoff * TOP_SHARE then
      hero.hero_frame = JUMP_UP
    elseif v.y < takeoff * TOP_SHARE then
      hero.hero_frame = JUMP_TOP
    else
      hero.hero_frame = JUMP_DOWN
    end
    return
  end

  if math.abs(v.x) > STILL_SPEED then
    hero.run_dist = (hero.run_dist + math.abs(v.x) * DT) % STRIDE
    hero.hero_frame = RUN_FIRST + math.floor(hero.run_dist / STRIDE * RUN_COUNT)
  else
    hero.idle_time = (hero.idle_time + DT) % IDLE_CYCLE
    hero.hero_frame = IDLE_FIRST + math.floor(hero.idle_time / IDLE_CYCLE * IDLE_COUNT)
  end
end

function fight_hero(hero)
  ensure(hero, HERO_DEFAULTS)
  step_no = step_no + 1
  -- В остановке время боя стоит, но кадр уже тот, что начался в миг попадания.
  if held(hero) then
    animate(hero)
    return
  end
  local pressed = hero.attack_key
  hero.attack_key = false
  if hero.act == "" then
    if pressed then
      begin_chop(hero, false)
    end
  else
    hero.act_time = hero.act_time + DT
    -- Нажатие в замахе и ударе пропадает, в возврате и в защите — помнится.
    if pressed and (hero.act ~= "chop" or hero.act_time >= CHOP_WINDUP + CHOP_STRIKE) then
      hero.attack_queued = true
    end
    if hero.act_time >= act_length(hero.act) then
      finish_act(hero)
    end
  end
  if hero.act == "chop" then
    chop_hits(hero)
  end
  hold_ground(hero)
  animate(hero)
end

-- Удар разбойника долетел до Ильи: исход решается один раз за удар.
local function resolve_blow(robber, hero)
  local outcome = "hurt"
  if not robber.special then
    local r = math.random()
    if r < PARRY_CHANCE then
      outcome = "parry"
    elseif r < PARRY_CHANCE + DODGE_CHANCE then
      outcome = "dodge"
    end
  end
  local away = center_x(hero) < center_x(robber) and -1 or 1
  -- Удар после защиты ничто не прерывает: защиты не видно, пропущенный удар — только вспышка.
  if hero.act == "chop" and hero.unstoppable then
    if outcome == "hurt" then
      spawn_fx("impact", center_x(hero), hero.position.y + 0.6)
    end
    return
  end
  -- Защита обрывает замах и удар, а нажатие не пропадает: удар пойдёт сразу после защиты.
  if hero.act == "chop" and hero.act_time < CHOP_WINDUP + CHOP_STRIKE then
    hero.attack_queued = true
  end
  hero.act = outcome
  hero.act_time = 0
  hero.unstoppable = false
  face_towards(hero, center_x(robber))
  if outcome == "parry" then
    spawn_fx("clash", (center_x(hero) + center_x(robber)) / 2, hero.position.y + 0.5)
    robber.mood = "recoil"
    robber.act_time = 0
    push(robber, -away, STAGGER_PUSH)
    hold(robber)
    hold(hero)
  elseif outcome == "hurt" then
    spawn_fx("impact", center_x(hero), hero.position.y + 0.6)
    push(hero, away, robber.special and SWEEP_PUSH or HURT_PUSH)
  end
  -- Кадр Ильи на этом шаге уже выбран раньше удара разбойника: защита показывается сразу.
  animate(hero)
end

-- Разбойник

local function stand(robber)
  robber.hold_left = false
  robber.hold_right = false
  if robber.push_time <= 0 then
    robber.velocity.x = 0
  end
end

-- Идёт к точке `x` со скоростью `speed`, не выходя за своё место; дошёл — стоит. Возвращает, дошёл ли.
local function walk_to(robber, x, speed)
  x = math.max(robber.home_left, math.min(robber.home_right, x))
  local dx = x - center_x(robber)
  if math.abs(dx) < 0.05 then
    stand(robber)
    return true
  end
  robber.run_speed = speed
  robber.hold_right = dx > 0
  robber.hold_left = dx < 0
  robber.flip_x = dx < 0
  return false
end

-- Стоит ли другой разбойник между этим и Ильёй ближе `ROBBER_GAP`.
local function blocked_by_mate(robber, hero_x)
  local cx = center_x(robber)
  local dir = hero_x > cx and 1 or -1
  for _, mate in ipairs(find({ has = { "robber" } })) do
    if mate.robber_no ~= robber.robber_no then
      local gap = (center_x(mate) - cx) * dir
      if gap > 0 and gap < ROBBER_GAP and (hero_x - center_x(mate)) * dir > -ROBBER_GAP then
        return true
      end
    end
  end
  return false
end

local function begin_blow(robber, hero)
  robber.attacks = robber.attacks + 1
  robber.special = robber.attacks % SPECIAL_EVERY == 0
  robber.mood = "windup"
  robber.act_time = 0
  robber.blow_done = false
  face_towards(robber, center_x(hero))
  stand(robber)
end

local function blow_zone(robber)
  local kind = robber.special and SWEEP or BLOW
  local x0, x1 = ahead(robber, kind.reach)
  if robber.special then
    return x0, x1, bottom(robber) - SWEEP.height, bottom(robber)
  end
  return x0, x1, robber.position.y - BLOW.above, bottom(robber)
end

local function sees(robber, hero)
  return math.abs(center_x(hero) - center_x(robber)) <= SIGHT and math.abs(bottom(hero) - bottom(robber)) <= SIGHT_HEIGHT
end

local function patrol(robber)
  if robber.mood == "watch" then
    stand(robber)
    if robber.act_time >= PATROL_REST then
      robber.mood = "patrol"
      robber.act_time = 0
      robber.patrol_dir = -robber.patrol_dir
    end
    return
  end
  local edge = robber.patrol_dir > 0 and robber.home_right or robber.home_left
  if walk_to(robber, edge, PATROL_SPEED) then
    robber.mood = "watch"
    robber.act_time = 0
  end
end

local function chase(robber, hero)
  local hx = center_x(hero)
  local dx = hx - center_x(robber)
  if math.abs(dx) <= ATTACK_RANGE and math.abs(bottom(hero) - bottom(robber)) <= ATTACK_HEIGHT and robber.cooldown <= 0 then
    begin_blow(robber, hero)
    return
  end
  if math.abs(dx) <= ATTACK_RANGE - 0.1 or blocked_by_mate(robber, hx) then
    stand(robber)
    face_towards(robber, hx)
    return
  end
  local target = hx - (dx > 0 and 1 or -1) * (ATTACK_RANGE - 0.2)
  if walk_to(robber, target, CHASE_SPEED) then
    face_towards(robber, hx)
  end
end

local function attack(robber, hero)
  local kind = robber.special and SWEEP or BLOW
  stand(robber)
  if robber.mood == "windup" then
    if robber.act_time >= kind.windup then
      robber.mood = "strike"
      robber.act_time = 0
    end
  elseif robber.mood == "strike" then
    if not robber.blow_done and hero and touches(hero, blow_zone(robber)) then
      robber.blow_done = true
      resolve_blow(robber, hero)
    end
    if robber.act_time >= kind.strike then
      robber.mood = "rest"
      robber.act_time = 0
    end
  elseif robber.act_time >= kind.rest then
    robber.mood = "chase"
    robber.act_time = 0
    robber.cooldown = BLOW_PAUSE
  end
end

local FIGHT_MOODS = { windup = true, strike = true, rest = true, stagger = true, recoil = true }

local function robber_frame(robber)
  local mood = robber.mood
  local t = robber.act_time
  if mood == "stagger" or mood == "recoil" then
    local len = mood == "stagger" and STAGGER_TIME or RECOIL_TIME
    return R_STAGGER_FIRST + (t < len / 2 and 0 or 1)
  end
  if mood == "windup" or mood == "strike" or mood == "rest" then
    local kind = robber.special and SWEEP or BLOW
    local first = robber.special and R_SWEEP_FIRST or R_BLOW_FIRST
    if mood == "windup" then
      return first + (t < kind.windup / 2 and 0 or 1)
    elseif mood == "strike" then
      return first + (t < kind.strike / 2 and 2 or 3)
    end
    return first + 4
  end
  local vx = math.abs(robber.velocity.x)
  if vx > STILL_SPEED and robber.push_time <= 0 then
    robber.walk_dist = (robber.walk_dist + vx * DT) % R_STRIDE
    return R_WALK_FIRST + math.floor(robber.walk_dist / R_STRIDE * R_WALK_COUNT)
  end
  robber.idle_time = (robber.idle_time + DT) % R_IDLE_CYCLE
  return R_IDLE_FIRST + math.floor(robber.idle_time / R_IDLE_CYCLE * R_IDLE_COUNT)
end

function fight_robber(robber)
  ensure(robber, ROBBER_DEFAULTS)
  if held(robber) then
    -- Кадр боя — по времени боя, он стоит; кадр шага и стойки не трогается.
    if FIGHT_MOODS[robber.mood] then
      robber.robber_frame = robber_frame(robber)
    end
    return
  end
  robber.flash_time = math.max(0, robber.flash_time - DT)
  robber.cooldown = math.max(0, robber.cooldown - DT)
  robber.act_time = robber.act_time + DT
  local hero = the_hero()
  local mood = robber.mood
  if mood == "windup" or mood == "strike" or mood == "rest" then
    attack(robber, hero)
  elseif mood == "stagger" or mood == "recoil" then
    stand(robber)
    if robber.act_time >= (mood == "stagger" and STAGGER_TIME or RECOIL_TIME) then
      robber.mood = "chase"
      robber.act_time = 0
      robber.cooldown = BLOW_PAUSE
    end
  elseif hero and sees(robber, hero) then
    robber.mood = "chase"
    chase(robber, hero)
  else
    if mood == "chase" then
      robber.mood = "patrol"
      robber.act_time = 0
    end
    patrol(robber)
  end
  if robber.push_time > 0 then
    robber.push_time = math.max(0, robber.push_time - DT)
    robber.velocity.x = robber.push_speed
  end
  robber.robber_frame = robber_frame(robber)
end

-- Отсвет стоит на своём разбойнике тем же кадром: белая вспышка от удара, красный ореол особого замаха.
function follow_robber(glow)
  local robber
  for _, r in ipairs(find({ has = { "robber" } })) do
    if r.robber_no == glow.robber_no then
      robber = r
    end
  end
  if not robber then
    glow.opacity = 0
    return
  end
  glow.position = { x = robber.position.x, y = robber.position.y }
  glow.flip_x = robber.flip_x
  glow.robber_frame = robber.robber_frame
  if robber.flash_time > 0 then
    glow.image = "robber_flash"
    glow.opacity = FLASH_OPACITY
  elseif robber.special and (robber.mood == "windup" or robber.mood == "strike") then
    glow.image = "robber_aura"
    local rise = robber.mood == "windup" and math.min(1, robber.act_time / AURA_RISE) or 1
    glow.opacity = AURA_OPACITY * rise
  else
    glow.opacity = 0
  end
end

function animate_fx(fx)
  local lived = FX_LIFE - fx.lifetime
  fx.fx_frame = math.min(fx.fx_frames - 1, math.floor(lived / FX_LIFE * fx.fx_frames))
end

function respawn(hero)
  hero.position = { x = START_X, y = START_Y }
  hero.velocity = { x = 0, y = 0 }
  hero.act = ""
  hero.act_time = 0
  hero.attack_queued = false
  hero.unstoppable = false
  hero.held = false
  hero.push_time = 0
end
