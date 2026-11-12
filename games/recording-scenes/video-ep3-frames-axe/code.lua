-- Новый хват топора для третьего ролика: сцена начинается со слов «так, чтобы» во фразе «…и перерисовала
-- кулак так, чтобы пальцы обхватывали топорище, а все пятнадцать кадров собрались заново сами». С первым
-- словом волной выскакивают 15 кадров прежнего листа — до правки хвата — на карточках стойки, прыжка и
-- бега с красной рамкой; на «пятнадцать» над ними проступает «15 кадров»; на «собрались» кадры быстро,
-- волной, один за другим сменяются новыми, рамка карточки горит золотом, пока её кадры меняются, потом
-- белым; на «сами» подпись гаснет, а кадр приближается ко второму кадру стойки — виден новый хват.
-- Моменты расчётные, пока голос не записан: 135 слов в минуту, слово номер i (с нуля) — i × 0,444 с,
-- счёт слов — от «так». Привязка: fifteen — «пятнадцать», swap — «собрались», zoom — «сами». После
-- записи голоса сюда ставятся настоящие моменты, и сцена записывается заново.
local T = {
  fifteen = 3.58,
  swap = 4.24,
  zoom = 5.18,
}

local CLOCK_START = 1000

-- Кадры выскакивают с первым словом, то есть с началом сцены, по одному с шагом APPEAR_STEP.
local APPEAR_AT = 0.05
local APPEAR_STEP = 0.03
local CARD_LEAD = 0.05
-- Порядок волны — как читаются карточки: верхний ряд (стойка, прыжок), потом нижний (бег).
local ORDER = { 0, 1, 2, 3, 12, 13, 14, 4, 5, 6, 7, 8, 9, 10, 11 }
-- Смена кадра: прежний гаснет за OLD_FADE, новый выскакивает за NEW_FADE; следующий — через WAVE_STEP.
local WAVE_STEP = 0.05
local OLD_FADE = 0.12
local NEW_FADE = 0.1
local SWAP_TIME = 0.22
-- Подпись проступает и гаснет за CAPTION_TIME, поднимаясь на CAPTION_RISE клетки.
local CAPTION_TIME = 0.2
local CAPTION_RISE = 0.3
local FADE_STEPS = 5
-- Приближение: точка FOCUS (доли прямоугольника) кадра стойки номер FOCUS_FRAME встаёт в середину
-- кадра, всё растёт в ZOOM раз за ZOOM_TIME.
local FOCUS = { x = 0.55, y = 0.56 }
local FOCUS_FRAME = 1
local ZOOM = 2
local ZOOM_TIME = 0.6
local SCREEN_CENTER = { x = 16, y = 9 }

-- Обод карточки по её размеру (свойство rim): прежний хват — красный, пока кадры меняются — золотой,
-- потом светлый.
local OLD_EDGE, SWAP_EDGE, DONE_EDGE = 1, 2, 3
local RIMS = {
  ["910x265"] = { [OLD_EDGE] = "rim_old_910x265", [SWAP_EDGE] = "rim_cur_910x265", [DONE_EDGE] = "rim_done_910x265" },
  ["692x265"] = { [OLD_EDGE] = "rim_old_692x265", [SWAP_EDGE] = "rim_cur_692x265", [DONE_EDGE] = "rim_done_692x265" },
  ["1782x265"] = { [OLD_EDGE] = "rim_old_1782x265", [SWAP_EDGE] = "rim_cur_1782x265", [DONE_EDGE] = "rim_done_1782x265" },
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

-- Выскок нового кадра поверх прежнего: начинается не с нуля, а почти с полного размера.
local function swap_scale(u)
  if u < 0.1 then return lerp(0.85, 1.08, u / 0.1) end
  if u < SWAP_TIME then return lerp(1.08, 1, (u - 0.1) / (SWAP_TIME - 0.1)) end
  return 1
end

local rank = {}
for i = 1, #ORDER do rank[ORDER[i]] = i - 1 end

local function swap_at(frame)
  return T.swap + rank[frame] * WAVE_STEP
end

local function scaled(r, c, s)
  return { x = c.x + (r.x - c.x) * s, y = c.y + (r.y - c.y) * s, w = r.w * s, h = r.h * s }
end

local function center(r)
  return { x = r.x + r.w / 2, y = r.y + r.h / 2 }
end

local home = nil
-- Первый и последний кадр волны у каждой карточки: по ним рамка становится золотой и белой.
local group_from, group_to = {}, {}
-- Кадр стойки, к которому приближается камера, и первое место карточки в волне выскакивания.
local focus_frame = nil
local group_rank = {}

local function key_of(p)
  return p.part .. ":" .. (p.group or "") .. ":" .. (p.frame or "")
end

local function remember(parts)
  home = {}
  for i = 1, #parts do
    local p = parts[i]
    home[key_of(p)] = { x = p.position.x, y = p.position.y, w = p.size.x, h = p.size.y }
    if p.part == "new" and p.frame == FOCUS_FRAME then focus_frame = home[key_of(p)] end
    if p.frame ~= nil then
      local r = rank[p.frame]
      if group_rank[p.group] == nil or r < group_rank[p.group] then group_rank[p.group] = r end
      local s = swap_at(p.frame)
      if group_from[p.group] == nil or s < group_from[p.group] then group_from[p.group] = s end
      if group_to[p.group] == nil or s > group_to[p.group] then group_to[p.group] = s end
    end
  end
end

-- Камера приближения: точка F уходит в середину кадра, всё вокруг неё растёт.
local function camera(r, t)
  local u = ease_in_out(clamp01((t - T.zoom) / ZOOM_TIME))
  if u <= 0 then return r end
  local f = { x = focus_frame.x + focus_frame.w * FOCUS.x, y = focus_frame.y + focus_frame.h * FOCUS.y }
  local z = lerp(1, ZOOM, u)
  local c = { x = lerp(f.x, SCREEN_CENTER.x, u), y = lerp(f.y, SCREEN_CENTER.y, u) }
  return { x = c.x + (r.x - f.x) * z, y = c.y + (r.y - f.y) * z, w = r.w * z, h = r.h * z }
end

local function put(p, r)
  p.size = { x = r.w, y = r.h }
  p.position = { x = r.x, y = r.y }
end

local function frame_appear(frame)
  return APPEAR_AT + rank[frame] * APPEAR_STEP
end

local function caption_tone(t)
  local fade_in = clamp01((t - T.fifteen) / CAPTION_TIME)
  local fade_out = 1 - clamp01((t - T.zoom) / CAPTION_TIME)
  return math.floor(math.min(fade_in, fade_out) * FADE_STEPS + 0.5)
end

function direct(director)
  local t = CLOCK_START - director.clock
  local parts = find({ has = { "part" } })
  if home == nil then remember(parts) end

  for i = 1, #parts do
    local p = parts[i]
    local h = home[key_of(p)]
    if p.part == "card" or p.part == "edge" then
      local appear = APPEAR_AT + group_rank[p.group] * APPEAR_STEP - CARD_LEAD
      if t < appear then
        p.position = HIDDEN
      else
        local card = home["card:" .. p.group .. ":"]
        put(p, camera(scaled(h, center(card), pop_scale(t - appear)), t))
        if p.part == "edge" then
          if t < group_from[p.group] then
            p.image = RIMS[p.rim][OLD_EDGE]
          elseif t < group_to[p.group] + SWAP_TIME then
            p.image = RIMS[p.rim][SWAP_EDGE]
          else
            p.image = RIMS[p.rim][DONE_EDGE]
          end
        end
      end
    elseif p.part == "old" or p.part == "new" then
      local appear = frame_appear(p.frame)
      local s = swap_at(p.frame)
      if t < appear or (p.part == "old" and t >= s + OLD_FADE) then
        p.position = HIDDEN
      elseif p.part == "old" then
        put(p, camera(scaled(h, center(h), pop_scale(t - appear)), t))
        p.opacity = 1 - clamp01((t - s) / OLD_FADE)
      elseif t < s then
        p.position = HIDDEN
      else
        put(p, camera(scaled(h, center(h), swap_scale(t - s)), t))
        p.opacity = clamp01((t - s) / NEW_FADE)
      end
    elseif p.part == "count" or p.part == "word" then
      if t < T.fifteen then
        p.tone = 0
      else
        local u = clamp01((t - T.fifteen) / CAPTION_TIME)
        p.position = { x = h.x, y = h.y + CAPTION_RISE * (1 - ease_out(u)) }
        p.tone = caption_tone(t)
      end
    end
  end
end
