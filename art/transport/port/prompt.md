# Порт в начале партии — запрос образца

`concept.png` — образец порта, каким он стоит на берегу моря 1 января 1930 года, в начале партии: внешний
мир, где недостающее покупают дорого, а лишнее продают дёшево. Нарисован 2026-10-25 через расширение Codex
для Claude Code на уровне рассуждения «max»; образец лавки `art/transport/lavka/concept.png` приложен как
образец вида, камеры, света, фона и полуторки, общий кадр игры `art/transport/game/concept.png` — как образец
того, как порт выглядит на карте издали. Принят хозяином с первого раза. Отступления от запроса: прямой
стенки вдоль берега нет, двор переходит в каменистый пляж; причал идёт наискось, почти вдоль берега; крыши
ржавые, а не крашеные.

```
Image 1 (a village shop) is the reference for the rendering style, the camera angle, the lighting, the plain light grey background and the GAZ-AA truck. Image 2 (a whole game screen) shows, small and far away at the top right, the port this picture is about: wooden piers, timber warehouses, stacks of lumber, a crane. Draw a new, close-up scene of that port; take nothing else from the images.

Concept art of a small old sea port on the northern coast of Russia in summer 1930, for a realistic city-building tycoon game: the modest port of a forested province, built before the revolution of timber and stone, that ships out lumber and brings in flour, kerosene and goods. View from above at about 50 degrees, from the front-left, like a game camera. Sunlight comes from the front-left, from behind the viewer, so the facades facing the viewer are lit. Realistic 3D look with physically based materials: not a painting, not a cartoon.

The land is in front, the sea is behind. A dirt road with wheel ruts comes in from the bottom left of the frame onto a yard of packed dirt and gravel along the shore, about 60 meters wide and 30 meters deep. Behind the yard, a straight timber quay wall drops into calm grey-blue sea; a narrow strip of sand beach runs along the shore on both sides of the port. Around the scene the land and the sea fade into the plain light grey background. No other buildings, no trees, no people, no animals.

1. On the left part of the yard, parallel to the shore: a long one-storey warehouse of weathered grey vertical planks on a low rubble stone base, about 30 by 12 meters, with a low-pitched roof of old sheet iron painted faded red-brown. Along its long side facing the viewer, a covered plank loading platform at the height of a truck bed under a wide roof overhang, with three wide plank sliding doors. On the platform: flour sacks, wooden crates and a few steel barrels.
2. On the right part of the yard, near the road: a small one-storey port office of round weathered grey logs, about 6 by 8 meters, with white window frames, a plank porch and a sheet iron roof painted faded red-brown, with a brick chimney. On the wall beside the door hangs a small dark blue enamel plaque with white letters "ПОРТ" — exactly these four Cyrillic letters: П О Р Т.
3. From the middle of the quay a wooden pier about 8 meters wide runs straight out into the sea for about 40 meters: a deck of heavy weathered planks on log crib piers filled with stones, iron bollards along its edges, old wooden fenders hanging on the sides. On the pier stands a wooden derrick crane with a long boom, guy ropes and a hand winch, lifting a sling of boards.
4. At the end of the pier is moored one small old cargo steamship with a black hull, a white deck house, a tall black funnel with a thin wisp of smoke and two masts with cargo booms.
5. On the yard between the warehouse and the pier: neat stacks of fresh yellow sawn boards on wooden bearers, a pile of grey round logs, and a row of steel kerosene barrels lying on wooden chocks.
6. At the loading platform of the warehouse stands one 1930s Soviet GAZ-AA 1.5-ton flatbed truck with its side to the platform, for scale: green cab and hood, near-black fenders, wooden sides painted green; a few flour sacks in its bed.

No other text, signs or letters anywhere. Muted natural colors: weathered grey wood, grey stone, brown earth, grey-blue sea, pale sand. The only bright accents are the fresh yellow boards, the green truck and the blue plaque.
```

## Склад — виды для модели

`sheet-warehouse.png` — склад порта с `concept.png` строго спереди, сзади и с обоих торцов, в одном масштабе,
для сверки размеров модели. `concept.png` приложен как образец самого склада. Нарисован 2026-10-25 с первого
раза. Оба торца нарисованы зеркально (навес и лестница у торца с окном слева, а у вида с -X фасад справа):
при нарезке видов их отражать по горизонтали. Масштаб торцов и длинных сторон расходится примерно на 10%.

```
Image 1 shows a small 1930 sea port. Draw an orthographic model sheet of only the long timber warehouse on the left of image 1: the same building with the same planks, stone base, roof, loading platform, doors and colors. Nothing else from image 1: no truck, no cargo on the platform, no other buildings, no sea, no ground.

An orthographic model sheet for a 3D modeler, 3:2 landscape, on a plain flat light grey background. Four views of the same building at exactly the same scale, with true orthographic projection: no perspective, no vanishing points. The camera is level with the middle of the walls, so no roof top surface and no top of the platform deck is visible from above; the roof is seen only as its edge and slope from the side. Even soft front light, no cast shadows on the ground, no ground plane, no labels, no text, no dimension lines, no frames.

Layout: top row, the front view (the long side with the loading platform, facing the viewer), filling the width. Middle row, the back view (the long side facing the sea), the same width. Bottom row, side by side: the left end view and the right end view (the gables). All four views stand on the same horizontal ground level within their row.

The building: a long one-storey warehouse about 30 meters long and 12 meters deep, walls about 4.5 meters high, of weathered grey vertical planks with battens on a low base of rubble stone about 0.6 meters high. A low-pitched gable roof of old rusty red-brown sheet iron with standing seams, its ridge along the length, with a wide overhang on the front side that covers the loading platform and rests on a row of square wooden posts. Along the whole front: a plank loading platform at the height of a truck bed, about 1.1 meters, on short posts, with wooden steps at its left end. In the front wall, three wide plank sliding doors on iron rails. In the back wall toward the sea: two wide plank sliding doors at ground level. In the right gable: a wide double plank door. In the left gable: one small square window high under the roof. A short sheet iron vent on the ridge.

Realistic weathered materials, the same rendering style as image 1. Nothing else in the image.
```

## Контора — виды для модели

`sheet-office.png` — контора порта с `concept.png` строго спереди, сзади и с обоих торцов, в одном масштабе,
для сверки размеров модели. `concept.png` приложен как образец самой конторы. Нарисован 2026-10-25 с первого
раза. На торцах нет крыльца, хотя оно выступает от фасада: вынос крыльца брать с `concept.png`. Труба на торцах
сдвинута с конька. Масштаб торцов и длинных сторон расходится примерно на 15%.

```
Image 1 shows a small 1930 sea port. Draw an orthographic model sheet of only the small log office building on the right of image 1, the one with the blue plaque: the same building with the same logs, windows, porch, roof, chimney and colors. Nothing else from image 1: no crate by the porch, no grass, no other buildings, no cargo, no sea, no ground.

An orthographic model sheet for a 3D modeler, 3:2 landscape, on a plain flat light grey background. Four views of the same building at exactly the same scale, with true orthographic projection: no perspective, no vanishing points. The camera is level with the middle of the walls, so no top surface of the porch or the steps is visible from above; the roof is seen only as its edge and slope from the side. Even soft front light, no cast shadows on the ground, no ground plane, no labels, no dimension lines, no frames.

Layout: top row, side by side: the front view (the long side with the porch and the plaque) and the back view (the opposite long side). Bottom row, side by side: the left end view and the right end view (the gables). All four views stand on the same horizontal ground level within their row.

The building: a one-storey house about 8 meters long and 6 meters deep, walls about 3 meters high, of round weathered grey logs laid horizontally, with log ends protruding about 30 cm at all four corners, on a low base of rubble stone about 0.5 meters high. A gable roof of old rusty red-brown sheet iron with standing seams, its ridge along the length, with a short overhang. A red brick chimney rises through the ridge near the right end. White window frames with simple white boards around them.
- Front: from left to right, one window; the door of vertical planks in the middle-left, reached by a plank porch with three wooden steps under a small sheet iron canopy on two square wooden posts; on the log wall right beside the door, a small dark blue enamel plaque with white letters "ПОРТ" — exactly these four Cyrillic letters: П О Р Т; then two windows.
- Back: two windows, no door.
- Left gable: one window in the wall, the gable triangle of vertical planks above it.
- Right gable: one window in the wall and one small window in the gable triangle of vertical planks.

Realistic weathered materials, the same rendering style as image 1. No other text or letters anywhere. Nothing else in the image.
```

## Причал — виды для модели

`sheet-pier.png` — причал порта с `concept.png` сбоку и с обоих концов, в одном масштабе, целиком до морского
дна, без воды и без крана. `concept.png` приложен как образец самого причала. Нарисован 2026-10-25 с первого
раза. Срубов четыре, а не пять, пролёты между ними разные; настил на 4,7 м над низом срубов при длине 40 м,
а не на 6 м. Виды с концов нарисованы в 1,65 раза крупнее вида сбоку, пропорции у них с ним сходятся.

```
Image 1 shows a small 1930 sea port. Draw an orthographic model sheet of only the wooden pier that runs out into the sea in image 1: the same heavy weathered deck planks, log crib piers filled with stones, iron bollards and hanging wooden fenders. Nothing else from image 1: no crane, no ship, no cargo, no warehouse, no shore.

Show the whole structure as a 3D model on its own, with no water at all: the cribs stand down to their base on a flat sea bed, which is not drawn. An orthographic model sheet for a 3D modeler, 3:2 landscape, on a plain flat light grey background. Three views of the same pier at exactly the same scale, with true orthographic projection: no perspective, no vanishing points. The camera is level with the deck, so no top surface of the deck is visible from above. Even soft front light, no cast shadows, no ground plane, no labels, no text, no dimension lines, no frames.

Layout: top row, the side view (one long side of the pier, the land end on the left, the sea end on the right), filling the width. Bottom row, side by side: the sea end view and the land end view. All three views stand on the same horizontal base level.

The pier: about 40 meters long and 8 meters wide. The deck of heavy weathered grey planks laid across the pier stands about 6 meters above the base of the cribs. It rests on five square log cribs, each about 6 meters long and as wide as the pier, spaced evenly along it with open gaps between them: each crib is a box of round grey logs laid horizontally and notched at the corners, with rough grey stones visible between the logs, the lower part darker and stained by the sea. Over each gap between cribs, the deck is carried by heavy longitudinal timber beams on a row of vertical round timber piles. Along both long edges of the deck: a low timber curb, and squat cast iron bollards painted black, about every 8 meters. On both long sides, vertical wooden fender logs hang on chains from the deck edge down to just below the deck level. The land end is cut straight where the pier meets the quay; the sea end is closed by a log wall of the last crib with fender logs across it.

Realistic weathered materials, the same rendering style as image 1. Nothing else in the image.
```

## Кран — виды для модели

`sheet-crane.png` — деревянный кран с причала на `concept.png` спереди, сзади и с обеих сторон, в одном
масштабе, без груза. `concept.png` приложен как образец самого крана. Нарисован 2026-10-25 с первого раза.
Спереди и сзади виды верные: стрела под ~43°, а не 55°; мачта и стрела относительно основания короче
заказанных (мачта около 1,1 ширины основания), как и на `concept.png`. Боковые виды неполные: на правом нет
ни стрелы, ни лебёдки, на левом не видно конца стрелы над мачтой; в запросе стороны боковых видов были
перепутаны (с -X стрела уходит от зрителя). Стрелу брать только с видов спереди и сзади, боковые — для
основания, мачты и лебёдки.

```
Image 1 shows a small 1930 sea port. Draw an orthographic model sheet of only the wooden derrick crane standing on the pier in image 1: the same timber mast, long boom, hand winch, ropes, ironwork and colors. Nothing else from image 1: no load of boards, no pier, no ship, no cargo, no buildings.

An orthographic model sheet for a 3D modeler, 3:2 landscape, on a plain flat light grey background. Four views of the same crane at exactly the same scale, with true orthographic projection: no perspective, no vanishing points. The camera is level with the middle of the mast, so no top surface of the base frame is visible from above. Even soft front light, no cast shadows, no ground plane, no labels, no text, no dimension lines, no frames.

Layout, two rows of two views, all four standing on the same horizontal base level within their row:
- Top left, the front view: the boom lies in the picture plane and leans up to the right.
- Top right, the back view: the same crane seen from behind, so the boom leans up to the left.
- Bottom left, the left side view: seen from the left of the front view, the boom points straight toward the viewer and is seen end-on, in front of the mast.
- Bottom right, the right side view: seen from the right of the front view, the boom points straight away from the viewer, behind the mast.

The crane: a square base frame of heavy weathered timber beams, about 4 by 4 meters and 0.6 meters high, bolted with iron straps. In its middle stands a vertical square timber mast about 8 meters tall with an iron cap and a pulley at the top. A long timber boom about 10 meters long is hinged with an iron pin at the foot of the mast and leans up and out at about 55 degrees. A topping rope runs from the mast top to the boom tip, and the hoist rope runs from the winch over the mast top pulley and the boom tip pulley down to an iron hook that hangs empty about 3 meters below the boom tip. Four guy ropes run from the mast top down to the four corners of the base frame. On the base frame, at the back of the mast, opposite the boom: a hand winch, an iron frame with a big wooden rope drum, gear wheels and two crank handles. A short wooden ladder leads up the side of the base frame.

Realistic weathered materials: grey timber, black iron, rusty bolts, tarred hemp ropes, the same rendering style as image 1. Nothing else in the image.
```
