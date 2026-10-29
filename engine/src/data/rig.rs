//! «Трёхмерная сцена» → «Модели и движения», «Загрузка и проверка»: скелет, веса вершин и движения из файла
//! `.glb`. Кости берутся со всей иерархии сцены, движения — по стандарту glTF: сдвиг, поворот и масштаб
//! костей во времени, переход линейный, ступенчатый и кубический.

use std::collections::HashMap;

use gltf::animation::util::ReadOutputs;
use gltf::animation::{Interpolation as FileInterpolation, Property};
use gltf::{Document, Node, Primitive, Scene};

use crate::core::math3::{IDENTITY, Mat4, mul};
use crate::core::model::SkinVertex;
use crate::core::skeleton::{
    Bone, Channel, Clip, Interpolation, MAX_JOINTS, Skeleton, Track, Trs, translation,
};

/// Обмен осей файла и сцены: y ↔ z. Отражение, поэтому сам себе обратный.
pub const AXES: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Наибольшее число наборов `JOINTS_n` и `WEIGHTS_n`, которые читаются: из них берутся четыре самых тяжёлых.
const MAX_INFLUENCE_SETS: u32 = 4;

/// Скелет файла: он пока в осях файла, середина основания модели станет началом, когда будет известна коробка.
pub fn read_skeleton(
    document: &Document,
    scene: &Scene<'_>,
    blob: Option<&[u8]>,
) -> Result<Option<Skeleton>, String> {
    if !document.nodes().any(|node| node.skin().is_some()) {
        return Ok(None);
    }
    let mut skins = document.skins();
    let Some(skin) = skins.next() else {
        return Ok(None);
    };
    if skins.next().is_some() {
        return Err("в файле несколько скелетов: движок читает один".to_string());
    }
    let mut bones = Vec::new();
    let mut slot = HashMap::new();
    for root in scene.nodes() {
        collect_bones(&root, None, &mut bones, &mut slot);
    }
    let joints = skin
        .joints()
        .map(|joint| {
            slot.get(&joint.index()).copied().ok_or_else(|| {
                format!(
                    "сустав \"{}\" скелета вне сцены файла",
                    joint.name().unwrap_or_default()
                )
            })
        })
        .collect::<Result<Vec<usize>, String>>()?;
    if joints.is_empty() || joints.len() > MAX_JOINTS {
        return Err(format!(
            "в скелете {} суставов, а читается от 1 до {MAX_JOINTS}",
            joints.len()
        ));
    }
    let inverse_binds: Vec<Mat4> = match skin.reader(|_| blob).read_inverse_bind_matrices() {
        Some(matrices) => matrices.collect(),
        None => vec![IDENTITY; joints.len()],
    };
    if inverse_binds.len() != joints.len() {
        return Err(format!(
            "у скелета {} суставов и {} обратных матриц связывания",
            joints.len(),
            inverse_binds.len()
        ));
    }
    let clips = document
        .animations()
        .map(|animation| read_clip(&animation, blob, &slot))
        .collect::<Result<Vec<Clip>, String>>()?;
    Ok(Some(Skeleton {
        bones,
        joints,
        head: AXES,
        tails: inverse_binds.iter().map(|ibm| mul(ibm, &AXES)).collect(),
        clips,
        idle: None,
        walk: None,
        climb: None,
        legs: None,
        spine: None,
        hands: None,
    }))
}

/// Номер сустава скелета по номеру узла файла: по нему сетка без `skin` на узле кости находит свою кость.
pub fn joint_nodes(document: &Document) -> HashMap<usize, usize> {
    document
        .skins()
        .next()
        .map(|skin| {
            skin.joints()
                .enumerate()
                .map(|(joint, node)| (node.index(), joint))
                .collect()
        })
        .unwrap_or_default()
}

/// Вершина, что целиком идёт за суставом `joint`.
pub fn held_by(joint: usize) -> SkinVertex {
    SkinVertex {
        joints: [joint as u16, 0, 0, 0],
        weights: [1.0, 0.0, 0.0, 0.0],
    }
}

/// Середина основания модели в начале: кости сдвигаются на `shift` в осях модели.
pub fn recentre(skeleton: &mut Skeleton, shift: [f32; 3]) {
    skeleton.head = mul(&translation(shift.map(|c| -c)), &AXES);
}

fn collect_bones(
    node: &Node<'_>,
    parent: Option<usize>,
    bones: &mut Vec<Bone>,
    slot: &mut HashMap<usize, usize>,
) {
    let at = bones.len();
    slot.insert(node.index(), at);
    let (translation, rotation, scale) = node.transform().decomposed();
    bones.push(Bone {
        name: node.name().unwrap_or_default().to_string(),
        parent,
        rest: Trs {
            translation,
            rotation,
            scale,
        },
    });
    for child in node.children() {
        collect_bones(&child, Some(at), bones, slot);
    }
}

fn read_clip(
    animation: &gltf::Animation<'_>,
    blob: Option<&[u8]>,
    slot: &HashMap<usize, usize>,
) -> Result<Clip, String> {
    let name = animation
        .name()
        .map_or_else(|| format!("animation{}", animation.index()), str::to_string);
    let mut tracks: Vec<Track> = Vec::new();
    let (mut first, mut last) = (f32::INFINITY, f32::NEG_INFINITY);
    for file_channel in animation.channels() {
        let Some(&bone) = slot.get(&file_channel.target().node().index()) else {
            continue;
        };
        let property = file_channel.target().property();
        let reader = file_channel.reader(|_| blob);
        let (Some(inputs), Some(outputs)) = (reader.read_inputs(), reader.read_outputs()) else {
            return Err(format!("движение \"{name}\": ключи не читаются"));
        };
        let times: Vec<f32> = inputs.collect();
        let rotation = matches!(outputs, ReadOutputs::Rotations(_));
        let values: Vec<[f32; 4]> = match outputs {
            ReadOutputs::Translations(values) | ReadOutputs::Scales(values) => {
                values.map(|[x, y, z]| [x, y, z, 0.0]).collect()
            }
            ReadOutputs::Rotations(values) => values.into_f32().collect(),
            ReadOutputs::MorphTargetWeights(_) => continue,
        };
        let interpolation = match file_channel.sampler().interpolation() {
            FileInterpolation::Linear => Interpolation::Linear,
            FileInterpolation::Step => Interpolation::Step,
            FileInterpolation::CubicSpline => Interpolation::Cubic,
        };
        let per_key = if interpolation == Interpolation::Cubic {
            3
        } else {
            1
        };
        if times.is_empty()
            || values.len() != times.len() * per_key
            || times.iter().any(|time| !time.is_finite())
            || times.windows(2).any(|pair| pair[1] < pair[0])
        {
            return Err(format!(
                "движение \"{name}\": ключи времени и значения не сходятся или время идёт назад"
            ));
        }
        first = first.min(times[0]);
        last = last.max(times[times.len() - 1]);
        let channel = Channel {
            times,
            values,
            interpolation,
            rotation,
        };
        let at = tracks
            .iter()
            .position(|track| track.bone == bone)
            .unwrap_or_else(|| {
                tracks.push(Track {
                    bone,
                    translation: None,
                    rotation: None,
                    scale: None,
                });
                tracks.len() - 1
            });
        let track = &mut tracks[at];
        match property {
            Property::Translation => track.translation = Some(channel),
            Property::Rotation => track.rotation = Some(channel),
            Property::Scale => track.scale = Some(channel),
            Property::MorphTargetWeights => {}
        }
    }
    let playable = !tracks.is_empty();
    Ok(Clip {
        name,
        start: if playable { first } else { 0.0 },
        length: if playable { last - first } else { 0.0 },
        tracks,
        retouch: None,
    })
}

/// Веса вершин части: до четырёх самых тяжёлых суставов из всех наборов `JOINTS_n`, веса сведены к единице.
pub fn skin_vertices(
    primitive: &Primitive<'_>,
    blob: Option<&[u8]>,
    vertices: usize,
    joints: usize,
) -> Result<Vec<SkinVertex>, String> {
    let reader = primitive.reader(|_| blob);
    let mut sets = Vec::new();
    for set in 0..MAX_INFLUENCE_SETS {
        let (Some(set_joints), Some(set_weights)) =
            (reader.read_joints(set), reader.read_weights(set))
        else {
            break;
        };
        let (set_joints, set_weights): (Vec<[u16; 4]>, Vec<[f32; 4]>) = (
            set_joints.into_u16().collect(),
            set_weights.into_f32().collect(),
        );
        if set_joints.len() != vertices || set_weights.len() != vertices {
            return Err("суставов и весов в сетке не столько, сколько вершин".to_string());
        }
        sets.push((set_joints, set_weights));
    }
    if sets.is_empty() {
        return Err("часть привязана к скелету, но в ней нет JOINTS_0 и WEIGHTS_0".to_string());
    }
    (0..vertices)
        .map(|at| {
            let pulls = sets.iter().flat_map(|(set_joints, set_weights)| {
                set_joints[at].into_iter().zip(set_weights[at])
            });
            strongest(pulls, joints)
        })
        .collect()
}

/// Четыре самых тяжёлых сустава из `pulls` — пар «сустав, вес»; веса сведены к единице, нулевые отброшены.
fn strongest(pulls: impl Iterator<Item = (u16, f32)>, joints: usize) -> Result<SkinVertex, String> {
    let mut pulls: Vec<(u16, f32)> = pulls.filter(|&(_, weight)| weight > 0.0).collect();
    pulls.sort_by(|a, b| b.1.total_cmp(&a.1));
    pulls.truncate(4);
    if pulls.iter().any(|&(joint, _)| usize::from(joint) >= joints) {
        return Err(format!(
            "в весах вершины сустав за пределами скелета из {joints} суставов"
        ));
    }
    let total: f32 = pulls.iter().map(|&(_, weight)| weight).sum();
    let mut vertex = SkinVertex::default();
    if total > 0.0 {
        for (slot, (joint, weight)) in pulls.into_iter().enumerate() {
            vertex.joints[slot] = joint;
            vertex.weights[slot] = weight / total;
        }
    }
    Ok(vertex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn of_eight_influences_the_four_heaviest_stay_and_their_weights_sum_to_one() {
        let pulls = [
            (0, 0.05),
            (1, 0.3),
            (2, 0.1),
            (3, 0.2),
            (4, 0.15),
            (5, 0.1),
            (6, 0.04),
            (7, 0.06),
        ];
        let vertex = strongest(pulls.into_iter(), 8).expect("веса годятся");
        assert_eq!(vertex.joints, [1, 3, 4, 2]);
        let total: f32 = vertex.weights.iter().sum();
        assert!((total - 1.0).abs() < 1e-6);
        assert!(vertex.weights[0] > vertex.weights[1] && vertex.weights[1] > vertex.weights[2]);
        assert!(
            (vertex.weights[0] - 0.3 / 0.75).abs() < 1e-6,
            "{:?}",
            vertex.weights
        );
    }

    #[test]
    fn a_vertex_without_weights_stands_still_and_a_joint_past_the_skeleton_is_an_error() {
        let still = strongest([(3, 0.0), (1, 0.0)].into_iter(), 8).expect("веса годятся");
        assert_eq!(still, SkinVertex::default());
        assert!(strongest([(8, 1.0)].into_iter(), 8).is_err());
        assert!(
            strongest([(8, 0.0)].into_iter(), 8).is_ok(),
            "нулевой вес не считается"
        );
    }
}
