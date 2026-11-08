-- Окно Claude Code для третьего ролика в оформлении «бронза» (панель тёмного дерева с ярлыком
-- «Claude Code» на размытом лесу уровня), фраза «Я показал это нейросети, она поменяла несколько углов
-- в позе стойки и перерисовала кулак». С первым словом в поле ввода по букве печатается просьба
-- поправить хват топора, на «она» она уходит, ниже бегут строки ответа: смотрю на позу, на «углов» —
-- меняю углы, на «перерисовала» — перерисовываю кулак, на «кулак» — собираю кадры заново. Строки
-- «Готово» нет: кадры собираются в следующей сцене. Сцена начинается с первого слова фразы.
-- Моменты расчётные, пока голос не записан: 135 слов в минуту, слово номер i (с нуля) — i × 0,444 с.
-- Привязка: type_from — «Я», enter — «она», angles — «углов», fist — «перерисовала»,
-- rebuild — «кулак». После записи голоса сюда ставятся настоящие моменты, и сцена записывается заново.
local T = {
  type_from = 0.3,
  enter = 1.98,
  angles = 3.32,
  fist = 4.86,
  rebuild = 5.48,
}

local CLOCK_START = 1000
local CURSOR = "|"
local BLINK = 0.5
-- Печать начинается чуть позже первого слова и кончается чуть раньше, чем просьба уходит.
local TYPE_DELAY = 0.15
local TYPE_END = 0.15
-- Первая строка ответа появляется через LOOK_DELAY после того, как просьба ушла.
local LOOK_DELAY = 0.25

local REQUEST = {
  "> В стойке Илья держит топор как нож, обратным хватом: лезвие",
  "  у бедра, топорище торчит вперёд. И кулак под топорищем —",
  "  топор будто приклеен. Поправь",
}
-- Строки ответа и моменты, когда каждая появляется.
local ANSWER = {
  { text = "• Смотрю на позу стойки и на кулак…", at = function() return T.enter + LOOK_DELAY end },
  { text = "• Меняю углы локтя и кисти в позе стойки", at = function() return T.angles end },
  { text = "• Перерисовываю кулак поверх топорища", at = function() return T.fist end },
  { text = "• Собираю кадры заново…", at = function() return T.rebuild end },
}

-- Первые n букв строки: кириллица в Lua занимает по два байта, поэтому счёт идёт по буквам.
local function first_letters(s, n)
  local total = utf8.len(s)
  if n >= total then return s end
  if n <= 0 then return "" end
  return string.sub(s, 1, utf8.offset(s, n + 1) - 1)
end

local typed_total = 0
for i = 1, #REQUEST do typed_total = typed_total + utf8.len(REQUEST[i]) end

function direct(director)
  local t = CLOCK_START - director.clock
  local from, to = T.type_from + TYPE_DELAY, T.enter - TYPE_END
  local u = (t - from) / (to - from)
  if u < 0 then u = 0 end
  if u > 1 then u = 1 end
  local typed = math.floor(u * typed_total)
  local cursor_on = t < T.enter and math.floor(t / BLINK) % 2 == 0

  local rows = find({ has = { "row" } })
  for i = 1, #rows do
    local r = rows[i]
    if r.row <= #REQUEST then
      -- Сколько букв просьбы уже напечатано до этой строки и в ней самой.
      local before = 0
      for k = 1, r.row - 1 do before = before + utf8.len(REQUEST[k]) end
      local text = first_letters(REQUEST[r.row], typed - before)
      local is_last = typed - before < utf8.len(REQUEST[r.row]) and typed >= before
      if cursor_on and (is_last or (r.row == #REQUEST and typed >= typed_total)) then
        text = text .. CURSOR
      end
      r.line = text
    else
      local a = ANSWER[r.row - #REQUEST]
      if t >= a.at() then r.line = a.text else r.line = "" end
    end
  end
end
