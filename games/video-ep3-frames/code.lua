-- Кадры Ильи для третьего ролика, фраза «Того Илью из прошлого ролика нарисовала нейросеть, но для
-- игры одной картинки мало: нужны кадры, как он стоит, бежит, прыгает, а скоро ещё как замахивается
-- и уворачивается». Сначала во весь кадр герой нейросети из прошлого ролика с медленным наездом; на
-- «одной» он уезжает влево маленькой карточкой, на «мало» справа волной выскакивают все 37 кадров
-- Ильи по группам, тусклыми; на «стоит», «бежит», «прыгает», «замахивается», «уворачивается» своя
-- группа проступает в цвете, рамка её карточки и подпись горят золотом, пока не назовут следующую.
-- Отбив и пропущенный удар остаются тусклыми. Сцена начинается с первого слова фразы.
-- Моменты расчётные, пока голос не записан: 135 слов в минуту, слово номер i (с нуля) — i × 0,444 с.
-- Привязка: small — «одной», frames — «мало», stand — «стоит», run — «бежит», jump — «прыгает»,
-- swing — «замахивается», dodge — «уворачивается». После записи голоса сюда ставятся настоящие
-- моменты, и сцена записывается заново.
local T = {
  small = 4.685,
  frames = 5.485,
  stand = 8.005,
  run = 8.485,
  jump = 9.305,
  swing = 11.025,
  dodge = 11.825,
}

local CLOCK_START = 1000

-- На чьё слово загорается группа: номер группы → момент; группы 5 и 7 не называются.
local LIGHT = { [1] = "stand", [2] = "run", [3] = "jump", [4] = "swing", [6] = "dodge" }
local CAPTIONS = { "стойка", "бег", "прыжок", "удар", "отбив", "уворот", "пропущенный удар" }

-- Герой нейросети: во весь кадр с наездом PHOTO_ZOOM, потом карточкой на месте из сцены.
local FULL = { x = 0, y = 0, w = 32, h = 18 }
local PHOTO_ZOOM = 0.04
local PHOTO_FOCUS = { x = 16, y = 8 }
local SHRINK_TIME = 0.85

-- Кадры выскакивают по одному с шагом APPEAR_STEP по порядку листа, карточка — чуть раньше своих кадров.
local APPEAR_STEP = 0.02
local CARD_LEAD = 0.05
-- Группа проступает в цвете волной: кадр за кадром с шагом LIGHT_STEP, каждый за LIGHT_TIME,
-- и чуть подпрыгивает на BUMP.
local LIGHT_STEP = 0.03
local LIGHT_TIME = 0.25
local BUMP = 0.07
-- Подпись проступает за CAPTION_TIME, поднимаясь на CAPTION_RISE клетки.
local CAPTION_TIME = 0.2
local CAPTION_RISE = 0.3
local FADE_STEPS = 5
local WHITE_TONE = 6

local WAITING, CURRENT, DONE = 0, 1, 2
-- Обод карточки по размеру карточки (её свойство rim) и состоянию группы.
local RIMS = {
  ["286x460"] = { [WAITING] = "rim_wait_286x460", [CURRENT] = "rim_cur_286x460", [DONE] = "rim_done_286x460" },
  ["526x460"] = { [WAITING] = "rim_wait_526x460", [CURRENT] = "rim_cur_526x460", [DONE] = "rim_done_526x460" },
  ["646x460"] = { [WAITING] = "rim_wait_646x460", [CURRENT] = "rim_cur_646x460", [DONE] = "rim_done_646x460" },
}
local HIDDEN = { x = -100, y = -100 }

local function clamp01(u)
  if u < 0 then return 0 end
  if u > 1 then return 1 end
  return u
end

local function lerp(a, b, u)
  return a + (b - a) * u
end

local function ease_in_out(u)
  if u < 0.5 then return 2 * u * u end
  return 1 - 2 * (1 - u) * (1 - u)
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
  local key = LIGHT[group]
  if key == nil then return nil end
  return T[key]
end

-- Ждёт, горит золотом, пока не назовут следующую группу, потом горит белым.
local function state_of(group, t)
  local at = light_at(group)
  if at == nil or t < at then return WAITING end
  for g, key in pairs(LIGHT) do
    if g ~= group and T[key] > at and t >= T[key] then return DONE end
  end
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

local function photo_rect(h, t)
  if t < T.small then
    return scaled(FULL, PHOTO_FOCUS, 1 + PHOTO_ZOOM * clamp01(t / T.small))
  end
  local u = ease_in_out(clamp01((t - T.small) / SHRINK_TIME))
  local from = scaled(FULL, PHOTO_FOCUS, 1 + PHOTO_ZOOM)
  return { x = lerp(from.x, h.x, u), y = lerp(from.y, h.y, u), w = lerp(from.w, h.w, u), h = lerp(from.h, h.h, u) }
end

local function frame_appear(frame)
  return T.frames + frame * APPEAR_STEP
end

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "part" } })
  if home == nil then remember(parts) end
  local photo_home = home["photo::"]

  for i = 1, #parts do
    local p = parts[i]
    local h = home[key_of(p)]
    if p.part == "photo" then
      put(p, photo_rect(h, t))
    elseif p.part == "photo_edge" then
      -- Рамка шире картинки на свою толщину со всех сторон; во весь кадр она за краем кадра.
      local r = photo_rect(photo_home, t)
      local e = (h.w - photo_home.w) / 2
      put(p, { x = r.x - e, y = r.y - e, w = r.w + 2 * e, h = r.h + 2 * e })
    elseif p.part == "card" or p.part == "edge" then
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
        local at = light_at(p.group)
        local lit = 0
        if at ~= nil then
          local start = at + (p.frame - first_frame[p.group]) * LIGHT_STEP
          lit = clamp01((t - start) / LIGHT_TIME)
          -- Кадр чуть подпрыгивает, стоя ногами на месте.
          local u = clamp01((t - start) / (LIGHT_TIME * 1.6))
          local feet = { x = h.x + h.w / 2, y = h.y + h.h }
          r = scaled(r, feet, 1 + BUMP * math.sin(math.pi * u))
        end
        put(p, r)
        if p.part == "lit" then p.opacity = lit end
      end
    elseif p.part == "cap" then
      local at = light_at(p.group)
      if at == nil or t < at then
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
