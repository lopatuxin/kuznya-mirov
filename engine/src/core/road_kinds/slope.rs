//! Фаза 53 в раскладке: куски вдоль оси-дуги по профилю высот и перекрёстки, чьи рукава ложатся на оси своих дорог в
//! плане и по высоте.

use super::fixtures::{self, MAIN, STREET};
use super::junction::{ArmAxis, Fit, SpotArm, place, place_on_axes, select};
use super::*;

fn road_form(id: u32, from: Vec2, to: Vec2, width: f64, bend: f64, z: [f64; 2]) -> RoadForm {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let chord = dx.hypot(dy);
    let middle = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
    RoadForm {
        shape: RoadShape {
            id,
            position: [middle[0] - chord / 2.0, middle[1] - width / 2.0],
            size: [chord, width],
            sin_cos: (dy / chord, dx / chord),
            z: z[0],
        },
        bend,
        end_z: z[1],
    }
}

fn laid(roads: &[(RoadForm, usize)]) -> (Vec<PlacedPiece>, Vec<Junction>, Vec<Junction>) {
    let (kinds, models) = fixtures::tables();
    let forms: Vec<LayoutForm> = roads
        .iter()
        .map(|&(form, kind)| LayoutForm {
            form,
            kind: Some(kind),
        })
        .collect();
    lay_out_forms(&forms, &kinds, &models)
}

fn world_points(piece: &Junction) -> Vec<([f64; 3], &ModelVertex)> {
    piece
        .parts
        .iter()
        .flat_map(|part| &part.vertices)
        .map(|vertex| {
            (
                [
                    piece.center[0] + f64::from(vertex.position[0]),
                    piece.center[1] + f64::from(vertex.position[1]),
                    piece.z + f64::from(vertex.position[2]),
                ],
                vertex,
            )
        })
        .collect()
}

fn near(a: f64, b: f64, eps: f64) {
    assert!((a - b).abs() <= eps, "ожидалось {b} ± {eps}, вышло {a}");
}

#[test]
fn a_straight_level_road_still_gets_instances_point_for_point_and_no_bent_copies() {
    let road = road_form(0, [0.0, 10.0], [20.0, 10.0], 1.0, 0.0, [0.0, 0.0]);
    let (pieces, junctions, bent) = laid(&[(road, STREET)]);
    assert!(bent.is_empty());
    assert_eq!(junctions.len(), 2, "два тупика");
    assert!(!pieces.is_empty());
    for piece in &pieces {
        near(piece.center[1], 10.0, 1e-12);
        near(piece.z, 0.0, 0.0);
        near(piece.cos, 0.0, 1e-12);
        near(piece.sin, -1.0, 1e-12);
    }
    let total: f64 = pieces.iter().map(|piece| piece.length).sum();
    let first = pieces[0].center[0] - pieces[0].length / 2.0;
    let last = pieces[pieces.len() - 1].center[0] + pieces[pieces.len() - 1].length / 2.0;
    near(total, last - first, 1e-9);
}

#[test]
fn the_pieces_of_an_arc_are_a_whole_number_of_equal_bent_copies_laid_edge_to_edge_on_the_strip() {
    let form = road_form(0, [0.0, 10.0], [20.0, 10.0], 1.0, 90.0, [0.0, 0.0]);
    let (pieces, junctions, bent) = laid(&[(form, STREET)]);
    assert!(pieces.is_empty(), "дуга не даёт прямых экземпляров");
    assert_eq!(junctions.len(), 2);
    let road = Road::from_form(&form);
    let (center, radius) = (road.arc_center(), road.radius());
    let mut spans: Vec<[f64; 2]> = Vec::new();
    for piece in &bent {
        assert!(piece.arms.is_empty());
        let mut low = f64::INFINITY;
        let mut high = f64::NEG_INFINITY;
        let (mut near_edge, mut far_edge) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut first, mut last) = (f64::INFINITY, f64::NEG_INFINITY);
        for (point, _) in world_points(piece) {
            let flat = [point[0], point[1]];
            let across = distance(flat, center) - radius;
            near_edge = near_edge.min(across);
            far_edge = far_edge.max(across);
            let s = road.closest(flat).s;
            first = first.min(s);
            last = last.max(s);
            low = low.min(point[2]);
            high = high.max(point[2]);
        }
        near(near_edge, -0.5, 1e-6);
        near(far_edge, 0.5, 1e-6);
        near(low, 0.0, 1e-6);
        near(high, 0.0, 1e-6);
        spans.push([first, last]);
    }
    spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let each = spans[0][1] - spans[0][0];
    for pair in spans.windows(2) {
        near(pair[1][0], pair[0][1], 1e-6);
        near(pair[1][1] - pair[1][0], each, 1e-6);
    }
    assert!(spans.len() >= 3, "целое число кусков по длине дуги");
}

#[test]
fn the_pieces_of_a_slope_follow_the_profile_in_height_and_tilt_their_normals_with_it() {
    let form = road_form(0, [0.0, 10.0], [20.0, 10.0], 1.0, 0.0, [0.0, 2.0]);
    let (pieces, _, bent) = laid(&[(form, STREET)]);
    assert!(pieces.is_empty());
    let road = Road::from_form(&form);
    let pitch = 0.1_f64.atan();
    for piece in &bent {
        for (point, vertex) in world_points(piece) {
            let s = road.closest([point[0], point[1]]).s;
            near(point[2], road.height_at(s), 1e-5);
            near(f64::from(vertex.normal[2]), pitch.cos(), 1e-5);
            near(f64::from(vertex.normal[0]), -pitch.sin(), 1e-5);
        }
    }
}

#[test]
fn the_pieces_of_a_sloping_arc_keep_both_the_arc_and_the_profile() {
    let form = road_form(0, [0.0, 10.0], [20.0, 10.0], 1.0, -60.0, [0.5, 2.5]);
    let (_, _, bent) = laid(&[(form, MAIN)]);
    let road = Road::from_form(&form);
    assert!(!bent.is_empty());
    for piece in &bent {
        for (point, _) in world_points(piece) {
            let near_axis = road.closest([point[0], point[1]]);
            assert!(near_axis.distance <= 0.5 + 1e-6, "{point:?}");
            near(point[2], road.height_at(near_axis.s), 1e-5);
        }
    }
}

#[test]
fn a_tee_on_a_sloping_arc_stands_at_the_height_of_the_earlier_road_and_its_arms_lie_on_their_axes()
{
    for (trunk_bend, trunk_rise, side_bend, side_rise) in [
        (0.0, 0.0, 0.0, 0.0),
        (0.0, 2.0, 0.0, 0.0),
        (40.0, 2.0, 0.0, 0.0),
        (-40.0, -1.5, 25.0, 0.5),
        (0.0, 2.0, -50.0, -0.8),
        (60.0, 2.0, 30.0, 1.0),
    ] {
        let trunk = road_form(
            0,
            [2.0, 10.0],
            [22.0, 10.0],
            1.0,
            trunk_bend,
            [0.0, trunk_rise],
        );
        let trunk_road = Road::from_form(&trunk);
        let middle = trunk_road.point_at(trunk_road.length() / 2.0);
        let height = trunk_road.height_at(trunk_road.length() / 2.0);
        let mut side = road_form(
            1,
            [middle[0], middle[1] - 9.0],
            middle,
            1.0,
            side_bend,
            [height - side_rise, height],
        );
        side.end_z = height;
        side.shape.z = height - side_rise;
        let (_, junctions, _) = laid(&[(trunk, STREET), (side, STREET)]);
        let joint = junctions
            .iter()
            .find(|junction| distance(junction.center, middle) < 1e-6 && junction.arms.len() == 3)
            .unwrap_or_else(|| panic!("тройник не найден: {junctions:?}"));
        near(joint.z, height, 1e-9);
        let side_road = Road::from_form(&side);
        for (road, name) in [(&trunk_road, "магистраль"), (&side_road, "улица")] {
            for (point, vertex) in world_points(joint) {
                let flat = [point[0], point[1]];
                let near_axis = road.closest(flat);
                let along = (near_axis.s - road.closest(middle).s).abs();
                let in_strip = near_axis.distance <= 0.5;
                let beyond_center =
                    along >= 1.6 && near_axis.s > 0.0 && near_axis.s < road.length();
                let on_other = |other: &Road| other.closest(flat).distance <= 0.55;
                let other = if name == "магистраль" {
                    &side_road
                } else {
                    &trunk_road
                };
                if in_strip && beyond_center && !on_other(other) {
                    assert!(
                        (point[2] - road.height_at(near_axis.s)).abs() <= 1e-5,
                        "{name}, изгибы {trunk_bend} и {side_bend}: точка {point:?}, s {}, along {along}, высота профиля {}",
                        near_axis.s,
                        road.height_at(near_axis.s)
                    );
                    assert!(
                        vertex.normal[2] > 0.9,
                        "{name}: нормаль {:?}",
                        vertex.normal
                    );
                }
            }
        }
    }
}

/// Площадь треугольника на карте со знаком.
fn plan_area(part: &Part, triangle: &[u32; 3]) -> f64 {
    let at = |index: u32| part.vertices[index as usize].position;
    let (a, b, c) = (at(triangle[0]), at(triangle[1]), at(triangle[2]));
    let (ux, uy) = (f64::from(b[0] - a[0]), f64::from(b[1] - a[1]));
    let (vx, vy) = (f64::from(c[0] - a[0]), f64::from(c[1] - a[1]));
    (ux * vy - uy * vx) / 2.0
}

fn flipped(flat: &[Part], laid_on_axes: &[Part]) -> usize {
    let mut count = 0;
    for (before, after) in flat.iter().zip(laid_on_axes) {
        for (b, a) in before
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .zip(after.indices.as_chunks::<3>().0.iter())
        {
            let original = plan_area(before, b);
            if original.abs() > 1e-9 && plan_area(after, a) * original < 1e-12 {
                count += 1;
            }
        }
    }
    count
}

fn arms_at(angles: &[(f64, usize)], turn: f64) -> Vec<SpotArm> {
    let mut arms: Vec<SpotArm> = angles
        .iter()
        .map(|&(degrees, kind)| SpotArm {
            angle: (degrees + turn)
                .to_radians()
                .rem_euclid(std::f64::consts::TAU),
            kind,
            width: if kind == MAIN { 1.5 } else { 1.0 },
            offset: 0.0,
        })
        .collect();
    arms.sort_by(|a, b| a.angle.total_cmp(&b.angle));
    arms
}

fn picked(arms: &[SpotArm]) -> (RoadKindTable, ModelTable, Fit) {
    let (kinds, models) = fixtures::tables();
    let fit = select(&kinds, &models, arms).expect("модель нашлась");
    (kinds, models, fit)
}

#[test]
fn bending_and_lifting_the_arms_never_turns_a_triangle_inside_out() {
    let shapes: [&[(f64, usize)]; 5] = [
        &[(0.0, STREET)],
        &[(0.0, STREET), (180.0, STREET)],
        &[(0.0, STREET), (90.0, STREET)],
        &[(0.0, STREET), (90.0, STREET), (180.0, STREET)],
        &[
            (0.0, STREET),
            (90.0, STREET),
            (180.0, STREET),
            (270.0, STREET),
        ],
    ];
    let bends = [-0.1, -0.03, 0.0, 0.03, 0.1];
    let slopes = [-0.1, 0.0, 0.1];
    let rises = [-0.4, 0.0, 0.4];
    for angles in shapes {
        for turn in [0.0, 13.0, 45.0] {
            let arms = arms_at(angles, turn);
            let (kinds, models, fit) = picked(&arms);
            let flat = place(&kinds, &models, &fit, &arms).expect("модель");
            for b in 0..bends.len() {
                for s in 0..slopes.len() {
                    for r in 0..rises.len() {
                        let axes: Vec<ArmAxis> = (0..arms.len())
                            .map(|k| ArmAxis {
                                bend: bends[(b + k) % bends.len()]
                                    * if k % 2 == 0 { 1.0 } else { -1.0 },
                                slope: slopes[(s + k) % slopes.len()],
                                rise: rises[(r + k) % rises.len()],
                            })
                            .collect();
                        let on_axes =
                            place_on_axes(&kinds, &models, &fit, &arms, &axes).expect("модель");
                        assert_eq!(
                            flipped(&flat.parts, &on_axes.parts),
                            0,
                            "{angles:?} +{turn}° {axes:?}"
                        );
                        assert_eq!(flat.mouths, on_axes.mouths);
                    }
                }
            }
        }
    }
}

#[test]
fn straight_level_axes_leave_the_warp_exactly_as_it_was() {
    let arms = arms_at(&[(0.0, STREET), (90.0, STREET), (180.0, STREET)], 20.0);
    let (kinds, models, fit) = picked(&arms);
    let flat = place(&kinds, &models, &fit, &arms).expect("модель");
    let axes = vec![ArmAxis::default(); arms.len()];
    let same = place_on_axes(&kinds, &models, &fit, &arms, &axes).expect("модель");
    assert_eq!(flat.parts, same.parts);
}

#[test]
fn the_mouth_of_an_arm_meets_the_first_piece_of_its_road_without_a_seam() {
    for (trunk_bend, trunk_rise) in [(0.0, 2.0), (40.0, 2.0), (-50.0, -1.5), (30.0, 0.0)] {
        let trunk = road_form(
            0,
            [2.0, 10.0],
            [22.0, 10.0],
            1.0,
            trunk_bend,
            [0.0, trunk_rise],
        );
        let trunk_road = Road::from_form(&trunk);
        let at = trunk_road.length() / 2.0;
        let middle = trunk_road.point_at(at);
        let side = road_form(
            1,
            [middle[0], middle[1] - 9.0],
            middle,
            1.0,
            0.0,
            [trunk_road.height_at(at), trunk_road.height_at(at)],
        );
        let (_, junctions, bent) = laid(&[(trunk, STREET), (side, STREET)]);
        let joint = junctions
            .iter()
            .find(|junction| distance(junction.center, middle) < 1e-6 && junction.arms.len() == 3)
            .expect("тройник");
        let nodes = world_points(joint);
        let mut checked = 0;
        for piece in &bent {
            for (point, _) in world_points(piece) {
                let s = trunk_road.closest([point[0], point[1]]).s;
                let at_mouth = joint
                    .arms
                    .iter()
                    .any(|arm| ((s - at).abs() - arm.mouth).abs() < 1e-6);
                if !at_mouth || trunk_road.closest([point[0], point[1]]).distance > 0.51 {
                    continue;
                }
                let gap = nodes
                    .iter()
                    .map(|(node, _)| {
                        (node[0] - point[0])
                            .hypot(node[1] - point[1])
                            .hypot(node[2] - point[2])
                    })
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    gap < 2e-3,
                    "шов {gap} у {point:?}, изгиб {trunk_bend}, подъём {trunk_rise}"
                );
                checked += 1;
            }
        }
        assert!(checked >= 6, "краёв кусков у устий: {checked}");
    }
}

#[test]
fn a_joint_of_roads_that_differ_in_height_stands_on_the_road_that_is_earlier_in_the_scene() {
    let trunk = road_form(0, [2.0, 10.0], [22.0, 10.0], 1.0, 0.0, [0.0, 2.0]);
    let middle = [12.0, 10.0];
    let on_trunk = 1.0;
    let side = |id: u32, rise: f64| {
        road_form(
            id,
            [middle[0], middle[1] - 9.0],
            middle,
            1.0,
            0.0,
            [on_trunk + rise - 0.9, on_trunk + rise],
        )
    };
    let tee_height = |first: (RoadForm, usize), second: (RoadForm, usize)| {
        let (_, junctions, _) = laid(&[first, second]);
        junctions
            .iter()
            .find(|junction| distance(junction.center, middle) < 1e-6 && junction.arms.len() == 3)
            .expect("тройник")
            .z
    };
    near(
        tee_height((trunk, STREET), (side(1, 0.3), STREET)),
        on_trunk,
        1e-9,
    );
    let mut early = side(0, 0.3);
    early.shape.id = 0;
    let mut late = trunk;
    late.shape.id = 1;
    near(
        tee_height((early, STREET), (late, STREET)),
        on_trunk + 0.3,
        1e-9,
    );
}
