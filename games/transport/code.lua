-- Транспортная компания: рабочий день, заказы клиентов, план машин, езда, загрузка и выгрузка, деньги,
-- доверие клиентов, конкурент и события на дорогах («Фаза-02-первый-час» игры). Движок везёт машины
-- правилом `drive`, считает время и путь по дорогам (`road_route`), рисует списки и кнопки; всё остальное
-- — здесь. Объекты карты (признак `map`) пишет программа перевода карты; код их только читает и меняет
-- им `road_speed` и подписи. Каждая партия перечитывает этот файл, поэтому всё ниже начинает партию заново.

local MINUTE = 3                 -- секунд мира в игровой минуте при скорости ×1
local STEPS_PER_MINUTE = MINUTE * 60  -- шагов движка в игровой минуте
local DAY_START = 7 * 60
local DAY_END = 19 * 60
local CELL_M = 20                -- метров в клетке карты
local FUEL_PER_KM = 9            -- рублей бензина на километр
local LOAN_PAYMENT = 2500        -- платёж по кредиту в конце дня
local OFFICE_RENT = 1500         -- аренда конторы за день
local CAR_PRICE = 90000
local DRIVER_FEE = 5000
local DRIVER_WAGE = 2500
local BANKRUPT_AT = -30000
local BREAKDOWN_CHANCE = 0.03
local REPAIR_COST = 3000
local REPAIR_MINUTES = 40
local MESSAGES_KEPT = 5
local WEEKDAYS = { "пн", "вт", "ср", "чт", "пт", "сб", "вс" }

local state = {
  game = nil,
  places = nil,
  trust = {},
  pending = {},
  last_minute = -1,
  next_order = 1,
  next_message = 1,
  next_car = 2,
  closed = {},
  jam = {},
  jam_until = nil,
  street_names = nil,
  earned = 0,
  spent_fuel = 0,
  delivered = 0,
  late = 0,
}

-- Числа и время --------------------------------------------------------------------------------

local function clamp(v, lo, hi)
  if v < lo then return lo end
  if v > hi then return hi end
  return v
end

local function fmt_time(minute)
  local m = math.floor(minute + 0.5)
  return string.format("%02d:%02d", math.floor(m / 60) % 24, m % 60)
end

local function parse_time(text)
  local h, m = string.match(text, "^(%d+):(%d+)$")
  return tonumber(h) * 60 + tonumber(m)
end

local function fmt_tons(w)
  local s = string.format("%.2f", w)
  s = string.gsub(s, "0+$", "")
  s = string.gsub(s, "%.$", "")
  return (string.gsub(s, "%.", ",")) .. " т"
end

local function fmt_money(n)
  local sign = n < 0 and "−" or ""
  local s = tostring(math.floor(math.abs(n) + 0.5))
  local out = ""
  while #s > 3 do
    out = " " .. string.sub(s, -3) .. out
    s = string.sub(s, 1, -4)
  end
  return sign .. s .. out .. " ₽"
end

-- Объекты игры ---------------------------------------------------------------------------------

local function game()
  if state.game == nil then
    state.game = find({ has = { "game" } })[1]
  end
  return state.game
end

local function now()
  return game().minute
end

-- Места игры: здания клиентов и магазинов на карте, по `place`.
local function places()
  if state.places == nil then
    state.places = {}
    for _, p in ipairs(find({ has = { "place" } })) do
      state.places[p.place] = p
    end
  end
  return state.places
end

local function place_point(id)
  local p = places()[id]
  if p == nil then
    error("в карте нет места \"" .. tostring(id) .. "\"")
  end
  return { x = p.stop_x, y = p.stop_y, name = p.name }
end

local function shop_ids()
  local list = {}
  for id, _ in pairs(places()) do
    if string.sub(id, 1, 5) == "shop_" then
      list[#list + 1] = id
    end
  end
  table.sort(list)
  return list
end

local function cars()
  return find({ has = { "car" } })
end

local function car_by_id(id)
  for _, c in ipairs(cars()) do
    if c.car_id == id then return c end
  end
  return nil
end

local function order_by_id(id)
  for _, o in ipairs(find({ has = { "order" }, without = { "done" } })) do
    if o.order_id == id then return o end
  end
  return nil
end

-- Остановки машины по порядку, без исполненных.
local function stops_of(car)
  local list = {}
  for _, s in ipairs(find({ has = { "stop" }, without = { "done" } })) do
    if s.stop_car == car.car_id then
      list[#list + 1] = s
    end
  end
  table.sort(list, function(a, b) return a.stop_seq < b.stop_seq end)
  return list
end

local function selected_car()
  return find({ has = { "car", "selected" } })[1]
end

-- Лента сообщений: последние несколько, новые сверху.
local function say(text)
  local g = game()
  create({ message = true, msg_time = state.next_message, msg_text = fmt_time(g.minute) .. "  " .. text })
  state.next_message = state.next_message + 1
  local all = find({ has = { "message" } })
  if #all >= MESSAGES_KEPT then
    table.sort(all, function(a, b) return a.msg_time < b.msg_time end)
    for i = 1, #all - MESSAGES_KEPT + 1 do
      delete(all[i])
    end
  end
end

-- Доверие клиентов ------------------------------------------------------------------------------

local function trust(client)
  return state.trust[client] or 50
end

local function trust_text(client)
  local t = trust(client)
  if t >= 75 then return "постоянный клиент" end
  if t >= 55 then return "доволен" end
  if t >= 35 then return "присматривается" end
  if t >= 20 then return "недоволен" end
  return "ушёл к Сидорову"
end

local function change_trust(client, delta)
  state.trust[client] = clamp(trust(client) + delta, 0, 100)
  local p = places()[client]
  if p ~= nil then
    p.info_text = tables.clients[client].about .. "\nДоверие: " .. trust_text(client)
  end
end

-- Цена с поправкой на доверие: постоянному клиенту выгоднее платить, недовольный торгуется.
local function price_factor(client)
  return 0.85 + trust(client) / 200
end

-- Деньги ---------------------------------------------------------------------------------------

local function refresh_money()
  local g = game()
  g.money_text = fmt_money(g.money)
  g.debt_text = fmt_money(g.debt)
  local has_spare_car = false
  for _, c in ipairs(cars()) do
    if not c.has_driver then has_spare_car = true end
  end
  g.can_buy = (g.money >= CAR_PRICE) and #cars() < 2
  g.can_hire = has_spare_car and g.money >= DRIVER_FEE
end

local function spend(amount)
  local g = game()
  g.money = g.money - amount
  refresh_money()
end

-- Заказы ---------------------------------------------------------------------------------------

local function address_point()
  local list = tables.addresses
  local a = list[math.random(1, #list)]
  return { x = a.x, y = a.y, name = a.address }
end

local function endpoint(kind, avoid)
  if kind == "address" then
    for _ = 1, 5 do
      local p = address_point()
      if avoid == nil or math.abs(p.x - avoid.x) + math.abs(p.y - avoid.y) > 10 then
        return p
      end
    end
    return address_point()
  end
  if kind == "shops" then
    local list = shop_ids()
    return place_point(list[math.random(1, #list)])
  end
  return place_point(kind)
end

local TAG_TEXT = { urgent = "срочно", careful = "бережно", opening = "к открытию" }

local function new_order(client, job)
  if trust(client) < 20 then
    return
  end
  local from = endpoint(job.from, nil)
  local to = endpoint(job.to, from)
  local t = now()
  local deadline = job.deadline and parse_time(job.deadline) or (t + job.deadline_after)
  local price = math.floor(job.price * price_factor(client) / 50 + 0.5) * 50
  local tags = {}
  local fields = {
    order = true,
    order_id = state.next_order,
    client = client,
    title = job.cargo .. " · " .. tables.clients[client].short,
    cargo = job.cargo,
    route_text = from.name .. " — " .. to.name,
    why_text = job.why or "",
    from_x = from.x, from_y = from.y, from_name = from.name,
    to_x = to.x, to_y = to.y, to_name = to.name,
    weight = job.weight,
    deadline = deadline,
    price = price,
    offer_until = t + job.offer,
  }
  for _, tag in ipairs(job.tags) do
    fields[tag] = true
    tags[#tags + 1] = TAG_TEXT[tag]
  end
  local extra = #tags > 0 and (" · " .. table.concat(tags, ", ")) or ""
  fields.terms_text = "до " .. fmt_time(deadline) .. " · " .. fmt_money(price) .. " · " ..
    fmt_tons(job.weight) .. extra
  state.next_order = state.next_order + 1
  create(fields)
end

-- Расписание клиентов на день: что и когда они закажут.
local function plan_day()
  state.pending = {}
  for client, info in pairs(tables.clients) do
    for _, job in ipairs(info.jobs) do
      local chance = job.chance * (trust(client) >= 75 and 1.2 or 1) * (trust(client) < 35 and 0.6 or 1)
      if math.random() < chance then
        local w0, w1 = parse_time(job.window[1]), parse_time(job.window[2])
        for r = 1, (job["repeat"] or 1) do
          local at
          if job.spread then
            at = w0 + math.random() * (w1 - w0)
          else
            at = w0 + math.random() * math.min(15, w1 - w0) + (r - 1) * 2
          end
          state.pending[#state.pending + 1] = { at = at, client = client, job = job }
        end
      end
    end
  end
  table.sort(state.pending, function(a, b) return a.at < b.at end)
end

local function release_pending(t)
  while #state.pending > 0 and state.pending[1].at <= t do
    local p = table.remove(state.pending, 1)
    new_order(p.client, p.job)
  end
end

-- Свободный заказ, который висит дольше, чем клиент готов ждать, забирает конкурент.
local function expire_offers(t)
  for _, o in ipairs(find({ has = { "order" }, without = { "taken", "done" } })) do
    if o.offer_until <= t then
      say("Сидоров забрал заказ: " .. o.title)
      o.done = true
      delete(o)
    end
  end
end

-- Машины ---------------------------------------------------------------------------------------

local function handling_minutes(order)
  local m = 10 + 8 * order.weight
  if order.careful then m = m * 1.5 end
  return m
end

local function car_center(car)
  return { x = car.position.x + car.size.x / 2, y = car.position.y + car.size.y / 2 }
end

local function mark_selected_stops()
  local sel = selected_car()
  for _, s in ipairs(find({ has = { "stop" } })) do
    s.of_selected = sel ~= nil and s.stop_car == sel.car_id
  end
end

-- Подпись машины в пути: куда и за чем едет.
local function driving_text(car, stop)
  local order = order_by_id(stop.stop_order)
  local verb = stop.stop_kind == "load" and "едет за грузом: " or "везёт: "
  car.state_text = verb .. (order and order.title or "") .. " — " .. stop.place_name
end

local function go_next(car)
  local list = stops_of(car)
  local first = list[1]
  if first == nil then
    car.state = "idle"
    car.state_text = "ждёт заказы"
    car.drive_to = nil
    return
  end
  car.state = "driving"
  car.trip_x = car_center(car).x
  car.trip_y = car_center(car).y
  car.drive_to = { x = first.stop_x, y = first.stop_y }
  driving_text(car, first)
  if math.random() < BREAKDOWN_CHANCE then
    car.breaks_at = now() + 2 + math.random() * 8
  end
end

local function pay_fuel(car)
  local here = car_center(car)
  local route = road_route({ x = car.trip_x, y = car.trip_y }, here, car.drive_speed)
  local km = route.length * CELL_M / 1000
  local cost = math.floor(km * FUEL_PER_KM + 0.5)
  state.spent_fuel = state.spent_fuel + cost
  spend(cost)
end

local function arrive(car)
  car.breaks_at = nil
  pay_fuel(car)
  local first = stops_of(car)[1]
  if first == nil then
    go_next(car)
    return
  end
  local order = order_by_id(first.stop_order)
  local minutes = order and handling_minutes(order) or 5
  car.wait = minutes * MINUTE
  if first.stop_kind == "load" then
    car.state = "loading"
    car.state_text = "грузит: " .. (order and order.title or "") .. ", " .. math.floor(minutes) .. " мин"
  else
    car.state = "unloading"
    car.state_text = "выгружает: " .. (order and order.title or "") .. ", " .. math.floor(minutes) .. " мин"
  end
end

local function deliver(car, order)
  local late = now() - order.deadline
  local paid = order.price
  local client = order.client
  if late <= 0 then
    change_trust(client, 6)
    say(tables.clients[client].short .. ": вовремя, +" .. fmt_money(paid))
  else
    local cut = clamp(late / 60 * 0.5, 0, 0.6)
    paid = math.floor(order.price * (1 - cut) / 10 + 0.5) * 10
    local penalty = (order.opening or late > 30) and -14 or -5
    change_trust(client, penalty)
    state.late = state.late + 1
    say(tables.clients[client].short .. ": опоздали на " .. math.floor(late) .. " мин, заплатили " .. fmt_money(paid))
  end
  state.delivered = state.delivered + 1
  state.earned = state.earned + paid
  local g = game()
  g.money = g.money + paid
  refresh_money()
  order.done = true
  delete(order)
end

local function finish_stop(car)
  local first = stops_of(car)[1]
  if first ~= nil then
    local order = order_by_id(first.stop_order)
    if order ~= nil then
      if first.stop_kind == "load" then
        order.picked = true
        car.load = car.load + order.weight
      else
        car.load = math.max(0, car.load - order.weight)
        deliver(car, order)
      end
    end
    first.done = true
    delete(first)
  end
  car.state = "idle"
  go_next(car)
end

local function break_down(car)
  car.breaks_at = nil
  pay_fuel(car)
  car.state = "broken"
  car.drive_to = nil
  car.wait = REPAIR_MINUTES * MINUTE
  car.state_text = "сломалась, ремонт " .. REPAIR_MINUTES .. " мин"
  spend(REPAIR_COST)
  say(car.name .. " сломалась: ремонт " .. REPAIR_MINUTES .. " мин и " .. fmt_money(REPAIR_COST))
end

function car_tick(car)
  local g = game()
  if g.day_over then
    return
  end
  if car.state == "broken" then
    if car.wait <= 0 then
      -- После ремонта — к первой остановке плана, каким он стал за время ремонта.
      go_next(car)
    end
  elseif car.state == "driving" then
    if car.drive_to == nil then
      arrive(car)
    elseif car.breaks_at ~= nil and now() >= car.breaks_at then
      break_down(car)
    end
  elseif car.state == "loading" or car.state == "unloading" then
    if car.wait <= 0 then
      finish_stop(car)
    end
  elseif car.has_driver and #stops_of(car) > 0 then
    go_next(car)
  end
end

-- Сроки по плану выбранной машины: когда она будет на каждой остановке, если ехать по порядку.
-- Подписи сроков для списка остановок машины по порядку: и для живых остановок, и для записей, которые
-- кнопка только собирается создать, — так срок виден сразу, и на паузе тоже.
local function eta_texts(car, list)
  local t = now()
  if car.state == "loading" or car.state == "unloading" or car.state == "broken" then
    t = t + math.max(0, car.wait) / MINUTE
  end
  local pos = car_center(car)
  for i, s in ipairs(list) do
    local order = order_by_id(s.stop_order)
    local busy = (i == 1 and (car.state == "loading" or car.state == "unloading"))
    if not busy then
      local r = road_route(pos, { x = s.stop_x, y = s.stop_y }, car.drive_speed)
      t = t + r.time / MINUTE
      if order ~= nil then
        t = t + handling_minutes(order)
      end
    end
    pos = { x = s.stop_x, y = s.stop_y }
    if s.stop_kind == "unload" and order ~= nil then
      if t > order.deadline then
        s.eta_text = "≈ " .. fmt_time(t) .. " — опоздает на " .. math.floor(t - order.deadline) .. " мин"
      else
        s.eta_text = "≈ " .. fmt_time(t) .. " — успевает к " .. fmt_time(order.deadline)
      end
    else
      s.eta_text = "≈ " .. fmt_time(t)
    end
  end
end

local function refresh_eta()
  local car = selected_car()
  if car == nil then return end
  eta_texts(car, stops_of(car))
end

-- События дня ----------------------------------------------------------------------------------

local function restore_roads()
  for _, pair in ipairs(state.closed) do
    pair.road.road_speed = pair.speed
  end
  state.closed = {}
  for _, pair in ipairs(state.jam) do
    pair.road.road_speed = pair.speed
  end
  state.jam = {}
  state.jam_until = nil
end

local function street_names()
  if state.street_names == nil then
    local seen, list = {}, {}
    for _, r in ipairs(find({ has = { "road" }, without = { "bridge" } })) do
      local n = r.name
      if n ~= nil and n ~= "" and not seen[n] and r.road_speed ~= nil then
        seen[n] = true
        list[#list + 1] = n
      end
    end
    table.sort(list)
    state.street_names = list
  end
  return state.street_names
end

local function start_events()
  if math.random() < 0.5 then
    for _, r in ipairs(find({ has = { "road", "bridge" } })) do
      state.jam[#state.jam + 1] = { road = r, speed = r.road_speed }
      r.road_speed = 1.5
    end
    state.jam_until = parse_time("09:00")
    say("Утренняя пробка на мостах до 09:00")
  end
  if math.random() < 0.35 then
    local names = street_names()
    if #names > 0 then
      local name = names[math.random(1, #names)]
      -- Закрываются только второстепенные куски улицы: у главных дорог предела скорости нет.
      for _, r in ipairs(find({ has = { "road" }, without = { "bridge" } })) do
        if r.name == name and r.road_speed ~= nil then
          state.closed[#state.closed + 1] = { road = r, speed = r.road_speed }
          r.road_speed = 0
        end
      end
      say("Ремонт: " .. name .. " закрыта до вечера")
    end
  end
end

local function end_jam(t)
  if state.jam_until ~= nil and t >= state.jam_until then
    for _, pair in ipairs(state.jam) do
      pair.road.road_speed = pair.speed
    end
    state.jam = {}
    state.jam_until = nil
    say("Пробка на мостах рассосалась")
  end
end

-- День ------------------------------------------------------------------------------------------

local function day_text(g)
  local day = math.floor(g.day)
  return "День " .. string.format("%d", day) .. ", " .. WEEKDAYS[((day - 1) % 7) + 1]
end

local function refresh_clock()
  local g = game()
  g.clock_text = day_text(g) .. " · " .. fmt_time(g.minute)
end

local function check_goal()
  local g = game()
  if g.goal_done then return end
  local list = cars()
  if #list < 2 then return end
  for _, c in ipairs(list) do
    if not c.has_driver then return end
  end
  g.goal_done = true
  g.goal_text = "Цель достигнута: две машины и водитель"
  say("Цель достигнута: у фирмы две машины и наёмный водитель!")
end

local function end_day()
  local g = game()
  local wages = 0
  for _, c in ipairs(cars()) do
    if c.has_driver and c.driver ~= "Вы" then
      wages = wages + DRIVER_WAGE
    end
  end
  local payment = math.min(LOAN_PAYMENT, g.debt)
  g.debt = g.debt - payment
  g.money = g.money - payment - wages - OFFICE_RENT
  refresh_money()
  local failed = 0
  for _, o in ipairs(find({ has = { "order", "taken" }, without = { "done" } })) do
    failed = failed + 1
    change_trust(o.client, -10)
  end
  g.summary_text = day_text(g) .. " окончен.\n" ..
    "Доставлено заказов: " .. state.delivered .. ", из них с опозданием: " .. state.late .. ".\n" ..
    "Заработано: " .. fmt_money(state.earned) .. ". Бензин: " .. fmt_money(state.spent_fuel) .. ".\n" ..
    "Платёж по кредиту: " .. fmt_money(payment) .. ". Аренда конторы: " .. fmt_money(OFFICE_RENT) .. (wages > 0 and (". Зарплата водителя: " .. fmt_money(wages)) or "") .. ".\n" ..
    (failed > 0 and ("Не довезли до вечера: " .. failed .. " — клиенты недовольны.\n") or "") ..
    "На счёте: " .. fmt_money(g.money) .. ", долг: " .. fmt_money(g.debt) .. "."
  g.day_over = true
  if g.money < BANKRUPT_AT then
    -- Время не останавливаем: правило конца партии сработает на следующем шаге.
    g.bankrupt = true
    return
  end
  g.time_scale = 0
  g.speed_text = "стоп"
end

local function clear_day()
  for _, o in ipairs(find({ has = { "order" } })) do
    o.done = true
    delete(o)
  end
  for _, s in ipairs(find({ has = { "stop" } })) do
    s.done = true
    delete(s)
  end
  local office = place_point("office")
  for i, c in ipairs(cars()) do
    c.drive_to = nil
    c.breaks_at = nil
    c.load = 0
    c.state = "idle"
    c.state_text = "стоит у конторы"
    c.position = { x = office.x - c.size.x / 2 + (i - 1) * 1.2, y = office.y - c.size.y / 2 }
  end
  restore_roads()
end

local function start_day()
  local g = game()
  g.ticks = 0
  g.minute = DAY_START
  state.last_minute = -1
  state.earned, state.spent_fuel, state.delivered, state.late = 0, 0, 0, 0
  plan_day()
  start_events()
  release_pending(g.minute)
  refresh_clock()
  refresh_money()
end

-- Первый шаг партии: подписи мест, доверие клиентов, расписание первого дня.
local function first_step(g)
  for id, p in pairs(places()) do
    local info = tables.clients[id]
    if info ~= nil then
      p.info_text = info.about .. "\nДоверие: " .. trust_text(id)
    elseif string.sub(id, 1, 5) == "shop_" then
      p.info_text = "Магазин: сюда возят хлеб и товар."
    elseif id == "office" then
      p.info_text = "Ваша контора: здесь стоят машины и начинается день."
    elseif id == "station" then
      p.info_text = "Товарный двор: отсюда грузы уходят поездом."
    elseif id == "site" then
      p.info_text = "Стройка: сюда возят цемент и кирпич."
    else
      p.info_text = ""
    end
  end
  g.started = true
  start_day()
  mark_selected_stops()
  g.time_scale = 0
  g.speed_text = "стоп"
  say("Первый день. Возьмите заказы слева и нажмите ×1, чтобы пошло время")
end

function tick(g)
  if not g.started then
    first_step(g)
  end
  if g.day_over then
    return
  end
  g.ticks = g.ticks + 1
  g.minute = DAY_START + g.ticks / STEPS_PER_MINUTE
  local whole = math.floor(g.minute)
  if whole ~= state.last_minute then
    state.last_minute = whole
    release_pending(g.minute)
    expire_offers(g.minute)
    end_jam(g.minute)
    refresh_clock()
    refresh_money()
    refresh_eta()
    check_goal()
    if g.minute >= DAY_END then
      end_day()
    end
  end
end

-- Щелчок по машине выбирает её, по зданию клиента — показывает, кто это.
function on_clicked(obj)
  obj.clicked = false
  if obj.car then
    select_car(obj)
  elseif obj.place then
    for _, p in ipairs(find({ has = { "place_selected" } })) do
      p.place_selected = false
    end
    obj.place_selected = true
  end
end

-- Кнопки ----------------------------------------------------------------------------------------

function select_car(car)
  for _, c in ipairs(cars()) do
    c.selected = false
  end
  car.selected = true
  mark_selected_stops()
  refresh_eta()
end

function take_order(order)
  if order.taken then return end
  local car = selected_car()
  if car == nil or not car.has_driver then
    for _, c in ipairs(cars()) do
      if c.has_driver then car = c break end
    end
  end
  if car == nil or not car.has_driver then
    say("Некому везти: у машины нет водителя")
    return
  end
  if order.weight > car.capacity then
    say("Не влезет: " .. order.title .. " — " .. fmt_tons(order.weight))
    return
  end
  local list = stops_of(car)
  local seq = (#list > 0) and (list[#list].stop_seq + 1) or 1
  local sel = selected_car()
  if sel == nil then
    car.selected = true
    sel = car
  end
  local shown = sel.car_id == car.car_id
  local load = { stop = true, stop_order = order.order_id, stop_car = car.car_id, stop_seq = seq, stop_kind = "load",
                 stop_x = order.from_x, stop_y = order.from_y, place_name = order.from_name,
                 stop_text = "Загрузка · " .. order.cargo .. " · " .. order.from_name, of_selected = shown }
  local unload = { stop = true, stop_order = order.order_id, stop_car = car.car_id, stop_seq = seq + 1, stop_kind = "unload",
                   stop_x = order.to_x, stop_y = order.to_y, place_name = order.to_name,
                   stop_text = "Выгрузка · " .. order.cargo .. " · " .. order.to_name .. " · до " .. fmt_time(order.deadline),
                   of_selected = shown }
  list[#list + 1] = load
  list[#list + 1] = unload
  eta_texts(car, list)
  create(load)
  create(unload)
  order.taken = true
end

-- Груз в машине по порядку остановок не больше грузоподъёмности, выгрузка — после загрузки.
local function plan_valid(car, list)
  local load = car.load
  local loaded = {}
  for _, s in ipairs(list) do
    local order = order_by_id(s.stop_order)
    if order ~= nil then
      if s.stop_kind == "load" then
        loaded[s.stop_order] = true
        load = load + order.weight
        if load > car.capacity + 1e-6 then
          return false, "перегруз: в машину влезает " .. fmt_tons(car.capacity)
        end
      else
        if not loaded[s.stop_order] and not order.picked then
          return false, "сначала надо загрузить " .. order.title
        end
        load = load - order.weight
      end
    end
  end
  return true
end

function stop_up(stop)
  local car = car_by_id(stop.stop_car)
  if car == nil then return end
  local list = stops_of(car)
  local i = 0
  for k, s in ipairs(list) do
    if s == stop or s.stop_seq == stop.stop_seq then i = k end
  end
  if i <= 1 then return end
  if i == 2 and (car.state == "loading" or car.state == "unloading") then
    say("Машина уже работает на этой остановке")
    return
  end
  local trial = {}
  for k, s in ipairs(list) do trial[k] = s end
  trial[i - 1], trial[i] = trial[i], trial[i - 1]
  local ok, why = plan_valid(car, trial)
  if not ok then
    say("Так нельзя: " .. why)
    return
  end
  local prev = list[i - 1]
  prev.stop_seq, stop.stop_seq = stop.stop_seq, prev.stop_seq
  if i == 2 and car.state == "driving" then
    car.drive_to = { x = stop.stop_x, y = stop.stop_y }
    driving_text(car, stop)
  end
  refresh_eta()
end

function cancel_stop(stop)
  local order = order_by_id(stop.stop_order)
  if order == nil then return end
  if order.picked then
    say("Груз уже в машине — его надо довезти")
    return
  end
  local car = car_by_id(stop.stop_car)
  -- Машина бросает остановку, только если работает на остановке этого заказа или едет к ней.
  local current = car ~= nil and stops_of(car)[1] or nil
  local busy_with_it = current ~= nil and current.stop_order == order.order_id
  for _, s in ipairs(find({ has = { "stop" }, without = { "done" } })) do
    if s.stop_order == order.order_id then
      s.done = true
      delete(s)
    end
  end
  order.taken = false
  refresh_eta()
  change_trust(order.client, -2)
  say("Отказались от заказа: " .. order.title)
  if busy_with_it then
    if car.state == "driving" then
      pay_fuel(car)
    end
    car.drive_to = nil
    car.breaks_at = nil
    car.wait = 0
    car.state = "idle"
    car.state_text = "ждёт заказы"
  end
end

local function set_speed(k, text)
  local g = game()
  if g.day_over then return end
  g.time_scale = k
  g.speed_text = text
end

function speed_stop() set_speed(0, "стоп") end
function speed_1() set_speed(1, "×1") end
function speed_2() set_speed(2, "×2") end
function speed_4() set_speed(4, "×4") end

function toggle_pause()
  local g = game()
  if g.time_scale > 0 then
    state.resume_speed = math.floor(g.time_scale)
    set_speed(0, "стоп")
  else
    local k = state.resume_speed or 1
    set_speed(k, "×" .. string.format("%d", k))
  end
end

function buy_car()
  local g = game()
  if g.money < CAR_PRICE or #cars() >= 2 then return end
  spend(CAR_PRICE)
  g.can_buy = false
  local office = place_point("office")
  local size = tables.templates.gazel.size
  create(tables.templates.gazel, {
    name = "ГАЗель 2", model = "gazel_green", car_id = state.next_car, has_driver = false, driver = "нет водителя",
    position = { office.x + 1.2 - size[1] / 2, office.y - size[2] / 2 },
    state = "idle", state_text = "стоит у конторы — нужен водитель",
  })
  state.next_car = state.next_car + 1
  -- Новая машина появится только после этой функции, поэтому кнопку найма включаем сразу.
  g.can_hire = g.money >= DRIVER_FEE
  say("Куплена вторая ГАЗель. Наймите водителя, чтобы она поехала")
end

function hire_driver()
  local g = game()
  if g.money < DRIVER_FEE then return end
  for _, c in ipairs(cars()) do
    if not c.has_driver then
      c.has_driver = true
      c.driver = "Михалыч"
      spend(DRIVER_FEE)
      c.state_text = "ждёт заказы"
      say("Нанят водитель Михалыч: " .. fmt_money(DRIVER_WAGE) .. " в день")
      return
    end
  end
end

function next_day()
  local g = game()
  if not g.day_over then return end
  clear_day()
  g.day = g.day + 1
  g.day_over = false
  g.time_scale = 0
  g.speed_text = "стоп"
  start_day()
  say("Новый день. Возьмите заказы и нажмите ×1")
end

function hide_help()
  game().show_help = false
end

function close_place()
  for _, p in ipairs(find({ has = { "place_selected" } })) do
    p.place_selected = false
  end
end
