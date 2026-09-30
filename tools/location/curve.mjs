// Линии и многоугольники описания: точки описания сглаживаются в кривую, проходящую через них, и
// дальше всё считается по ломаной с шагом в четверть клетки — расстояние, место вдоль линии, сторона.

const STEP = 0.25;

function distance(a, b) {
  return Math.hypot(b[0] - a[0], b[1] - a[1]);
}

function lerp(a, b, t) {
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
}

// Центростремительный сплайн Катмулла — Рома между `p1` и `p2`: в отличие от обычного, он не даёт
// петель и острых выбросов на неравных промежутках между точками.
function span(p0, p1, p2, p3, out) {
  const t1 = Math.max(Math.sqrt(distance(p0, p1)), 1e-6);
  const t2 = t1 + Math.max(Math.sqrt(distance(p1, p2)), 1e-6);
  const t3 = t2 + Math.max(Math.sqrt(distance(p2, p3)), 1e-6);
  const steps = Math.max(1, Math.ceil(distance(p1, p2) / STEP));
  for (let k = 0; k < steps; k++) {
    const t = t1 + ((t2 - t1) * k) / steps;
    const a1 = lerp(p0, p1, t / t1);
    const a2 = lerp(p1, p2, (t - t1) / (t2 - t1));
    const a3 = lerp(p2, p3, (t - t2) / (t3 - t2));
    const b1 = lerp(a1, a2, t / t2);
    const b2 = lerp(a2, a3, (t - t1) / (t3 - t1));
    out.push(lerp(b1, b2, (t - t1) / (t2 - t1)));
  }
}

/** Точки кривой через `points`; `closed` — многоугольник, `sharp` — ломаная без сглаживания. */
export function smooth(points, { closed = false, sharp = false } = {}) {
  if (sharp || points.length < 3) return points.map((p) => [p[0], p[1]]);
  const n = points.length;
  const at = (i) => {
    if (closed) return points[((i % n) + n) % n];
    if (i < 0) return lerp(points[0], points[1], -1);
    if (i >= n) return lerp(points[n - 1], points[n - 2], -1);
    return points[i];
  };
  const out = [];
  const spans = closed ? n : n - 1;
  for (let i = 0; i < spans; i++) span(at(i - 1), at(i), at(i + 1), at(i + 2), out);
  if (!closed) out.push([points[n - 1][0], points[n - 1][1]]);
  return out;
}

/**
 * Ломаная для расчётов: длина, ближайшая точка, место вдоль линии и сторона. Сторона `left` —
 * слева по ходу линии, если смотреть на план сверху, где север — верх сцены (y растёт на юг).
 */
export class Path {
  constructor(points, closed = false) {
    // Повтор соседней точки дал бы отрезок без направления, а у него нет стороны.
    const distinct = points.filter((p, i) => i === 0 || distance(p, points[i - 1]) > 1e-9);
    if (closed && distinct.length > 1 && distance(distinct[0], distinct[distinct.length - 1]) <= 1e-9) distinct.pop();
    this.points = distinct;
    this.closed = closed;
    const count = closed ? distinct.length : distinct.length - 1;
    this.segments = [];
    let start = 0;
    for (let i = 0; i < count; i++) {
      const a = distinct[i];
      const b = distinct[(i + 1) % distinct.length];
      const length = distance(a, b);
      this.segments.push({
        a,
        dx: b[0] - a[0],
        dy: b[1] - a[1],
        length,
        start,
        // Нормаль влево по ходу: при y, растущем на юг, это (dy, −dx).
        left: [(b[1] - a[1]) / length, -(b[0] - a[0]) / length],
        box: { minX: Math.min(a[0], b[0]), minY: Math.min(a[1], b[1]), maxX: Math.max(a[0], b[0]), maxY: Math.max(a[1], b[1]) },
      });
      start += length;
    }
    this.length = start;
    const xs = distinct.map((p) => p[0]);
    const ys = distinct.map((p) => p[1]);
    this.box = { minX: Math.min(...xs), minY: Math.min(...ys), maxX: Math.max(...xs), maxY: Math.max(...ys) };
  }

  /** Ближайшая точка линии к `p`: расстояние, место вдоль линии `s`, сама точка и сторона. */
  nearest(p) {
    let best = { distance: Infinity, index: 0, t: 0, point: this.points[0] };
    this.segments.forEach((segment, index) => {
      const { box } = segment;
      // Отрезок, чья рамка дальше найденного, ближе быть не может — так поиск по длинной линии быстрый.
      const outX = Math.max(box.minX - p[0], 0, p[0] - box.maxX);
      const outY = Math.max(box.minY - p[1], 0, p[1] - box.maxY);
      if (outX * outX + outY * outY >= best.distance * best.distance) return;
      const { a, dx, dy, length } = segment;
      const t = Math.min(1, Math.max(0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (length * length)));
      const point = [a[0] + dx * t, a[1] + dy * t];
      const d = distance(p, point);
      if (d < best.distance) best = { distance: d, index, t, point };
    });
    const segment = this.segments[best.index];
    return { distance: best.distance, s: segment.start + segment.length * best.t, point: best.point, left: this.sideAt(p, best) };
  }

  // Сторона по нормали отрезка, а в вершине — по сумме нормалей двух отрезков у неё: на остром
  // изломе точка за вершиной иначе получила бы сторону не того отрезка.
  sideAt(p, { index, t, point }) {
    const n = this.segments.length;
    let normal = this.segments[index].left;
    const neighbour = t === 0 && (index > 0 || this.closed) ? (index - 1 + n) % n : t === 1 && (index < n - 1 || this.closed) ? (index + 1) % n : -1;
    if (neighbour >= 0 && neighbour !== index) {
      const other = this.segments[neighbour].left;
      normal = [normal[0] + other[0], normal[1] + other[1]];
    }
    return (p[0] - point[0]) * normal[0] + (p[1] - point[1]) * normal[1] > 0;
  }

  /** Лежит ли `p` внутри замкнутой линии — по чётности пересечений луча. */
  contains(p) {
    let inside = false;
    const pts = this.points;
    for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
      const [xi, yi] = pts[i];
      const [xj, yj] = pts[j];
      if (yi > p[1] !== yj > p[1] && p[0] < ((xj - xi) * (p[1] - yi)) / (yj - yi) + xi) inside = !inside;
    }
    return inside;
  }

  /** Расстояние до границы замкнутой линии: внутри — со знаком минус. */
  signedDistance(p) {
    const d = this.nearest(p).distance;
    return this.contains(p) ? -d : d;
  }
}

/** Кривая описания как `Path`: сглаженная, если не `sharp`. */
export function path(points, { closed = false, sharp = false } = {}) {
  return new Path(smooth(points, { closed, sharp }), closed);
}

export function isNumber(value) {
  return typeof value === "number" && Number.isFinite(value);
}

export function isPoint(value) {
  return Array.isArray(value) && value.length === 2 && value.every(isNumber);
}

/**
 * Линия или многоугольник из списка точек описания. Не подошло — `fail(сообщение)` с ключом `key`;
 * `fail` бросает ошибку с местом, где искать, — операцией или покрытием.
 */
export function readLine(value, key, { closed, sharp }, fail) {
  const least = closed ? 3 : 2;
  if (!Array.isArray(value) || value.length < least || !value.every(isPoint)) {
    fail(`«${key}» — список не меньше ${least} точек [x, y]`);
  }
  const line = path(value, { closed, sharp });
  if (line.segments.length === 0) fail(`«${key}» — все точки совпадают`);
  if (closed) {
    const pts = line.points;
    const area = pts.reduce((sum, p, i) => {
      const q = pts[(i + 1) % pts.length];
      return sum + p[0] * q[1] - q[0] * p[1];
    }, 0);
    if (Math.abs(area) < 1e-9) fail(`«${key}» — многоугольник без площади: точки на одной прямой`);
  }
  return line;
}
