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
| `background_wide.png` | фон, расширенный по бокам до 2072 × 941 (около 2,2:1): слева лес, справа берёза и река; середина — `background.png` точка в точку | правка Codex по `background.png` на розовом холсте с полосами по 200 точек |
| `background_wide_2048.png` | `background_wide.png`, уменьшенный до 2048 × 930 — предел движка | программой |
| `start_bg.png` | фон игры: `background_wide_2048.png` с высеченными строками камня | `node tools/art/carve.mjs` по `inscriptions.json` |
| `inscriptions.json` | строки камня: текст, середина, верх, размер и шаг строк в точках фона 2048 × 930, шрифт Monomakh | вручную по образцу |
| `glow_ilya.png`, `glow_alyosha.png`, `glow_vasilisa.png`, `glow.json` | те же буквы строк, горящие золотом, и их прямоугольники на фоне | `carve.mjs` вместе с резьбой |
| `font_compare.png`, `font_compare2.png` | сравнения шрифтов меню с образцом; хозяин выбрал Monomakh (древнерусский устав) | ffmpeg drawtext |
| `ilya.png` | Илья в позе ожидания на зелёном фоне | запрос героя, затем очистка и перекраска зелёного ромбика на поясе |
| `ilya_step_start.png` | `ilya.png`, уменьшенный в 0,8 раза, ступни на 66 точек выше, зелёный продолжен краевыми точками | программой: первый кадр ролика шага, последний — сам `ilya.png` |
| `ilya_step.mp4` | ролик syntx: один шаг вперёд от `ilya_step_start.png` до `ilya.png`, дальше стоит; 1916×1080 | запрос шага ниже, две картинки: первый и последний кадр |
| `ilya_flex.mp4` | ролик syntx по `ilya.png`: бицепс и рука обратно на пояс, ступни на месте; 1916×1080 | запрос бицепса ниже |
| `ilya_greet.mp4` | шаг и бицепс одним роликом, 181 кадр | кадры 1–60 `ilya_step.mp4` (60-й ближе всех к первому кадру бицепса) и весь `ilya_flex.mp4`, склейка ffmpeg `trim` и `concat` |
| `ilya_greet_masked.mp4` | ролик без зелёного: сверху цвет, снизу прозрачность | `node tools/art/sheet.mjs art/platformer/start/ilya_greet.mp4 art/platformer/start/ilya_greet_masked.mp4 --range 1-181` |
| `alyosha.png` | Алёша в позе ожидания на зелёном фоне: голубая рубаха, жёлтый пояс, нож в ножнах, руки скрещены | картинка хозяина 3840×2161, уменьшена до размера `ilya.png`; крапинок нет, очистка не нужна |
| `alyosha_greet.mp4` | утверждённый ролик syntx: руки опускает, стучит пальцем по виску с прищуром, снова скрещивает руки; 1920×1080 | запрос ролика Алёши ниже, по `alyosha.png` без уменьшения, поэтому шаг вперёд почти не виден |
| `alyosha_greet_masked.mp4` | ролик без зелёного, как у Ильи | `node tools/art/sheet.mjs art/platformer/start/alyosha_greet.mp4 art/platformer/start/alyosha_greet_masked.mp4 --range 1-121` |
| `vasilisa.png` | Василиса в позе ожидания на розовом фоне (`#FF00FF`): рыжая коса, красный сарафан, золотой огонёк в ладонях | запрос Codex по образцу `ilya.png` и облику с `sample.png`, затем очистка от крапинок (стала светлее на 2–4%) |
| `frog.png` | Царевна-лягушка в золотой короне на розовом фоне, ростом до колена Василисы, на месте её ступней | запрос Codex по `vasilisa.png`, программой опущена на 26 точек до уровня её подошв |
| `vasilisa_greet.mp4` | ролик syntx: лягушка в золотом сиянии превращается в Василису; 1916×1080 | запрос ролика Василисы ниже, первый кадр `frog.png`, последний `vasilisa.png` |
| `vasilisa_greet_masked.mp4` | ролик без розового фона | `node tools/art/sheet.mjs art/platformer/start/vasilisa_greet.mp4 art/platformer/start/vasilisa_greet_masked.mp4 --range 1-121 --key magenta --keep-color` |

Строки камня пишет движок поверх гладкой стороны: «Налево пойдёшь — силой возьмёшь», «Прямо пойдёшь — хитростью
пройдёшь», «Направо пойдёшь — мудростью одолеешь». Кнопки меню и название в игре — тоже текст движка.

## Порядок для каждого героя

1. Герой на зелёном фоне (`#00FF00`) запросом Codex по образцу `ilya.png`: тот же стиль, масштаб, свет справа.
   На герое ничего зелёного — иначе ключ вырежет дырку; героиню с зелёными чарами — на розовом фоне.
2. Отдельный запрос Codex на очистку от крапинок (GPT Image сыплет точки по всей картинке), сравнить с исходником
   кусками ×3.
3. Уменьшить кадр до 0,6 высоты и добить зелёным для ролика, запрос ролика в syntx, вырезка `sheet.mjs`.

Фильтр безопасности Codex отказывал в правке и рисовании Алёши (похоже, лицо смахивало на настоящего человека,
потом — временная блокировка после серии отказов); поэтому `alyosha.png` нарисован хозяином вне Codex.

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

## Ролик Ильи — два запроса и склейка (syntx; 16:9, 5 секунд, 1080p)

Шаг и жест в одном запросе syntx комкает: Илья не шагает, раздувает руку, рисует пол или подходит ближе нужного и
пятится. Поэтому шаг и бицепс — отдельные ролики, склеенные там, где шаг кончается позой `ilya.png`. В 720p
ролик выходил мыльным: на экране герой показан почти в родном размере.

Шаг — первый кадр `ilya_step_start.png`, последний `ilya.png`:

```
Animate from the first image to the last image: the first image is the first frame, the last image is the last
frame. Static camera: no zoom, no pan, no camera movement at all. The background stays perfectly flat, uniform
pure green for the whole video: no floor, no ground, no shadow under his feet, no light changes, no particles.

The bearded man does only one thing: in the first two seconds he takes one single calm step forward toward the
camera and stops exactly at the size and place of the last frame. He never comes closer and never gets bigger than
in the last frame, his head and feet stay fully inside the frame. For the rest of the video he just stands still
and relaxed in the pose of the last frame, breathing calmly. He keeps the same three-quarter turn and the same
pose the whole time: his right hand stays on his belt, his other hand keeps holding the axe, blade down, still.
No gestures, no turning, no walking back.

Keep his face, beard, hair, clothes, colors and the hand-painted animated film style exactly as in the images.
Calm, weighty, smooth motion with no motion blur. No morphing, no extra objects, no other people, no text.
```

Бицепс — по `ilya.png`:

```
Animate the attached image. Static camera: no zoom, no pan, no camera movement at all. The background stays
perfectly flat, uniform pure green for the whole video: no floor, no ground, no shadow under his feet, no light
changes, no particles. His feet stay planted on exactly the same spot for the whole video.

The bearded man starts exactly as in the image and starts moving right away. Over the first two seconds he
smoothly raises the arm that rests on his belt and flexes his biceps at shoulder height: upper arm out to the side,
forearm up, fist next to his head, with a broad proud grin. His arm and muscles keep the same size as in the image.
He holds the pose for one second, then over the last two seconds smoothly lowers the arm back to his belt and
ends exactly as in the first frame. His other hand keeps holding the axe, blade down and still, the whole time.

Keep his face, beard, hair, clothes, colors and the hand-painted animated film style exactly as in the image.
Calm, weighty, smooth motion with no motion blur. No morphing, no extra objects, no other people, no text.
```

## Запрос ролика Алёши (syntx, по картинке; 16:9, 5 секунд, 1080p)

```
Animate the attached image. Static camera: no zoom, no pan, no camera movement at all. The background stays
perfectly flat, uniform pure green for the whole video: no floor, no shadows, no light changes, no particles.

The young man starts exactly as in the image, standing relaxed with his arms crossed. He takes one light, easy
step forward toward the camera and stops; he gets a little bigger as he comes closer, and his feet stay fully
inside the frame. Then he uncrosses his arms, gives the viewer a sly wink with a knowing grin and taps his temple
twice with one finger, as if saying "I have a clever plan". He holds the look for a moment, then crosses his arms
again and stands relaxed. The knife stays in its sheath on his belt the whole time.

Keep his face, hair, blue shirt, yellow sash, clothes, colors and the hand-painted animated film style exactly as
in the image. Smooth, natural, nimble motion of a quick-witted young man. No morphing, no extra objects, no other
people, no text.
```

Вышло: вместо ухмылки с подмигиванием — прищур на оба глаза; хозяину так понравилось, ролик утверждён.

## Запрос ролика Василисы (syntx; 16:9, 5 секунд, 1080p; первый кадр `frog.png`, последний `vasilisa.png`)

Вышло с первого раза. Вырезка — `--key magenta --keep-color`: без `--keep-color` красный сарафан становится охристым.

```
Animate from the first image to the last image: the first image is the first frame, the last image is the last
frame. Static camera: no zoom, no pan, no camera movement at all. The background stays perfectly flat, uniform
pure magenta for the whole video: no floor, no ground, no shadow, no light changes on the background.

The crowned frog starts exactly as in the first frame, sitting calmly. It looks at the viewer, smiles and closes
its eyes; a soft warm golden glow appears around it and a swirl of tiny golden sparkles rises around it like a
small whirlwind. Inside the glow the frog grows and rises up and smoothly turns into the young woman, her figure
taking shape from the light: first the long red sarafan, then her blouse, her braid and her face. The glow fades,
the sparkles gather into the small golden light floating above her cupped hands, and she ends exactly as in the
last frame, standing calm with a gentle wise smile. The crown disappears in the glow.

Keep the frog, the young woman, their colors and the hand-painted animated film style exactly as in the images.
Smooth, magical, graceful motion. No morphing outside the glow, no extra objects, no other people or animals,
no text.
```

## Имена героев (программа, без нейросети)

Имя проявляется по буквам тем же волшебством, что превращение Василисы: дымка, полоса света снизу вверх, искры.
Задержка — когда герой вышел: Илья шагает, Алёша сразу, Василиса после превращения.

```
node tools/art/materialize.mjs games/platformer/fonts/Monomakh.ttf art/platformer/start/ilya_name.mp4 --text "Илья|Муромец" --delay 0.5 --reverse
node tools/art/materialize.mjs games/platformer/fonts/Monomakh.ttf art/platformer/start/alyosha_name.mp4 --text "Алёша|Попович" --delay 0.2 --reverse
node tools/art/materialize.mjs games/platformer/fonts/Monomakh.ttf art/platformer/start/vasilisa_name.mp4 --text "Василиса|Премудрая" --delay 2.6 --reverse
```

## Живой фон (фаза 08)

Сначала фон оживляли кусками: река и облака программой, листва берёзы роликом syntx поверх картины. За качающимися листьями виднелась неподвижная листва картины, а края кусков спорили между собой — хозяин 2026-10-26 выбрал оживить всю картину одним роликом.

Листопад с берёзы: `games/platformer/images/birch_leaves.png` — точки кроны берёзы из `start_bg.png` (многоугольник (1520,0)–(2048,0)–(2048,330)–(1900,300)–(1760,250)–(1700,140)–(1560,90)–(1520,60), точка зелёнее синего больше чем на 25), остальное прозрачно; панель с ней невидима (`opacity` 0), листья срываются с этих точек.

### Ролик всей картины (syntx, Seedance, Keyframes; 21:9, 5 секунд, 1080p; первый и последний кадр — `live/whole_frame.png`)

Хозяин 2026-10-26: оживить всю картину разом вместо кусков. Кадр — `background_wide.png` без строк камня (0, 26, 2072 × 888), увеличенный до 2520 × 1080. Пришёл 2200 × 940 (`live/whole_syntx.mp4`). Камень, дороги и крепость стоят; тёмный лес слева нейросеть не качает, а перерисовывает — там и на камне со строками остаётся картина (`hold` в `live/whole.json`). Нейросеть к началу не вернулась: лучшая пара кадров отличается в восемь раз сильнее соседних, и склейка смешиванием видна (хозяин, 2026-10-26). Поэтому круг — туда и обратно (`pingpong`): листва, трава и туман качаются назад естественно. Река назад течь не может — поверх неё свой кусок `flow.mjs`, сделанный из первого кадра ролика той же нарезкой, что у живого фона (`node tools/art/living.mjs art/platformer/start/live/whole.json --first art/platformer/start/live/whole_first.png`), чтобы вода и берега легли на ролик точка в точку. Неподвижные лес и камень — тоже из первого кадра ролика, строки высечены в нём (`node tools/art/carve.mjs art/platformer/start/live/whole_first.png art/platformer/start/inscriptions.json art/platformer/start/live/carved`, свечения оттуда не нужны), чтобы на краях цвет ролика встречался с тем же цветом. Сборка: `node tools/art/living.mjs art/platformer/start/live/whole.json art/platformer/start/live/whole.mp4`, готовое копируется в `games/platformer/images/living_bg.mp4`; река — `node tools/art/flow.mjs art/platformer/start/live/whole_first.png art/platformer/start/live/river.json art/platformer/start/live/river.mp4`, готовое — в `games/platformer/images/river.mp4`. Живой фон — размера всей картины: вне кадра ролика (полоски по 25 точек сверху и снизу) неподвижный кадр, у края ролика движение мягко сходит на нет — в окне 16:10 шва нет.

```
Static camera: locked off, no zoom, no pan, no camera movement at all. The first image and the last image are the
same frame, so the video loops seamlessly.

Bring this still painting to life with gentle natural motion, as a calm living picture: the leaves of the birch on
the right flutter and its hanging branches sway in a light summer breeze blowing to the right; the grass, the
wildflowers, the ferns and the bushes sway softly in slow waves; the river and the rapids on the right flow and
foam; the clouds drift slowly to the right; thin mist moves over the valley; the tops of the fir trees sway very
slightly. Then everything settles back exactly to the first frame.

The big grey standing stone in the middle, the boulders, the dirt roads, the castle and its towers, the hills and
the tree trunks stay perfectly still and unchanged.

Keep the hand-painted style, the colors, the light and every detail exactly as in the image. Subtle, calm motion.
No people, no animals, no birds, no insects, no new objects, no text.
```
