use std::f32::consts::PI;

use crate::core::screens::Book;
use crate::core::value::ImageId;
use crate::data::load::ImageDecl;
use crate::render::DrawRect;
use crate::render::atlas::{self, ImagePlace, RectPaint};

use super::draw_rect;

const SECONDS_PER_LEAF: f64 = 0.49;
const LEAF_STRIPS: usize = 24;

#[derive(Default)]
pub(super) struct Motion {
    opened: Option<u64>,
    from: f32,
    to: usize,
    started: f64,
}

impl Motion {
    pub(super) fn update(&mut self, opened: u64, selected: usize, now: f64) -> f32 {
        if self.opened != Some(opened) {
            *self = Self {
                opened: Some(opened),
                from: selected as f32,
                to: selected,
                started: now,
            };
            return self.from;
        }
        let target = self.to as f32;
        let distance = target - self.from;
        let travelled = ((now - self.started) / SECONDS_PER_LEAF) as f32;
        let position = if travelled >= distance.abs() {
            target
        } else {
            self.from + distance.signum() * travelled.max(0.0)
        };
        if self.to != selected {
            self.from = position;
            self.to = selected;
            self.started = now;
        }
        position
    }
}

pub(super) struct Frame {
    pub page: usize,
    pub text_opacity: f32,
    pub turning: bool,
}

struct Canvas<'a> {
    rects: &'a mut Vec<DrawRect>,
    images: &'a [ImageDecl],
    places: &'a [ImagePlace],
}

impl Canvas<'_> {
    fn picture(&self, image: ImageId, [x, y, w, h]: [f32; 4], uv: [f32; 4]) -> DrawRect {
        let rect = atlas::frame_atlas_rect(image, 0.0, self.images, self.places).rect;
        let mut draw = draw_rect(&RectPaint::flat(
            [x, y],
            [w, h],
            [1.0; 4],
            rect,
            self.images[image].smooth,
            false,
        ));
        let [u, v, width, height] = uv;
        draw.atlas_pos[0] += u * rect.w as f32;
        draw.atlas_pos[1] += v * rect.h as f32;
        draw.atlas_size = [width * rect.w as f32, height * rect.h as f32];
        draw
    }

    fn spread(&mut self, image: ImageId, window: [f32; 2], sides: [f32; 2], opacity: f32) {
        let [left, right] = sides;
        let draw = self.picture(
            image,
            [left, 0.0, right - left, window[1]],
            [left / window[0], 0.0, (right - left) / window[0], 1.0],
        );
        self.rects.push(DrawRect {
            color: [1.0, 1.0, 1.0, opacity],
            ..draw
        });
    }
}

struct Leaf {
    spine: f32,
    middle_y: f32,
    reach: f32,
    rise: f32,
}

impl Leaf {
    fn project(&self, x: f32, top: f32, bottom: f32, width: f32) -> [f32; 3] {
        let along = (x - self.spine).abs() / width;
        let depth = self.rise * along.min(1.5);
        let scale = 1.0 / (1.0 - 0.115 * depth);
        let lift = 9.0 * depth * along.min(1.5);
        let vertical = -0.064 * depth;
        [
            self.spine + (x - self.spine).abs() * self.reach * scale,
            (top - self.middle_y) * vertical - lift,
            (bottom - self.middle_y) * vertical - lift,
        ]
    }

    fn project_strip(&self, draw: &mut DrawRect, width: f32) {
        let [x, y] = draw.position;
        let bottom = y + draw.size[1];
        let [left, top_left, bottom_left] = self.project(x, y, bottom, width);
        let [right, top_right, bottom_right] = self.project(x + draw.size[0], y, bottom, width);
        draw.position[0] = left;
        draw.size[0] = right - left;
        draw.vertical_offsets = [top_left, top_right, bottom_left, bottom_right];
    }
}

struct Layout {
    window: [f32; 2],
    spine: f32,
    paper: [f32; 4],
    tabs: [f32; 4],
}

impl Layout {
    fn new(book: &Book, window: [f32; 2]) -> Self {
        let [x, y] = book.leaf.top_left(window);
        let [w, h] = book.leaf.size;
        let [tab_x, tab_y] = book.tab_area.top_left(window);
        let [tab_w, tab_h] = book.tab_area.size;
        Self {
            window,
            spine: window[0] * 0.5 + book.spine,
            paper: [x, y, x + w, y + h],
            tabs: [tab_x, tab_y, tab_w, tab_h / book.bookmarks.len() as f32],
        }
    }

    fn tab(&self, row: usize, left: bool) -> [f32; 4] {
        let [x, y, w, h] = self.tabs;
        [
            if left { self.window[0] - x - w } else { x },
            y + row as f32 * h,
            w,
            h,
        ]
    }
}

fn bookmark(
    canvas: &mut Canvas<'_>,
    book: &Book,
    layout: &Layout,
    row: usize,
    left: bool,
    red: f32,
    turn: Option<(&Leaf, f32)>,
) {
    let rows = book.bookmarks.len() as f32;
    let region = [0.0, row as f32 / rows, 0.5, 1.0 / rows];
    let mut draw = canvas.picture(book.tabs, layout.tab(row, left), region);
    draw.flip_x = if left { 1.0 } else { 0.0 };
    if let Some((leaf, width)) = turn {
        leaf.project_strip(&mut draw, width);
        let shade = 1.0 - 0.23 * leaf.rise;
        draw.color = [shade, shade, shade, 1.0];
    } else {
        let mut shadow = draw;
        shadow.position[0] += if left { -1.5 } else { 1.5 };
        shadow.position[1] += 3.5;
        shadow.color = [0.16, 0.06, 0.025, 0.45];
        canvas.rects.push(shadow);
    }
    if red < 1.0 {
        canvas.rects.push(draw);
    }
    if red > 0.0 {
        draw.atlas_pos[0] += draw.atlas_size[0];
        draw.color[3] = red;
        canvas.rects.push(draw);
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn draw_leaf(canvas: &mut Canvas<'_>, book: &Book, layout: &Layout, page: usize, progress: f32) {
    let [left, top, right, bottom] = layout.paper;
    let angle = PI * progress;
    let leaf = Leaf {
        spine: layout.spine,
        middle_y: (top + bottom) * 0.5,
        reach: angle.cos(),
        rise: angle.sin(),
    };
    let back = progress >= 0.5;
    let bounds = if back {
        [left, layout.spine]
    } else {
        [layout.spine, right]
    };
    let width = bounds[1] - bounds[0];
    let height = bottom - top;
    let [w, h] = layout.window;
    let image = book.bookmarks[page + usize::from(back)].spread;
    for strip in 0..LEAF_STRIPS {
        let x = bounds[0] + width * strip as f32 / LEAF_STRIPS as f32;
        let next = bounds[0] + width * (strip + 1) as f32 / LEAF_STRIPS as f32;
        let mut draw = canvas.picture(
            image,
            [x, top, next - x, height],
            [x / w, top / h, (next - x) / w, height / h],
        );
        leaf.project_strip(&mut draw, width);
        let away = ((x - layout.spine) / width).abs();
        let shade = 1.0 - leaf.rise * (0.30 - 0.18 * away);
        draw.color = [shade, shade, shade, 1.0];
        canvas.rects.push(draw);
    }
    bookmark(
        canvas,
        book,
        layout,
        page,
        back,
        1.0 - ease(progress * 2.0),
        Some((&leaf, width)),
    );
}

fn draw_fold_shadow(canvas: &mut Canvas<'_>, layout: &Layout, progress: f32) {
    let [_, y, _, bottom] = layout.paper;
    let rise = (PI * progress).sin();
    let width = rise * 65.0 / 12.0;
    for i in 0..12 {
        let distance = i as f32;
        let alpha = rise * 0.25 * (1.0 - distance / 12.0).powi(2);
        canvas.rects.push(draw_rect(&RectPaint::flat(
            [layout.spine - (distance + 1.0) * width, y],
            [width, bottom - y],
            [0.15, 0.055, 0.015, alpha],
            atlas::WHITE_PIXEL,
            false,
            false,
        )));
    }
}

pub(super) fn draw(
    rects: &mut Vec<DrawRect>,
    book: &Book,
    window: [f32; 2],
    position: f32,
    images: &[ImageDecl],
    places: &[ImagePlace],
) -> Frame {
    let mut canvas = Canvas {
        rects,
        images,
        places,
    };
    let layout = Layout::new(book, window);
    let page = position.floor() as usize;
    let fraction = position.fract();
    let full = [0.0, window[0]];
    canvas.spread(book.bookmarks[page].spread, window, full, 1.0);
    let turning = fraction > 0.0;
    let progress = ease(fraction);
    if turning {
        let next = book.bookmarks[page + 1].spread;
        canvas.spread(
            next,
            window,
            [layout.spine, window[0]],
            ease(progress * 8.0),
        );
        canvas.spread(
            next,
            window,
            [0.0, layout.spine],
            ease((progress - 0.82) / 0.18),
        );
    }
    canvas.spread(book.binding, window, full, 1.0);
    let shown = page + usize::from(progress >= 0.5);
    for row in 0..book.bookmarks.len() {
        if !turning || row != page {
            bookmark(
                &mut canvas,
                book,
                &layout,
                row,
                row < page,
                if turning && row == page + 1 {
                    ease((progress - 0.35) / 0.35)
                } else if row == shown {
                    1.0
                } else {
                    0.0
                },
                None,
            );
        }
    }
    if turning {
        draw_fold_shadow(&mut canvas, &layout, progress);
        draw_leaf(&mut canvas, book, &layout, page, progress);
    }
    let text_opacity = if shown == page {
        1.0 - ease((progress - 0.18) / 0.32)
    } else {
        ease((progress - 0.82) / 0.18)
    };
    Frame {
        page: shown,
        text_opacity,
        turning,
    }
}
