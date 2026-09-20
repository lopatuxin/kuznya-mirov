//! «Картинки» → «Атлас и отрисовка»: packs every declared image's whole strip (frames and all —
//! slicing a strip into its individual frames is `frame_atlas_rect`'s job at draw time, not
//! `pack`'s) into one fixed-size canvas by shelves (sort tallest first, lay left to right, start a
//! new shelf when a row would overflow), with a one-pixel gap between neighbors and one opaque
//! white pixel reserved for color fills; also picks each image's current frame and, from `Game`'s
//! `World`, the world's own list of things to draw, by `layer`. No `wgpu`, no browser types — this
//! is the part of «Отрисовка» plain enough to run and test on any target, unlike the GPU resources
//! built from its result (see `super::gpu`, wasm32-only).

use crate::core::property;
use crate::core::screens::Fill;
use crate::core::time::frame_index;
use crate::core::value::ImageId;
use crate::core::world::World;
use crate::data::load::ImageDecl;

/// «Картинки» → «Атлас и отрисовка»: the canvas is this size in every browser, so the game either
/// starts everywhere or nowhere, never differently depending on what WebGPU would actually allow.
pub const ATLAS_SIZE: u32 = 2048;

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

/// A rectangle inside the atlas, in atlas pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// «Картинки» → «Атлас и отрисовка»: a plain color fill is this pixel, stretched over the
/// rectangle and multiplied by the color — `pack` always places it here, at a fixed spot every
/// atlas has regardless of which images the game declares (including none at all).
pub const WHITE_PIXEL: AtlasRect = AtlasRect {
    x: 0,
    y: 0,
    w: 1,
    h: 1,
};

/// The packed canvas: `pixels` is `ATLAS_SIZE * ATLAS_SIZE * 4` bytes of RGBA8, meant to be
/// uploaded to the GPU once and then dropped — «точки после сборки атласа в памяти движка не
/// остаются» — `rects[i]` is where the `i`-th input image (its whole strip) landed, in the same
/// order `pack` was given them, so `i` doubles as that image's `ImageId`.
#[derive(Debug)]
pub struct Atlas {
    pub pixels: Vec<u8>,
    pub rects: Vec<AtlasRect>,
}

fn set_pixel(pixels: &mut [u8], x: u32, y: u32, rgba: [u8; 4]) {
    let start = ((y * ATLAS_SIZE + x) * 4) as usize;
    pixels[start..start + 4].copy_from_slice(&rgba);
}

fn blit(pixels: &mut [u8], rect: AtlasRect, src: &[u8]) {
    for row in 0..rect.h {
        let dst_start = (((rect.y + row) * ATLAS_SIZE + rect.x) * 4) as usize;
        let dst_len = (rect.w * 4) as usize;
        let src_start = (row * rect.w * 4) as usize;
        pixels[dst_start..dst_start + dst_len]
            .copy_from_slice(&src[src_start..src_start + dst_len]);
    }
}

/// Total atlas-pixel area every image would occupy, `+1` for the white pixel — the "занятое
/// место" a doesn't-fit error names next to `ATLAS_SIZE`'s own limit.
fn occupied_area(images: &[AtlasImage]) -> u64 {
    1 + images
        .iter()
        .map(|img| img.width as u64 * img.height as u64)
        .sum::<u64>()
}

/// Shelf-packs every image's whole strip into one `ATLAS_SIZE`×`ATLAS_SIZE` canvas — «Картинки» →
/// «Атлас и отрисовка»: sorted tallest first (a stable sort, so two images of the same height keep
/// `images`' own order), laid left to right on a shelf, wrapping to a new shelf when a row would
/// overflow. `Err` names what didn't fit: a single image wider or taller than the atlas on its
/// own, or the declared set's combined area next to the atlas's own limit.
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
            h: 0
        };
        images.len()
    ];
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
            return Err(format!(
                "картинки не умещаются в атлас {ATLAS_SIZE}×{ATLAS_SIZE}: полка на высоте {shelf_y} требует ещё {} точек по высоте и выходит за предел; суммарно все картинки занимают {} точек, предел {}",
                img.height,
                occupied_area(images),
                ATLAS_SIZE as u64 * ATLAS_SIZE as u64
            ));
        }
        rects[i] = AtlasRect {
            x: cursor_x,
            y: shelf_y,
            w: img.width,
            h: img.height,
        };
        cursor_x += img.width + PADDING;
        shelf_height = shelf_height.max(img.height);
    }

    let mut pixels = vec![0u8; ATLAS_SIZE as usize * ATLAS_SIZE as usize * 4];
    set_pixel(
        &mut pixels,
        WHITE_PIXEL.x,
        WHITE_PIXEL.y,
        [255, 255, 255, 255],
    );
    for (i, img) in images.iter().enumerate() {
        blit(&mut pixels, rects[i], &img.pixels);
    }

    Ok(Atlas { pixels, rects })
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
    let whole = atlas_rects[image];
    let frame_w = whole.w / decl.frames.max(1);
    let frame = frame_index(elapsed_steps, decl.frame_steps, decl.frames);
    AtlasRect {
        x: whole.x + frame * frame_w,
        y: whole.y,
        w: frame_w,
        h: whole.h,
    }
}

/// A `Fill`'s resolved `(color, atlas rect)` for the current frame — «Картинки» → «Атлас и
/// отрисовка»: a color fill is the atlas's white pixel stretched and multiplied by the color; an
/// image fill leaves the color white (only its alpha carries `opacity`) and samples the image's
/// current frame instead. Either way the result is one draw instance, so a fill and an image go
/// out through the very same math — shared by a screen element's own `Fill`
/// (`wasm::compose_ui`) and, via a `Fill` built from a world object's `image`/`opacity`, by
/// `compose_world_paints` below.
pub fn fill_paint(
    fill: &Fill,
    elapsed_steps: f64,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> ([f32; 4], AtlasRect) {
    match *fill {
        Fill::Color(c) => (c, WHITE_PIXEL),
        Fill::Image { image, opacity } => (
            [1.0, 1.0, 1.0, opacity],
            frame_atlas_rect(image, elapsed_steps, images, atlas_rects),
        ),
    }
}

/// A rectangle's resolved placement and paint, free of `bytemuck`/`wgpu` so this builds and tests
/// natively — `wasm::compose_instances` turns each into a `DrawRect` for the GPU, unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectPaint {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub atlas_rect: AtlasRect,
}

/// «Картинки» → «Кадры»: the world's own frame is picked by steps taken (`elapsed_steps`, frozen
/// exactly when the world stops stepping — paused, or on the outcome screen); «Исполнение игры»:
/// rendering never changes the world, this just reads it. Every object with `position`, `size`
/// and a `color` or `image` sorts into one list by `layer`, a color fill and an image fill mixed
/// in the same order — «Картинки» → «Атлас и отрисовка»: a color fill is the atlas's white pixel,
/// an image fill samples its current frame, so the two interleave exactly as if both were images.
pub fn compose_world_paints(
    world: &World,
    ids: impl Iterator<Item = u32>,
    elapsed_steps: f64,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> Vec<RectPaint> {
    let mut ordered: Vec<(i32, u32)> = ids
        .filter(|&id| {
            world.vec2(id, property::POSITION).is_some()
                && world.vec2(id, property::SIZE).is_some()
                && (world.color(id, property::COLOR).is_some()
                    || world.image(id, property::IMAGE).is_some())
        })
        .map(|id| (world.layer(id, property::LAYER).unwrap_or(0), id))
        .collect();
    ordered.sort_unstable();

    ordered
        .into_iter()
        .map(|(_, id)| {
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
            let (color, atlas_rect) = fill_paint(&fill, elapsed_steps, images, atlas_rects);
            RectPaint {
                position: [p[0] as f32, p[1] as f32],
                size: [s[0] as f32, s[1] as f32],
                color,
                atlas_rect,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn empty_input_still_places_the_white_pixel() {
        let atlas = pack(&[]).expect("пустой набор — не ошибка");
        assert!(atlas.rects.is_empty());
        let idx = ((WHITE_PIXEL.y * ATLAS_SIZE + WHITE_PIXEL.x) * 4) as usize;
        assert_eq!(&atlas.pixels[idx..idx + 4], &[255, 255, 255, 255]);
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

    #[test]
    fn blitted_pixels_land_at_their_own_rect_unmodified() {
        let images = vec![img(4, 3)];
        let atlas = pack(&images).unwrap();
        let rect = atlas.rects[0];
        for row in 0..rect.h {
            let start = (((rect.y + row) * ATLAS_SIZE + rect.x) * 4) as usize;
            let len = (rect.w * 4) as usize;
            assert!(atlas.pixels[start..start + len].iter().all(|&b| b == 7));
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

    #[test]
    fn a_set_that_does_not_fit_together_is_rejected_with_area_and_limit_named() {
        let images: Vec<AtlasImage> = (0..10).map(|_| img(ATLAS_SIZE, 250)).collect();
        let err = pack(&images).expect_err("суммарно больше 2048×2048 — ошибка");
        assert!(err.contains(&ATLAS_SIZE.to_string()), "{err}");
    }

    #[test]
    fn a_set_that_fits_by_area_but_not_by_shelf_packing_names_the_real_reason() {
        // Two 1200×1100 images: combined area (2×1,320,000 + 1) is well under 2048×2048's own
        // 4,194,304, but shelf packing needs two 1100-tall shelves (2201 > 2048) since neither
        // image fits next to the other on one row — the doesn't-fit error must say so, not just
        // repeat the area that, read alone, says it should have fit.
        let images = vec![img(1200, 1100), img(1200, 1100)];
        let err =
            pack(&images).expect_err("влезают по площади, но не по полкам — всё равно ошибка");
        assert!(err.contains("полк"), "{err}");
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
    use crate::core::screens::button_fill;

    fn one_frame_image() -> ImageDecl {
        ImageDecl {
            name: "x".to_string(),
            path: "x.png".to_string(),
            frames: 1,
            frame_steps: 1,
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
        }];

        let paints = compose_world_paints(&world, world.ids(), 0.0, &images, &atlas_rects);
        assert_eq!(paints.len(), 2, "{paints:?}");
        assert_eq!(paints[0].color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(paints[0].atlas_rect, WHITE_PIXEL);
        assert_eq!(paints[1].atlas_rect, atlas_rects[0]);
        assert_eq!(paints[1].color, [1.0, 1.0, 1.0, 1.0]);
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

        let paints = compose_world_paints(&world, world.ids(), 0.0, &images, &atlas_rects);
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
        let (_, atlas_rect) = fill_paint(chosen, 0.0, &images, &atlas_rects);
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
            },
            AtlasRect {
                x: 50,
                y: 50,
                w: 10,
                h: 10,
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
        let (_, atlas_rect) = fill_paint(chosen, 0.0, &images, &atlas_rects);
        assert_eq!(atlas_rect, atlas_rects[1]);
    }
}
