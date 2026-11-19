# Портреты Алёши и Василисы для игрового интерфейса «Распутья»

Встроенный imagegen, 19 ноября 2026 года. `gameplay_hud_alyosha.png` и `gameplay_hud_vasilisa.png` — два макета на основе утверждённого круглого портрета Ильи. `gameplay_hud_alyosha_vasilisa_detail.png` показывает оба портрета рядом со шкалами в масштабе исходного макета.

Входные изображения

1. `references/iron_frame_portrait_pair_template.png` — две копии утверждённого портрета Ильи из `gameplay_hud_bogatyr_embroidery_v4.png`.
2. `references/alyosha_in_game_book.png` — фрагмент рисунка Алёши из `games/platformer/images/hero_book_alyosha.png`.
3. `references/vasilisa_in_game_book.png` — фрагмент рисунка Василисы из `games/platformer/images/hero_book_vasilisa.png`.

Исходник генерации сохранён в `references/iron_frame_alyosha_vasilisa.png`. В макеты вставлены только рисунки внутри рамок. Железные ободья и шкалы взяты из утверждённого макета Ильи.

```text
Use case: precise-object-edit. Create ONE two-cell HUD portrait sheet for «Распутье» by editing image 1 exactly in place. Return one wide 2:1 image, two equal square cells side by side, both at the same zoom and positions as the template. Keep each circular iron ring, its pointed side tabs and rivets, and the dark forest outside exactly as they are. Replace the engraved person INSIDE each circle, and only inside its thin iron border. Each portrait is close-cropped from the top of the hair to the collar and shoulders, sized for a 110 px icon. Preserve the medium and color character of approved Ilya in image 1: the same warm ivory parchment, dark brown book engraving contours, controlled hatching, broad matte colors, warm amber light, rich facial definition, and restrained red embroidery. Continue the pictorial parchment into each inner edge. Do not add rays, lettering, props, new ornaments, inset outlines, modern effects, or extra rims.

LEFT circle — Alyosha Popovich, straight from the actual gameplay character book in image 2. Match THAT specific youthful clean-shaven face: sharp straight brows, cheek and chin, light wheat-blond swept wavy hair and his poised, slightly left-looking three-quarter angle. Show a little of his dusted plum tunic with the narrow cream-and-cinnabar embroidery on the neckline. No hands. Do not borrow features from Ilya.

RIGHT circle — Vasilisa Premudraya, straight from the actual gameplay character book in image 3. Match THAT specific front-facing calm expression and contours, chestnut center-parted waves, small gold drop earrings, and the thick braid dropping over the shoulder on the viewer's left. Show a little of her cream blouse, gold-trimmed straps and muted red sarafan. Keep the braid recognizable as it meets the bottom of the opening. No hands. Do not redesign her jewelry or hair.

Maintain equal portrait scale. Keep faces large, distinctive and recognizable at gameplay size. The circle centers, opening sizes, camera and ring outlines must match image 1 for exact fitting back into the approved HUD. One sheet, two portraits and no labels.
```
