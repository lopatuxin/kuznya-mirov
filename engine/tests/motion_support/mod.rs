//! Общее для проверок движений с поправками (`phase55_motions.rs`): маленькая модель человека со скелетом, которую
//! собирает сам тест, и позы её движений в осях модели.

#![allow(dead_code)]

use engine::core::skeleton::{Blend, Playing, PoseScratch, Skeleton};
use serde_json::{Value, json};

/// Кость файла: имя, номер родителя и сдвиг от родителя в осях файла (+Y вверх, +Z вперёд).
pub struct Node {
    pub name: &'static str,
    pub parent: Option<usize>,
    pub at: [f32; 3],
}

/// Один канал движения файла: кость, свойство (`translation` или `rotation`), время и значения ключей.
pub struct Channel {
    pub node: usize,
    pub path: &'static str,
    pub times: Vec<f32>,
    pub values: Vec<Vec<f32>>,
}

pub struct ClipSpec {
    pub name: &'static str,
    pub channels: Vec<Channel>,
}

pub const ARMATURE: usize = 0;
pub const HIPS: usize = 1;
pub const FOOT_LEFT: usize = 4;
pub const FOOT_RIGHT: usize = 7;
pub const SPINE: usize = 8;
pub const CHEST: usize = 9;
pub const ARM_LEFT: usize = 10;
pub const HAND_LEFT: usize = 12;
pub const ARM_RIGHT: usize = 13;
pub const HAND_RIGHT: usize = 15;

/// Человек ростом 1,9: таз на высоте 1, ноги вниз, позвоночник и грудь вверх, руки вниз от плеч.
pub fn person() -> Vec<Node> {
    let node = |name, parent, at| Node { name, parent, at };
    vec![
        node("armature", None, [0.0, 0.0, 0.0]),
        node("hips", Some(0), [0.0, 1.0, 0.0]),
        node("thigh_l", Some(1), [-0.1, 0.0, 0.0]),
        node("shin_l", Some(2), [0.0, -0.5, 0.0]),
        node("foot_l", Some(3), [0.0, -0.5, 0.0]),
        node("thigh_r", Some(1), [0.1, 0.0, 0.0]),
        node("shin_r", Some(5), [0.0, -0.5, 0.0]),
        node("foot_r", Some(6), [0.0, -0.5, 0.0]),
        node("spine", Some(1), [0.0, 0.2, 0.0]),
        node("chest", Some(8), [0.0, 0.3, 0.0]),
        node("arm_l", Some(9), [-0.15, 0.2, 0.0]),
        node("forearm_l", Some(10), [0.0, -0.3, 0.0]),
        node("hand_l", Some(11), [0.0, -0.3, 0.0]),
        node("arm_r", Some(9), [0.15, 0.2, 0.0]),
        node("forearm_r", Some(13), [0.0, -0.3, 0.0]),
        node("hand_r", Some(14), [0.0, -0.3, 0.0]),
    ]
}

fn floats(values: impl IntoIterator<Item = f32>) -> Vec<u8> {
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}

fn rest_position(nodes: &[Node], node: usize) -> [f32; 3] {
    let own = nodes[node].at;
    match nodes[node].parent {
        Some(parent) => {
            let above = rest_position(nodes, parent);
            [above[0] + own[0], above[1] + own[1], above[2] + own[2]]
        }
        None => own,
    }
}

/// `.glb` модели: `nodes` как кости (все, кроме первой, — суставы скелета), плоский щиток 0,6 × 1,9 на первом
/// суставе и движения `clips`.
pub fn rig_glb(nodes: &[Node], clips: &[ClipSpec]) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let mut views: Vec<Value> = Vec::new();
    let mut accessors: Vec<Value> = Vec::new();
    let mut put = |bytes: Vec<u8>, component: u32, kind: &str, count: usize, extra: Value| {
        let offset = bin.len();
        bin.extend(&bytes);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        views.push(json!({"buffer": 0, "byteOffset": offset, "byteLength": bytes.len()}));
        let mut accessor = json!({
            "bufferView": views.len() - 1, "componentType": component, "count": count, "type": kind
        });
        if let (Some(extra), Some(target)) = (extra.as_object(), accessor.as_object_mut()) {
            target.extend(extra.clone());
        }
        accessors.push(accessor);
        accessors.len() - 1
    };
    let position = put(
        floats([-0.3, 0.0, 0.0, 0.3, 0.0, 0.0, 0.3, 1.9, 0.0, -0.3, 1.9, 0.0]),
        5126,
        "VEC3",
        4,
        json!({"min": [-0.3, 0, 0], "max": [0.3, 1.9, 0]}),
    );
    let indices = put(
        vec![0, 0, 1, 0, 2, 0, 0, 0, 2, 0, 3, 0],
        5123,
        "SCALAR",
        6,
        json!({}),
    );
    let joints = put(vec![0; 16], 5121, "VEC4", 4, json!({}));
    let weights = put(
        floats([1.0, 0.0, 0.0, 0.0].repeat(4)),
        5126,
        "VEC4",
        4,
        json!({}),
    );
    let inverse_binds = put(
        floats((1..nodes.len()).flat_map(|node| {
            let at = rest_position(nodes, node);
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -at[0], -at[1], -at[2],
                1.0,
            ]
        })),
        5126,
        "MAT4",
        nodes.len() - 1,
        json!({}),
    );
    let mut animations = Vec::new();
    for clip in clips {
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for channel in &clip.channels {
            let last = channel.times.last().copied().unwrap_or(0.0);
            let input = put(
                floats(channel.times.iter().copied()),
                5126,
                "SCALAR",
                channel.times.len(),
                json!({"min": [0], "max": [last]}),
            );
            let width = if channel.path == "rotation" { 4 } else { 3 };
            let output = put(
                floats(channel.values.iter().flatten().copied()),
                5126,
                if width == 4 { "VEC4" } else { "VEC3" },
                channel.values.len(),
                json!({}),
            );
            samplers.push(json!({"input": input, "output": output, "interpolation": "LINEAR"}));
            channels.push(json!({
                "sampler": samplers.len() - 1,
                "target": {"node": channel.node, "path": channel.path}
            }));
        }
        animations.push(json!({"name": clip.name, "samplers": samplers, "channels": channels}));
    }
    let mut json_nodes: Vec<Value> = nodes
        .iter()
        .enumerate()
        .map(|(at, node)| {
            let children: Vec<usize> = (0..nodes.len())
                .filter(|&child| nodes[child].parent == Some(at))
                .collect();
            let mut value = json!({"name": node.name, "translation": node.at});
            if at == ARMATURE {
                value["children"] = json!(
                    children
                        .iter()
                        .copied()
                        .chain([nodes.len()])
                        .collect::<Vec<_>>()
                );
            } else if !children.is_empty() {
                value["children"] = json!(children);
            }
            value
        })
        .collect();
    json_nodes.push(json!({"name": "body", "mesh": 0, "skin": 0}));
    let document = json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": json_nodes,
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": position, "JOINTS_0": joints, "WEIGHTS_0": weights},
            "indices": indices
        }]}],
        "skins": [{
            "joints": (1..nodes.len()).collect::<Vec<_>>(),
            "inverseBindMatrices": inverse_binds
        }],
        "animations": animations,
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": views,
        "accessors": accessors
    });
    let mut text = document.to_string().into_bytes();
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut file = Vec::new();
    file.extend(b"glTF");
    file.extend(2u32.to_le_bytes());
    file.extend((total as u32).to_le_bytes());
    file.extend((text.len() as u32).to_le_bytes());
    file.extend(0x4E4F534Au32.to_le_bytes());
    file.extend(text);
    file.extend((bin.len() as u32).to_le_bytes());
    file.extend(0x004E4942u32.to_le_bytes());
    file.extend(bin);
    file
}

fn channel(node: usize, path: &'static str, keys: &[(f32, &[f32])]) -> Channel {
    Channel {
        node,
        path,
        times: keys.iter().map(|key| key.0).collect(),
        values: keys.iter().map(|key| key.1.to_vec()).collect(),
    }
}

/// Движения человека: стойка, бег с подскоком таза, взмах рукой, скольжение правой ступни по земле на стоящем теле,
/// бег на месте с равномерным уходом стоящей ступни назад, тот же бег с ускорившейся посреди шага левой ступнёй и
/// уход спины.
pub fn person_clips() -> Vec<ClipSpec> {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    vec![
        ClipSpec {
            name: "Idle",
            channels: vec![channel(
                HIPS,
                "translation",
                &[(0.0, &[0.0, 1.0, 0.0]), (1.0, &[0.0, 1.0, 0.0])],
            )],
        },
        ClipSpec {
            name: "Run",
            channels: vec![channel(
                HIPS,
                "translation",
                &[
                    (0.0, &[0.0, 1.0, 0.0]),
                    (0.5, &[0.0, 1.1, 0.0]),
                    (1.0, &[0.0, 1.0, 0.0]),
                ],
            )],
        },
        ClipSpec {
            name: "Wave",
            channels: vec![
                channel(
                    HIPS,
                    "translation",
                    &[(0.0, &[0.0, 1.3, 0.0]), (1.0, &[0.0, 1.3, 0.0])],
                ),
                channel(
                    ARM_LEFT,
                    "rotation",
                    &[
                        (0.0, &[0.0, 0.0, half, half]),
                        (1.0, &[0.0, 0.0, half, half]),
                    ],
                ),
            ],
        },
        ClipSpec {
            name: "Skate",
            channels: vec![channel(
                FOOT_RIGHT,
                "translation",
                &[
                    (0.0, &[0.0, -0.5, 0.0]),
                    (0.25, &[0.0, -0.5, 0.5]),
                    (0.5, &[0.0, -0.5, 0.0]),
                    (1.0, &[0.0, -0.5, 0.0]),
                ],
            )],
        },
        ClipSpec {
            name: "Treadmill",
            channels: vec![
                channel(
                    FOOT_LEFT,
                    "translation",
                    &[
                        (0.0, &[0.0, -0.5, 0.2]),
                        (0.5, &[0.0, -0.5, -0.2]),
                        (0.6, &[0.0, -0.3, -0.28]),
                        (0.9, &[0.0, -0.3, 0.28]),
                        (1.0, &[0.0, -0.5, 0.2]),
                    ],
                ),
                channel(
                    FOOT_RIGHT,
                    "translation",
                    &[
                        (0.0, &[0.0, -0.5, -0.2]),
                        (0.1, &[0.0, -0.3, -0.28]),
                        (0.4, &[0.0, -0.3, 0.28]),
                        (0.5, &[0.0, -0.5, 0.2]),
                        (1.0, &[0.0, -0.5, -0.2]),
                    ],
                ),
            ],
        },
        ClipSpec {
            name: "Stumble",
            channels: vec![
                channel(
                    FOOT_LEFT,
                    "translation",
                    &[
                        (0.0, &[0.0, -0.5, 0.3]),
                        (0.25, &[0.0, -0.5, 0.1]),
                        (0.5, &[0.0, -0.5, -0.5]),
                        (0.6, &[0.0, -0.3, -0.58]),
                        (0.9, &[0.0, -0.3, 0.38]),
                        (1.0, &[0.0, -0.5, 0.3]),
                    ],
                ),
                channel(
                    FOOT_RIGHT,
                    "translation",
                    &[
                        (0.0, &[0.0, -0.5, -0.2]),
                        (0.1, &[0.0, -0.3, -0.28]),
                        (0.4, &[0.0, -0.3, 0.28]),
                        (0.5, &[0.0, -0.5, 0.2]),
                        (1.0, &[0.0, -0.5, -0.2]),
                    ],
                ),
            ],
        },
        ClipSpec {
            name: "Drift",
            channels: vec![channel(
                SPINE,
                "rotation",
                &[
                    (0.0, &[0.0, 0.0, 0.0, 1.0]),
                    (1.0, &[0.258819, 0.0, 0.0, 0.9659258]),
                ],
            )],
        },
    ]
}

/// Поза движения `clip` в долю `phase`, как её ставит стенд: движение играет один раз.
pub fn pose_at(skeleton: &Skeleton, clip: usize, phase: f32) -> PoseScratch {
    let mut scratch = PoseScratch::default();
    let time = phase * skeleton.clips[clip].length;
    skeleton.pose(
        &Blend::single(Playing {
            clip: Some(clip),
            time,
            looped: false,
        }),
        &mut scratch,
    );
    scratch
}

/// Где в осях модели начало кости `name` в позе `scratch`.
pub fn point(skeleton: &Skeleton, scratch: &PoseScratch, name: &str) -> [f32; 3] {
    let bone = skeleton
        .bone_named(name)
        .unwrap_or_else(|| panic!("нет кости {name}"));
    let matrix = scratch.globals()[bone];
    [matrix[3][0], matrix[3][1], matrix[3][2]]
}
