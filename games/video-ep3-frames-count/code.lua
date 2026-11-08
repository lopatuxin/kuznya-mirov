-- Сколько кадров у Ильи — для третьего ролика, фраза «Для бега и прыжка у Ильи четыре кадра стойки,
-- восемь кадров бега и три кадра прыжка, а для боя потом добавились ещё двадцать два». Та же раскладка
-- всех 37 кадров, что в сцене кадров, посередине кадра: с первым словом волной выскакивают тусклые
-- кадры; на «четыре», «восемь», «три», «двадцать» своя группа проступает в цвете, рамка карточки горит
-- золотом, в шапке выскакивает золотой квадрат с числом 4, 8, 3 или 22 и подпись группы; названная
-- раньше группа горит белым. Бой — все 22 кадра боя одной карточкой. Сцена начинается с первого слова.
-- Моменты расчётные, пока голос не записан: 135 слов в минуту, слово номер i (с нуля) — i × 0,444 с.
-- Привязка: four — «четыре», eight — «восемь», three — «три», fight — «двадцать». После записи голоса
-- сюда ставятся настоящие моменты, и сцена записывается заново.
local T = {
  four = 1.92,
  eight = 3.17,
  three = 4.7,
  fight = 8.34,
}

local CLOCK_START = 1000

-- Раскладка выскакивает с первым словом фразы, то есть с началом сцены.
local APPEAR_AT = 0.05

local LIGHT = { [1] = "four", [2] = "eight", [3] = "three", [4] = "fight" }
local CAPTIONS = { "стойка", "бег", "прыжок", "бой" }
local NUMBERS = { "4", "8", "3", "22" }

-- Кадры выскакивают по одному с шагом APPEAR_STEP по порядку листа, карточка — чуть раньше своих кадров.
local APPEAR_STEP = 0.015
local CARD_LEAD = 0.05
-- Группа проступает в цвете волной: кадр за кадром с шагом LIGHT_STEP, каждый за LIGHT_TIME,
-- и чуть подпрыгивает на BUMP.
local LIGHT_STEP = 0.03
local LIGHT_TIME = 0.25
local BUMP = 0.07
-- Квадрат с числом выскакивает с перелётом, число в нём появляется через LABEL_DELAY.
local LABEL_DELAY = 0.05
-- Подпись проступает за CAPTION_TIME, поднимаясь на CAPTION_RISE клетки.
local CAPTION_TIME = 0.2
local CAPTION_RISE = 0.3
local FADE_STEPS = 5
local WHITE_TONE = 6

local WAITING, CURRENT, DONE = 0, 1, 2
-- Обод карточки по размеру карточки (её свойство rim) и состоянию группы.
local RIMS = {
  ["286x488"] = { [WAITING] = "rim_wait_286x488", [CURRENT] = "rim_cur_286x488", [DONE] = "rim_done_286x488" },
  ["526x488"] = { [WAITING] = "rim_wait_526x488", [CURRENT] = "rim_cur_526x488", [DONE] = "rim_done_526x488" },
  ["1534x488"] = { [WAITING] = "rim_wait_1534x488", [CURRENT] = "rim_cur_1534x488", [DONE] = "rim_done_1534x488" },
}
local BADGE_IMAGE = { [CURRENT] = "rhomb_lit", [DONE] = "rhomb_done" }
local HIDDEN = { x = -100, y = -100 }

local function clamp01(u)
  if u < 0 then return 0 end
  if u > 1 then return 1 end
  return u
end

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function ease_out(u)
  return 1 - (1 - u) * (1 - u)
end

local function pop_scale(u)
  if u < 0.12 then return lerp(0.3, 1.1, u / 0.12) end
  if u < 0.22 then return lerp(1.1, 1, (u - 0.12) / 0.1) end
  return 1
end

local function light_at(group)
  return T[LIGHT[group]]
end

-- Ждёт, горит золотом, пока не назовут следующую группу, потом горит белым.
local function state_of(group, t)
  local at = light_at(group)
  if t < at then return WAITING end
  local next_group = LIGHT[group + 1]
  if next_group ~= nil and t >= T[next_group] then return DONE end
  return CURRENT
end

-- Первый кадр группы: от него считается, когда выскакивает карточка и когда начинается волна цвета.
local first_frame = {}

local function put(p, r)
  p.size = { x = r.w, y = r.h }
  p.position = { x = r.x, y = r.y }
end

-- Прямоугольник, увеличенный в s раз вокруг точки c.
local function scaled(r, c, s)
  return { x = c.x + (r.x - c.x) * s, y = c.y + (r.y - c.y) * s, w = r.w * s, h = r.h * s }
end

local function center(r)
  return { x = r.x + r.w / 2, y = r.y + r.h / 2 }
end

local home = nil

local function key_of(p)
  return p.part .. ":" .. (p.group or "") .. ":" .. (p.frame or "")
end

local function remember(parts)
  home = {}
  for i = 1, #parts do
    local p = parts[i]
    home[key_of(p)] = { x = p.position.x, y = p.position.y, w = p.size.x, h = p.size.y }
    if p.frame ~= nil then
      local f = first_frame[p.group]
      if f == nil or p.frame < f then first_frame[p.group] = p.frame end
    end
  end
end

local function frame_appear(frame)
  return APPEAR_AT + frame * APPEAR_STEP
end

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "part" } })
  if home == nil then remember(parts) end

  for i = 1, #parts do
    local p = parts[i]
    local h = home[key_of(p)]
    local at = light_at(p.group)
    if p.part == "card" or p.part == "edge" then
      local appear = frame_appear(first_frame[p.group]) - CARD_LEAD
      if t < appear then
        p.position = HIDDEN
      else
        local card = home["card:" .. p.group .. ":"]
        put(p, scaled(h, center(card), pop_scale(t - appear)))
        if p.part == "edge" then p.image = RIMS[p.rim][state_of(p.group, t)] end
      end
    elseif p.part == "dim" or p.part == "lit" then
      local appear = frame_appear(p.frame)
      if t < appear then
        p.position = HIDDEN
      else
        local r = scaled(h, center(h), pop_scale(t - appear))
        local start = at + (p.frame - first_frame[p.group]) * LIGHT_STEP
        -- Кадр чуть подпрыгивает, стоя ногами на месте.
        local u = clamp01((t - start) / (LIGHT_TIME * 1.6))
        local feet = { x = h.x + h.w / 2, y = h.y + h.h }
        put(p, scaled(r, feet, 1 + BUMP * math.sin(math.pi * u)))
        if p.part == "lit" then p.opacity = clamp01((t - start) / LIGHT_TIME) end
      end
    elseif p.part == "badge" then
      if t < at then
        p.position = HIDDEN
      else
        put(p, scaled(h, center(h), pop_scale(t - at)))
        p.image = BADGE_IMAGE[state_of(p.group, t)]
      end
    elseif p.part == "num" then
      if t < at + LABEL_DELAY then
        p.num = ""
      else
        p.num = NUMBERS[p.group]
      end
    elseif p.part == "cap" then
      if t < at then
        p.caption = ""
      else
        local u = clamp01((t - at) / CAPTION_TIME)
        p.caption = CAPTIONS[p.group]
        p.position = { x = h.x, y = h.y + CAPTION_RISE * (1 - ease_out(u)) }
        if state_of(p.group, t) == DONE then
          p.tone = WHITE_TONE
        else
          p.tone = math.floor(u * FADE_STEPS + 0.5)
        end
      end
    end
  end
end
