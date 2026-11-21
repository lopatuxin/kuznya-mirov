-- Первый уровень: кадры героя, бой с разбойниками, гибель героя и возврат в начало после падения в ущелье.
-- Разбойник теряет здоровье от попаданий, которые его отталкивают, заставляют отшатнуться или бьют отшатнувшегося
-- после парирования, и от сгустка Василисы, который не отталкивает, а раненого издали заставляет рвануться к ней.
-- Все числа боя — в таблицах heroes, items и enemies; здесь только правила их расчёта.

local DT = 1 / 60
local START_X = 3
local START_Y = 7.1

-- Кадры листа героя (hero_ilya.png, hero_alyosha.png, hero_vasilisa.png — раскладка одна), по 6 в строке: стойка, бег, прыжок (подъём, верх, падение), затем бой —
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

-- Удар героя с топором: секунды замаха, удара и возврата. Время удара другого оружия — те же доли, растянутые
-- по его swing из таблицы items. Место удара — клеток выше макушки, а вперёд от середины — докуда достаёт
-- оружие, reach из таблицы героев.
local CHOP_WINDUP, CHOP_STRIKE, CHOP_RECOVER = 0.18, 0.08, 0.22
local CHOP_TOTAL = CHOP_WINDUP + CHOP_STRIKE + CHOP_RECOVER
local CHOP_ABOVE = 0.4

-- Сгусток оружия с ranged в таблице items: в начале удара срывается с навершия — клеток вперёд от середины
-- героя и вверх от её низа — и летит вперёд с этой скоростью, клеток в секунду; пролетев reach героя, гаснет,
-- последние клетки тускнея. Его урон растёт от Мудрости, как сила заклинаний, а не от Силы.
local BOLT_TIP_X, BOLT_TIP_Y = 1.45, 1.4
local BOLT_SPEED, BOLT_FADE = 14, 0.5
local BOLT_SIZE = 0.4
local SPELL_PER_WISDOM = 0.05

-- Защита героя: секунды каждой; шансы парирования и уклонения — свойства героя, остальное — пропущенный удар.
local DEFENCE_TIME = { parry = 0.3, dodge = 0.35, hurt = 0.35 }

-- Числа героя из характеристик: здоровье, прибавка к урону, ускорение удара, шансы защит и удара в слабое место.
local HEALTH_BASE, HEALTH_PER_STRENGTH = 100, 2
local DAMAGE_PER_STRENGTH = 0.05
local SWING_PER_SKILL = 0.03
local PARRY_PER_SKILL = 0.015
local DODGE_PER_AGILITY, CRIT_PER_AGILITY = 0.02, 0.02
-- Чары пока только у Василисы: зарядов Мудрость / 2 вниз, заряд возвращается за 4 с, на 5 % быстрее за единицу Мудрости.
local WARD_HERO = "vasilisa"
local WARD_WISDOM_PER_CHARGE = 2
local WARD_RECHARGE, WARD_SPEED_PER_WISDOM = 4, 0.05
-- Удар по отшатнувшемуся после парирования и в спину у этого героя всегда двойной.
local DOUBLE_HERO = "alyosha"

-- Строка таблицы enemies, по которой живёт разбойник, и сколько секунд убитый гаснет.
local ENEMY = "robber"
local DEATH_FADE = 0.3

-- Отлёт от удара: на сколько клеток и за сколько секунд.
local PUSH_TIME = 0.12
local HURT_PUSH, SWEEP_PUSH, STAGGER_PUSH = 0.33, 0.5, 0.33
-- Остановка обоих в миг попадания и парирования: шагов (0,05 с).
local HOLD_STEPS = 3

-- Разбойник: где замечает героя и откуда бьёт — по ширине между серединами и по высоте между низами.
local SIGHT, SIGHT_HEIGHT = 7, 2
local ATTACK_RANGE, ATTACK_HEIGHT = 1.2, 1.2
local PATROL_SPEED, CHASE_SPEED, PATROL_REST = 1.5, 3.5, 1
-- Не подходит к другому разбойнику, стоящему между ним и героем, ближе этого.
local ROBBER_GAP = 1
-- Обычный удар: секунды замаха, удара, передышки; место удара — вперёд от середины и выше макушки.
local BLOW = { windup = 0.45, strike = 0.1, rest = 0.7, reach = 1.7, above = 0.6 }
-- Особый удар понизу, каждый третий: место взмаха — вперёд от середины и вверх от низа.
local SWEEP = { windup = 0.75, strike = 0.15, rest = 0.9, reach = 2.1, height = 0.7 }
local SPECIAL_EVERY = 3
-- После передышки или отшатывания — столько секунд до нового замаха.
local BLOW_PAUSE = 0.4
-- Отшатнулся от удара героя и после парирования.
local STAGGER_TIME, RECOIL_TIME = 0.35, 0.4
-- Рывок к герою, ранившему издали: не дальше этих клеток, за эти секунды, и не ближе к его середине, чем на
-- ATTACK_RANGE − DASH_SHORT.
local DASH_DIST, DASH_TIME, DASH_SHORT = 2.5, 0.25, 0.2

-- Кадры листа robber.png, по 5 в строке.
local R_IDLE_FIRST, R_IDLE_COUNT, R_IDLE_CYCLE = 0, 4, 1.4
local R_WALK_FIRST, R_WALK_COUNT, R_STRIDE = 4, 8, 1.6
local R_BLOW_FIRST, R_SWEEP_FIRST, R_STAGGER_FIRST = 12, 17, 22

-- Отсвет: белая вспышка от удара героя и красный ореол особого замаха.
local FLASH_TIME, FLASH_OPACITY = 0.1, 0.6
local AURA_RISE, AURA_OPACITY = 0.2, 0.9

-- Вспышки боя: кадры и сколько секунд живут; кадр — по прожитому. Вспышка чар стоит на герое и идёт за ним.
local FX = {
  clash = { frames = 4, life = 0.2 },
  impact = { frames = 3, life = 0.2 },
  ward = { frames = 5, life = 0.4, follow = true },
}

local HERO_DEFAULTS = {
  act = "", act_time = 0, swing = 0, cast_swing = 0, hold_until = 0, push_time = 0, push_speed = 0, ward_wait = 0,
}
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

local function round(x)
  return math.floor(x + 0.5)
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
  local fx = FX[kind]
  create({
    name = kind,
    fx = true,
    fx_follow = fx.follow,
    image = kind,
    fx_frames = fx.frames,
    fx_frame = 0,
    fx_life = fx.life,
    position = { x - 0.1, y - 0.1 },
    size = { 0.2, 0.2 },
    layer = 55,
    lifetime = fx.life,
  })
end

-- Номер шага партии; ведёт его fight_hero — правило героя идёт первым среди правил боя.
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

local function living_robbers()
  return find({ has = { "robber" }, without = { "dead" } })
end

local function enemy_row()
  local row = tables.enemies[ENEMY]
  if row == nil then
    error("нет врага " .. ENEMY .. " в таблице enemies")
  end
  return row
end

-- Отсвет разбойника: его признак dead выводит отсвет из правила, пока разбойник убит.
local function set_glow_dead(robber, dead)
  for _, glow in ipairs(find({ has = { "robber_glow" } })) do
    if glow.robber_no == robber.robber_no then
      glow.dead = dead
      glow.opacity = 0
    end
  end
end

-- Здоровье разбойника берётся из таблицы enemies при его первом шаге в партии; место он запоминает для возврата.
local function take_robber_stats(robber)
  if robber.max_health ~= nil then
    return
  end
  robber.max_health = enemy_row().health
  robber.health = robber.max_health
  robber.start_x, robber.start_y = robber.position.x, robber.position.y
end

-- Разбойник встаёт на своё место с полным здоровьем, в обходе и без счёта ударов до особого.
local function reset_robber(robber)
  for key, value in pairs(ROBBER_DEFAULTS) do
    robber[key] = value
  end
  robber.health = robber.max_health
  robber.position = { x = robber.start_x, y = robber.start_y }
  robber.velocity = { x = 0, y = 0 }
  robber.flip_x = false
  robber.opacity = 1
  robber.special = false
  robber.blow_done = false
  robber.held = false
  robber.dead = false
  robber.show_bar = false
  robber.hold_left = false
  robber.hold_right = false
  set_glow_dead(robber, false)
end

-- Герой

local function begin_chop(hero, unstoppable)
  hero.act = "chop"
  hero.act_time = 0
  hero.attack_queued = false
  hero.unstoppable = unstoppable
  hero.swing = hero.swing + 1
end

-- Когда у удара героя кончается замах, кончается удар и кончается возврат.
local function chop_marks(hero)
  local k = hero.swing_time / CHOP_TOTAL
  return CHOP_WINDUP * k, (CHOP_WINDUP + CHOP_STRIKE) * k, hero.swing_time
end

local function act_length(hero)
  if hero.act == "chop" then
    return hero.swing_time
  end
  return DEFENCE_TIME[hero.act]
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

-- Смотрит ли разбойник от героя.
local function turned_away(robber, hero)
  return (center_x(hero) - center_x(robber)) * facing(robber) < 0
end

-- Двойной ли удар: выпал удар в слабое место, а у DOUBLE_HERO он ещё всегда двойной по отшатнувшемуся после
-- парирования и в спину. Двойной удар и удар в слабое место не складываются.
local function double_blow(robber, hero)
  if hero.hero_id == DOUBLE_HERO and (robber.mood == "recoil" or turned_away(robber, hero)) then
    return true
  end
  return math.random() < hero.crit_chance
end

local function blow_damage(robber, hero)
  local kind = tables.items[hero.weapon].damage_type
  local factor = enemy_row().damage_taken[kind]
  if double_blow(robber, hero) then
    factor = factor * 2
  end
  return round(hero.damage * factor)
end

-- Здоровье разбойника дошло до нуля: он гаснет на кадре отшатывания и больше не бьёт, не ходит и не принимает удары.
local function begin_dying(robber)
  robber.health = 0
  robber.mood = "dying"
  robber.act_time = 0
  robber.push_time = 0
  robber.velocity.x = 0
end

-- Герой попал по разбойнику. Урон идёт от попадания, от которого разбойник отшатывается или отлетает, и по
-- отшатнувшемуся после парирования; в замахе и ударе — только вспышка. Отшатывается разбойник только в
-- передышке и пока героя не заметил: иначе герой, часто нажимая, не давал бы ему замахнуться.
local function robber_hit(robber, hero)
  robber.flash_time = FLASH_TIME
  robber.show_bar = true
  hold(robber)
  hold(hero)
  spawn_fx("impact", center_x(robber), robber.position.y + 0.6)
  local mood = robber.mood
  local can_stagger = mood == "rest" or mood == "patrol" or mood == "watch"
  if not (can_stagger or mood == "chase" or mood == "recoil") then
    return
  end
  robber.health = math.max(0, robber.health - blow_damage(robber, hero))
  if robber.health == 0 then
    begin_dying(robber)
  elseif can_stagger then
    stagger(robber, hero)
  elseif mood == "chase" then
    push(robber, center_x(robber) < center_x(hero) and -1 or 1, STAGGER_PUSH)
  end
end

local function chop_hits(hero)
  local t = hero.act_time
  local windup_end, strike_end = chop_marks(hero)
  if t < windup_end or t >= strike_end then
    return
  end
  local x0, x1 = ahead(hero, hero.reach)
  local y0, y1 = hero.position.y - CHOP_ABOVE, bottom(hero)
  for _, robber in ipairs(living_robbers()) do
    if robber.mood ~= "dying" and robber.last_swing ~= hero.swing and touches(robber, x0, x1, y0, y1) then
      robber.last_swing = hero.swing
      robber_hit(robber, hero)
    end
  end
end

-- Дойдёт ли разбойник до героя: середина героя не дальше ATTACK_RANGE за краями места разбойника, а низы
-- различаются не больше ATTACK_HEIGHT.
local function can_reach(robber, hero)
  local hx = center_x(hero)
  return hx >= robber.home_left - ATTACK_RANGE and hx <= robber.home_right + ATTACK_RANGE
    and math.abs(bottom(hero) - bottom(robber)) <= ATTACK_HEIGHT
end

-- Рывок к герою: разбойник поворачивается к нему и за DASH_TIME пробегает до DASH_DIST клеток, но не ближе
-- ATTACK_RANGE − DASH_SHORT к его середине и не за край своего места.
local function begin_dash(robber, hero)
  local hx, cx = center_x(hero), center_x(robber)
  local dir = hx > cx and 1 or -1
  local target = math.max(robber.home_left, math.min(robber.home_right, hx - dir * (ATTACK_RANGE - DASH_SHORT)))
  local dist = math.min(DASH_DIST, math.max(0, (target - cx) * dir))
  robber.mood = "dash"
  robber.act_time = 0
  robber.hold_left = false
  robber.hold_right = false
  face_towards(robber, hx)
  robber.push_time = DASH_TIME
  robber.push_speed = dir * dist / DASH_TIME
end

-- Сгусток попал в разбойника в точке (x, y). Разбойник, который до героя не дойдёт, его отбивает. Урон — в тех
-- же состояниях, что у удара вблизи, но сгусток не отбрасывает и не заставляет отшатнуться; раненый дальше
-- своего удара разбойник делает рывок. В миг попадания останавливается только разбойник.
local function bolt_hit(robber, hero, x, y)
  if not can_reach(robber, hero) then
    spawn_fx("clash", x, y)
    return
  end
  robber.flash_time = FLASH_TIME
  robber.show_bar = true
  hold(robber)
  spawn_fx("impact", x, y)
  local mood = robber.mood
  if not (mood == "rest" or mood == "patrol" or mood == "watch" or mood == "chase" or mood == "recoil") then
    return
  end
  robber.health = math.max(0, robber.health - blow_damage(robber, hero))
  if robber.health == 0 then
    begin_dying(robber)
  elseif math.abs(center_x(hero) - center_x(robber)) > ATTACK_RANGE then
    begin_dash(robber, hero)
  end
end

-- В конце замаха оружие с ranged выпускает с навершия сгусток — один за удар, в ту сторону, куда смотрит герой.
-- Хвост — откуда сгусток летел на прошлом шаге: попадание ищется по всему пролёту, и в первом шаге — от
-- середины героя, чтобы сгусток доставал и вплотную стоящего разбойника.
local function cast_bolt(hero)
  local windup_end = chop_marks(hero)
  if hero.act_time < windup_end or hero.cast_swing == hero.swing then
    return
  end
  hero.cast_swing = hero.swing
  local dir = facing(hero)
  local head = center_x(hero) + dir * BOLT_TIP_X
  local y = bottom(hero) - BOLT_TIP_Y
  create({
    name = "сгусток",
    bolt = true,
    image = "bolt",
    fx_frame = 0,
    flip_x = dir < 0,
    position = { head - BOLT_SIZE / 2, y - BOLT_SIZE / 2 },
    size = { BOLT_SIZE, BOLT_SIZE },
    layer = 55,
    bolt_dir = dir,
    bolt_left = hero.reach,
    bolt_tail = center_x(hero),
  })
end

-- На земле в бою герой стоит; отлёт от удара ведёт его сам. В прыжке бег не меняется.
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
    local windup_end, strike_end = chop_marks(hero)
    local t = hero.act_time
    local k = 4
    if t < windup_end then
      k = t < windup_end / 2 and 0 or 1
    elseif t < strike_end then
      k = t < (windup_end + strike_end) / 2 and 2 or 3
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

-- Лист кадров, портрет, дальность удара, характеристики и оружие героя — из таблиц heroes и items по его hero_id,
-- и числа боя из них — один раз за партию.
local function take_stats(hero)
  if hero.strength ~= nil then
    return
  end
  local row = tables.heroes[hero.hero_id]
  if row == nil then
    error("нет героя " .. tostring(hero.hero_id) .. " в таблице heroes")
  end
  local weapon = tables.items[row.weapon]
  if weapon == nil then
    error("нет оружия " .. tostring(row.weapon) .. " героя " .. hero.hero_id .. " в таблице items")
  end
  hero.strength, hero.skill, hero.agility, hero.wisdom = row.strength, row.skill, row.agility, row.wisdom
  hero.image = row.sheet
  hero.hud_portrait = row.hud_portrait
  hero.reach = row.reach
  hero.weapon = row.weapon
  hero.ranged = weapon.ranged == true
  hero.max_health = HEALTH_BASE + HEALTH_PER_STRENGTH * hero.strength
  hero.health = hero.max_health
  if hero.ranged then
    hero.damage = round(weapon.damage * (1 + SPELL_PER_WISDOM * hero.wisdom))
  else
    hero.damage = round(weapon.damage * (1 + DAMAGE_PER_STRENGTH * hero.strength))
  end
  hero.swing_time = CHOP_TOTAL * weapon.swing / (1 + SWING_PER_SKILL * hero.skill)
  hero.parry_chance = weapon.parry + PARRY_PER_SKILL * hero.skill
  hero.dodge_chance = DODGE_PER_AGILITY * hero.agility
  hero.crit_chance = CRIT_PER_AGILITY * hero.agility
  hero.max_spirit = hero.hero_id == WARD_HERO and math.floor(hero.wisdom / WARD_WISDOM_PER_CHARGE) or 0
  hero.spirit = hero.max_spirit
  hero.ward_time = WARD_RECHARGE / (1 + WARD_SPEED_PER_WISDOM * hero.wisdom)
end

-- Заряд чар возвращается через ward_time; время идёт, пока запас не полон, и начинается заново после каждого
-- потраченного заряда.
local function recharge_ward(hero)
  if hero.spirit >= hero.max_spirit then
    hero.ward_wait = 0
    return
  end
  hero.ward_wait = hero.ward_wait + DT
  if hero.ward_wait >= hero.ward_time then
    hero.spirit = hero.spirit + 1
    hero.ward_wait = 0
  end
end

function fight_hero(hero)
  ensure(hero, HERO_DEFAULTS)
  take_stats(hero)
  step_no = step_no + 1
  recharge_ward(hero)
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
    local _, strike_end = chop_marks(hero)
    if pressed and (hero.act ~= "chop" or hero.act_time >= strike_end) then
      hero.attack_queued = true
    end
    if hero.act_time >= act_length(hero) then
      finish_act(hero)
    end
  end
  if hero.act == "chop" and hero.ranged then
    cast_bolt(hero)
  elseif hero.act == "chop" then
    chop_hits(hero)
  end
  hold_ground(hero)
  animate(hero)
end

-- Сгусток пролетает шаг вперёд и бьёт первого живого разбойника на пролёте; попав или пролетев свою дальность,
-- гаснет на следующем шаге. Последние BOLT_FADE клеток тускнеет, кадр мерцания — по пройденному пути.
function fly_bolt(bolt)
  if bolt.bolt_left <= 0 then
    return
  end
  local dir = bolt.bolt_dir
  local step = math.min(BOLT_SPEED * DT, bolt.bolt_left)
  bolt.bolt_left = bolt.bolt_left - step
  bolt.position = { x = bolt.position.x + dir * step, y = bolt.position.y }
  local head = bolt.position.x + (dir > 0 and BOLT_SIZE or 0)
  local x0, x1 = math.min(bolt.bolt_tail, head), math.max(bolt.bolt_tail, head)
  local y0, y1 = bolt.position.y, bolt.position.y + BOLT_SIZE
  bolt.bolt_tail = head
  bolt.opacity = math.min(1, bolt.bolt_left / BOLT_FADE)
  bolt.fx_frame = math.floor(bolt.bolt_left / (BOLT_SPEED * DT)) % 3
  local hero = the_hero()
  local nearest
  for _, robber in ipairs(living_robbers()) do
    if robber.mood ~= "dying" and touches(robber, x0, x1, y0, y1)
      and (nearest == nil or (center_x(robber) - center_x(nearest)) * dir < 0) then
      nearest = robber
    end
  end
  if nearest then
    bolt_hit(nearest, hero, center_x(nearest) - dir * nearest.size.x / 2, y0 + BOLT_SIZE / 2)
    bolt.bolt_left = 0
  end
  if bolt.bolt_left <= 0 then
    bolt.opacity = 0
    bolt.lifetime = DT
  end
end

-- Начало уровня: герой стоит на месте, действие и отлёт сброшены. Так он встаёт и после ущелья, и после гибели.
local function place_at_start(hero)
  hero.position = { x = START_X, y = START_Y }
  hero.velocity = { x = 0, y = 0 }
  hero.act = ""
  hero.act_time = 0
  hero.attack_queued = false
  hero.unstoppable = false
  hero.held = false
  hero.push_time = 0
end

-- Здоровье героя дошло до нуля: он встаёт в начале уровня с полным здоровьем и запасом чар, а все разбойники,
-- и убитые, и раненые, встают на свои места, а летящие сгустки гаснут.
local function die(hero)
  place_at_start(hero)
  hero.health = hero.max_health
  hero.spirit = hero.max_spirit
  hero.ward_wait = 0
  for _, robber in ipairs(find({ has = { "robber" } })) do
    reset_robber(robber)
  end
  for _, bolt in ipairs(find({ has = { "bolt" } })) do
    bolt.bolt_left = 0
    bolt.opacity = 0
    bolt.lifetime = DT
  end
end

-- Пропущенный удар снимает здоровье; возвращает, погиб ли герой.
local function wound(hero, damage)
  hero.health = hero.health - damage
  if hero.health > 0 then
    return false
  end
  die(hero)
  return true
end

-- Чары принимают обычный удар первыми, пока в запасе есть заряд: здоровье не меняется, герой не отшатывается
-- и не прерывает своего удара.
local function ward_takes(hero)
  if hero.spirit < 1 then
    return false
  end
  hero.spirit = hero.spirit - 1
  hero.ward_wait = 0
  spawn_fx("ward", center_x(hero), hero.position.y + hero.size.y / 2)
  return true
end

-- Удар разбойника долетел до героя: исход решается один раз за удар. Возвращает, погиб ли герой.
local function resolve_blow(robber, hero)
  local row = enemy_row()
  local outcome = "hurt"
  if not robber.special then
    if ward_takes(hero) then
      return false
    end
    local r = math.random()
    if r < hero.parry_chance then
      outcome = "parry"
    elseif r < hero.parry_chance + hero.dodge_chance then
      outcome = "dodge"
    end
  end
  if outcome == "hurt" and wound(hero, robber.special and row.sweep_damage or row.blow_damage) then
    return true
  end
  local away = center_x(hero) < center_x(robber) and -1 or 1
  -- Удар после защиты ничто не прерывает: защиты не видно, пропущенный удар — только вспышка.
  if hero.act == "chop" and hero.unstoppable then
    if outcome == "hurt" then
      spawn_fx("impact", center_x(hero), hero.position.y + 0.6)
    end
    return false
  end
  -- Защита обрывает замах и удар, а нажатие не пропадает: удар пойдёт сразу после защиты.
  local _, strike_end = chop_marks(hero)
  if hero.act == "chop" and hero.act_time < strike_end then
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
  -- Кадр героя на этом шаге уже выбран раньше удара разбойника: защита показывается сразу.
  animate(hero)
  return false
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

-- Стоит ли другой разбойник между этим и героем ближе `ROBBER_GAP`.
local function blocked_by_mate(robber, hero_x)
  local cx = center_x(robber)
  local dir = hero_x > cx and 1 or -1
  for _, mate in ipairs(living_robbers()) do
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
      -- Герой погиб — разбойники уже стоят на местах, этот удар их не продолжает.
      if resolve_blow(robber, hero) then
        return
      end
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

local FIGHT_MOODS = { windup = true, strike = true, rest = true, stagger = true, recoil = true, dying = true }

local function robber_frame(robber)
  local mood = robber.mood
  local t = robber.act_time
  if mood == "dying" then
    return R_STAGGER_FIRST
  end
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
  if vx > STILL_SPEED and (robber.push_time <= 0 or robber.mood == "dash") then
    robber.walk_dist = (robber.walk_dist + vx * DT) % R_STRIDE
    return R_WALK_FIRST + math.floor(robber.walk_dist / R_STRIDE * R_WALK_COUNT)
  end
  robber.idle_time = (robber.idle_time + DT) % R_IDLE_CYCLE
  return R_IDLE_FIRST + math.floor(robber.idle_time / R_IDLE_CYCLE * R_IDLE_COUNT)
end

-- Убитый разбойник гаснет на месте; погас — он dead, и правила его больше не ведут.
local function fade(robber)
  stand(robber)
  robber.opacity = 1 - math.min(1, robber.act_time / DEATH_FADE)
  if robber.act_time >= DEATH_FADE then
    robber.dead = true
    set_glow_dead(robber, true)
  end
end

function fight_robber(robber)
  ensure(robber, ROBBER_DEFAULTS)
  take_robber_stats(robber)
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
  elseif mood == "dash" then
    -- После рывка — погоня, и бить разбойник может сразу.
    if robber.act_time >= DASH_TIME then
      robber.mood = "chase"
      robber.act_time = 0
      robber.cooldown = 0
      robber.push_time = 0
    end
  elseif mood == "dying" then
    fade(robber)
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
  robber.show_bar = not (robber.mood == "patrol" or robber.mood == "watch")
end

-- Отсвет стоит на своём разбойнике тем же кадром: белая вспышка от удара, красный ореол особого замаха.
function follow_robber(glow)
  local robber
  for _, r in ipairs(find({ has = { "robber" } })) do
    if r.robber_no == glow.robber_no then
      robber = r
    end
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
  if fx.fx_follow then
    local hero = the_hero()
    fx.position = { x = center_x(hero) - 0.1, y = hero.position.y + hero.size.y / 2 - 0.1 }
  end
  local lived = fx.fx_life - fx.lifetime
  fx.fx_frame = math.min(fx.fx_frames - 1, math.floor(lived / fx.fx_life * fx.fx_frames))
end

-- Падение в ущелье: герой в начале уровня, здоровье и разбойники не меняются.
function respawn(hero)
  place_at_start(hero)
end
