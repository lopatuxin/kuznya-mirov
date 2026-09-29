// Проверка проходимости по прямоугольникам препятствий, не по клеткам текстовой карты (план фазы
// 2.5: «тест проходимости работает по прямоугольникам препятствий»): обход в ширину по целым
// клеткам сцены, клетка закрыта, если её квадрат 1×1 пересекается с каким-нибудь прямоугольником
// из `blockingRects`. Тот же обход, что раньше был внутри теста сборщика сцены по текстовой карте
// (`buildRpgScene.test.mjs`), вынесен сюда как переиспользуемая чистая функция — требование плана:
// «тест проходимости по прямоугольникам препятствий».

function isBlocked(blockingRects, x, y) {
  return blockingRects.some((r) => x + 1 > r.position[0] && x < r.position[0] + r.size[0] && y + 1 > r.position[1] && y < r.position[1] + r.size[1]);
}

// Клетки сцены, достижимые от `(startX, startY)` без пересечения `blockingRects`, — как булева
// сетка `height`×`width` (обход `walkableWidth`×`walkableHeight` клеток, отсчёт с нуля).
export function reachableCells(width, height, blockingRects, startX, startY) {
  const visited = Array.from({ length: height }, () => new Array(width).fill(false));
  if (isBlocked(blockingRects, startX, startY)) return visited;
  const queue = [[startX, startY]];
  visited[startY][startX] = true;
  while (queue.length > 0) {
    const [x, y] = queue.pop();
    for (const [dx, dy] of [
      [1, 0],
      [-1, 0],
      [0, 1],
      [0, -1],
    ]) {
      const nx = x + dx;
      const ny = y + dy;
      if (nx < 0 || ny < 0 || nx >= width || ny >= height || visited[ny][nx]) continue;
      if (isBlocked(blockingRects, nx, ny)) continue;
      visited[ny][nx] = true;
      queue.push([nx, ny]);
    }
  }
  return visited;
}

// Клетка `(x, y)` достижима согласно сетке `reachableCells`.
export function isReachable(visited, x, y) {
  return Boolean(visited[y]?.[x]);
}
