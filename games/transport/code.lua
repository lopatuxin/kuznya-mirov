-- Транспортная компания: рабочий день, заказы клиентов, план машин, езда, загрузка и выгрузка, деньги,
-- доверие клиентов, конкурент и события на дорогах («Фаза-02-первый-час» игры); бензин, износ и ТО машин
-- («Фаза-03-машина»). Движок везёт машины
-- правилом `drive`, считает время и путь по дорогам (`road_route`), рисует списки и кнопки; всё остальное
-- — здесь. Объекты карты (признак `map`) пишет программа перевода карты; код их только читает и меняет
-- им `road_speed` и подписи. Каждая партия перечитывает этот файл, поэтому всё ниже начинает партию заново.

local MINUTE = 3                 -- секунд мира в игровой минуте при скорости ×1
local STEPS_PER_MINUTE = MINUTE * 60  -- шагов движка в игровой минуте
local DAY_START = 7 * 60
local DAY_END = 19 * 60
local CELL_M = 20                -- метров в клетке карты
local FULL_LOAD_SLOWDOWN = 1 / 6 -- насколько медленнее едет машина с полным кузовом: 60 км/ч → 50 км/ч
local TANK_L = 40                -- бак ГАЗ-АА, литров
local FUEL_EMPTY_L100 = 18.6     -- расход пустой, литров на 100 км: 40 литров на 215 км запаса хода
local FUEL_FULL_L100 = 20.5     -- расход с полным кузовом: норма ГАЗ-ММ
local FUEL_PRICE = 62            -- рублей за литр
local FUEL_RESERVE_L = 8         -- меньше — водитель перед поездкой заезжает на заправку
local REFUEL_MINUTES = 10
local TO_INTERVAL_KM = 750       -- ТО ГАЗ-АА: смена масла в моторе по инструкции
local TO_MINUTES = 90
local TO_COST = 6000
local TO_WEAR_DROP = 15          -- на сколько процентных пунктов ТО снижает износ
local WEAR_FLOOR = 25            -- ниже ТО не опускает износ старой машины, %
local WEAR_PER_100KM = 1         -- рост износа, процентных пунктов на 100 км
local OVERDUE_WEAR = 3           -- во сколько раз быстрее изнашивается машина с просроченным ТО
local LOAN_PAYMENT = 2500        -- платёж по кредиту в конце дня
local OFFICE_RENT = 1500         -- аренда конторы за день
local CAR_PRICE = 90000
local SECOND_CAR = { odo = 21000, wear = 30, fuel = 20, to_at = 21300 }  -- подержанная вторая полуторка
local DRIVER_FEE = 5000
local DRIVER_WAGE = 2500
local BANKRUPT_AT = -30000
local REPAIR_COST = 3000
local REPAIR_MINUTES = 40
local MESSAGES_KEPT = 30         -- записей в окне уведомлений
local TOAST_SECONDS = 5          -- настоящих секунд, пока висит всплывающее уведомление
local TOASTS_SHOWN = 3           -- всплывающих уведомлений на экране разом
local WEEKDAYS = { "пн", "вт", "ср", "чт", "пт", "сб", "вс" }

local WINDOWS = { "win_orders", "win_cars", "win_notes", "win_firm" }  -- окна над полосой, открыто не больше одного

local state = {
  game = nil,
  places = nil,
  trust = {},
  last_pos = {},     -- машина → где была на прошлом шаге, для пробега
  kmh = {},          -- машина → скорость на последнем шаге езды
  work_kind = {},    -- машина → «погрузка», «выгрузка», «заправка» или «ТО», пока она стоит на остановке
  pending = {},
  last_minute = -1,
  next_order = 1,
  next_message = 1,
  next_car = 2,
  free_orders = 0,
  closed = {},
  jam = {},
  jam_until = nil,
  street_names = nil,
  earned = 0,
  spent_fuel = 0,
  spent_repair = 0,  -- ремонт после поломок и ТО за день
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

local NBSP = "\u{A0}"             -- неразрывный пробел: перенос строки не рвёт «1 500 ₽»

-- Целое число с неразрывными пробелами между тысячами: 287 412.
local function fmt_int(n)
  local sign = n < 0 and "−" or ""
  local s = tostring(math.floor(math.abs(n) + 0.5))
  local out = ""
  while #s > 3 do
    out = NBSP .. string.sub(s, -3) .. out
    s = string.sub(s, 1, -4)
  end
  return sign .. s .. out
end

local function fmt_money(n)
  return fmt_int(n) .. NBSP .. "₽"
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

-- Места одного вида по началу их `place`: "shop_" — магазины, "fuel_" — заправки, "service_" — автосервисы.
local function place_ids(prefix)
  local list = {}
  for id, _ in pairs(places()) do
    if string.sub(id, 1, #prefix) == prefix then
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

local function refresh_notes()
  local g = game()
  if g.unread > 0 then
    g.notes_text = "Сообщения · " .. math.floor(g.unread)
  else
    g.notes_text = "Сообщения"
  end
end

-- Свободные заказы считает сам код: `find` не видит заказов, созданных в том же шаге, а первый шаг
-- партии создаёт их и сразу останавливает время.
local function change_free_orders(delta)
  state.free_orders = state.free_orders + delta
  game().orders_text = "Заказы · " .. state.free_orders
end

-- Уведомление: запись в окно уведомлений, всплывающее окошко и звук — `cash`, когда пришли деньги,
-- иначе `notify`.
local function say(text, sound)
  local g = game()
  local seq = state.next_message
  state.next_message = seq + 1
  create({ message = true, msg_time = seq, msg_text = fmt_time(g.minute) .. "  " .. text, listed = g.win_notes })
  -- Текст интерфейса движок рисует поверх всех плашек, поэтому, пока посередине подсказка или итог
  -- дня, всплывающих окошек нет: событие остаётся только в окне «Сообщения».
  if not (g.show_help or g.day_over) then
    -- Место новому: старые сверх TOASTS_SHOWN уходят и на паузе, когда шагов нет.
    local shown = find({ has = { "toast" } })
    table.sort(shown, function(a, b) return a.toast_seq > b.toast_seq end)
    for i = TOASTS_SHOWN, #shown do
      delete(shown[i])
    end
    create({ toast = true, toast_seq = seq, toast_text = text, toast_left = TOAST_SECONDS })
  end
  play_sound(sound or "notify")
  if not g.win_notes then
    g.unread = g.unread + 1
    refresh_notes()
  end
  local all = find({ has = { "message" } })
  if #all >= MESSAGES_KEPT then
    table.sort(all, function(a, b) return a.msg_time < b.msg_time end)
    for i = 1, #all - MESSAGES_KEPT + 1 do
      delete(all[i])
    end
  end
end

-- Всплывающие уведомления живут TOAST_SECONDS настоящих секунд при любой скорости: шаг мира при
-- скорости k длится 1/(60·k) секунды. На паузе шагов нет — уведомление ждёт, пока время пойдёт или
-- его закроют щелчком. Сверх TOASTS_SHOWN старые уходят сразу.
local function age_toasts(g)
  if g.time_scale <= 0 then return end
  local list = find({ has = { "toast" } })
  table.sort(list, function(a, b) return a.toast_seq > b.toast_seq end)
  local dt = 1 / (60 * g.time_scale)
  for i, t in ipairs(list) do
    t.toast_left = t.toast_left - dt
    if i > TOASTS_SHOWN or t.toast_left <= 0 then
      delete(t)
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

-- Карточка места: первой строкой улица, где оно стоит, — в имени места её нет.
local function set_place_info(p, text)
  if p.street ~= nil and p.street ~= "" then
    text = p.street .. (text ~= "" and "\n" .. text or "")
  end
  p.info_text = text
end

local function change_trust(client, delta)
  state.trust[client] = clamp(trust(client) + delta, 0, 100)
  local p = places()[client]
  if p ~= nil then
    set_place_info(p, tables.clients[client].about .. "\nДоверие: " .. trust_text(client))
  end
end

-- Цена с поправкой на доверие: постоянному клиенту выгоднее платить, недовольный торгуется.
local function price_factor(client)
  return 0.85 + trust(client) / 200
end

-- Деньги ---------------------------------------------------------------------------------------

-- Зарплата наёмных водителей за день; хозяин возит сам и себе не платит.
local function daily_wages()
  local wages = 0
  for _, c in ipairs(cars()) do
    if c.has_driver and c.driver ~= "Вы" then
      wages = wages + DRIVER_WAGE
    end
  end
  return wages
end

-- Деньги, кнопки покупки и найма и текст окна «Фирма».
-- `bought` — машин без водителя, купленных в этом же шаге: `find` их ещё не видит.
local function refresh_money(bought)
  local g = game()
  g.money_text = fmt_money(g.money)
  g.debt_text = fmt_money(g.debt)
  local count = #cars() + (bought or 0)
  local has_spare_car = (bought or 0) > 0
  for _, c in ipairs(cars()) do
    if not c.has_driver then has_spare_car = true end
  end
  g.can_buy = (g.money >= CAR_PRICE) and count < 2
  g.can_hire = has_spare_car and g.money >= DRIVER_FEE
  g.firm_alert = g.can_buy or g.can_hire
  local wages = daily_wages()
  local text = "На счёте " .. fmt_money(g.money) .. ", долг по кредиту " .. fmt_money(g.debt) .. ".\n" ..
    "Каждый вечер: платёж по кредиту " .. fmt_money(math.min(LOAN_PAYMENT, g.debt)) ..
    ", аренда конторы " .. fmt_money(OFFICE_RENT) ..
    (wages > 0 and (", зарплата водителя " .. fmt_money(wages)) or "") .. "."
  if count < 2 and not g.can_buy then
    text = text .. "\nВторая полуторка — " .. fmt_money(CAR_PRICE) .. ": не хватает " .. fmt_money(CAR_PRICE - g.money) .. "."
  elseif has_spare_car and not g.can_hire then
    text = text .. "\nВодитель — " .. fmt_money(DRIVER_FEE) .. ": не хватает " .. fmt_money(DRIVER_FEE - g.money) .. "."
  end
  g.firm_text = text
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
    local list = place_ids("shop_")
    return place_point(list[math.random(1, #list)])
  end
  return place_point(kind)
end

local TAG_TEXT = { urgent = "срочно", careful = "бережно", opening = "к открытию" }

-- Создаёт заказ клиента. `announce` — показать уведомление: заказы начала дня
-- уже названы уведомлением о новом дне.
local function new_order(client, job, announce)
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
  fields.listed = game().win_orders
  state.next_order = state.next_order + 1
  create(fields)
  change_free_orders(1)
  if announce then
    say("Новый заказ: " .. fields.title .. ", до " .. fmt_time(deadline))
  end
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

local function release_pending(t, announce)
  while #state.pending > 0 and state.pending[1].at <= t do
    local p = table.remove(state.pending, 1)
    new_order(p.client, p.job, announce)
  end
end

-- Свободный заказ, который висит дольше, чем клиент готов ждать, забирает конкурент.
local function expire_offers(t)
  for _, o in ipairs(find({ has = { "order" }, without = { "taken", "done" } })) do
    if o.offer_until <= t then
      say("Сидоров забрал заказ: " .. o.title)
      o.done = true
      delete(o)
      change_free_orders(-1)
    end
  end
end

-- Машины ---------------------------------------------------------------------------------------

local function handling_minutes(order)
  local m = 10 + 8 * order.weight
  if order.careful then m = m * 1.5 end
  return m
end

-- Сколько машина стоит на остановке: погрузка и выгрузка — по грузу, заправка и ТО — своё время.
local function stop_minutes(stop, order)
  if stop.stop_kind == "fuel" then return REFUEL_MINUTES end
  if stop.stop_kind == "service" then return TO_MINUTES end
  return order and handling_minutes(order) or 5
end

local function car_center(car)
  return { x = car.position.x + car.size.x / 2, y = car.position.y + car.size.y / 2 }
end

-- Скорость машины с грузом `load`: пустая едет со скоростью из шаблона полуторки (60 км/ч), с полным
-- кузовом — на шестую часть медленнее, 50 км/ч: гружёная медленнее разгоняется. Где предел улицы ниже,
-- машина едет по пределу.
local function speed_for(car, load)
  return tables.templates.polutorka.drive_speed * (1 - FULL_LOAD_SLOWDOWN * clamp(load / car.capacity, 0, 1))
end

-- Расход бензина с грузом `load`, литров на километр: от 18,6 пустой до 20,5 с полным кузовом на 100 км.
local function fuel_per_km(car, load)
  local share = clamp(load / car.capacity, 0, 1)
  return (FUEL_EMPTY_L100 + (FUEL_FULL_L100 - FUEL_EMPTY_L100) * share) / 100
end

-- План выбранной машины виден, пока открыто окно «Заказы».
local function mark_selected_stops()
  local sel = selected_car()
  local open = game().win_orders
  for _, s in ipairs(find({ has = { "stop" } })) do
    s.of_selected = open and sel ~= nil and s.stop_car == sel.car_id
  end
end

-- Строки машин в окнах «Заказы» и «Машины»: список движка нельзя спрятать целиком, поэтому строки
-- закрытого окна прячет признак.
local function mark_car_rows()
  local g = game()
  for _, c in ipairs(cars()) do
    c.row_o = g.win_orders
    c.row_c = g.win_cars
  end
end

-- Когда ТО: через сколько километров или на сколько оно просрочено.
local function to_text(car)
  local left = car.to_at - car.odo
  if left >= 0 then
    return "ТО через " .. fmt_int(left) .. " км"
  end
  return "ТО просрочено на " .. fmt_int(-left) .. " км"
end

-- Карточка машины в окне «Машины»: груз, скорость, бензин, пробег, ТО, износ и подпись кнопки слежения.
local function refresh_cards()
  local g = game()
  for _, c in ipairs(cars()) do
    local speed = "стоит"
    if c.state == "driving" then
      speed = fmt_int(state.kmh[c.car_id] or 0) .. " км/ч"
    end
    c.card_text = "Груз: " .. fmt_tons(c.load) .. " из " .. fmt_tons(c.capacity) .. " · скорость: " .. speed .. "\n" ..
      "Бензин: " .. fmt_int(c.fuel) .. " л из " .. TANK_L .. " · пробег: " .. fmt_int(c.odo) .. " км\n" ..
      to_text(c) .. " · износ: " .. fmt_int(c.wear) .. "%"
    c.selected_mark = c.selected and " · выбрана" or ""
    c.follow_text = (c.selected and g.follow_on) and "Следить: да" or "Следить: нет"
  end
end

-- Пробег, бензин, износ и скорость: путь машины — сумма её сдвигов за шаги, так он идёт по улицам,
-- а не по прямой. С просроченным ТО машина изнашивается втрое быстрее.
local function measure_drive(car)
  local here = car_center(car)
  local id = car.car_id
  local last = state.last_pos[id]
  state.last_pos[id] = here
  if last == nil then return end
  local km = math.sqrt((here.x - last.x) ^ 2 + (here.y - last.y) ^ 2) * CELL_M / 1000
  car.odo = car.odo + km
  car.fuel = math.max(0, car.fuel - km * fuel_per_km(car, car.load))
  local rate = WEAR_PER_100KM * ((car.odo > car.to_at) and OVERDUE_WEAR or 1)
  car.wear = math.min(100, car.wear + km / 100 * rate)
  -- Скорость в игровых км/ч: путь за шаг, умноженный на число шагов в игровом часе.
  state.kmh[id] = km * STEPS_PER_MINUTE * 60
end

-- Работа на остановке — погрузка, выгрузка, заправка, ТО: полоска над машиной растёт от нуля до полной,
-- подпись — сколько минут осталось. Признак `working` стоит, пока машина работает на остановке.
local function update_work(car)
  local left = math.max(0, car.wait)
  car.work_done = car.work_total - left
  car.work_text = state.work_kind[car.car_id] .. " · " .. math.ceil(left / MINUTE) .. " мин"
end

local function start_work(car, kind, minutes)
  car.wait = minutes * MINUTE
  car.work_total = minutes * MINUTE
  car.working = true
  state.work_kind[car.car_id] = kind
  update_work(car)
end

local function stop_work(car)
  car.working = false
end

-- Камера следит за выбранной машиной, пока слежение включено. Запись `camera_follows` возвращает
-- слежение и после того, как игрок сдвинул карту и камера отпустила машину.
local function follow_selected()
  local on = game().follow_on
  local sel = selected_car()
  for _, c in ipairs(cars()) do
    c.camera_follows = on and sel ~= nil and c.car_id == sel.car_id
  end
end

-- Подпись машины в пути: куда и зачем едет.
local function driving_text(car, stop)
  if stop.stop_kind == "fuel" then
    car.state_text = "едет заправиться — " .. stop.place_name
  elseif stop.stop_kind == "service" then
    car.state_text = "едет на ТО — " .. stop.place_name
  else
    local order = order_by_id(stop.stop_order)
    local verb = stop.stop_kind == "load" and "едет за грузом: " or "везёт: "
    car.state_text = verb .. (order and order.title or "") .. " — " .. stop.place_name
  end
end

-- Ближайшее по дорогам место из списка `ids` от точки `from`.
local function nearest_place(ids, from, speed)
  local best, best_length = nil, nil
  for _, id in ipairs(ids) do
    local p = place_point(id)
    local r = road_route(from, { x = p.x, y = p.y }, speed)
    if best == nil or r.length < best_length then
      best, best_length = p, r.length
    end
  end
  return best
end

-- Запись остановки без заказа — заправки (`fuel`) или ТО (`service`) — в план машины под номером `seq`;
-- создаёт её вызывающий: `find` увидит остановку только со следующего шага.
local function service_stop(car, kind, point, seq)
  local sel = selected_car()
  local title = kind == "fuel" and "Заправка" or "ТО"
  return { stop = true, stop_order = 0, stop_car = car.car_id, stop_seq = seq, stop_kind = kind,
           stop_x = point.x, stop_y = point.y, place_name = point.name,
           stop_text = title .. " · " .. point.name, eta_text = "",
           of_selected = game().win_orders and sel ~= nil and sel.car_id == car.car_id }
end

-- Вероятность поломки в поездке растёт с износом: при 45% — 2%, при 80% — 6%.
local function breakdown_chance(car)
  local w = car.wear / 100
  return 0.005 + 0.08 * w * w
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
  -- Бензина мало — водитель сам заезжает на заправку, прежде чем ехать дальше. Заправка уже в плане,
  -- но игрок поднял над ней другие остановки — водитель возвращает её первой, а не ставит вторую.
  if car.fuel < FUEL_RESERVE_L and first.stop_kind ~= "fuel" then
    local planned = nil
    for _, s in ipairs(list) do
      if s.stop_kind == "fuel" then planned = s end
    end
    if planned ~= nil then
      planned.stop_seq = first.stop_seq - 1
      first = planned
    else
      local station = nearest_place(place_ids("fuel_"), car_center(car), car.drive_speed)
      first = service_stop(car, "fuel", station, first.stop_seq - 1)
      create(first)
    end
  end
  car.state = "driving"
  car.drive_to = { x = first.stop_x, y = first.stop_y }
  driving_text(car, first)
  if math.random() < breakdown_chance(car) then
    car.breaks_at = now() + 2 + math.random() * 8
  end
end

local function arrive(car)
  car.breaks_at = nil
  local first = stops_of(car)[1]
  if first == nil then
    go_next(car)
    return
  end
  local order = order_by_id(first.stop_order)
  local minutes = stop_minutes(first, order)
  if first.stop_kind == "load" then
    car.state = "loading"
    car.state_text = "грузит: " .. (order and order.title or "") .. ", " .. math.floor(minutes) .. " мин"
    start_work(car, "погрузка", minutes)
  elseif first.stop_kind == "unload" then
    car.state = "unloading"
    car.state_text = "выгружает: " .. (order and order.title or "") .. ", " .. math.floor(minutes) .. " мин"
    start_work(car, "выгрузка", minutes)
  elseif first.stop_kind == "fuel" then
    car.state = "fueling"
    car.state_text = "заправляется: " .. first.place_name
    start_work(car, "заправка", minutes)
  else
    car.state = "servicing"
    car.state_text = "на ТО: " .. first.place_name .. ", " .. TO_MINUTES .. " мин"
    start_work(car, "ТО", minutes)
  end
end

local function deliver(car, order)
  local late = now() - order.deadline
  local paid = order.price
  local client = order.client
  if late <= 0 then
    change_trust(client, 6)
    say(tables.clients[client].short .. ": вовремя, +" .. fmt_money(paid), "cash")
  else
    local cut = clamp(late / 60 * 0.5, 0, 0.6)
    paid = math.floor(order.price * (1 - cut) / 10 + 0.5) * 10
    local penalty = (order.opening or late > 30) and -14 or -5
    change_trust(client, penalty)
    state.late = state.late + 1
    say(tables.clients[client].short .. ": опоздали на " .. math.floor(late) .. " мин, заплатили " .. fmt_money(paid), "cash")
  end
  state.delivered = state.delivered + 1
  state.earned = state.earned + paid
  local g = game()
  g.money = g.money + paid
  refresh_money()
  order.done = true
  delete(order)
end

-- Заправка: бак до полного, фирма платит за литры.
local function refuel(car)
  local liters = TANK_L - car.fuel
  local cost = math.floor(liters * FUEL_PRICE + 0.5)
  car.fuel = TANK_L
  state.spent_fuel = state.spent_fuel + cost
  spend(cost)
  say(car.name .. " заправилась: " .. fmt_int(liters) .. " л, " .. fmt_money(cost))
end

-- ТО: износ ниже, но не ниже, чем позволяет возраст машины; следующее ТО — через TO_INTERVAL_KM.
local function service(car)
  car.wear = math.max(math.min(car.wear, WEAR_FLOOR), car.wear - TO_WEAR_DROP)
  car.to_at = car.odo + TO_INTERVAL_KM
  state.spent_repair = state.spent_repair + TO_COST
  spend(TO_COST)
  say(car.name .. " прошла ТО: " .. fmt_money(TO_COST) .. ", износ " .. fmt_int(car.wear) .. "%, следующее через " ..
    fmt_int(TO_INTERVAL_KM) .. " км")
end

local function finish_stop(car)
  local first = stops_of(car)[1]
  if first ~= nil then
    if first.stop_kind == "fuel" then
      refuel(car)
    elseif first.stop_kind == "service" then
      service(car)
    else
      local order = order_by_id(first.stop_order)
      if order ~= nil then
        if first.stop_kind == "load" then
          order.picked = true
          car.load = car.load + order.weight
        else
          car.load = math.max(0, car.load - order.weight)
          deliver(car, order)
        end
        car.drive_speed = speed_for(car, car.load)
      end
    end
    first.done = true
    delete(first)
  end
  stop_work(car)
  car.state = "idle"
  go_next(car)
end

local function break_down(car)
  car.breaks_at = nil
  car.state = "broken"
  car.drive_to = nil
  car.wait = REPAIR_MINUTES * MINUTE
  car.state_text = "сломалась, ремонт " .. REPAIR_MINUTES .. " мин"
  state.spent_repair = state.spent_repair + REPAIR_COST
  spend(REPAIR_COST)
  say(car.name .. " сломалась: ремонт " .. REPAIR_MINUTES .. " мин и " .. fmt_money(REPAIR_COST) ..
    ", износ " .. fmt_int(car.wear) .. "%")
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
    measure_drive(car)
    if car.drive_to == nil then
      arrive(car)
    elseif car.breaks_at ~= nil and now() >= car.breaks_at then
      break_down(car)
    end
  elseif car.working then
    if car.wait <= 0 then
      finish_stop(car)
    else
      update_work(car)
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
  if car.working or car.state == "broken" then
    t = t + math.max(0, car.wait) / MINUTE
  end
  local pos = car_center(car)
  local load = car.load
  for i, s in ipairs(list) do
    local order = order_by_id(s.stop_order)
    local busy = (i == 1 and car.working)
    if not busy then
      -- Отрезок до остановки машина едет с тем грузом, что в кузове после прошлых остановок.
      local r = road_route(pos, { x = s.stop_x, y = s.stop_y }, speed_for(car, load))
      t = t + r.time / MINUTE + stop_minutes(s, order)
    end
    if order ~= nil then
      load = load + (s.stop_kind == "load" and order.weight or -order.weight)
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

-- Улицы, которые может закрыть ремонт: только целиком второстепенные — у улицы с главными кусками
-- закрылся бы короткий обрывок, а сообщение говорило бы обо всей улице.
local function street_names()
  if state.street_names == nil then
    local seen, list = {}, {}
    for _, r in ipairs(find({ has = { "road", "main_road" } })) do
      if r.name ~= nil then seen[r.name] = true end
    end
    for _, r in ipairs(find({ has = { "road" }, without = { "bridge", "main_road" } })) do
      local n = r.name
      if n ~= nil and n ~= "" and not seen[n] then
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
      -- Закрываются только второстепенные куски улицы: главные дороги (`main_road`) ремонт не перекрывает.
      for _, r in ipairs(find({ has = { "road" }, without = { "bridge", "main_road" } })) do
        if r.name == name then
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

-- Окна над полосой: кнопка полосы открывает своё окно и закрывает открытое, повторный щелчок закрывает.
-- Список движка нельзя спрятать целиком, поэтому строки закрытого окна прячут признаки `listed`,
-- `row_o`, `row_c` и `of_selected`, которые код ставит строкам окна.
local function set_window(name)
  local g = game()
  for _, w in ipairs(WINDOWS) do
    g[w] = (w == name)
  end
  for _, o in ipairs(find({ has = { "order" } })) do
    o.listed = g.win_orders
  end
  for _, m in ipairs(find({ has = { "message" } })) do
    m.listed = g.win_notes
  end
  mark_car_rows()
  mark_selected_stops()
  if g.win_notes then
    g.unread = 0
    refresh_notes()
  end
end

local function open_window(name)
  local g = game()
  -- Пока посередине подсказка или итог дня, окна не открываются: их текст лёг бы поверх.
  if g.show_help or g.day_over then return end
  if g[name] then
    set_window(nil)
  else
    set_window(name)
  end
end

-- Скорость времени: кнопка текущей скорости в полосе подсвечена признаком.
local SPEED_FLAGS = { [0] = "spd_0", [1] = "spd_1", [2] = "spd_2", [4] = "spd_4" }

local function show_speed(g, k)
  g.time_scale = k
  for speed, flag in pairs(SPEED_FLAGS) do
    g[flag] = (speed == k)
  end
end

local function stop_time(g)
  show_speed(g, 0)
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
  local wages = daily_wages()
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
    "Заработано: " .. fmt_money(state.earned) .. ". Бензин: " .. fmt_money(state.spent_fuel) ..
    (state.spent_repair > 0 and (". Ремонт и ТО: " .. fmt_money(state.spent_repair)) or "") .. ".\n" ..
    "Платёж по кредиту: " .. fmt_money(payment) .. ". Аренда конторы: " .. fmt_money(OFFICE_RENT) .. (wages > 0 and (". Зарплата водителя: " .. fmt_money(wages)) or "") .. ".\n" ..
    (failed > 0 and ("Не довезли до вечера: " .. failed .. " — клиенты недовольны.\n") or "") ..
    "На счёте: " .. fmt_money(g.money) .. ", долг: " .. fmt_money(g.debt) .. "."
  g.day_over = true
  -- Итог дня — посередине экрана: окна и всплывающие окошки закрываются, иначе их текст лёг бы поверх.
  set_window(nil)
  for _, t in ipairs(find({ has = { "toast" } })) do
    delete(t)
  end
  if g.money < BANKRUPT_AT then
    -- Время не останавливаем: правило конца партии сработает на следующем шаге.
    g.bankrupt = true
    return
  end
  stop_time(g)
end

local function clear_day()
  for _, o in ipairs(find({ has = { "order" } })) do
    o.done = true
    delete(o)
  end
  for _, s in ipairs(find({ has = { "stop" } })) do
    if s.stop_kind == "service" then
      say("ТО не успели до вечера: " .. car_by_id(s.stop_car).name .. " — поставьте его снова")
    end
    s.done = true
    delete(s)
  end
  change_free_orders(-state.free_orders)
  local office = place_point("office")
  -- Машины встают у конторы скачком: прошлое место не годится для счёта пробега.
  state.last_pos = {}
  for i, c in ipairs(cars()) do
    c.drive_to = nil
    c.breaks_at = nil
    c.load = 0
    c.drive_speed = speed_for(c, 0)
    stop_work(c)
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
  state.earned, state.spent_fuel, state.spent_repair, state.delivered, state.late = 0, 0, 0, 0, 0
  plan_day()
  start_events()
  release_pending(g.minute, false)
  refresh_clock()
  refresh_money()
end

-- Первый шаг партии: подписи мест, доверие клиентов, расписание первого дня.
local function first_step(g)
  for id, p in pairs(places()) do
    local info = tables.clients[id]
    local text = ""
    if info ~= nil then
      text = info.about .. "\nДоверие: " .. trust_text(id)
    elseif string.sub(id, 1, 5) == "shop_" then
      text = "Магазин: сюда возят хлеб и товар."
    elseif string.sub(id, 1, 5) == "fuel_" then
      text = "Заправка: водитель сам заезжает сюда, когда в баке меньше " .. FUEL_RESERVE_L ..
        " литров. Бензин — " .. fmt_money(FUEL_PRICE) .. " за литр."
    elseif string.sub(id, 1, 8) == "service_" then
      text = "Автосервис: ТО за " .. fmt_money(TO_COST) .. " и полтора часа. Ставится кнопкой «На ТО» в карточке машины."
    elseif id == "office" then
      text = "Ваша контора: здесь стоят машины и начинается день."
    elseif id == "station" then
      text = "Товарный двор: отсюда грузы уходят поездом."
    elseif id == "site" then
      text = "Стройка: сюда возят цемент и кирпич."
    end
    set_place_info(p, text)
  end
  g.started = true
  mark_car_rows()
  refresh_notes()
  start_day()
  mark_selected_stops()
  follow_selected()
  refresh_cards()
  stop_time(g)
  say("Первый день. Возьмите заказы в окне «Заказы» и нажмите ×1")
end

function tick(g)
  if not g.started then
    first_step(g)
  end
  age_toasts(g)
  if g.day_over then
    return
  end
  g.ticks = g.ticks + 1
  g.minute = DAY_START + g.ticks / STEPS_PER_MINUTE
  local whole = math.floor(g.minute)
  if whole ~= state.last_minute then
    state.last_minute = whole
    release_pending(g.minute, true)
    expire_offers(g.minute)
    end_jam(g.minute)
    refresh_clock()
    refresh_money()
    refresh_eta()
    refresh_cards()
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
  follow_selected()
  refresh_eta()
  refresh_cards()
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
    follow_selected()
    refresh_cards()
  end
  local shown = game().win_orders and sel.car_id == car.car_id
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
  change_free_orders(-1)
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
  if i == 2 and car.working then
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

-- Машина бросает текущую остановку и встаёт; со следующего шага едет по тому, что осталось в плане.
local function drop_current(car)
  car.drive_to = nil
  car.breaks_at = nil
  car.wait = 0
  stop_work(car)
  car.state = "idle"
  car.state_text = "ждёт заказы"
end

function cancel_stop(stop)
  local car = car_by_id(stop.stop_car)
  local current = car ~= nil and stops_of(car)[1] or nil
  if stop.stop_kind == "fuel" then
    say("Когда заправляться, водитель решает сам")
    return
  end
  if stop.stop_kind == "service" then
    -- ТО — решение владельца: его можно убрать из плана, даже когда машина уже в сервисе.
    local busy_with_it = current ~= nil and current.stop_seq == stop.stop_seq
    stop.done = true
    delete(stop)
    refresh_eta()
    say("ТО убрано из плана: " .. car.name)
    if busy_with_it and car.state ~= "broken" then
      drop_current(car)
    end
    return
  end
  local order = order_by_id(stop.stop_order)
  if order == nil then return end
  if order.picked then
    say("Груз уже в машине — его надо довезти")
    return
  end
  -- Машина бросает остановку, только если работает на остановке этого заказа или едет к ней.
  local busy_with_it = current ~= nil and current.stop_order == order.order_id
  for _, s in ipairs(find({ has = { "stop" }, without = { "done" } })) do
    if s.stop_order == order.order_id then
      s.done = true
      delete(s)
    end
  end
  order.taken = false
  change_free_orders(1)
  refresh_eta()
  change_trust(order.client, -2)
  say("Отказались от заказа: " .. order.title)
  -- Сломанная машина остаётся в ремонте: после него поедет по тому, что осталось в плане.
  if busy_with_it and car.state ~= "broken" then
    drop_current(car)
  end
end

-- «На ТО» в карточке машины: остановка в ближайшем к машине автосервисе — в конец её плана; поднять её
-- выше можно, как любую остановку.
function service_car(car)
  if not car.has_driver then
    say("Некому вести машину на ТО: у неё нет водителя")
    return
  end
  local list = stops_of(car)
  for _, s in ipairs(list) do
    if s.stop_kind == "service" then
      say("ТО уже в плане: " .. car.name)
      return
    end
  end
  local seq = (#list > 0) and (list[#list].stop_seq + 1) or 1
  local garage = nearest_place(place_ids("service_"), car_center(car), car.drive_speed)
  local stop = service_stop(car, "service", garage, seq)
  list[#list + 1] = stop
  eta_texts(car, list)
  create(stop)
  say("ТО поставлено в план: " .. car.name .. " — " .. garage.name .. ", полтора часа и " .. fmt_money(TO_COST))
end

local function set_speed(k)
  local g = game()
  if g.day_over or g.show_help then return end
  show_speed(g, k)
end

function speed_stop() set_speed(0) end
function speed_1() set_speed(1) end
function speed_2() set_speed(2) end
function speed_4() set_speed(4) end

function toggle_pause()
  local g = game()
  if g.time_scale > 0 then
    state.resume_speed = math.floor(g.time_scale)
    set_speed(0)
  else
    set_speed(state.resume_speed or 1)
  end
end

function buy_car()
  local g = game()
  if g.money < CAR_PRICE or #cars() >= 2 then return end
  spend(CAR_PRICE)
  local office = place_point("office")
  local size = tables.templates.polutorka.size
  create(tables.templates.polutorka, {
    name = "Полуторка 2", model = "gaz_aa", car_id = state.next_car, has_driver = false, driver = "нет водителя",
    position = { office.x + 1.2 - size[1] / 2, office.y - size[2] / 2 },
    odo = SECOND_CAR.odo, wear = SECOND_CAR.wear, fuel = SECOND_CAR.fuel, to_at = SECOND_CAR.to_at,
    state = "idle", state_text = "стоит у конторы — нужен водитель", row_o = g.win_orders, row_c = g.win_cars,
    card_text = "", selected_mark = "", follow_text = "Следить: нет",
  })
  state.next_car = state.next_car + 1
  -- Новая машина появится только после этой функции: кнопку найма и окно «Фирма» считаем с ней.
  refresh_money(1)
  say("Куплена вторая полуторка. Наймите водителя в окне «Фирма», чтобы она поехала")
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
  stop_time(g)
  -- Окно — до заказов дня: `find` не видит заказов, созданных в том же шаге, и не открыл бы их строки.
  set_window("win_orders")
  start_day()
  refresh_cards()
  say("Новый день. Возьмите заказы и нажмите ×1")
end

-- «Понятно» в подсказке: партия начинается с открытого окна «Заказы».
function hide_help()
  game().show_help = false
  set_window("win_orders")
end

function open_orders() open_window("win_orders") end
function open_cars() open_window("win_cars") end
function open_notes() open_window("win_notes") end
function open_firm() open_window("win_firm") end

function close_toast(toast)
  delete(toast)
end

-- «Следить» в карточке машины: у выбранной машины переключает слежение камеры, другую машину
-- выбирает и ставит под камеру.
function follow_car(car)
  local g = game()
  if car.selected then
    g.follow_on = not g.follow_on
    follow_selected()
    refresh_cards()
    return
  end
  g.follow_on = true
  select_car(car)
end

-- «План» в карточке машины: выбирает её и открывает окно «Заказы» с её планом.
function plan_car(car)
  select_car(car)
  set_window("win_orders")
end

function close_place()
  for _, p in ipairs(find({ has = { "place_selected" } })) do
    p.place_selected = false
  end
end
