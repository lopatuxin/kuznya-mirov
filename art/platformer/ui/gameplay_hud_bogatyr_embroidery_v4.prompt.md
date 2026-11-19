# Круглый портрет Ильи в железной рамке

Встроенный imagegen, 19 ноября 2026 года. `gameplay_hud_bogatyr_embroidery_v4.png` — полный макет. `gameplay_hud_bogatyr_embroidery_v4_detail.png` — портрет и обе шкалы в исходном масштабе.

Входные изображения

1. `references/iron_tips_bookmark_portrait.png` — верхний левый фрагмент предыдущего макета `gameplay_hud_bogatyr_embroidery_v3.png`.
2. `references/bar_tip_user.png` — образец вида железных наконечников.

Фрагмент `references/iron_tips_round_portrait.png` вставлен на место портрета. Обе шкалы и четыре железных наконечника совпадают с принятым макетом по пикселям.

```text
Use case: precise-object-edit. Edit image 1 in place. It is a square patch of the accepted «Распутье» HUD. Change only the shape and frame of the illustrated Ilya portrait on the left. Return the same square crop, at the same zoom and position, with the existing forest backdrop and partial red and yellow bars untouched. Image 2 is the reference for the forged iron style and shape of the gauge end fittings; ignore its embroidery and do not copy its gauges into the crop.

Make the portrait a precisely round compact medallion, roughly centered at x=44%, y=44% of the square and approximately 56% of the square wide. Replace the straight parchment edges with a narrow sculpted ring of dark forged gray iron. Match the caps already on the right: faceted bevel, slightly warm silver edge catching the light, and dark recessed joints. Add two very small pointed tabs on the ring at nine and three o'clock, an understated echo of the two iron fittings: each sweeps outward to a short triangular point and has one small round bolt, in the very same metal finish. The round silhouette must be dominant. Leave a definite gap between the right tab and the gauge tips. The entire portrait assembly including the tabs stays to the left of x=76% of the crop. Keep it simple and the rim slim, like another piece made by the same smith. No gold, color inlay, leather, horns, ornate flourishes or text.

Inside the ring retain the exact Ilya already painted in image 1: same engraved illustration, curls, brow, eyes, facial expression, angle, shapes of moustache and beard, warm ivory parchment, and embroidered red linen collar. Keep the hair and beard legible in the opening. Adjust the portrait's crop or scale only enough for it to fit inside the round opening. Do not redesign, reinterpret or change the face, the coloring or the book engraving style. Allow the forest artwork immediately around the new rim to show through where the old corners were. Keep the position and shape of both bars and all iron tips on those bars exactly as in image 1. Return a single square edit, edge to edge.
```
