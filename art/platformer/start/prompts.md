# Начальный экран платформера «Распутье»

Слева меню, остальное место — выбор героя: камень на распутье, три дороги, на каждой свой герой. При наведении
мыши герой шагает вперёд и показывает приветствие, строка его дороги на камне загорается.

Картинки рисует Codex встроенной нейросетью (GPT Image), ролики — видеонейросеть syntx по картинке. Неудачные
варианты лежат в `art/platformer/rejected/start/` (вне git).

## Файлы

| Файл | Что это | Как получен |
|---|---|---|
| `sample.png` | утверждённый образец экрана: меню, название, камень со строками, Илья шагнул вперёд | третья правка образца, запрос ниже |
| `background.png` | фон экрана без меню, героев и букв; сторона камня гладкая под надпись | правка образца, затем очистка от крапинок и затемнение полосы под меню |
| `ilya.png` | Илья в позе ожидания на зелёном фоне | запрос героя, затем очистка и перекраска зелёного ромбика на поясе |
| `ilya_greet_start.png` | `ilya.png`, уменьшенный до 0,6 высоты кадра, зелёный продолжен краевыми точками | программой: место для шага и поднятой руки |
| `ilya_greet.mp4` | ролик syntx по `ilya_greet_start.png`: шаг вперёд, бицепс, рука обратно на пояс | запрос ролика ниже |
| `ilya_greet_masked.mp4` | ролик без зелёного: сверху цвет, снизу прозрачность | `node tools/art/sheet.mjs art/platformer/start/ilya_greet.mp4 art/platformer/start/ilya_greet_masked.mp4 --range 1-121` |

Строки камня пишет движок поверх гладкой стороны: «Налево пойдёшь — силой возьмёшь», «Прямо пойдёшь — хитростью
пройдёшь», «Направо пойдёшь — мудростью одолеешь». Кнопки меню и название в игре — тоже текст движка.

## Порядок для каждого героя

1. Герой на зелёном фоне (`#00FF00`) запросом Codex по образцу `ilya.png`: тот же стиль, масштаб, свет справа.
   На герое ничего зелёного — иначе ключ вырежет дырку; героиню с зелёными чарами — на розовом фоне.
2. Отдельный запрос Codex на очистку от крапинок (GPT Image сыплет точки по всей картинке), сравнить с исходником
   кусками ×3.
3. Уменьшить кадр до 0,6 высоты и добить зелёным для ролика, запрос ролика в syntx, вырезка `sheet.mjs`.

Фильтр безопасности Codex отказывал в правке и рисовании Алёши (похоже, лицо смахивало на настоящего человека,
потом — временная блокировка после серии отказов); Алёшу рисовать заново с придуманным лицом.

## Запрос героя (Илья)

```
Ilya Muromets alone, full body from head to feet, on a perfectly flat, uniform, pure bright green background
(#00FF00, chroma key green) that fills the whole picture. Landscape 16:9. He stands in the middle of the picture.

Ilya: exactly the man from the LOOK reference (art/platformer/reference_frame.png): about 40 years old, a full
thick beard, broad shoulders, a thick neck, normal human height, natural adult proportions about 7.5 heads tall,
a realistic adult face with normal-sized eyes, no cartoon, Disney or anime look. Clothes exactly as in the
reference: an off-white linen shirt with red embroidery at the collar, cuffs and hem, a red woven sash with
tassels, dark brown trousers, white linen foot wraps with crossed laces, bast shoes. No armor, no shield, no helmet.

Pose (idle, waiting to be chosen): he stands relaxed and calm, weight on one leg, body in three-quarter view
turned toward the viewer's right, face toward the viewer with a calm confident half-smile. In his left hand he
holds his simple wood axe by the very end of the handle, blade down, hanging by his leg. His right thumb is
hooked behind his red sash.

Light: warm late-afternoon sunlight from the right, soft shading. No cast shadow and no ground: the background
stays perfectly flat pure green everywhere. Clean smooth painted surfaces: no tiny specks, no sprinkled particles,
no glitter, no dotted or scale-like texture anywhere. No text, no other people, no border.
```

## Запрос очистки

```
Edit the image to remove these artifacts: scattered random specks one to three pixels in size, light and dark,
sprinkled over the clothes, skin, hair and beard. Clean, smooth painted surfaces with soft shading and smooth
gradients; keep the embroidery, the sash weave and the bast shoe weave as they are. Keep everything else exactly
the same: the man, face, pose, hands, the axe, clothes and colors, light, style, size and position, framing. The
background stays a perfectly flat, uniform, pure bright green (#00FF00); no green on the man. Do not add anything.
```

## Запрос ролика (syntx, по картинке; 16:9, 5 секунд, 720p)

```
Animate the attached image. Static camera: no zoom, no pan, no camera movement at all. The background stays
perfectly flat, uniform pure green for the whole video: no floor, no shadows, no light changes, no particles.

The bearded man starts exactly as in the image, standing relaxed. He takes one calm, confident step forward toward
the camera and stops; he gets a little bigger as he comes closer, and his feet stay fully inside the frame. Then he
raises the arm that rests on his belt and proudly flexes his biceps with a broad grin, holds the pose for a moment,
then lowers the arm back to his belt and stands relaxed again. His other hand keeps holding the axe, blade down,
the whole time.

Keep his face, beard, hair, clothes, colors and the hand-painted animated film style exactly as in the image.
Smooth, natural, weighty motion of a strong man. No morphing, no extra objects, no other people, no text.
```
