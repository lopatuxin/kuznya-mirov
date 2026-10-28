//! «Картинки» → «Атлас»: раскладка прямоугольников по листу способом MaxRects — лист помнит все
//! наибольшие свободные прямоугольники, кусок встаёт в тот, где после него остаётся меньше всего
//! места по короткой стороне (при равенстве — по длинной, затем тот, что раньше в списке).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Space {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) w: u32,
    pub(super) h: u32,
}

impl Space {
    fn right(self) -> u32 {
        self.x + self.w
    }

    fn bottom(self) -> u32 {
        self.y + self.h
    }

    fn touches(self, other: Space) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    fn contains(self, other: Space) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }
}

pub(super) struct Sheet {
    free: Vec<Space>,
}

impl Sheet {
    pub(super) fn new(width: u32, height: u32) -> Sheet {
        Sheet {
            free: vec![Space {
                x: 0,
                y: 0,
                w: width,
                h: height,
            }],
        }
    }

    /// Закрывает `used` — место, где уже что-то лежит.
    pub(super) fn reserve(&mut self, used: Space) {
        let mut remains = Vec::with_capacity(self.free.len() + 4);
        for free in self.free.drain(..) {
            if !free.touches(used) {
                remains.push(free);
                continue;
            }
            if used.x > free.x {
                remains.push(Space {
                    w: used.x - free.x,
                    ..free
                });
            }
            if used.right() < free.right() {
                remains.push(Space {
                    x: used.right(),
                    w: free.right() - used.right(),
                    ..free
                });
            }
            if used.y > free.y {
                remains.push(Space {
                    h: used.y - free.y,
                    ..free
                });
            }
            if used.bottom() < free.bottom() {
                remains.push(Space {
                    y: used.bottom(),
                    h: free.bottom() - used.bottom(),
                    ..free
                });
            }
        }
        self.free = without_covered(remains);
    }

    /// Занимает прямоугольник `w` × `h` в лучшем свободном месте и отдаёт его левый верхний угол;
    /// `None` — не влезает.
    pub(super) fn take(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let best = self
            .free
            .iter()
            .filter(|free| free.w >= w && free.h >= h)
            .min_by_key(|free| {
                let (left_w, left_h) = (free.w - w, free.h - h);
                (left_w.min(left_h), left_w.max(left_h))
            })?;
        let (x, y) = (best.x, best.y);
        self.reserve(Space { x, y, w, h });
        Some((x, y))
    }
}

/// Свободные прямоугольники без тех, что целиком лежат внутри другого; из двух равных остаётся первый.
fn without_covered(spaces: Vec<Space>) -> Vec<Space> {
    let covered = |index: usize| {
        spaces.iter().enumerate().any(|(other, space)| {
            other != index
                && space.contains(spaces[index])
                && (*space != spaces[index] || other < index)
        })
    };
    let keep: Vec<bool> = (0..spaces.len()).map(|index| !covered(index)).collect();
    spaces
        .iter()
        .zip(keep)
        .filter_map(|(space, keep)| keep.then_some(*space))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_rectangle_is_never_handed_out_twice() {
        let mut sheet = Sheet::new(10, 10);
        let mut taken: Vec<Space> = Vec::new();
        for (w, h) in [(6, 4), (4, 6), (5, 5), (3, 3), (10, 1), (2, 2)] {
            if let Some((x, y)) = sheet.take(w, h) {
                let space = Space { x, y, w, h };
                assert!(space.right() <= 10 && space.bottom() <= 10, "{space:?}");
                assert!(taken.iter().all(|other| !other.touches(space)), "{space:?}");
                taken.push(space);
            }
        }
        assert!(taken.len() >= 4, "{taken:?}");
    }

    #[test]
    fn the_piece_sits_where_the_leftover_short_side_is_smallest() {
        let mut sheet = Sheet::new(100, 100);
        sheet.reserve(Space {
            x: 30,
            y: 40,
            w: 40,
            h: 30,
        });
        assert_eq!(sheet.take(100, 25), Some((0, 70)));
    }

    #[test]
    fn nothing_fits_into_a_sheet_that_is_too_small() {
        assert_eq!(Sheet::new(4, 4).take(5, 1), None);
        assert_eq!(Sheet::new(4, 4).take(4, 4), Some((0, 0)));
    }

    #[test]
    fn rectangles_lying_inside_another_free_one_are_dropped() {
        let outer = Space {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        let inner = Space {
            x: 2,
            y: 2,
            w: 3,
            h: 3,
        };
        assert_eq!(without_covered(vec![inner, outer, outer]), vec![outer]);
    }
}
