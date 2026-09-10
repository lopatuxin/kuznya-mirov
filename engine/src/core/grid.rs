use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    /// Strict overlap on both axes: touching exactly at the border is not an intersection.
    /// Returns the overlap amount on each axis when they do intersect.
    pub fn overlap(&self, other: &Rect) -> Option<(f64, f64)> {
        let ox = (self.x + self.w).min(other.x + other.w) - self.x.max(other.x);
        let oy = (self.y + self.h).min(other.y + other.h) - self.y.max(other.y);
        if ox > 0.0 && oy > 0.0 {
            Some((ox, oy))
        } else {
            None
        }
    }
}

pub fn largest_dimension(objects: &[(u32, Rect)]) -> f64 {
    objects
        .iter()
        .map(|(_, r)| r.w.max(r.h))
        .fold(0.0_f64, f64::max)
}

#[derive(Debug)]
pub struct SpatialGrid {
    cell_size: f64,
    cells: HashMap<(i64, i64), Vec<u32>>,
}

impl SpatialGrid {
    pub fn new() -> Self {
        SpatialGrid {
            cell_size: 1.0,
            cells: HashMap::new(),
        }
    }

    /// Rebuilds the grid from scratch (reusing bucket memory) and returns the sorted,
    /// deduplicated pairs of ids whose rectangles strictly overlap.
    pub fn find_pairs(&mut self, cell_size: f64, objects: &[(u32, Rect)]) -> Vec<(u32, u32)> {
        self.cell_size = cell_size.max(1e-6);
        for bucket in self.cells.values_mut() {
            bucket.clear();
        }
        for &(id, rect) in objects {
            let (cx0, cy0) = self.cell_of(rect.x, rect.y);
            let (cx1, cy1) = self.cell_of(rect.x + rect.w - 1e-9, rect.y + rect.h - 1e-9);
            for cx in cx0..=cx1 {
                for cy in cy0..=cy1 {
                    self.cells.entry((cx, cy)).or_default().push(id);
                }
            }
        }

        let mut raw_pairs = Vec::new();
        for bucket in self.cells.values() {
            for i in 0..bucket.len() {
                for j in (i + 1)..bucket.len() {
                    let (a, b) = (bucket[i], bucket[j]);
                    raw_pairs.push(if a < b { (a, b) } else { (b, a) });
                }
            }
        }
        raw_pairs.sort_unstable();
        raw_pairs.dedup();

        let rect_of: HashMap<u32, Rect> = objects.iter().copied().collect();
        raw_pairs.retain(|&(a, b)| rect_of[&a].overlap(&rect_of[&b]).is_some());
        raw_pairs
    }

    fn cell_of(&self, x: f64, y: f64) -> (i64, i64) {
        (
            (x / self.cell_size).floor() as i64,
            (y / self.cell_size).floor() as i64,
        )
    }
}

impl Default for SpatialGrid {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touching_boundary_is_not_an_overlap() {
        let a = Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        let b = Rect {
            x: 1.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        assert_eq!(a.overlap(&b), None);
    }

    #[test]
    fn genuine_overlap_is_found() {
        let a = Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        let b = Rect {
            x: 0.5,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        let (ox, oy) = a.overlap(&b).expect("rects overlap");
        assert!((ox - 0.5).abs() < 1e-9);
        assert!((oy - 1.0).abs() < 1e-9);
    }

    #[test]
    fn find_pairs_ignores_touching_and_dedups_across_cells() {
        let mut grid = SpatialGrid::new();
        let objects = vec![
            (
                1,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                },
            ),
            (
                2,
                Rect {
                    x: 0.9,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                },
            ),
            (
                3,
                Rect {
                    x: 2.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                },
            ),
        ];
        let pairs = grid.find_pairs(1.0, &objects);
        assert_eq!(pairs, vec![(1, 2)]);
    }
}
