/** Как поле вида частиц правится: число, пара «от и до» или ряд точек видимости. */
type ParticleNumericFieldKind = "number" | "pair" | "points";

export type ParticleFieldSpec = {
  /** Ключ вида в `particles.json`. */
  key: string;
  /** Подпись поля во вкладке «Частицы» («Редактор», требование 25). */
  label: string;
  kind: ParticleNumericFieldKind;
  /** Подсказка поля: ключ файла и его смысл из таблицы «Ветер и частицы» → «Частицы». */
  hint: string;
  /** Значение, которое движок берёт без ключа; `undefined` — ключ обязателен, поле без него пусто. */
  defaultValue: number | undefined;
};

type ParticleFieldGroup = { title: string; fields: readonly ParticleFieldSpec[] };


/** Три группы полей вида — «Редактор», требование 25: подписи и порядок как в плане, подсказки — из таблицы ключей. */
export const PARTICLE_FIELD_GROUPS: readonly ParticleFieldGroup[] = [
  {
    title: "Вылет",
    fields: [
      { key: "rate", label: "густота", kind: "number", hint: "rate — густота: сколько частиц вылетает в секунду", defaultValue: undefined },
      { key: "lifetime", label: "живёт, с", kind: "pair", hint: "lifetime — сколько секунд живёт частица; пара «от и до» — каждая берёт своё наугад", defaultValue: undefined },
      { key: "speed", label: "скорость", kind: "pair", hint: "speed — начальная скорость в клетках в секунду; пара «от и до» — каждая берёт своё наугад", defaultValue: 0 },
      { key: "direction", label: "куда, °", kind: "number", hint: "direction — куда летит, в градусах по часовой стрелке: 0 — вверх, 90 — вправо, 180 — вниз", defaultValue: 0 },
      { key: "spread", label: "разброс, °", kind: "number", hint: "spread — разброс направления в градусах в обе стороны, от 0 до 180", defaultValue: 0 },
    ],
  },
  {
    title: "Вид",
    fields: [
      { key: "size", label: "размер", kind: "pair", hint: "size — ширина частицы в клетках, высота по пропорциям картинки; пара «от и до» — каждая берёт своё наугад", defaultValue: undefined },
      { key: "grow", label: "рост, раз", kind: "number", hint: "grow — во сколько раз частица больше к концу жизни", defaultValue: 1 },
      { key: "opacity", label: "видимость", kind: "points", hint: "opacity — просвечивание: одна точка на всю жизнь или ряд точек через равные промежутки жизни, между ними плавно; [0, 0.7, 0] — проступает и тает", defaultValue: 1 },
      { key: "spin", label: "вращение, °/с", kind: "pair", hint: "spin — вращение в градусах в секунду, меньше нуля — против часовой стрелки; с ним частицы вылетают повёрнутыми наугад", defaultValue: 0 },
    ],
  },
  {
    title: "Полёт",
    fields: [
      { key: "gravity", label: "тяжесть", kind: "number", hint: "gravity — ускорение вниз в клетках в секунду за секунду, меньше нуля — вверх", defaultValue: 0 },
      { key: "wind", label: "ветер, доля", kind: "number", hint: "wind — какую долю скорости ветра частица набирает, от 0 до 1", defaultValue: 1 },
      { key: "wobble", label: "виляние", kind: "number", hint: "wobble — на сколько клеток частица виляет в стороны на лету, как падающий лист", defaultValue: 0 },
    ],
  },
];
