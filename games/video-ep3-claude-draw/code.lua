-- Окно Claude Code для третьего ролика в оформлении «бронза» (панель тёмного дерева с ярлыком
-- «Claude Code» на размытом лесу уровня), фраза «Поэтому пока героя и весь уровень рисует другая
-- нейросеть, та, что пишет со мной код». С первым словом в поле ввода по букве печатается запрос
-- нарисовать Илью программой, на «нейросеть» он уходит, ниже одна за другой бегут строки ответа,
-- на «код» — золотая строка «Готово». Сцена начинается с первого слова фразы.
-- Моменты расчётные, пока голос не записан: 135 слов в минуту, слово номер i (с нуля) — i × 0,444 с.
-- Привязка: type_from — «Поэтому», enter — «нейросеть», answer — «та», done — «код». После записи
-- голоса сюда ставятся настоящие моменты, и сцена записывается заново.
local T = {
  type_from = 0.3,
  enter = 2.76,
  answer = 3.72,
  done = 5.28,
}

local CLOCK_START = 1000
local CURSOR = "|"
local BLINK = 0.5
-- Печать начинается чуть позже первого слова и кончается чуть раньше, чем запрос уходит.
local TYPE_DELAY = 0.3
local TYPE_END = 0.15
-- Строки ответа до «Готово» идут с шагом ANSWER_STEP от момента answer.
local ANSWER_STEP = 0.45

local REQUEST = {
  "> Нарисуй Илью программой: собери его из частей,",
  "  а позы задай углами в суставах",
}
local ANSWER = {
  "• Смотрю, как сейчас рисуется герой…",
  "• Собираю Илью из частей: голова, туловище, руки, ноги, топор",
  "• Задаю каждую позу углами в суставах",
  "• Рисую кадры стойки, бега и прыжка",
}
local DONE = "• Готово: Илья нарисован программой"

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
      -- Сколько букв запроса уже напечатано до этой строки и в ней самой.
      local before = 0
      for k = 1, r.row - 1 do before = before + utf8.len(REQUEST[k]) end
      local text = first_letters(REQUEST[r.row], typed - before)
      local is_last = typed - before < utf8.len(REQUEST[r.row]) and typed >= before
      if cursor_on and (is_last or (r.row == #REQUEST and typed >= typed_total)) then
        text = text .. CURSOR
      end
      r.line = text
    else
      local k = r.row - #REQUEST
      if k > #ANSWER then
        if t >= T.done then r.line = DONE else r.line = "" end
      elseif t >= T.answer + (k - 1) * ANSWER_STEP then
        r.line = ANSWER[k]
      else
        r.line = ""
      end
    end
  end
end
