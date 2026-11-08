-- Окно Claude Code для второго ролика: в поле ввода по букве печатается обычный запрос про движок,
-- после ввода ниже одна за другой бегут строки ответа. Окно — под фразу «Чтобы собрать прототип,
-- его нужно на чём-то запускать»; печать начинается с первым словом. Секунды расчётные (130 слов
-- в минуту); после записи голоса сюда ставятся настоящие, и сцена записывается заново.
local T = {
  type_from = 0.3,
  type_to = 1.6,
  enter = 1.75,
  answer_from = 2,
  answer_step = 0.3,
}

local CLOCK_START = 1000
local CURSOR = "▌"
local BLINK = 0.5

local HEADER = "* Добро пожаловать в Claude Code"
local REQUEST = {
  "> Сделай каркас движка для моей 2D-игры: пусть читает описание игры",
  "  из файлов, много раз в секунду двигает все предметы, проверяет,",
  "  кто в кого врезался, и рисует всё это на экране.",
}
local ANSWER = {
  "● Смотрю, что уже есть в проекте…",
  "● Делаю шаг мира: 60 раз в секунду двигаю все предметы",
  "● Проверяю, кто в кого врезался",
  "● Рисую кадр в браузере",
  "● Собираю и запускаю пробную игру",
  "● Готово: каркас запускает игру",
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
  local u = (t - T.type_from) / (T.type_to - T.type_from)
  if u < 0 then u = 0 end
  if u > 1 then u = 1 end
  local typed = math.floor(u * typed_total)
  local cursor_on = t < T.enter and math.floor(t / BLINK) % 2 == 0

  local rows = find({ has = { "row" } })
  for i = 1, #rows do
    local r = rows[i]
    if r.row == 0 then
      r.line = HEADER
    elseif r.row <= #REQUEST then
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
      if t >= T.answer_from + (k - 1) * T.answer_step then
        r.line = ANSWER[k]
      else
        r.line = ""
      end
    end
  end
end
