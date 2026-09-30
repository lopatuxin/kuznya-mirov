//! «Картинки» → «Атлас и отрисовка»: packs every declared image's whole strip (frames and all —
//! slicing a strip into its individual frames is `frame_atlas_rect`'s job at draw time, not
//! `pack`'s) into a stack of fixed-size sheets by shelves (sort tallest first, lay left to right,
//! start a new shelf when a row would overflow, a new sheet when a shelf would), with a one-pixel
//! gap between neighbors and one opaque white pixel reserved for color fills, then writes the
//! points one sheet at a time (`fill_sheet`); also picks each image's current frame and, from `Game`'s
//! `World`, the world's own list of things to draw, by `layer`. No `wgpu`, no browser types — this
//! is the part of «Отрисовка» plain enough to run and test on any target, unlike the GPU resources
//! built from its result (see `super::gpu`, wasm32-only).

use crate::core::property;
use crate::core::scene::{self, CellRange, GroundLayer, SceneConfig};
use crate::core::screens::Fill;
use crate::core::time::frame_index;
use crate::core::value::{ImageId, Vec2};
use crate::core::world::World;
use crate::data::load::ImageDecl;

/// «Картинки» → «Атлас», требование 14: the canvas is this size in every browser, so the game
/// either starts everywhere or nowhere, never differently depending on what WebGPU would actually
/// allow.
pub const ATLAS_SIZE: u32 = 2048;

/// «Картинки» → «Атлас», требования 14, 21: the atlas is a stack of at most this many
/// `ATLAS_SIZE`×`ATLAS_SIZE` sheets — `pack` fails once a set of images needs one more.
pub const MAX_SHEETS: u32 = 16;

const PADDING: u32 = 1;

/// One image's whole strip, in RGBA8 — straight from the page's `getImageData`, not premultiplied
/// by alpha. `pixels.len()` must be `width * height * 4`; `pack` checks this itself rather than
/// trusting the caller, since it crosses the wasm/JS boundary before it ever gets here.
#[derive(Debug, Clone)]
pub struct AtlasImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A rectangle inside one atlas sheet, in that sheet's own pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// «Картинки» → «Атлас», требование 14: which of the atlas's stacked sheets this rectangle
    /// lives on — below `Atlas::sheet_count`, the sheet `fill_sheet` writes it into and, on the
    /// GPU, a layer of the same `D2Array` texture (`render::gpu`).
    pub sheet: u32,
}

/// «Картинки» → «Атлас»: a plain color fill is this pixel, stretched over the rectangle and
/// multiplied by the color — `pack` always places it here, on the first sheet, at a fixed spot
/// every atlas has regardless of which images the game declares (including none at all).
pub const WHITE_PIXEL: AtlasRect = AtlasRect {
    x: 0,
    y: 0,
    w: 1,
    h: 1,
    sheet: 0,
};

/// The packed layout, without the points themselves: `rects[i]` is where the `i`-th input image
/// (its whole strip) landed, in the same order `pack` was given them, so `i` doubles as that
/// image's `ImageId`; `sheet_count` sheets hold them all. The points are written by `fill_sheet`
/// one sheet at a time into one reused buffer, uploaded and overwritten by the next — wasm memory
/// never shrinks, so holding every sheet at once would keep up to `MAX_SHEETS` × `SHEET_BYTES`
/// reserved for the page's whole life.
#[derive(Debug)]
pub struct Atlas {
    pub rects: Vec<AtlasRect>,
    pub sheet_count: u32,
}

/// Bytes of one sheet's points: RGBA8, `ATLAS_SIZE` × `ATLAS_SIZE`.
pub const SHEET_BYTES: usize = ATLAS_SIZE as usize * ATLAS_SIZE as usize * 4;

fn set_pixel(pixels: &mut [u8], x: u32, y: u32, rgba: [u8; 4]) {
    let start = ((y * ATLAS_SIZE + x) * 4) as usize;
    pixels[start..start + 4].copy_from_slice(&rgba);
}

/// «Картинки» → «Сглаживание», требование 5: copies `src` into `rect` with its color channels
/// multiplied by its own alpha — so a sampled edge between an opaque point and a transparent
/// neighbor blends only the picture's own color and whatever is behind it, never a fraction of
/// black. Rounds to the nearest whole channel value (требование 6's own one-step tolerance).
fn blit(pixels: &mut [u8], rect: AtlasRect, src: &[u8]) {
    for row in 0..rect.h {
        let dst_row_start = (((rect.y + row) * ATLAS_SIZE + rect.x) * 4) as usize;
        let src_row_start = (row * rect.w * 4) as usize;
        for col in 0..rect.w as usize {
            let s = src_row_start + col * 4;
            let d = dst_row_start + col * 4;
            let alpha = src[s + 3];
            let factor = alpha as f32 / 255.0;
            pixels[d] = (src[s] as f32 * factor).round() as u8;
            pixels[d + 1] = (src[s + 1] as f32 * factor).round() as u8;
            pixels[d + 2] = (src[s + 2] as f32 * factor).round() as u8;
            pixels[d + 3] = alpha;
        }
    }
}

/// Total atlas-pixel area every image would occupy, `+1` for the white pixel — the "занятое
/// место" a doesn't-fit error names next to the atlas's own `MAX_SHEETS` limit.
fn occupied_area(images: &[AtlasImage]) -> u64 {
    1 + images
        .iter()
        .map(|img| img.width as u64 * img.height as u64)
        .sum::<u64>()
}

/// Shelf-packs every image's whole strip into a stack of `ATLAS_SIZE`×`ATLAS_SIZE` sheets —
/// «Картинки» → «Атлас», требование 14: sorted tallest first (a stable sort, so two images of the
/// same height keep `images`' own order), laid left to right on a shelf, wrapping to a new shelf
/// when a row would overflow the sheet's width, and to a new sheet when a shelf would overflow its
/// height — free space left on an earlier sheet is never revisited. `Err` names what didn't fit: a
/// single image wider or taller than one sheet on its own, or the declared set needing more than
/// `MAX_SHEETS` sheets (требование 21).
pub fn pack(images: &[AtlasImage]) -> Result<Atlas, String> {
    for img in images {
        let expected = img.width as usize * img.height as usize * 4;
        if img.pixels.len() != expected {
            return Err(format!(
                "картинка {}×{}: получено {} байт точек, ожидалось {expected}",
                img.width,
                img.height,
                img.pixels.len()
            ));
        }
        if img.width > ATLAS_SIZE || img.height > ATLAS_SIZE {
            return Err(format!(
                "картинка {}×{} шире или выше полотна {ATLAS_SIZE}×{ATLAS_SIZE}",
                img.width, img.height
            ));
        }
    }

    let mut order: Vec<usize> = (0..images.len()).collect();
    order.sort_by(|&a, &b| images[b].height.cmp(&images[a].height));

    let mut rects = vec![
        AtlasRect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            sheet: 0,
        };
        images.len()
    ];
    let mut sheet = 0u32;
    let mut cursor_x = WHITE_PIXEL.w + PADDING;
    let mut shelf_y = 0u32;
    let mut shelf_height = WHITE_PIXEL.h;

    for i in order {
        let img = &images[i];
        if cursor_x + img.width > ATLAS_SIZE {
            shelf_y += shelf_height + PADDING;
            cursor_x = 0;
            shelf_height = 0;
        }
        if shelf_y + img.height > ATLAS_SIZE {
            sheet += 1;
            if sheet >= MAX_SHEETS {
                return Err(format!(
                    "картинки не умещаются в атлас: предел {MAX_SHEETS} листов {ATLAS_SIZE}×{ATLAS_SIZE}; суммарно все картинки занимают {} точек",
                    occupied_area(images)
                ));
            }
            shelf_y = 0;
            cursor_x = 0;
            shelf_height = 0;
        }
        rects[i] = AtlasRect {
            x: cursor_x,
            y: shelf_y,
            w: img.width,
            h: img.height,
            sheet,
        };
        cursor_x += img.width + PADDING;
        shelf_height = shelf_height.max(img.height);
    }

    Ok(Atlas {
        rects,
        sheet_count: sheet + 1,
    })
}

/// «Картинки» → «Атлас», требование 14: writes sheet `sheet`'s points into `pixels`
/// (`SHEET_BYTES` long, reused from one sheet to the next): cleared to transparent, the white
/// pixel on the first sheet, and every image `atlas` placed on this sheet, color multiplied by
/// alpha (требование 5). `images` is the same slice `pack` was given.
pub fn fill_sheet(atlas: &Atlas, images: &[AtlasImage], sheet: u32, pixels: &mut [u8]) {
    pixels.fill(0);
    if sheet == WHITE_PIXEL.sheet {
        set_pixel(pixels, WHITE_PIXEL.x, WHITE_PIXEL.y, [255, 255, 255, 255]);
    }
    for (img, rect) in images.iter().zip(&atlas.rects) {
        if rect.sheet == sheet {
            blit(pixels, *rect, &img.pixels);
        }
    }
}

/// «Стек и инфраструктура» → «Ловушка WebGL2»: `wgpu-hal` 29.0.4's GLES backend turns a `D2`
/// texture of exactly one layer into a plain `TEXTURE_2D` and, when it's also square, one whose
/// layer count is a multiple of 6 into a cube map — either way `render::gpu`'s `D2Array` view stops
/// working. Called only for the WebGL2 backend (`render::gpu::Renderer::build_atlas`); WebGPU
/// allocates exactly `needed` layers. Pure and native-testable on purpose — the GPU texture itself
/// is wasm32-only (`render::gpu`, gated out of a native build), but this number has to be right
/// before any texture gets created.
pub fn webgl2_safe_layer_count(needed: u32) -> u32 {
    let mut layers = needed.max(1);
    while layers == 1 || layers.is_multiple_of(6) {
        layers += 1;
    }
    layers
}

/// «Картинки» → «Кадры»: the atlas rectangle an image's *current* frame occupies. The frame
/// itself is picked by `frame_index` from `elapsed_steps` (world steps taken, or interface time
/// already converted to the same unit — see `compose_world_paints`/`wasm::compose_ui`), then
/// sliced out of the strip's own whole-image rect left to right, no lookup table and no
/// allocation.
pub fn frame_atlas_rect(
    image: ImageId,
    elapsed_steps: f64,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> AtlasRect {
    let decl = &images[image];
    // «Картинки» → «Кадры», требование 5: a frame/tile set with neither `frame_time` nor
    // `frame_by` always shows frame 0 — never cycled by elapsed time.
    let frame = if decl.animated {
        frame_index(elapsed_steps, decl.frame_steps, decl.frames)
    } else {
        0
    };
    frame_atlas_rect_at(image, frame, images, atlas_rects)
}

/// «Картинки», требование 23; требования 1–3 (сетка): the atlas rectangle a *given* frame number
/// occupies — used by `frame_atlas_rect` (time-driven), `frame_by` (object-property-driven, see
/// `compose_world_paints`) and ground tiles (`compose_ground_paints`), which only differ in how
/// they arrive at `frame`. Without `columns` every frame lies in one row, left to right, as
/// before; with it, frame `n` lies at row `n / columns`, column `n % columns` — both slice an
/// exact whole-number fraction of the strip's own rect, checked at load time
/// (`data::load::validate_image_files`), so this never rounds.
fn frame_atlas_rect_at(
    image: ImageId,
    frame: u32,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> AtlasRect {
    let decl = &images[image];
    let whole = atlas_rects[image];
    let frame = frame.min(decl.frames.saturating_sub(1));
    match decl.columns {
        None => {
            let frame_w = whole.w / decl.frames.max(1);
            AtlasRect {
                x: whole.x + frame * frame_w,
                y: whole.y,
                w: frame_w,
                h: whole.h,
                sheet: whole.sheet,
            }
        }
        Some(columns) => {
            let columns = columns.max(1);
            let rows = decl.frames.div_ceil(columns);
            let frame_w = whole.w / columns;
            let frame_h = whole.h / rows.max(1);
            AtlasRect {
                x: whole.x + (frame % columns) * frame_w,
                y: whole.y + (frame / columns) * frame_h,
                w: frame_w,
                h: frame_h,
                sheet: whole.sheet,
            }
        }
    }
}

/// «Картинки», требование 23: `frame_by`'s own frame number — the named property, floored and
/// clamped to the strip's first/last frame; the first frame when the object doesn't carry the
/// property at all.
fn frame_by_index(world: &World, id: u32, prop: property::PropertyId, frame_count: u32) -> u32 {
    let raw = world.number_like(id, prop).unwrap_or(0.0);
    if raw <= 0.0 {
        return 0;
    }
    (raw.floor() as u64).min(frame_count.saturating_sub(1) as u64) as u32
}

/// A `Fill`'s resolved `(color, atlas rect, smooth)` for the current frame — «Картинки» → «Атлас»:
/// a color fill is the atlas's white pixel stretched and multiplied by the color, never smoothed;
/// an image fill leaves the color white (only its alpha carries `opacity`), samples the image's
/// current frame instead, and carries that image's own `smooth` (требование 3: a panel or button
/// draws an image the same way an object does). Either way the result is one draw instance, so a
/// fill and an image go out through the very same math — shared by a screen element's own `Fill`
/// (`wasm::compose_ui`) and, via a `Fill` built from a world object's `image`/`opacity`, by
/// `compose_world_paints` below.
pub fn fill_paint(
    fill: &Fill,
    elapsed_steps: f64,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> ([f32; 4], AtlasRect, bool) {
    match *fill {
        Fill::Color(c) => (c, WHITE_PIXEL, false),
        Fill::Image { image, opacity } => (
            [1.0, 1.0, 1.0, opacity],
            frame_atlas_rect(image, elapsed_steps, images, atlas_rects),
            images[image].smooth,
        ),
    }
}

/// «Трёхмерная сцена», требование 19: поворот плоского прямоугольника на земле — вся заливка вместе
/// с картинкой поворачивается вокруг середины объекта на угол `rotation`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn {
    pub pivot: [f32; 2],
    pub sin: f32,
    pub cos: f32,
}

impl Turn {
    pub const NONE: Turn = Turn {
        pivot: [0.0, 0.0],
        sin: 0.0,
        cos: 1.0,
    };
}

/// A rectangle's resolved placement and paint, free of `bytemuck`/`wgpu` so this builds and tests
/// natively — `wasm::compose_instances` turns each into a `DrawRect` for the GPU, unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectPaint {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub atlas_rect: AtlasRect,
    /// «Картинки», требование 24: quarter turns (0–3) clockwise, from the object's own
    /// `rotation` — always 0 for a color fill (rotation never affects a plain color).
    pub rotation_quarters: u8,
    /// «Картинки» → «Сглаживание»: the drawn image's own `smooth` — always `false` for a color
    /// fill (требование 3: only a picture's points blend into their neighbors).
    pub smooth: bool,
    /// «Картинки» → «Отражение», требование 7: the drawn object's own `flip_x` — always `false`
    /// for a color fill (требование 12: `flip_x` changes nothing about how a color draws).
    pub flip_x: bool,
    /// Поворот на земле в трёхмерной сцене; `Turn::NONE` в плоской.
    pub turn: Turn,
    /// Объект, чей это прямоугольник; `None` у плитки земли.
    pub object: Option<u32>,
}

/// «Картинки» → «Кадры»: the world's own frame is picked by steps taken (`elapsed_steps`, frozen
/// exactly when the world stops stepping — paused, or on the outcome screen); «Исполнение игры»:
/// rendering never changes the world, this just reads it. Every object with `position`, `size`
/// and a `color` or `image` sorts into one list by «Порядок рисования» (`scene::draw_order`) — a
/// color fill and an image fill mixed in the same order — «Картинки» → «Атлас и отрисовка»: a
/// color fill is the atlas's white pixel, an image fill samples its current frame, so the two
/// interleave exactly as if both were images.
pub fn compose_world_paints(
    world: &World,
    scene: &SceneConfig,
    ids: impl Iterator<Item = u32>,
    elapsed_steps: f64,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> Vec<RectPaint> {
    let three_d = world.three_d();
    // «Трёхмерная сцена», требование 19: фигуры рисует свой проход, плоскими прямоугольниками на земле
    // остаются только объекты без фигуры.
    let mut ordered: Vec<u32> = ids
        .filter(|&id| {
            world.vec2(id, property::POSITION).is_some()
                && world.vec2(id, property::SIZE).is_some()
                && (world.color(id, property::COLOR).is_some()
                    || world.image(id, property::IMAGE).is_some())
                && !(three_d && world.shape(id, property::SHAPE).is_some())
        })
        .collect();
    ordered.sort_by(|&a, &b| scene::draw_order(world, scene, a, b));

    ordered
        .into_iter()
        .map(|id| {
            let p = world.vec2(id, property::POSITION).expect("filtered above");
            let s = world.vec2(id, property::SIZE).expect("filtered above");
            let fill = match world.image(id, property::IMAGE) {
                Some(image) => {
                    // «Картинки»: `opacity` is checked to be within 0..=1 at load time, but a
                    // rule can still push it out of range at runtime (`["add", "opacity", 5]`) —
                    // clamped here rather than re-validated, since drawing never rejects data.
                    let opacity = world
                        .number_like(id, property::OPACITY)
                        .unwrap_or(1.0)
                        .clamp(0.0, 1.0) as f32;
                    Fill::Image { image, opacity }
                }
                None => Fill::Color(world.color(id, property::COLOR).expect("filtered above")),
            };
            let (color, mut atlas_rect, smooth) =
                fill_paint(&fill, elapsed_steps, images, atlas_rects);
            // «Картинки», требование 23: `frame_by` overrides the time-driven frame `fill_paint`
            // already picked — the object's own property decides instead.
            if let Fill::Image { image, .. } = fill
                && let Some(frame_by) = images[image].frame_by
            {
                let frame = frame_by_index(world, id, frame_by, images[image].frames);
                atlas_rect = frame_atlas_rect_at(image, frame, images, atlas_rects);
            }
            // «Картинки», требование 24: заливка цветом поворот не видит; требование 12: и
            // отражение тоже.
            let is_image = matches!(fill, Fill::Image { .. });
            let rotation_quarters = if is_image && !three_d {
                world
                    .rotation(id, property::ROTATION)
                    .map_or(0, |r| r.quarters())
            } else {
                0
            };
            let turn = match world.rotation(id, property::ROTATION) {
                Some(rotation) if three_d => {
                    let (sin, cos) = rotation.sin_cos();
                    Turn {
                        pivot: [(p[0] + s[0] / 2.0) as f32, (p[1] + s[1] / 2.0) as f32],
                        sin: sin as f32,
                        cos: cos as f32,
                    }
                }
                _ => Turn::NONE,
            };
            let flip_x = is_image && world.flag(id, property::FLIP_X);
            // «Картинки» → «Картинка своего размера», требования 8–10: own-size placement (and,
            // with a nonzero rotation, требование 9's rigid rotation around the object's own
            // middle) replaces the object's own rectangle only for drawing — every filter/sort
            // above already ran against `p`/`s` unchanged.
            let (position, size) = match fill {
                Fill::Image { image, .. } => {
                    own_size_rect(&images[image], p, s, rotation_quarters, flip_x)
                        .unwrap_or(([p[0] as f32, p[1] as f32], [s[0] as f32, s[1] as f32]))
                }
                Fill::Color(_) => ([p[0] as f32, p[1] as f32], [s[0] as f32, s[1] as f32]),
            };
            RectPaint {
                position,
                size,
                color,
                atlas_rect,
                rotation_quarters,
                smooth,
                flip_x,
                turn,
                object: Some(id),
            }
        })
        .collect()
}

/// «Картинки» → «Картинка своего размера», требование 9: rotates `v` by `quarters` (0–3)
/// clockwise, in scene cells (`y` down) — the same direction «Картинки», требование 24 rotates a
/// frame's sampled corners in.
fn rotate_quarter(v: [f32; 2], quarters: u8) -> [f32; 2] {
    match quarters % 4 {
        0 => v,
        1 => [-v[1], v[0]],
        2 => [-v[0], -v[1]],
        _ => [v[1], -v[0]],
    }
}

/// «Картинки» → «Картинка своего размера», требования 6, 8–9: `decl.size`'s own drawn rectangle —
/// `None` without one (требование 7: the object's own rectangle draws unchanged). `anchor`'s own
/// point on the image's rectangle coincides with the same point on the object's rectangle,
/// shifted by `offset` (требование 8); a nonzero `rotation_quarters` then rotates that whole
/// rectangle, as a rigid body, around the object's own middle (требование 9) — width/height swap
/// on an odd quarter turn, since only a swap keeps an axis-aligned rectangle axis-aligned after a
/// quarter turn around a point other than its own center. «Картинки» → «Отражение», требование 8:
/// `flip_x` mirrors left and right before any of that — the anchor's own left/right point swaps
/// (`Anchor::flip_x`) and `offset.x` negates — so the same math places the mirrored rectangle, and
/// требование 9's rotation then turns that already-mirrored rectangle exactly as it would any
/// other.
fn own_size_rect(
    decl: &ImageDecl,
    obj_position: Vec2,
    obj_size: Vec2,
    rotation_quarters: u8,
    flip_x: bool,
) -> Option<([f32; 2], [f32; 2])> {
    let size = decl.size?;
    let obj_pos = [obj_position[0] as f32, obj_position[1] as f32];
    let obj_size = [obj_size[0] as f32, obj_size[1] as f32];
    let img_size = [size[0] as f32, size[1] as f32];
    let mut offset = [decl.offset[0] as f32, decl.offset[1] as f32];
    let anchor = if flip_x {
        offset[0] = -offset[0];
        decl.anchor.flip_x()
    } else {
        decl.anchor
    };

    let anchor_point = anchor.point_on(obj_pos, obj_size);
    let target = [anchor_point[0] + offset[0], anchor_point[1] + offset[1]];
    let own_anchor = anchor.point_on([0.0, 0.0], img_size);
    let top_left = [target[0] - own_anchor[0], target[1] - own_anchor[1]];

    if rotation_quarters == 0 {
        return Some((top_left, img_size));
    }
    let pivot = [
        obj_pos[0] + obj_size[0] / 2.0,
        obj_pos[1] + obj_size[1] / 2.0,
    ];
    let center = [
        top_left[0] + img_size[0] / 2.0,
        top_left[1] + img_size[1] / 2.0,
    ];
    let rotated = rotate_quarter(
        [center[0] - pivot[0], center[1] - pivot[1]],
        rotation_quarters,
    );
    let new_center = [pivot[0] + rotated[0], pivot[1] + rotated[1]];
    let new_size = if rotation_quarters % 2 == 1 {
        [img_size[1], img_size[0]]
    } else {
        img_size
    };
    let new_top_left = [
        new_center[0] - new_size[0] / 2.0,
        new_center[1] - new_size[1] / 2.0,
    ];
    Some((new_top_left, new_size))
}

/// «Мир на экране» → «Земля», требования 17–22: one rect per scene cell at least partly inside
/// `visible` (`SceneConfig::visible_cell_range` — требование 20), layer by layer in `ground`'s own
/// list order (требование 18: every one of these ends up first in the caller's own draw list, so
/// every object — even one with a negative `layer` — always draws on top). `-1` (требование 16)
/// and any row/column `cells` doesn't reach (never happens once loaded — checked at load time)
/// both just draw nothing.
pub fn compose_ground_paints(
    ground: &[GroundLayer],
    visible: CellRange,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> Vec<RectPaint> {
    let cols = (visible.x1 - visible.x0) as usize;
    let rows = (visible.y1 - visible.y0) as usize;
    let mut out = Vec::with_capacity(cols * rows * ground.len());
    for layer in ground {
        for y in visible.y0..visible.y1 {
            let Some(row) = layer.cells.get(y as usize) else {
                continue;
            };
            for x in visible.x0..visible.x1 {
                let Some(&n) = row.get(x as usize) else {
                    continue;
                };
                if n < 0 {
                    continue;
                }
                out.push(RectPaint {
                    position: [x as f32, y as f32],
                    size: [1.0, 1.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    atlas_rect: frame_atlas_rect_at(layer.image, n as u32, images, atlas_rects),
                    rotation_quarters: 0,
                    smooth: images[layer.image].smooth,
                    flip_x: false,
                    turn: Turn::NONE,
                    object: None,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_scene() -> SceneConfig {
        SceneConfig {
            width: 100,
            height: 100,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
            camera: None,
            light: Default::default(),
        }
    }

    fn img(w: u32, h: u32) -> AtlasImage {
        AtlasImage {
            width: w,
            height: h,
            pixels: vec![7u8; (w * h * 4) as usize],
        }
    }

    fn overlap(a: AtlasRect, b: AtlasRect) -> bool {
        !(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y)
    }

    fn sheet_pixels(atlas: &Atlas, images: &[AtlasImage], sheet: u32) -> Vec<u8> {
        let mut pixels = vec![0u8; SHEET_BYTES];
        fill_sheet(atlas, images, sheet, &mut pixels);
        pixels
    }

    fn pixel_at(pixels: &[u8], x: u32, y: u32) -> &[u8] {
        let start = ((y * ATLAS_SIZE + x) * 4) as usize;
        &pixels[start..start + 4]
    }

    #[test]
    fn empty_input_still_places_the_white_pixel() {
        let atlas = pack(&[]).expect("пустой набор — не ошибка");
        assert!(atlas.rects.is_empty());
        assert_eq!(atlas.sheet_count, 1, "игра без картинок — один лист");
        let sheet = sheet_pixels(&atlas, &[], 0);
        assert_eq!(
            pixel_at(&sheet, WHITE_PIXEL.x, WHITE_PIXEL.y),
            &[255, 255, 255, 255]
        );
    }

    /// Один буфер на все листы: лист, заполненный поверх прежнего, не хранит его картинок — только
    /// свои.
    #[test]
    fn a_sheet_filled_over_a_reused_buffer_keeps_nothing_of_the_previous_sheet() {
        let images = vec![
            AtlasImage {
                width: 1200,
                height: 1100,
                pixels: [9u8, 9, 9, 255].repeat(1200 * 1100),
            },
            AtlasImage {
                width: 10,
                height: 1100,
                pixels: [5u8, 5, 5, 255].repeat(10 * 1100),
            },
            AtlasImage {
                width: 1200,
                height: 1100,
                pixels: [3u8, 3, 3, 255].repeat(1200 * 1100),
            },
        ];
        let atlas = pack(&images).unwrap();
        let first = atlas.rects[0];
        let last = atlas.rects[2];
        assert_ne!(first.sheet, last.sheet, "{:?}", atlas.rects);

        let mut pixels = vec![0u8; SHEET_BYTES];
        fill_sheet(&atlas, &images, first.sheet, &mut pixels);
        assert_eq!(pixel_at(&pixels, first.x, first.y), &[9, 9, 9, 255]);
        fill_sheet(&atlas, &images, last.sheet, &mut pixels);
        assert_eq!(pixel_at(&pixels, last.x, last.y), &[3, 3, 3, 255]);
        let foreign = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .find(|px| **px != [0, 0, 0, 0] && **px != [3, 3, 3, 255]);
        assert_eq!(foreign, None, "на втором листе остались точки первого");
    }

    #[test]
    fn three_images_of_different_heights_fit_without_overlapping_and_with_a_one_pixel_gap() {
        let images = vec![img(10, 40), img(15, 5), img(8, 20)];
        let atlas = pack(&images).expect("три разных картинки должны уместиться");
        assert_eq!(atlas.rects.len(), 3);
        let mut all = atlas.rects.clone();
        all.push(WHITE_PIXEL);
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert!(!overlap(all[i], all[j]), "{:?} vs {:?}", all[i], all[j]);
            }
        }
        // The one-pixel gap: every rect's left edge is either 0 or at least one pixel past
        // every other rect it shares a row with.
        for &r in &atlas.rects {
            for &other in &all {
                if r == other || r.y != other.y {
                    continue;
                }
                if other.x < r.x {
                    assert!(r.x >= other.x + other.w + PADDING, "{r:?} vs {other:?}");
                }
            }
        }
    }

    /// Опаque points don't change under premultiplication (требование 6) — «unmodified» here
    /// means an opaque source pixel, not any pixel: a translucent one is scaled down, checked
    /// separately in `blit_multiplies_color_by_alpha_and_leaves_opaque_and_transparent_points`.
    #[test]
    fn blitted_pixels_land_at_their_own_rect_unmodified() {
        let images = vec![AtlasImage {
            width: 4,
            height: 3,
            pixels: [7u8, 7, 7, 255].repeat(4 * 3),
        }];
        let atlas = pack(&images).unwrap();
        let rect = atlas.rects[0];
        let sheet = sheet_pixels(&atlas, &images, rect.sheet);
        for row in 0..rect.h {
            let start = (((rect.y + row) * ATLAS_SIZE + rect.x) * 4) as usize;
            let len = (rect.w * 4) as usize;
            assert!(
                sheet[start..start + len]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|px| *px == [7, 7, 7, 255])
            );
        }
    }

    #[test]
    fn an_image_wider_than_the_atlas_is_rejected() {
        let images = vec![img(ATLAS_SIZE + 1, 10)];
        let err = pack(&images).expect_err("шире полотна — ошибка");
        assert!(err.contains("полотна"), "{err}");
    }

    #[test]
    fn an_image_taller_than_the_atlas_is_rejected() {
        let images = vec![img(10, ATLAS_SIZE + 1)];
        assert!(pack(&images).is_err());
    }

    /// «Картинки» → «Атлас», требование 21: 17 картинок 2048×2048 нужны 17+ листов (белая точка
    /// на первом сдвигает даже первую картинку на второй) — больше предела `MAX_SHEETS`, ошибка
    /// называет и предел, и суммарно занятые точки.
    #[test]
    fn seventeen_full_sheet_images_exceed_the_sheet_limit() {
        let images: Vec<AtlasImage> = (0..17).map(|_| img(ATLAS_SIZE, ATLAS_SIZE)).collect();
        let err = pack(&images).expect_err("17 листов 2048×2048 — ошибка предела листов");
        assert!(err.contains(&MAX_SHEETS.to_string()), "{err}");
        assert!(err.contains(&ATLAS_SIZE.to_string()), "{err}");
    }

    /// «Крайние случаи»: «Две картинки 1200×1100 … каждая на своём листе, игра запускается» —
    /// раньше (одно полотно) это была ошибка нехватки полок; с листами каждая просто уходит на
    /// свой собственный лист.
    #[test]
    fn two_images_that_do_not_share_a_shelf_land_on_separate_sheets() {
        let images = vec![img(1200, 1100), img(1200, 1100)];
        let atlas = pack(&images).expect("каждая картинка получает свой лист");
        assert_ne!(
            atlas.rects[0].sheet, atlas.rects[1].sheet,
            "{:?}",
            atlas.rects
        );
        assert_eq!(atlas.sheet_count, 2);
    }

    /// «Крайние случаи»: «Одна картинка ровно 2048×2048: на первом листе ей мешает белая точка,
    /// она целиком ложится на второй лист.»
    #[test]
    fn one_full_sheet_image_lands_entirely_on_the_second_sheet() {
        let images = vec![img(ATLAS_SIZE, ATLAS_SIZE)];
        let atlas = pack(&images).expect("одна картинка ровно с лист — не ошибка");
        assert_eq!(atlas.rects[0].sheet, 1);
        assert_eq!(
            atlas.rects[0],
            AtlasRect {
                x: 0,
                y: 0,
                w: ATLAS_SIZE,
                h: ATLAS_SIZE,
                sheet: 1,
            }
        );
        assert_eq!(atlas.sheet_count, 2);
    }

    /// «Картинки» → «Атлас», требования 14, 16: картинки суммарно больше одного листа ложатся на
    /// два, ни одна не пересекает другую и край своего листа, у каждой свой номер листа.
    #[test]
    fn images_that_overflow_one_sheet_spread_across_two_without_crossing_edges() {
        let images: Vec<AtlasImage> = (0..4).map(|_| img(1100, 1100)).collect();
        let atlas = pack(&images).expect("должно уместиться на нескольких листах");
        assert!(atlas.sheet_count >= 2, "{:?}", atlas.rects);
        for r in &atlas.rects {
            assert!(r.x + r.w <= ATLAS_SIZE && r.y + r.h <= ATLAS_SIZE, "{r:?}");
        }
        for i in 0..atlas.rects.len() {
            for j in (i + 1)..atlas.rects.len() {
                let a = atlas.rects[i];
                let b = atlas.rects[j];
                if a.sheet != b.sheet {
                    continue;
                }
                assert!(!overlap(a, b), "{a:?} vs {b:?}");
            }
        }
    }

    /// «Картинки» → «Сглаживание», требования 5–6: атлас хранит цвет, умноженный на прозрачность
    /// — `(200, 100, 50, 128)` → `(100, 50, 25, 128)`, непрозрачная точка не меняется, прозрачная
    /// становится `(0, 0, 0, 0)`.
    #[test]
    fn blit_multiplies_color_by_alpha_and_leaves_opaque_and_transparent_points() {
        let images = vec![AtlasImage {
            width: 3,
            height: 1,
            pixels: vec![
                200, 100, 50, 128, // полупрозрачная
                10, 20, 30, 255, // непрозрачная
                255, 255, 255, 0, // полностью прозрачная
            ],
        }];
        let atlas = pack(&images).unwrap();
        let rect = atlas.rects[0];
        let sheet = sheet_pixels(&atlas, &images, rect.sheet);
        let px = |col: u32| pixel_at(&sheet, rect.x + col, rect.y);
        assert_eq!(px(0), [100, 50, 25, 128]);
        assert_eq!(px(1), [10, 20, 30, 255]);
        assert_eq!(px(2), [0, 0, 0, 0]);
    }

    /// «Стек и инфраструктура» → «Ловушка WebGL2»: 1 и число, кратное 6, поднимаются до
    /// ближайшего безопасного значения; всё остальное остаётся как есть.
    #[test]
    fn webgl2_safe_layer_count_avoids_one_and_multiples_of_six() {
        for (needed, expected) in [(1, 2), (6, 7), (12, 13), (2, 2)] {
            assert_eq!(webgl2_safe_layer_count(needed), expected, "needed={needed}");
        }
    }

    #[test]
    fn rect_wgsl_atlas_size_matches_the_rust_constant() {
        let shader = include_str!("../../shaders/rect.wgsl");
        let marker = "const ATLAS_SIZE: f32 = ";
        let start = shader.find(marker).expect("шейдер объявляет ATLAS_SIZE") + marker.len();
        let rest = &shader[start..];
        let end = rest
            .find(';')
            .expect("объявление ATLAS_SIZE оканчивается ;");
        let declared: f32 = rest[..end].trim().parse().expect("ATLAS_SIZE — число");
        assert_eq!(declared, ATLAS_SIZE as f32);
    }

    /// `smooth` is a WGSL reserved word — naga rejects an instance field with that name, but only
    /// wasm32-side (`include_str!` is opaque to a native build), so this is checked on the source
    /// text instead, same as `rect_wgsl_atlas_size_matches_the_rust_constant` above.
    #[test]
    fn rect_wgsl_does_not_declare_a_reserved_smooth_field() {
        let shader = include_str!("../../shaders/rect.wgsl");
        assert!(
            !shader.contains("smooth: f32"),
            "поле smooth — зарезервированное слово WGSL"
        );
        assert!(
            shader.contains("smooth_flag"),
            "должно называться smooth_flag"
        );
    }

    /// «Стек и инфраструктура» → «Ловушка WebGL2»: naga's GLSL writer refuses to sample one
    /// texture through two samplers (`ImageMultipleSamplers`) — the shader must bind at most one.
    #[test]
    fn rect_wgsl_binds_the_atlas_texture_to_at_most_one_sampler() {
        let shader = include_str!("../../shaders/rect.wgsl");
        let sampler_bindings = shader.matches(": sampler;").count();
        assert_eq!(sampler_bindings, 1, "{shader}");
    }

    /// «Картинки» → «Сглаживание», требование 5: `PREMULTIPLIED_ALPHA_BLENDING` (`render::gpu`)
    /// expects the fragment's whole output premultiplied, not just the atlas's own points — a
    /// color fill's own alpha (opacity, translucent panels) must be premultiplied too.
    #[test]
    fn rect_wgsl_premultiplies_the_instance_color_by_its_own_alpha() {
        let shader = include_str!("../../shaders/rect.wgsl");
        assert!(
            shader.contains("instance.color.rgb * instance.color.a"),
            "{shader}"
        );
    }

    #[test]
    fn wrong_pixel_buffer_length_is_rejected() {
        let broken = AtlasImage {
            width: 4,
            height: 4,
            pixels: vec![0u8; 10],
        };
        assert!(pack(&[broken]).is_err());
    }

    #[test]
    fn rects_are_indexed_the_same_as_the_input_order_not_the_packing_order() {
        // Sorted tallest-first internally, but `rects[i]` must still answer for `images[i]`.
        let images = vec![img(5, 1), img(5, 100), img(5, 50)];
        let atlas = pack(&images).unwrap();
        assert_eq!(atlas.rects[0].h, 1);
        assert_eq!(atlas.rects[1].h, 100);
        assert_eq!(atlas.rects[2].h, 50);
    }

    use crate::core::input::MouseState;
    use crate::core::property::PropertyTable;
    use crate::core::screens::{Anchor, button_fill};

    fn one_frame_image() -> ImageDecl {
        ImageDecl {
            name: "x".to_string(),
            path: "x.png".to_string(),
            frames: 1,
            frame_steps: 1,
            animated: false,
            columns: None,
            size: None,
            anchor: Anchor::Center,
            offset: [0.0, 0.0],
            frame_by: None,
            frame_by_name: None,
            smooth: false,
        }
    }

    /// «Картинки»: план фазы 04 — «объект с color и объект с image попадают в один список
    /// отрисовки в порядке layer, у первого прямоугольник атласа — белая точка».
    #[test]
    fn color_and_image_objects_share_one_list_ordered_by_layer() {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);

        let image_obj = world.create();
        world.set_vec2(image_obj, property::POSITION, [2.0, 2.0]);
        world.set_vec2(image_obj, property::SIZE, [3.0, 3.0]);
        world.set_image(image_obj, property::IMAGE, 0);
        world.set_layer(image_obj, property::LAYER, 1);

        let color_obj = world.create();
        world.set_vec2(color_obj, property::POSITION, [0.0, 0.0]);
        world.set_vec2(color_obj, property::SIZE, [1.0, 1.0]);
        world.set_color(color_obj, property::COLOR, [1.0, 0.0, 0.0, 1.0]);
        world.set_layer(color_obj, property::LAYER, 0);

        let images = vec![one_frame_image()];
        let atlas_rects = vec![AtlasRect {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
            sheet: 0,
        }];

        let scene = test_scene();
        let paints = compose_world_paints(&world, &scene, world.ids(), 0.0, &images, &atlas_rects);
        assert_eq!(paints.len(), 2, "{paints:?}");
        assert_eq!(paints[0].color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(paints[0].atlas_rect, WHITE_PIXEL);
        assert_eq!(paints[1].atlas_rect, atlas_rects[0]);
        assert_eq!(paints[1].color, [1.0, 1.0, 1.0, 1.0]);
    }

    /// «Картинки» → «Сглаживание», требование 3: `smooth` доходит от описания картинки до списка
    /// рисования у объекта мира — заливка цветом никогда не сглажена, что бы ни стояло у самого
    /// объекта.
    #[test]
    fn smooth_reaches_the_draw_list_for_a_world_object_but_never_for_a_color_fill() {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);

        let mut smooth_image = one_frame_image();
        smooth_image.smooth = true;
        let images = vec![smooth_image, one_frame_image()];
        let atlas_rects = vec![
            AtlasRect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
                sheet: 0,
            },
            AtlasRect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
                sheet: 0,
            },
        ];

        let smooth_obj = world.create();
        world.set_vec2(smooth_obj, property::POSITION, [0.0, 0.0]);
        world.set_vec2(smooth_obj, property::SIZE, [1.0, 1.0]);
        world.set_image(smooth_obj, property::IMAGE, 0);

        let crisp_obj = world.create();
        world.set_vec2(crisp_obj, property::POSITION, [0.0, 0.0]);
        world.set_vec2(crisp_obj, property::SIZE, [1.0, 1.0]);
        world.set_image(crisp_obj, property::IMAGE, 1);

        let color_obj = world.create();
        world.set_vec2(color_obj, property::POSITION, [0.0, 0.0]);
        world.set_vec2(color_obj, property::SIZE, [1.0, 1.0]);
        world.set_color(color_obj, property::COLOR, [1.0, 1.0, 1.0, 1.0]);

        let scene = test_scene();
        let paints = compose_world_paints(&world, &scene, world.ids(), 0.0, &images, &atlas_rects);
        let smooth: Vec<bool> = paints.iter().map(|p| p.smooth).collect();
        assert_eq!(smooth, vec![true, false, false], "{paints:?}");
    }

    /// «Картинки»: план фазы 04 — «opacity 0.5 умножает прозрачность, opacity 1 и его отсутствие
    /// дают одно и то же»; a rule can still push it out of range at runtime, clamped to 0..=1.
    #[test]
    fn opacity_multiplies_alpha_and_clamps_to_the_valid_range() {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);
        let images = vec![one_frame_image()];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 8,
            h: 8,
            sheet: 0,
        }];

        let mut object = |opacity: Option<f64>| {
            let id = world.create();
            world.set_vec2(id, property::POSITION, [0.0, 0.0]);
            world.set_vec2(id, property::SIZE, [1.0, 1.0]);
            world.set_image(id, property::IMAGE, 0);
            if let Some(o) = opacity {
                world.set_number(id, property::OPACITY, o);
            }
            id
        };
        object(Some(0.5));
        object(Some(1.0));
        object(None);
        object(Some(5.0));
        object(Some(-1.0));

        let scene = test_scene();
        let paints = compose_world_paints(&world, &scene, world.ids(), 0.0, &images, &atlas_rects);
        let alpha: Vec<f32> = paints.iter().map(|p| p.color[3]).collect();
        assert_eq!(alpha, vec![0.5, 1.0, 1.0, 1.0, 0.0], "{alpha:?}");
    }

    /// «Картинки»: план фазы 04 — «кнопка без image_hover под курсором показывает image, с
    /// image_hover — его», доведённое до самого атласного прямоугольника.
    #[test]
    fn button_hover_without_its_own_image_keeps_showing_the_base_image() {
        let images = vec![one_frame_image()];
        let atlas_rects = vec![AtlasRect {
            x: 5,
            y: 5,
            w: 20,
            h: 20,
            sheet: 0,
        }];
        let base = Fill::Image {
            image: 0,
            opacity: 1.0,
        };
        let mouse = MouseState {
            hover: Some(0),
            captured: None,
            ..Default::default()
        };

        let chosen = button_fill(&base, &base, &base, 0, &mouse);
        let (_, atlas_rect, _) = fill_paint(chosen, 0.0, &images, &atlas_rects);
        assert_eq!(atlas_rect, atlas_rects[0]);
    }

    #[test]
    fn button_hover_with_its_own_image_shows_that_image() {
        let images = vec![one_frame_image(), one_frame_image()];
        let atlas_rects = vec![
            AtlasRect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
                sheet: 0,
            },
            AtlasRect {
                x: 50,
                y: 50,
                w: 10,
                h: 10,
                sheet: 0,
            },
        ];
        let base = Fill::Image {
            image: 0,
            opacity: 1.0,
        };
        let hover = Fill::Image {
            image: 1,
            opacity: 1.0,
        };
        let mouse = MouseState {
            hover: Some(0),
            captured: None,
            ..Default::default()
        };

        let chosen = button_fill(&base, &hover, &base, 0, &mouse);
        let (_, atlas_rect, _) = fill_paint(chosen, 0.0, &images, &atlas_rects);
        assert_eq!(atlas_rect, atlas_rects[1]);
    }

    /// «Картинки» → «Сглаживание», требование 3: панель или кнопка рисует картинку так же, как
    /// объект — `fill_paint` доносит `smooth` до самого результата; заливка цветом — всегда
    /// `false`.
    #[test]
    fn fill_paint_carries_the_images_own_smooth_but_never_for_a_color_fill() {
        let mut smooth_image = one_frame_image();
        smooth_image.smooth = true;
        let images = vec![smooth_image];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            sheet: 0,
        }];
        let image_fill = Fill::Image {
            image: 0,
            opacity: 1.0,
        };
        let (_, _, smooth) = fill_paint(&image_fill, 0.0, &images, &atlas_rects);
        assert!(smooth);
        let color_fill = Fill::Color([1.0, 0.0, 0.0, 1.0]);
        let (_, _, smooth) = fill_paint(&color_fill, 0.0, &images, &atlas_rects);
        assert!(!smooth);
    }

    /// «Картинки», требование 23: `frame_by` picks the frame from the object's own property,
    /// floored and clamped to the strip's first/last frame — the first frame when the object
    /// doesn't carry the property at all — ignoring `elapsed_steps` entirely.
    #[test]
    fn frame_by_overrides_the_time_driven_frame_with_the_objects_own_property() {
        let mut properties = PropertyTable::new();
        let hits = properties
            .declare_author("hits", crate::core::value::PropKind::Number)
            .unwrap();
        let mut world = World::new(&properties);

        let images = vec![ImageDecl {
            name: "strip".to_string(),
            path: "strip.png".to_string(),
            frames: 3,
            frame_steps: 1,
            animated: false,
            columns: None,
            size: None,
            anchor: Anchor::Center,
            offset: [0.0, 0.0],
            frame_by: Some(hits),
            frame_by_name: None,
            smooth: false,
        }];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 30,
            h: 10,
            sheet: 0,
        }];

        let no_hits = world.create();
        world.set_vec2(no_hits, property::POSITION, [0.0, 0.0]);
        world.set_vec2(no_hits, property::SIZE, [1.0, 1.0]);
        world.set_image(no_hits, property::IMAGE, 0);

        let mid = world.create();
        world.set_vec2(mid, property::POSITION, [0.0, 0.0]);
        world.set_vec2(mid, property::SIZE, [1.0, 1.0]);
        world.set_image(mid, property::IMAGE, 0);
        world.set_number(mid, hits, 1.7);

        let past_end = world.create();
        world.set_vec2(past_end, property::POSITION, [0.0, 0.0]);
        world.set_vec2(past_end, property::SIZE, [1.0, 1.0]);
        world.set_image(past_end, property::IMAGE, 0);
        world.set_number(past_end, hits, 50.0);

        // `elapsed_steps` is nonzero on purpose: frame_by must ignore it entirely.
        let scene = test_scene();
        let paints =
            compose_world_paints(&world, &scene, world.ids(), 999.0, &images, &atlas_rects);
        assert_eq!(
            paints[0].atlas_rect,
            AtlasRect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
                sheet: 0,
            },
            "нет свойства — первый кадр"
        );
        assert_eq!(
            paints[1].atlas_rect,
            AtlasRect {
                x: 10,
                y: 0,
                w: 10,
                h: 10,
                sheet: 0,
            },
            "1.7 округляется вниз до 1"
        );
        assert_eq!(
            paints[2].atlas_rect,
            AtlasRect {
                x: 20,
                y: 0,
                w: 10,
                h: 10,
                sheet: 0,
            },
            "за концом — последний кадр"
        );
    }

    // -----------------------------------------------------------------------------------------
    // Фаза 13 — кадры сеткой
    // -----------------------------------------------------------------------------------------

    fn grid_image(frames: u32, columns: Option<u32>) -> ImageDecl {
        ImageDecl {
            name: "grid".to_string(),
            path: "grid.png".to_string(),
            frames,
            frame_steps: 1,
            animated: false,
            columns,
            size: None,
            anchor: Anchor::Center,
            offset: [0.0, 0.0],
            frame_by: None,
            frame_by_name: None,
            smooth: false,
        }
    }

    /// «Картинки», требования 1–2: `frames: 7, columns: 3` — три строки по три кадра, последняя
    /// неполная; кадр `n` лежит в строке `n / 3`, столбце `n % 3` — прямоугольник кадров 0, 2, 3,
    /// 6 (300×140 whole rect: 3 столбца по 100, три строки по ceil(7/3)=3, каждая 140/3 — но чтобы
    /// делить нацело, целое полотно 300×140 не подходит; берём 300×90, три строки по 30).
    #[test]
    fn grid_frame_rects_at_frames_0_2_3_6_for_seven_frames_three_columns() {
        let images = vec![grid_image(7, Some(3))];
        let whole = AtlasRect {
            x: 10,
            y: 20,
            w: 300,
            h: 90,
            sheet: 0,
        };
        let atlas_rects = vec![whole];
        let rect_of = |frame: u32| frame_atlas_rect_at(0, frame, &images, &atlas_rects);
        assert_eq!(
            rect_of(0),
            AtlasRect {
                x: 10,
                y: 20,
                w: 100,
                h: 30,
                sheet: 0,
            }
        );
        assert_eq!(
            rect_of(2),
            AtlasRect {
                x: 210,
                y: 20,
                w: 100,
                h: 30,
                sheet: 0,
            },
            "строка 0, столбец 2"
        );
        assert_eq!(
            rect_of(3),
            AtlasRect {
                x: 10,
                y: 50,
                w: 100,
                h: 30,
                sheet: 0,
            },
            "строка 1, столбец 0"
        );
        assert_eq!(
            rect_of(6),
            AtlasRect {
                x: 10,
                y: 80,
                w: 100,
                h: 30,
                sheet: 0,
            },
            "последняя, неполная строка — столбец 0"
        );
    }

    /// «Картинки», требование 2: `columns` равно `frames` — одна строка, то же, что лента.
    #[test]
    fn columns_equal_to_frames_behaves_like_a_single_row() {
        let with_columns = vec![grid_image(4, Some(4))];
        let without_columns = vec![grid_image(4, None)];
        let whole = AtlasRect {
            x: 0,
            y: 0,
            w: 400,
            h: 50,
            sheet: 0,
        };
        let atlas_rects = vec![whole];
        for frame in 0..4 {
            assert_eq!(
                frame_atlas_rect_at(0, frame, &with_columns, &atlas_rects),
                frame_atlas_rect_at(0, frame, &without_columns, &atlas_rects),
                "кадр {frame}"
            );
        }
    }

    /// «Картинки», требование 4: `frame_time`/`frame_by` выбирают номер кадра как обычно; сетка
    /// меняет только то, где этот кадр лежит в файле.
    #[test]
    fn frame_time_picks_the_same_frame_number_whether_gridded_or_not() {
        let mut decl = grid_image(6, Some(3));
        decl.animated = true;
        decl.frame_steps = 10;
        let images = vec![decl];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 300,
            h: 20,
            sheet: 0,
        }];
        // 25 шагов / 10 на кадр = кадр 2 (строка 0, столбец 2).
        let rect = frame_atlas_rect(0, 25.0, &images, &atlas_rects);
        assert_eq!(
            rect,
            AtlasRect {
                x: 200,
                y: 0,
                w: 100,
                h: 10,
                sheet: 0,
            }
        );
    }

    /// «Картинки», требование 5: `frames` без `frame_time` и без `frame_by` — набор кадров:
    /// объект показывает кадр 0 всегда, сколько бы шагов ни прошло.
    #[test]
    fn a_frame_set_without_frame_time_or_frame_by_always_shows_frame_zero() {
        let images = vec![grid_image(5, None)];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 50,
            h: 10,
            sheet: 0,
        }];
        for elapsed in [0.0, 1.0, 500.0] {
            assert_eq!(
                frame_atlas_rect(0, elapsed, &images, &atlas_rects),
                AtlasRect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                    sheet: 0,
                },
                "elapsed_steps = {elapsed}"
            );
        }
    }

    /// «Картинки», требование 23: at every `columns`/`frames` combination this test tries, no two
    /// frames' atlas rects overlap and each stays fully inside the whole strip's own rect — the
    /// slicing math never bleeds a neighboring frame's points into this one's.
    #[test]
    fn grid_frames_tile_the_whole_rect_exactly_with_no_overlap() {
        let whole = AtlasRect {
            x: 5,
            y: 7,
            w: 360,
            h: 240,
            sheet: 0,
        };
        for (frames, columns) in [(7, Some(3)), (12, Some(4)), (5, Some(1)), (9, None)] {
            let images = vec![grid_image(frames, columns)];
            let atlas_rects = vec![whole];
            let rects: Vec<AtlasRect> = (0..frames)
                .map(|f| frame_atlas_rect_at(0, f, &images, &atlas_rects))
                .collect();
            for &r in &rects {
                assert!(
                    r.x >= whole.x
                        && r.y >= whole.y
                        && r.x + r.w <= whole.x + whole.w
                        && r.y + r.h <= whole.y + whole.h,
                    "{r:?} выходит за {whole:?} ({frames}, {columns:?})"
                );
            }
            for i in 0..rects.len() {
                for j in (i + 1)..rects.len() {
                    assert!(
                        !overlap(rects[i], rects[j]),
                        "{:?} и {:?} пересекаются ({frames}, {columns:?})",
                        rects[i],
                        rects[j]
                    );
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Фаза 13 — картинка своего размера
    // -----------------------------------------------------------------------------------------

    fn sized_image(size: [f64; 2], anchor: Anchor, offset: [f64; 2]) -> ImageDecl {
        ImageDecl {
            name: "sized".to_string(),
            path: "sized.png".to_string(),
            frames: 1,
            frame_steps: 1,
            animated: false,
            columns: None,
            size: Some(size),
            anchor,
            offset,
            frame_by: None,
            frame_by_name: None,
            smooth: false,
        }
    }

    fn one_sized_object(
        world: &mut World,
        position: [f64; 2],
        size: [f64; 2],
        rotation: Option<u16>,
    ) -> u32 {
        let id = world.create();
        world.set_vec2(id, property::POSITION, position);
        world.set_vec2(id, property::SIZE, size);
        world.set_image(id, property::IMAGE, 0);
        if let Some(degrees) = rotation {
            world.set_rotation(
                id,
                property::ROTATION,
                crate::core::value::Rotation::from_degrees_exact(degrees as f64).unwrap(),
            );
        }
        id
    }

    /// «Картинки», требование 8 — числовой пример плана: объект `position [10, 5], size [1,
    /// 1.5]`; картинка `size [2, 2], anchor bottom, offset [0, 0.1]` — прямоугольник рисования от
    /// `(9.5, 4.6)` до `(11.5, 6.6)`.
    #[test]
    fn own_size_placement_matches_the_plans_own_worked_example() {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);
        let id = one_sized_object(&mut world, [10.0, 5.0], [1.0, 1.5], None);

        let images = vec![sized_image([2.0, 2.0], Anchor::Bottom, [0.0, 0.1])];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            sheet: 0,
        }];
        let scene = test_scene();
        let paints = compose_world_paints(&world, &scene, world.ids(), 0.0, &images, &atlas_rects);
        assert_eq!(paints.len(), 1);
        assert!((paints[0].position[0] - 9.5).abs() < 1e-5, "{paints:?}");
        assert!((paints[0].position[1] - 4.6).abs() < 1e-5, "{paints:?}");
        assert!((paints[0].size[0] - 2.0).abs() < 1e-5, "{paints:?}");
        assert!((paints[0].size[1] - 2.0).abs() < 1e-5, "{paints:?}");
        let _ = id;
    }

    /// «Картинки», требование 6: all nine anchors place the same 2×2 image around a 1×1 object at
    /// `[0, 0]`, no `offset` — the anchor's own point of the image coincides with the same point
    /// of the object.
    #[test]
    fn every_anchor_places_the_own_size_image_at_the_matching_point() {
        let properties = PropertyTable::new();
        let cases: [(Anchor, [f32; 2]); 9] = [
            (Anchor::TopLeft, [0.0, 0.0]),
            (Anchor::Top, [-0.5, 0.0]),
            (Anchor::TopRight, [-1.0, 0.0]),
            (Anchor::Left, [0.0, -0.5]),
            (Anchor::Center, [-0.5, -0.5]),
            (Anchor::Right, [-1.0, -0.5]),
            (Anchor::BottomLeft, [0.0, -1.0]),
            (Anchor::Bottom, [-0.5, -1.0]),
            (Anchor::BottomRight, [-1.0, -1.0]),
        ];
        for (anchor, expected_top_left) in cases {
            let mut world = World::new(&properties);
            one_sized_object(&mut world, [0.0, 0.0], [1.0, 1.0], None);
            let images = vec![sized_image([2.0, 2.0], anchor, [0.0, 0.0])];
            let atlas_rects = vec![AtlasRect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
                sheet: 0,
            }];
            let scene = test_scene();
            let paints =
                compose_world_paints(&world, &scene, world.ids(), 0.0, &images, &atlas_rects);
            assert!(
                (paints[0].position[0] - expected_top_left[0]).abs() < 1e-5
                    && (paints[0].position[1] - expected_top_left[1]).abs() < 1e-5,
                "{anchor:?}: {:?} != {expected_top_left:?}",
                paints[0].position
            );
        }
    }

    /// «Картинки», требование 7: без `size` картинка растягивается на прямоугольник объекта, как
    /// раньше — `own_size_rect` не подменяет ни позицию, ни размер.
    #[test]
    fn without_size_the_image_still_draws_over_the_objects_own_rectangle() {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);
        one_sized_object(&mut world, [3.0, 4.0], [2.0, 5.0], None);
        let images = vec![one_frame_image()];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            sheet: 0,
        }];
        let scene = test_scene();
        let paints = compose_world_paints(&world, &scene, world.ids(), 0.0, &images, &atlas_rects);
        assert_eq!(paints[0].position, [3.0, 4.0]);
        assert_eq!(paints[0].size, [2.0, 5.0]);
    }

    /// «Картинки», требование 9: `rotation` 90/180/270 rotates the own-size rectangle as a rigid
    /// body around the *object's* own middle — width/height swap on the odd quarter turns.
    #[test]
    fn rotation_rotates_the_own_size_rectangle_around_the_objects_middle() {
        let properties = PropertyTable::new();
        let images = vec![sized_image([1.0, 3.0], Anchor::TopLeft, [0.0, 0.0])];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            sheet: 0,
        }];
        let scene = test_scene();
        // Object [0,0]..[4,2], middle (2,1). Unrotated image rect at the object's top-left,
        // 1×3: [0,0]..[1,3].
        let expectations = [
            (0u16, [0.0f32, 0.0], [1.0f32, 3.0]),
            (90, [0.0, -1.0], [3.0, 1.0]),
            (180, [3.0, -1.0], [1.0, 3.0]),
            (270, [1.0, 2.0], [3.0, 1.0]),
        ];
        for (degrees, expected_pos, expected_size) in expectations {
            let mut world = World::new(&properties);
            one_sized_object(&mut world, [0.0, 0.0], [4.0, 2.0], Some(degrees));
            let paints =
                compose_world_paints(&world, &scene, world.ids(), 0.0, &images, &atlas_rects);
            assert!(
                (paints[0].position[0] - expected_pos[0]).abs() < 1e-4
                    && (paints[0].position[1] - expected_pos[1]).abs() < 1e-4,
                "{degrees}°: позиция {:?} != {expected_pos:?}",
                paints[0].position
            );
            assert!(
                (paints[0].size[0] - expected_size[0]).abs() < 1e-4
                    && (paints[0].size[1] - expected_size[1]).abs() < 1e-4,
                "{degrees}°: размер {:?} != {expected_size:?}",
                paints[0].size
            );
        }
    }

    // -----------------------------------------------------------------------------------------
    // Фаза 14 — отражение
    // -----------------------------------------------------------------------------------------

    fn one_flipped_sized_object(
        world: &mut World,
        position: [f64; 2],
        size: [f64; 2],
        rotation: Option<u16>,
    ) -> u32 {
        let id = one_sized_object(world, position, size, rotation);
        world.set_flag(id, property::FLIP_X, true);
        id
    }

    /// «Картинки», требование 8 — тот же числовой пример, что и без отражения, но с `flip_x`:
    /// линия отражения `x = 10.5` (середина объекта), прямоугольник от `(8.5, 4.5)` до
    /// `(10.5, 6.5)`.
    #[test]
    fn flip_x_mirrors_the_own_size_placement_from_the_plans_worked_example() {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);
        one_flipped_sized_object(&mut world, [10.0, 5.0], [1.0, 1.5], None);

        let images = vec![sized_image([2.0, 2.0], Anchor::BottomLeft, [0.5, 0.0])];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            sheet: 0,
        }];
        let scene = test_scene();
        let paints = compose_world_paints(&world, &scene, world.ids(), 0.0, &images, &atlas_rects);
        assert_eq!(paints.len(), 1);
        assert!((paints[0].position[0] - 8.5).abs() < 1e-5, "{paints:?}");
        assert!((paints[0].position[1] - 4.5).abs() < 1e-5, "{paints:?}");
        assert!((paints[0].size[0] - 2.0).abs() < 1e-5, "{paints:?}");
        assert!((paints[0].size[1] - 2.0).abs() < 1e-5, "{paints:?}");
        assert!(paints[0].flip_x);
    }

    /// «Картинки», требование 8: `flip_x` mirrors each of the nine anchors left/right before
    /// placing — an anchor's flipped result matches the *unflipped* result of its mirror anchor
    /// (`every_anchor_places_the_own_size_image_at_the_matching_point`).
    #[test]
    fn flip_x_swaps_each_anchors_placement_with_its_mirror() {
        let properties = PropertyTable::new();
        let cases: [(Anchor, [f32; 2]); 9] = [
            (Anchor::TopLeft, [-1.0, 0.0]),
            (Anchor::Top, [-0.5, 0.0]),
            (Anchor::TopRight, [0.0, 0.0]),
            (Anchor::Left, [-1.0, -0.5]),
            (Anchor::Center, [-0.5, -0.5]),
            (Anchor::Right, [0.0, -0.5]),
            (Anchor::BottomLeft, [-1.0, -1.0]),
            (Anchor::Bottom, [-0.5, -1.0]),
            (Anchor::BottomRight, [0.0, -1.0]),
        ];
        for (anchor, expected_top_left) in cases {
            let mut world = World::new(&properties);
            one_flipped_sized_object(&mut world, [0.0, 0.0], [1.0, 1.0], None);
            let images = vec![sized_image([2.0, 2.0], anchor, [0.0, 0.0])];
            let atlas_rects = vec![AtlasRect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
                sheet: 0,
            }];
            let scene = test_scene();
            let paints =
                compose_world_paints(&world, &scene, world.ids(), 0.0, &images, &atlas_rects);
            assert!(
                (paints[0].position[0] - expected_top_left[0]).abs() < 1e-5
                    && (paints[0].position[1] - expected_top_left[1]).abs() < 1e-5,
                "{anchor:?}: {:?} != {expected_top_left:?}",
                paints[0].position
            );
        }
    }

    /// «Картинки», требование 9 — тот же поворот на 90°, что и
    /// `rotation_rotates_the_own_size_rectangle_around_the_objects_middle`, но с `flip_x`:
    /// картинка сначала отражается в своих осях (искомый прямоугольник до поворота сдвигается с
    /// `[0,0]..[1,3]` на `[3,0]..[4,3]`), затем поворачивается вокруг середины объекта как обычно
    /// — результат отличается от простого поворота без отражения.
    #[test]
    fn flip_x_mirrors_before_rotation_rotates_around_the_objects_middle() {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);
        one_flipped_sized_object(&mut world, [0.0, 0.0], [4.0, 2.0], Some(90));
        let images = vec![sized_image([1.0, 3.0], Anchor::TopLeft, [0.0, 0.0])];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            sheet: 0,
        }];
        let scene = test_scene();
        let paints = compose_world_paints(&world, &scene, world.ids(), 0.0, &images, &atlas_rects);
        assert!((paints[0].position[0] - 0.0).abs() < 1e-4, "{paints:?}");
        assert!((paints[0].position[1] - 2.0).abs() < 1e-4, "{paints:?}");
        assert!((paints[0].size[0] - 3.0).abs() < 1e-4, "{paints:?}");
        assert!((paints[0].size[1] - 1.0).abs() < 1e-4, "{paints:?}");
        assert!(paints[0].flip_x);
    }

    /// «Картинки», требование 8: без `size` `flip_x` не двигает прямоугольник (он остаётся
    /// прямоугольником объекта, как и без отражения) — признак отражения всё равно попадает в
    /// список рисования, чтобы шейдер зеркалил саму выборку.
    #[test]
    fn flip_x_without_size_keeps_the_objects_own_rectangle_but_still_flags_the_draw() {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);
        one_flipped_sized_object(&mut world, [3.0, 4.0], [2.0, 5.0], None);
        let images = vec![one_frame_image()];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            sheet: 0,
        }];
        let scene = test_scene();
        let paints = compose_world_paints(&world, &scene, world.ids(), 0.0, &images, &atlas_rects);
        assert_eq!(paints[0].position, [3.0, 4.0]);
        assert_eq!(paints[0].size, [2.0, 5.0]);
        assert!(paints[0].flip_x);
    }

    /// «Картинки», требование 12: `flip_x` у объекта с заливкой цветом ничего не меняет в
    /// рисовании.
    #[test]
    fn flip_x_has_no_effect_on_a_color_fill() {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);
        let id = world.create();
        world.set_vec2(id, property::POSITION, [1.0, 1.0]);
        world.set_vec2(id, property::SIZE, [2.0, 2.0]);
        world.set_color(id, property::COLOR, [1.0, 0.0, 0.0, 1.0]);
        world.set_flag(id, property::FLIP_X, true);

        let scene = test_scene();
        let paints = compose_world_paints(&world, &scene, world.ids(), 0.0, &[], &[]);
        assert_eq!(paints.len(), 1);
        assert!(!paints[0].flip_x, "{paints:?}");
        assert_eq!(paints[0].position, [1.0, 1.0]);
        assert_eq!(paints[0].size, [2.0, 2.0]);
    }

    // -----------------------------------------------------------------------------------------
    // Фаза 13 — земля из плиток
    // -----------------------------------------------------------------------------------------

    fn tileset(frames: u32, columns: Option<u32>) -> ImageDecl {
        grid_image(frames, columns)
    }

    /// «Картинки» → «Сглаживание», требование 3: `smooth` набора плиток доходит до списка
    /// рисования земли так же, как у объекта.
    #[test]
    fn smooth_reaches_the_draw_list_for_a_ground_tile() {
        let mut smooth_tileset = tileset(1, None);
        smooth_tileset.smooth = true;
        let images = vec![smooth_tileset];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            sheet: 0,
        }];
        let ground = vec![GroundLayer {
            image: 0,
            cells: vec![vec![0]],
        }];
        let visible = CellRange {
            x0: 0,
            y0: 0,
            x1: 1,
            y1: 1,
        };
        let paints = compose_ground_paints(&ground, visible, &images, &atlas_rects);
        assert_eq!(paints.len(), 1);
        assert!(paints[0].smooth);
    }

    /// «Мир на экране» → «Земля», требования 16–18: плитки в своих клетках с кадрами набора по
    /// сетке; `-1` пусто; слои рисуются в порядке списка.
    #[test]
    fn ground_tiles_land_in_their_own_cells_in_layer_order() {
        let images = vec![tileset(4, Some(2))];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 20,
            h: 20,
            sheet: 0,
        }];
        let ground = vec![
            GroundLayer {
                image: 0,
                cells: vec![vec![0, -1], vec![-1, 2]],
            },
            GroundLayer {
                image: 0,
                cells: vec![vec![-1, -1], vec![3, -1]],
            },
        ];
        let visible = CellRange {
            x0: 0,
            y0: 0,
            x1: 2,
            y1: 2,
        };
        let paints = compose_ground_paints(&ground, visible, &images, &atlas_rects);
        // Layer 0: (0,0)=tile 0, (1,1)=tile 2. Layer 1: (0,1)=tile 3. Layer 0 entries come first.
        assert_eq!(paints.len(), 3);
        assert_eq!(paints[0].position, [0.0, 0.0]);
        assert_eq!(
            paints[0].atlas_rect,
            frame_atlas_rect_at(0, 0, &images, &atlas_rects)
        );
        assert_eq!(paints[1].position, [1.0, 1.0]);
        assert_eq!(
            paints[1].atlas_rect,
            frame_atlas_rect_at(0, 2, &images, &atlas_rects)
        );
        assert_eq!(paints[2].position, [0.0, 1.0]);
        assert_eq!(
            paints[2].atlas_rect,
            frame_atlas_rect_at(0, 3, &images, &atlas_rects)
        );
        for p in &paints {
            assert_eq!(p.size, [1.0, 1.0]);
        }
    }

    /// «Мир на экране» → «Земля», требование 20: only cells inside `visible` are drawn.
    #[test]
    fn ground_tiles_outside_the_visible_range_are_skipped() {
        let images = vec![tileset(1, None)];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            sheet: 0,
        }];
        let ground = vec![GroundLayer {
            image: 0,
            cells: vec![vec![0, 0, 0], vec![0, 0, 0], vec![0, 0, 0]],
        }];
        let visible = CellRange {
            x0: 1,
            y0: 1,
            x1: 2,
            y1: 2,
        };
        let paints = compose_ground_paints(&ground, visible, &images, &atlas_rects);
        assert_eq!(paints.len(), 1);
        assert_eq!(paints[0].position, [1.0, 1.0]);
    }

    /// A layer whose cells are all `-1` draws nothing.
    #[test]
    fn a_layer_of_all_minus_one_draws_nothing() {
        let images = vec![tileset(1, None)];
        let atlas_rects = vec![AtlasRect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            sheet: 0,
        }];
        let ground = vec![GroundLayer {
            image: 0,
            cells: vec![vec![-1, -1], vec![-1, -1]],
        }];
        let visible = CellRange {
            x0: 0,
            y0: 0,
            x1: 2,
            y1: 2,
        };
        assert!(compose_ground_paints(&ground, visible, &images, &atlas_rects).is_empty());
    }
}
