# Экран героя платформера «Распутье»

Фаза 10 платформера. Экран открывается «Новой игрой», когда герой выбран на «Распутье»: слева герой на дороге,
справа лист в расписной рамке с его именем, характеристиками, снаряжением, духом и ростом.

## Файлы

| Файл | Что это | Как получен |
|---|---|---|
| `background_sample.png` | утверждённый фон, 1672 × 941: утренняя дорога к кремлю слева, пустой лист в рамке в духе Билибина справа | запрос фона ниже, Codex |
| `background_wide.png` | фон, дорисованный по бокам до 2072 × 941 (около 2,2:1); середина — `background_sample.png` точка в точку | запрос расширения ниже, Codex по холсту с пурпурными полосами по 200 точек |
| `sketch_ilya.png`, `sketch_alyosha.png`, `sketch_vasilisa.png` | одобренные хозяином наброски экрана (2026-10-27) | фон + герой из ролика покоя + надписи шрифтами игры (ffmpeg drawtext) |
| `vasilisa_maiden.png` | Василиса-девица с прозрачностью: последний кадр `games/platformer/images/vasilisa_greet.mp4` | маска нижней половины кадра наложена на верхнюю |
| `ilya_portrait.mp4`, `alyosha_portrait.mp4`, `vasilisa_portrait.mp4` | портреты для экрана на одном холсте 900 × 1016 (ролики двойной высоты: сверху цвет, снизу маска; у Василисы — две секунды одного кадра) | `node tools/art/portrait.mjs <ролик покоя или vasilisa_maiden.png> <выход.mp4> <рост>`: Илья и Алёша — 760, Василиса — 709 |

В игру: `background_wide.png`, уменьшенный до 2048 × 930, — `games/platformer/images/hero_bg.png`; портреты — копиями
в `games/platformer/images/`. Лист в рамке, вырезанный из `background_wide.png` (`ffmpeg -vf crop=806:787:996:52`), —
`games/platformer/images/pause_sheet.png`, подложка меню паузы.

## Запрос фона (Codex, образец стиля — `art/platformer/start/sample.png`)

```
Picture: the background of a "hero sheet" screen of a 2D fairy-tale game, landscape 16:9, about 1672 x 941 pixels.

Scene: a sunny early morning at the start of a forest road. A dirt road begins in the foreground at the left and winds
away into the depth of the picture toward a small wooden kremlin on a far hill. Pines and birches frame the scene,
grass and small wildflowers along the road. Warm sunlight from the upper right, long soft shadows falling to the left,
light morning haze in the distance.

Layout, strictly:
1. Left part (from the left edge to about 42% of the width): an open sunlit patch of the road where a full-height
character will be placed later by the game. Keep it EMPTY. The ground where the character's feet will stand is at about
88% of the picture height, in the left third.
2. Right part (from about 45% to about 96% of the width, from about 6% to about 92% of the height): one large flat
upright panel lying over the scene like a page of an old illustrated Russian fairy-tale book in the manner of Ivan
Bilibin: warm aged parchment, front-facing, a clean rectangle with straight edges, framed by a rich ornamental border of
Russian folk ornament in deep red, dark green and gold. The INSIDE of the panel must be plain warm parchment —
completely empty, ready for text.
3. No text, no letters, no numbers, no symbols, no people, no animals anywhere in the picture.
```

## Запрос расширения (Codex, правка по холсту 2072 × 941)

```
Edit the attached image. Change only the two magenta strips: paint them as a natural continuation of the scene so the
picture becomes one seamless wider painting, 2072 x 941, with the same hand-painted style, light and colours.
- Left strip: continue the forest edge at the left — birch trunks, pines, bushes, grass and wildflowers, the dirt road
  and its verge continuing at the bottom; sky and foliage at the top continuing the birch leaves.
- Right strip: continue the forest at the right of the ornamental panel — pines, birch foliage, bushes, grass and
  wildflowers, a mossy boulder at the bottom.
- Keep the middle part (x 200 to 1872) exactly as it is, pixel for pixel.
- No magenta left anywhere. No people, no animals, no text, no letters, no new panels or frames.
```

Первая сохранённая попытка этого запроса вышла приближенным куском середины (`art/platformer/rejected/hero/`), вторая
сохранённая — годная.
