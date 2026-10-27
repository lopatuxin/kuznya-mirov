//! «Трёхмерная сцена» → «Модели и движения»: скелет модели и движения из её файла — кости с позой покоя, их
//! ключи во времени, поза между ключами, переход между движениями и ступни на земле. Всё в осях модели
//! (+x вправо, +y к игроку, +z вверх), середина основания в начале. Ни видеокарты, ни браузера: позы
//! считает процессор, сгибает сетку видеокарта → `render::gpu_models`.

use super::math3::{IDENTITY, Mat4, mul};
use super::surface;

type V3 = [f32; 3];
type Quat = [f32; 4];

/// Предел костей скелета: матрица кости — четыре точки текстуры данных, а в ширину текстура у запасного
/// WebGL2 не больше 2048 точек.
pub const MAX_JOINTS: usize = 512;

/// Сколько шагов длится плавный переход между движениями — около 0,2 с.
pub const FADE_STEPS: u32 = 12;

/// Сколько движений играет разом: идущее и затухающие; при переполнении самое лёгкое отбрасывается.
pub const MAX_LAYERS: usize = 4;

/// Сдвиг, поворот и масштаб кости относительно родителя.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trs {
    pub translation: V3,
    /// Кватернион `[x, y, z, w]`.
    pub rotation: Quat,
    pub scale: V3,
}

impl Trs {
    pub const REST: Trs = Trs {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
    };

    fn matrix(&self) -> Mat4 {
        let [x, y, z, w] = self.rotation;
        let [sx, sy, sz] = self.scale;
        [
            [
                (1.0 - 2.0 * (y * y + z * z)) * sx,
                2.0 * (x * y + z * w) * sx,
                2.0 * (x * z - y * w) * sx,
                0.0,
            ],
            [
                2.0 * (x * y - z * w) * sy,
                (1.0 - 2.0 * (x * x + z * z)) * sy,
                2.0 * (y * z + x * w) * sy,
                0.0,
            ],
            [
                2.0 * (x * z + y * w) * sz,
                2.0 * (y * z - x * w) * sz,
                (1.0 - 2.0 * (x * x + y * y)) * sz,
                0.0,
            ],
            [
                self.translation[0],
                self.translation[1],
                self.translation[2],
                1.0,
            ],
        ]
    }
}

/// Кость или узел иерархии файла; родитель всегда стоит в списке раньше потомка.
#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    pub rest: Trs,
}

/// Как значение меняется между ключами.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Step,
    /// Кубический сплайн glTF: на каждый ключ три значения — входная касательная, значение, выходная.
    Cubic,
}

/// Ключи одного свойства кости во времени. Сдвиг и масштаб — первые три числа значения, поворот — все четыре.
#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    pub times: Vec<f32>,
    pub values: Vec<[f32; 4]>,
    pub interpolation: Interpolation,
    pub rotation: bool,
}

impl Channel {
    fn value_at(&self, key: usize) -> [f32; 4] {
        match self.interpolation {
            Interpolation::Cubic => self.values[key * 3 + 1],
            _ => self.values[key],
        }
    }

    fn sample(&self, at: f32) -> Option<[f32; 4]> {
        let last = self.times.len().checked_sub(1)?;
        if last == 0 || at <= self.times[0] {
            return Some(self.value_at(0));
        }
        if at >= self.times[last] {
            return Some(self.value_at(last));
        }
        let key = self.times.partition_point(|&time| time <= at) - 1;
        let (from, to) = (self.times[key], self.times[key + 1]);
        let along = (at - from) / (to - from);
        Some(match self.interpolation {
            Interpolation::Step => self.value_at(key),
            Interpolation::Linear => self.mix(self.value_at(key), self.value_at(key + 1), along),
            Interpolation::Cubic => self.hermite(key, to - from, along),
        })
    }

    fn mix(&self, a: [f32; 4], b: [f32; 4], along: f32) -> [f32; 4] {
        if self.rotation {
            slerp(a, b, along)
        } else {
            std::array::from_fn(|k| a[k] + (b[k] - a[k]) * along)
        }
    }

    fn hermite(&self, key: usize, span: f32, s: f32) -> [f32; 4] {
        let (s2, s3) = (s * s, s * s * s);
        let [h00, h10, h01, h11] = [
            2.0 * s3 - 3.0 * s2 + 1.0,
            s3 - 2.0 * s2 + s,
            -2.0 * s3 + 3.0 * s2,
            s3 - s2,
        ];
        let from = &self.values[key * 3..key * 3 + 3];
        let to = &self.values[key * 3 + 3..key * 3 + 6];
        let value: [f32; 4] = std::array::from_fn(|k| {
            h00 * from[1][k] + h10 * span * from[2][k] + h01 * to[1][k] + h11 * span * to[0][k]
        });
        if self.rotation {
            normalized_quat(value)
        } else {
            value
        }
    }
}

/// Ключи одной кости движения.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub bone: usize,
    pub translation: Option<Channel>,
    pub rotation: Option<Channel>,
    pub scale: Option<Channel>,
}

/// Поправка кости «Движения» → «Движение с поправками»: повороты вокруг трёх осей кости в градусах `[x, y, z]`,
/// постоянные или по отметкам — доле движения от 0 до 1, по возрастанию.
#[derive(Debug, Clone, PartialEq)]
pub enum Turns {
    Constant(V3),
    Keyed(Vec<(f32, V3)>),
}

impl Turns {
    /// Поправка в доле `phase`. Между отметками — кубическая кривая Катмулла — Рома; у повторяющегося движения
    /// после последней отметки она идёт по кругу к первой, у остальных — держится крайняя.
    pub fn at(&self, phase: f32, cyclic: bool) -> V3 {
        let marks = match self {
            Turns::Constant(turn) => return *turn,
            Turns::Keyed(marks) => marks,
        };
        let last = marks.len() - 1;
        if last == 0 {
            return marks[0].1;
        }
        if !cyclic {
            if phase <= marks[0].0 {
                return marks[0].1;
            }
            if phase >= marks[last].0 {
                return marks[last].1;
            }
        }
        let count = marks.len() as isize;
        let pick = |index: isize| -> (f32, V3) {
            if cyclic {
                let (at, turn) = marks[index.rem_euclid(count) as usize];
                (at + index.div_euclid(count) as f32, turn)
            } else {
                marks[index.clamp(0, last as isize) as usize]
            }
        };
        let after = marks.partition_point(|&(at, _)| at <= phase) as isize;
        let (from_at, from) = pick(after - 1);
        let (to_at, to) = pick(after);
        let (_, before) = pick(after - 2);
        let (_, beyond) = pick(after + 1);
        let t = ((phase - from_at) / (to_at - from_at).max(1e-6)).clamp(0.0, 1.0);
        let (t2, t3) = (t * t, t * t * t);
        std::array::from_fn(|k| {
            0.5 * (2.0 * from[k]
                + (-before[k] + to[k]) * t
                + (2.0 * before[k] - 5.0 * from[k] + 4.0 * to[k] - beyond[k]) * t2
                + (-before[k] + 3.0 * from[k] - 3.0 * to[k] + beyond[k]) * t3)
        })
    }
}

/// Поправка одной кости движения.
#[derive(Debug, Clone, PartialEq)]
pub struct BoneTurn {
    pub bone: usize,
    pub turns: Turns,
}

/// Таз для `bounce`: кость, направление вертикали в осях её родителя и средняя высота кости за движение вдоль него.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pelvis {
    pub bone: usize,
    pub up: V3,
    pub mean_rise: f32,
}

/// Движение `motions.json`: движение файла с поправками. Длина такого `Clip` — длина движения файла, делённая на
/// `speed`.
#[derive(Debug, Clone, PartialEq)]
pub struct Retouch {
    pub base: usize,
    pub looped: Option<bool>,
    /// Движение, у которого берётся верх тела от кости `spine`.
    pub upper: Option<usize>,
    pub bounce: f32,
    pub pelvis: Option<Pelvis>,
    pub lean: f32,
    pub arms_out: f32,
    pub bones: Vec<BoneTurn>,
}

/// Движение файла: имя, начало первого ключа, длина и ключи костей; у движения `motions.json` — длина и поправки.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub name: String,
    pub start: f32,
    pub length: f32,
    pub tracks: Vec<Track>,
    pub retouch: Option<Retouch>,
}

impl Clip {
    /// Есть ли в движении что играть: ключи и ненулевая длина; иначе модель стоит в позе покоя.
    pub fn is_playable(&self) -> bool {
        self.length > 0.0 && (self.retouch.is_some() || !self.tracks.is_empty())
    }

    fn apply(&self, time: f32, locals: &mut [Trs]) {
        let at = self.start + time;
        for track in &self.tracks {
            let Some(local) = locals.get_mut(track.bone) else {
                continue;
            };
            if let Some([x, y, z, _]) = track.translation.as_ref().and_then(|c| c.sample(at)) {
                local.translation = [x, y, z];
            }
            if let Some(rotation) = track.rotation.as_ref().and_then(|c| c.sample(at)) {
                local.rotation = normalized_quat(rotation);
            }
            if let Some([x, y, z, _]) = track.scale.as_ref().and_then(|c| c.sample(at)) {
                local.scale = [x, y, z];
            }
        }
    }
}

/// Нога, которую ставят на землю: кости бедра, голени и ступни и таз над бедром.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leg {
    pub pelvis: usize,
    pub thigh: usize,
    pub shin: usize,
    pub foot: usize,
}

/// Скелет модели с движениями. `head` переводит кость из осей файла в оси модели (середина основания в
/// начале), `tails[j]` — обратная матрица связывания кости `j` вместе с обратным переводом осей.
#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
    pub joints: Vec<usize>,
    pub head: Mat4,
    pub tails: Vec<Mat4>,
    pub clips: Vec<Clip>,
    pub idle: Option<usize>,
    pub walk: Option<usize>,
    pub legs: Option<[Leg; 2]>,
    /// Кость, от которой начинается верх тела.
    pub spine: Option<usize>,
    /// Две кисти; плечо руки — их родительские кости.
    pub hands: Option<[usize; 2]>,
}

/// Какое движение и в какой его миг играет.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Playing {
    pub clip: Option<usize>,
    /// Секунды от начала движения.
    pub time: f32,
    pub looped: bool,
}

/// Движение в смеси и его доля в позе.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    pub playing: Playing,
    pub weight: f32,
}

/// Поза: идущее движение первым и затухающие за ним, не больше `MAX_LAYERS`; сумма долей — единица.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blend {
    layers: [Layer; MAX_LAYERS],
    len: usize,
}

impl Blend {
    pub const REST: Blend = Blend::single(Playing {
        clip: None,
        time: 0.0,
        looped: false,
    });

    /// Поза одного движения.
    pub const fn single(playing: Playing) -> Blend {
        let layer = Layer {
            playing,
            weight: 1.0,
        };
        Blend {
            layers: [layer; MAX_LAYERS],
            len: 1,
        }
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers[..self.len]
    }

    /// Идущее движение.
    pub fn current(&self) -> &Layer {
        &self.layers[0]
    }

    /// Смесь из `layers`, идущее движение первым; пустой список — поза покоя.
    pub(crate) fn from_layers(layers: impl IntoIterator<Item = Layer>) -> Blend {
        let mut blend = Blend {
            layers: Blend::REST.layers,
            len: 0,
        };
        for layer in layers.into_iter().take(MAX_LAYERS) {
            blend.layers[blend.len] = layer;
            blend.len += 1;
        }
        if blend.len == 0 { Blend::REST } else { blend }
    }
}

/// Где модель стоит в мире и на какой высоте её основание: по ним ступни находят землю под собой.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stance {
    /// Середина основания на земле.
    pub place: [f64; 2],
    pub cos: f64,
    pub sin: f64,
    pub scale: f64,
    pub base: f64,
}

impl Stance {
    fn world(&self, local: V3) -> [f64; 2] {
        let (x, y) = (
            f64::from(local[0]) * self.scale,
            f64::from(local[1]) * self.scale,
        );
        [
            self.place[0] + self.cos * x - self.sin * y,
            self.place[1] + self.sin * x + self.cos * y,
        ]
    }
}

/// Рабочие буферы поз: один набор на кадр, чтобы позы не выделяли память.
#[derive(Debug, Default)]
pub struct PoseScratch {
    locals: Vec<Trs>,
    other: Vec<Trs>,
    globals: Vec<Mat4>,
    subtree: Vec<bool>,
    upper: Vec<Trs>,
}

impl PoseScratch {
    /// Матрицы всех костей в осях модели после `Skeleton::pose`.
    pub fn globals(&self) -> &[Mat4] {
        &self.globals
    }
}

impl Skeleton {
    /// Сколько ключей в самом частом канале движения файла `clip`: кадров в нём на один больше, чем промежутков.
    pub fn key_count(&self, clip: usize) -> usize {
        self.clips.get(clip).map_or(0, |clip| {
            clip.tracks
                .iter()
                .flat_map(|track| [&track.translation, &track.rotation, &track.scale])
                .flatten()
                .map(|channel| channel.times.len())
                .max()
                .unwrap_or(0)
        })
    }

    /// Движение с именем `name`.
    pub fn clip_named(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|clip| clip.name == name)
    }

    /// Кость с именем `name` среди всех узлов иерархии файла.
    pub fn bone_named(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|bone| bone.name == name)
    }

    /// Таз для `bounce` движения `clip` файла: у модели со ступнями — кость таза над бёдрами. `None`, если движение не
    /// сдвигает таз.
    pub fn pelvis_rise(&self, clip: usize) -> Option<Pelvis> {
        const SAMPLES: usize = 64;
        let bone = self.legs?[0].pelvis;
        let channel = self.clips[clip]
            .tracks
            .iter()
            .find(|track| track.bone == bone)?
            .translation
            .as_ref()?;
        let rests: Vec<Trs> = self.bones.iter().map(|bone| bone.rest).collect();
        let parent = match self.bones[bone].parent {
            Some(parent) => self.global_of(parent, &rests),
            None => self.head,
        };
        let up = unit([parent[0][2], parent[1][2], parent[2][2]]);
        let clip = &self.clips[clip];
        let mean_rise = (0..SAMPLES)
            .filter_map(|step| {
                let at = clip.start + clip.length * step as f32 / SAMPLES as f32;
                let [x, y, z, _] = channel.sample(at)?;
                Some(dot([x, y, z], up))
            })
            .sum::<f32>()
            / SAMPLES as f32;
        Some(Pelvis {
            bone,
            up,
            mean_rise,
        })
    }

    /// Наибольшее расхождение конца движения с его началом по поворотам костей — градусы и кость; `cyclic` —
    /// движение повторяется, и отметки поправок идут по кругу. `None` у движения, которое нечем играть.
    pub fn ends_gap(&self, clip: usize, cyclic: bool) -> Option<(f32, usize)> {
        let clip = self.clips.get(clip).filter(|clip| clip.is_playable())?;
        let sample = |time: f32| {
            let mut locals: Vec<Trs> = self.bones.iter().map(|bone| bone.rest).collect();
            self.apply_clip(clip, time, cyclic, &mut locals, &mut Vec::new());
            locals
        };
        let (start, end) = (sample(0.0), sample(clip.length));
        start
            .iter()
            .zip(&end)
            .enumerate()
            .map(|(bone, (a, b))| {
                let cos: f32 = a.rotation.iter().zip(&b.rotation).map(|(x, y)| x * y).sum();
                (2.0 * cos.abs().min(1.0).acos().to_degrees(), bone)
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
    }

    /// Кость-сустав с именем `name`: номер в списке суставов, как в весах вершин.
    pub fn joint_named(&self, name: &str) -> Option<usize> {
        self.joints
            .iter()
            .position(|&bone| self.bones[bone].name == name)
    }

    /// Ноги по костям ступней: у каждой должны быть голень, бедро и таз над ним.
    pub fn leg_of(&self, joint: usize) -> Option<Leg> {
        let foot = self.joints[joint];
        let shin = self.bones[foot].parent?;
        let thigh = self.bones[shin].parent?;
        let pelvis = self.bones[thigh].parent?;
        Some(Leg {
            pelvis,
            thigh,
            shin,
            foot,
        })
    }

    /// Движение, его миг и повторяется ли оно: `loop` движения `motions.json` сильнее, чем то, как его позвали.
    fn clip_time(&self, playing: &Playing) -> Option<(&Clip, f32, bool)> {
        let clip = self.clips.get(playing.clip?)?;
        if !clip.is_playable() {
            return None;
        }
        let looped = clip
            .retouch
            .as_ref()
            .and_then(|retouch| retouch.looped)
            .unwrap_or(playing.looped);
        let time = if looped {
            playing.time.rem_euclid(clip.length)
        } else {
            playing.time.min(clip.length)
        };
        Some((clip, time, looped))
    }

    fn sample(&self, playing: &Playing, locals: &mut Vec<Trs>, upper: &mut Vec<Trs>) {
        locals.clear();
        locals.extend(self.bones.iter().map(|bone| bone.rest));
        if let Some((clip, time, looped)) = self.clip_time(playing) {
            self.apply_clip(clip, time, looped, locals, upper);
        }
    }

    fn apply_clip(
        &self,
        clip: &Clip,
        time: f32,
        looped: bool,
        locals: &mut [Trs],
        upper: &mut Vec<Trs>,
    ) {
        let Some(retouch) = &clip.retouch else {
            clip.apply(time, locals);
            return;
        };
        let phase = (time / clip.length).clamp(0.0, 1.0);
        self.apply_retouched(retouch, phase, looped, true, locals, upper);
    }

    /// Поза движения с поправками в доле `phase`: движение файла, верх тела из `upper` (если `with_upper`), подскок
    /// таза, повороты костей, наклон и разведённые руки.
    fn apply_retouched(
        &self,
        retouch: &Retouch,
        phase: f32,
        looped: bool,
        with_upper: bool,
        locals: &mut [Trs],
        upper: &mut Vec<Trs>,
    ) {
        let base = &self.clips[retouch.base];
        base.apply(phase * base.length, locals);
        if let (true, Some(clip), Some(spine)) = (with_upper, retouch.upper, self.spine) {
            let from = &self.clips[clip];
            upper.clear();
            upper.extend(self.bones.iter().map(|bone| bone.rest));
            match &from.retouch {
                Some(own) => {
                    let mut unused = Vec::new();
                    self.apply_retouched(own, phase, looped, false, upper, &mut unused);
                }
                None => from.apply(phase * from.length, upper),
            }
            for bone in 0..locals.len() {
                if self.is_under(bone, spine) {
                    locals[bone] = upper[bone];
                }
            }
        }
        if let Some(pelvis) = retouch.pelvis.filter(|_| retouch.bounce != 1.0) {
            let local = &mut locals[pelvis.bone];
            let rise = dot(local.translation, pelvis.up) - pelvis.mean_rise;
            local.translation = sub(
                local.translation,
                scaled(pelvis.up, rise * (1.0 - retouch.bounce)),
            );
        }
        for turn in &retouch.bones {
            let angles = turn.turns.at(phase, looped);
            let local = &mut locals[turn.bone];
            local.rotation = quat_mul(local.rotation, euler_quat(angles));
        }
        if retouch.lean != 0.0
            && let Some(spine) = self.spine
        {
            self.turn_about_model_axis(spine, [1.0, 0.0, 0.0], -retouch.lean, locals);
        }
        if retouch.arms_out != 0.0
            && let Some(hands) = self.hands
        {
            for hand in hands {
                let Some(arm) = self.bones[hand]
                    .parent
                    .and_then(|elbow| self.bones[elbow].parent)
                else {
                    continue;
                };
                let side = if point_of(&self.global_of(arm, locals))[0] < 0.0 {
                    1.0
                } else {
                    -1.0
                };
                self.turn_about_model_axis(arm, [0.0, 1.0, 0.0], side * retouch.arms_out, locals);
            }
        }
    }

    /// Кость `bone` — сама `root` или её потомок.
    fn is_under(&self, bone: usize, root: usize) -> bool {
        let mut at = Some(bone);
        while let Some(current) = at {
            if current == root {
                return true;
            }
            at = self.bones[current].parent;
        }
        false
    }

    /// Матрица кости `bone` в осях модели по локальным позам `locals`.
    pub fn global_of(&self, bone: usize, locals: &[Trs]) -> Mat4 {
        let parent = match self.bones[bone].parent {
            Some(parent) => self.global_of(parent, locals),
            None => self.head,
        };
        mul(&parent, &locals[bone].matrix())
    }

    /// Поворачивает кость `bone` вместе с потомками на `degrees` вокруг оси модели `axis` через её начало: поворот
    /// пересчитывается в оси родителя кости. Оси модели — зеркало осей файла, и угол в них считается в другую
    /// сторону.
    fn turn_about_model_axis(&self, bone: usize, axis: V3, degrees: f32, locals: &mut [Trs]) {
        let parent = match self.bones[bone].parent {
            Some(parent) => self.global_of(parent, locals),
            None => self.head,
        };
        let column = |c: usize| [parent[c][0], parent[c][1], parent[c][2]];
        let local_axis = unit([
            dot(column(0), axis),
            dot(column(1), axis),
            dot(column(2), axis),
        ]);
        let mirrored = dot(column(0), cross(column(1), column(2))) < 0.0;
        let angle = if mirrored { -degrees } else { degrees };
        let turn = axis_quat(local_axis, angle.to_radians());
        let local = &mut locals[bone];
        local.rotation = quat_mul(turn, local.rotation);
    }

    /// Матрицы всех костей в осях модели для позы `blend`: в `scratch`, годные для `plant_feet` и
    /// `skin_matrices`.
    pub fn pose(&self, blend: &Blend, scratch: &mut PoseScratch) {
        let current = blend.current();
        self.sample(&current.playing, &mut scratch.locals, &mut scratch.upper);
        let mut total = current.weight;
        for layer in &blend.layers()[1..] {
            total += layer.weight;
            if layer.weight <= 0.0 {
                continue;
            }
            self.sample(&layer.playing, &mut scratch.other, &mut scratch.upper);
            let share = layer.weight / total;
            for (local, next) in scratch.locals.iter_mut().zip(&scratch.other) {
                *local = mix_trs(local, next, share);
            }
        }
        scratch.globals.clear();
        for (bone, local) in self.bones.iter().zip(&scratch.locals) {
            let parent = bone
                .parent
                .map_or(self.head, |parent| scratch.globals[parent]);
            scratch.globals.push(mul(&parent, &local.matrix()));
        }
    }

    /// Матрицы суставов для сгиба сетки: вершина связывания, умноженная на матрицу своего сустава, ложится в
    /// позу.
    pub fn skin_matrices(&self, scratch: &PoseScratch, out: &mut Vec<Mat4>) {
        out.clear();
        out.extend(
            self.joints
                .iter()
                .zip(&self.tails)
                .map(|(&bone, tail)| mul(&scratch.globals[bone], tail)),
        );
    }

    /// Матрицы суставов в позе покоя файла.
    pub fn rest_matrices(&self) -> Vec<Mat4> {
        let mut scratch = PoseScratch::default();
        let mut out = Vec::new();
        self.pose(&Blend::REST, &mut scratch);
        self.skin_matrices(&scratch, &mut out);
        out
    }

    /// Ставит ступни на землю: `ground` отдаёт высоту земли под точкой мира или `None` за краем сцены. Таз
    /// уходит на меньшую из разниц земли под ступнями и основанием, а нога поднятой ступни сгибается в
    /// колене вперёд. Ступня без опоры — за краем сцены или с перепадом больше ступеньки — остаётся как в
    /// движении и в сдвиге таза не участвует.
    pub fn plant_feet(
        &self,
        stance: &Stance,
        mut ground: impl FnMut([f64; 2]) -> Option<f64>,
        scratch: &mut PoseScratch,
    ) {
        let Some(legs) = &self.legs else {
            return;
        };
        let scale = stance.scale.max(f64::EPSILON);
        let supports: [Option<f32>; 2] = legs.map(|leg| {
            let foot = point_of(&scratch.globals[leg.foot]);
            let rise = ground(stance.world(foot))? - stance.base;
            (rise.abs() <= surface::STEP + surface::TOLERANCE).then(|| (rise / scale) as f32)
        });
        let pelvis_lift = supports
            .into_iter()
            .flatten()
            .reduce(f32::min)
            .unwrap_or(0.0);
        let lifts = supports.map(|support| support.unwrap_or(0.0));
        let ankles = legs.map(|leg| point_of(&scratch.globals[leg.foot]));
        if pelvis_lift != 0.0 {
            self.mark_subtree(legs[0].pelvis, &mut scratch.subtree);
            for (matrix, _) in scratch
                .globals
                .iter_mut()
                .zip(&scratch.subtree)
                .filter(|(_, inside)| **inside)
            {
                matrix[3][2] += pelvis_lift;
            }
        }
        for ((leg, lift), ankle) in legs.iter().zip(lifts).zip(ankles) {
            if (lift - pelvis_lift).abs() > 1e-5 {
                let target = [ankle[0], ankle[1], ankle[2] + lift];
                self.bend_leg(leg, target, scratch);
            }
        }
    }

    fn mark_subtree(&self, root: usize, flags: &mut Vec<bool>) {
        flags.clear();
        for (at, bone) in self.bones.iter().enumerate() {
            let inside = at == root || bone.parent.is_some_and(|parent| flags[parent]);
            flags.push(inside);
        }
    }

    fn bend_leg(&self, leg: &Leg, target: V3, scratch: &mut PoseScratch) {
        let [hip, knee, ankle] =
            [leg.thigh, leg.shin, leg.foot].map(|b| point_of(&scratch.globals[b]));
        let (upper, lower) = (length(sub(knee, hip)), length(sub(ankle, knee)));
        let to_target = sub(target, hip);
        let reach = length(to_target);
        if upper < 1e-6 || lower < 1e-6 || reach < 1e-6 {
            return;
        }
        let reach = reach.clamp((upper - lower).abs() + 1e-3, upper + lower - 1e-4);
        let axis = unit(to_target);
        let along = (upper * upper - lower * lower + reach * reach) / (2.0 * reach);
        let rise = (upper * upper - along * along).max(0.0).sqrt();
        let forward = [0.0, 1.0, 0.0];
        let pole = sub(forward, scaled(axis, dot(forward, axis)));
        let pole = if length(pole) < 1e-4 {
            sub(knee, scaled(axis, dot(sub(knee, hip), axis)))
        } else {
            pole
        };
        let pole = unit(pole);
        let new_knee = add(add(hip, scaled(axis, along)), scaled(pole, rise));
        self.turn_subtree(
            leg.thigh,
            hip,
            turn_between(sub(knee, hip), sub(new_knee, hip)),
            scratch,
        );
        let knee = point_of(&scratch.globals[leg.shin]);
        let ankle = point_of(&scratch.globals[leg.foot]);
        let aimed = add(hip, scaled(axis, reach));
        self.turn_subtree(
            leg.shin,
            knee,
            turn_between(sub(ankle, knee), sub(aimed, knee)),
            scratch,
        );
    }

    fn turn_subtree(&self, root: usize, pivot: V3, turn: [V3; 3], scratch: &mut PoseScratch) {
        self.mark_subtree(root, &mut scratch.subtree);
        for (matrix, _) in scratch
            .globals
            .iter_mut()
            .zip(&scratch.subtree)
            .filter(|(_, inside)| **inside)
        {
            *matrix = turned_about(matrix, pivot, &turn);
        }
    }
}

fn point_of(matrix: &Mat4) -> V3 {
    [matrix[3][0], matrix[3][1], matrix[3][2]]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scaled(a: V3, k: f32) -> V3 {
    a.map(|c| c * k)
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: V3) -> f32 {
    dot(a, a).sqrt()
}

fn unit(a: V3) -> V3 {
    scaled(a, 1.0 / length(a).max(1e-6))
}

/// Поворот в виде столбцов, переводящий направление `from` в направление `to`.
fn turn_between(from: V3, to: V3) -> [V3; 3] {
    let (from, to) = (unit(from), unit(to));
    let c = dot(from, to);
    if c < -0.9999 {
        let helper = if from[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let [x, y, z] = unit(cross(from, helper));
        return [
            [2.0 * x * x - 1.0, 2.0 * x * y, 2.0 * x * z],
            [2.0 * x * y, 2.0 * y * y - 1.0, 2.0 * y * z],
            [2.0 * x * z, 2.0 * y * z, 2.0 * z * z - 1.0],
        ];
    }
    let [x, y, z] = cross(from, to);
    let k = 1.0 / (1.0 + c);
    [
        [1.0 - k * (y * y + z * z), z + k * x * y, -y + k * x * z],
        [-z + k * x * y, 1.0 - k * (x * x + z * z), x + k * y * z],
        [y + k * x * z, -x + k * y * z, 1.0 - k * (x * x + y * y)],
    ]
}

/// `turn` вокруг `pivot`, приложенный к матрице `m`.
fn turned_about(m: &Mat4, pivot: V3, turn: &[V3; 3]) -> Mat4 {
    let apply = |v: V3| -> V3 {
        std::array::from_fn(|row| turn[0][row] * v[0] + turn[1][row] * v[1] + turn[2][row] * v[2])
    };
    let mut out = *m;
    for column in 0..3 {
        let axis = apply([m[column][0], m[column][1], m[column][2]]);
        out[column] = [axis[0], axis[1], axis[2], 0.0];
    }
    let moved = add(pivot, apply(sub(point_of(m), pivot)));
    out[3] = [moved[0], moved[1], moved[2], 1.0];
    out
}

fn quat_mul(a: Quat, b: Quat) -> Quat {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    normalized_quat([
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ])
}

fn axis_quat(axis: V3, radians: f32) -> Quat {
    let (sin, cos) = (radians / 2.0).sin_cos();
    [axis[0] * sin, axis[1] * sin, axis[2] * sin, cos]
}

/// Повороты вокруг осей `x`, `y`, `z` кости в градусах: сначала вокруг `x`, потом вокруг `y`, потом вокруг `z`,
/// как в Blender с порядком XYZ.
pub fn euler_quat(degrees: V3) -> Quat {
    let [x, y, z] = degrees.map(f32::to_radians);
    let qx = axis_quat([1.0, 0.0, 0.0], x);
    let qy = axis_quat([0.0, 1.0, 0.0], y);
    let qz = axis_quat([0.0, 0.0, 1.0], z);
    quat_mul(qz, quat_mul(qy, qx))
}

/// Поправка кости `turn` после ещё одного поворота на `degrees` вокруг её оси `axis` (0 — x, 1 — y, 2 — z): тройка
/// того же порядка XYZ, что и `euler_quat`, с `y` в пределах ±90°.
pub fn turn_after(turn: V3, axis: usize, degrees: f32) -> V3 {
    let mut about = [0.0; 3];
    about[axis % 3] = 1.0;
    let [x, y, z, w] = quat_mul(euler_quat(turn), axis_quat(about, degrees.to_radians()));
    let m20 = 2.0 * (x * z - y * w);
    if m20.abs() > 0.99999 {
        let (m11, m12) = (1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w));
        return [
            (-m12).atan2(m11).to_degrees(),
            (-m20.signum()).asin().to_degrees(),
            0.0,
        ];
    }
    let (m21, m22) = (2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y));
    let (m10, m00) = (2.0 * (x * y + z * w), 1.0 - 2.0 * (y * y + z * z));
    [
        m21.atan2(m22).to_degrees(),
        (-m20).asin().to_degrees(),
        m10.atan2(m00).to_degrees(),
    ]
}

fn normalized_quat(q: Quat) -> Quat {
    let length = q.iter().map(|c| c * c).sum::<f32>().sqrt();
    if length < 1e-12 {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        q.map(|c| c / length)
    }
}

fn slerp(a: Quat, b: Quat, along: f32) -> Quat {
    let mut cos: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
    let b = if cos < 0.0 {
        cos = -cos;
        b.map(|c| -c)
    } else {
        b
    };
    let (weight_a, weight_b) = if cos > 0.9995 {
        (1.0 - along, along)
    } else {
        let angle = cos.acos();
        let sin = angle.sin();
        (
            ((1.0 - along) * angle).sin() / sin,
            (along * angle).sin() / sin,
        )
    };
    normalized_quat(std::array::from_fn(|k| a[k] * weight_a + b[k] * weight_b))
}

fn mix_trs(before: &Trs, after: &Trs, share: f32) -> Trs {
    let lerp = |a: V3, b: V3| -> V3 { std::array::from_fn(|k| a[k] + (b[k] - a[k]) * share) };
    Trs {
        translation: lerp(before.translation, after.translation),
        rotation: slerp(before.rotation, after.rotation, share),
        scale: lerp(before.scale, after.scale),
    }
}

/// Матрица сдвига на `by`.
pub fn translation(by: V3) -> Mat4 {
    let mut m = IDENTITY;
    m[3] = [by[0], by[1], by[2], 1.0];
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(times: &[f32], values: &[[f32; 4]], interpolation: Interpolation) -> Channel {
        Channel {
            times: times.to_vec(),
            values: values.to_vec(),
            interpolation,
            rotation: false,
        }
    }

    #[test]
    fn a_value_between_keys_is_linear_and_outside_them_holds_the_nearest_key() {
        let moves = channel(
            &[1.0, 2.0],
            &[[0.0, 0.0, 0.0, 0.0], [10.0, 20.0, 30.0, 0.0]],
            Interpolation::Linear,
        );
        assert_eq!(moves.sample(1.5), Some([5.0, 10.0, 15.0, 0.0]));
        assert_eq!(moves.sample(0.0), Some([0.0; 4]));
        assert_eq!(moves.sample(9.0), Some([10.0, 20.0, 30.0, 0.0]));
    }

    #[test]
    fn a_step_channel_holds_the_key_until_the_next_one() {
        let jumps = channel(
            &[0.0, 1.0, 2.0],
            &[[1.0; 4], [2.0; 4], [3.0; 4]],
            Interpolation::Step,
        );
        assert_eq!(jumps.sample(0.99), Some([1.0; 4]));
        assert_eq!(jumps.sample(1.0), Some([2.0; 4]));
        assert_eq!(jumps.sample(1.99), Some([2.0; 4]));
    }

    #[test]
    fn a_cubic_channel_passes_through_its_keys_and_follows_the_tangents() {
        let curve = channel(
            &[0.0, 1.0],
            &[
                [0.0; 4],
                [0.0; 4],
                [1.0, 0.0, 0.0, 0.0],
                [0.0; 4],
                [10.0, 0.0, 0.0, 0.0],
                [0.0; 4],
            ],
            Interpolation::Cubic,
        );
        assert_eq!(curve.sample(0.0), Some([0.0; 4]));
        assert_eq!(curve.sample(1.0), Some([10.0, 0.0, 0.0, 0.0]));
        let middle = curve.sample(0.5).unwrap()[0];
        assert!((middle - 5.125).abs() < 1e-5, "{middle}");
    }

    #[test]
    fn a_rotation_between_keys_takes_the_short_arc_and_stays_a_unit() {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let turn = Channel {
            times: vec![0.0, 1.0],
            values: vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, -half, -half]],
            interpolation: Interpolation::Linear,
            rotation: true,
        };
        let [x, y, z, w] = turn.sample(0.5).unwrap();
        assert!((x * x + y * y + z * z + w * w - 1.0).abs() < 1e-5);
        assert!(w > 0.0 && z > 0.0, "{z} {w}");
    }

    fn keyed(marks: &[(f32, f32)]) -> Turns {
        Turns::Keyed(marks.iter().map(|&(at, z)| (at, [0.0, 0.0, z])).collect())
    }

    #[test]
    fn turns_pass_through_their_marks_and_hold_the_outer_ones_when_the_motion_plays_once() {
        let turns = keyed(&[(0.25, 10.0), (0.5, 40.0), (0.75, 20.0)]);
        for (at, z) in [(0.25, 10.0), (0.5, 40.0), (0.75, 20.0)] {
            assert!((turns.at(at, false)[2] - z).abs() < 1e-4, "{at}");
            assert!((turns.at(at, true)[2] - z).abs() < 1e-4, "{at} по кругу");
        }
        assert_eq!(turns.at(0.0, false)[2], 10.0);
        assert_eq!(turns.at(1.0, false)[2], 20.0);
        assert_eq!(
            Turns::Constant([1.0, 2.0, 3.0]).at(0.7, true),
            [1.0, 2.0, 3.0]
        );
        assert_eq!(keyed(&[(0.4, 7.0)]).at(0.9, true)[2], 7.0);
    }

    #[test]
    fn turns_of_a_repeating_motion_go_round_from_the_last_mark_to_the_first() {
        let turns = keyed(&[(0.25, 40.0), (0.75, -40.0)]);
        let gap = |phase: f32| turns.at(phase, true)[2];
        assert!(
            (gap(0.0) - gap(1.0)).abs() < 1e-4,
            "конец круга сходится с началом"
        );
        assert!(
            gap(0.0).abs() < 1e-4,
            "зеркальные отметки — посреди пути ноль"
        );
        assert!(gap(0.1) > 0.0 && gap(0.1) < 40.0 && (gap(0.1) + gap(0.9)).abs() < 1e-4);
        assert!(
            turns.at(0.0, false)[2] > 39.9,
            "без круга до первой отметки держится она"
        );
    }

    #[test]
    fn a_turn_after_a_ring_drag_composes_in_the_bone_axes_and_keeps_the_euler_triple_readable() {
        assert_eq!(
            turn_after([0.0; 3], 2, 30.0).map(f32::round),
            [0.0, 0.0, 30.0]
        );
        let again = turn_after([0.0, 0.0, 30.0], 2, 15.0);
        assert!((again[2] - 45.0).abs() < 1e-3, "{again:?}");
        let around = turn_after([10.0, 20.0, 30.0], 0, 25.0);
        assert!((around[0] - 35.0).abs() < 1e-3 && (around[1] - 20.0).abs() < 1e-3);
        let (a, b) = (
            euler_quat([30.0, -40.0, 50.0]),
            euler_quat(turn_after([30.0, -40.0, 50.0], 1, 0.0)),
        );
        let dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert!(dot.abs() > 0.99999, "нулевой поворот ничего не меняет");
        let past_up = turn_after([0.0, 80.0, 0.0], 1, 20.0);
        let (a, b) = (euler_quat(past_up), euler_quat([0.0, 100.0, 0.0]));
        let dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert!(
            dot.abs() > 0.99999 && past_up[1].abs() <= 90.0,
            "{past_up:?}"
        );
    }

    #[test]
    fn the_euler_order_turns_around_x_first_then_y_then_z_all_in_the_bone_axes() {
        let rotate = |q: Quat, v: V3| -> V3 {
            let m = Trs {
                rotation: q,
                ..Trs::REST
            }
            .matrix();
            [
                m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2],
                m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2],
                m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2],
            ]
        };
        let turned = rotate(euler_quat([90.0, 0.0, 90.0]), [0.0, 1.0, 0.0]);
        for (got, want) in turned.iter().zip([0.0, 0.0, 1.0]) {
            assert!((got - want).abs() < 1e-5, "{turned:?}");
        }
    }

    fn leg_skeleton() -> Skeleton {
        let bone = |name: &str, parent: Option<usize>, at: V3| Bone {
            name: name.to_string(),
            parent,
            rest: Trs {
                translation: at,
                ..Trs::REST
            },
        };
        let bones = vec![
            bone("pelvis", None, [0.0, 0.0, 1.0]),
            bone("left_thigh", Some(0), [-0.1, 0.0, 0.0]),
            bone("left_shin", Some(1), [0.0, 0.0, -0.5]),
            bone("left_foot", Some(2), [0.0, 0.0, -0.5]),
            bone("right_thigh", Some(0), [0.1, 0.0, 0.0]),
            bone("right_shin", Some(4), [0.0, 0.0, -0.5]),
            bone("right_foot", Some(5), [0.0, 0.0, -0.5]),
        ];
        let joints: Vec<usize> = (0..bones.len()).collect();
        let tails = vec![IDENTITY; bones.len()];
        let mut skeleton = Skeleton {
            bones,
            joints,
            head: IDENTITY,
            tails,
            clips: Vec::new(),
            idle: None,
            walk: None,
            legs: None,
            spine: None,
            hands: None,
        };
        skeleton.legs = Some([skeleton.leg_of(3).unwrap(), skeleton.leg_of(6).unwrap()]);
        skeleton
    }

    fn stance() -> Stance {
        Stance {
            place: [10.0, 10.0],
            cos: 1.0,
            sin: 0.0,
            scale: 1.0,
            base: 0.0,
        }
    }

    fn posed(skeleton: &Skeleton, ground: impl FnMut([f64; 2]) -> Option<f64>) -> PoseScratch {
        let mut scratch = PoseScratch::default();
        skeleton.pose(&Blend::REST, &mut scratch);
        skeleton.plant_feet(&stance(), ground, &mut scratch);
        scratch
    }

    #[test]
    fn on_level_ground_the_feet_stay_where_the_pose_put_them() {
        let skeleton = leg_skeleton();
        let scratch = posed(&skeleton, |_| Some(0.0));
        assert_eq!(point_of(&scratch.globals[3]), [-0.1, 0.0, 0.0]);
        assert_eq!(point_of(&scratch.globals[6]), [0.1, 0.0, 0.0]);
    }

    #[test]
    fn a_foot_over_a_step_rises_onto_it_and_its_knee_bends_forward() {
        let skeleton = leg_skeleton();
        let scratch = posed(&skeleton, |at| Some(if at[0] < 10.0 { 0.3 } else { 0.0 }));
        let [pelvis, knee, ankle] = [0, 2, 3].map(|b| point_of(&scratch.globals[b]));
        assert_eq!(
            pelvis[2], 1.0,
            "таз остался на месте: правая ступня на основании"
        );
        assert!((ankle[2] - 0.3).abs() < 1e-3, "{ankle:?}");
        assert!(knee[1] > 0.05, "колено ушло вперёд: {knee:?}");
        assert_eq!(point_of(&scratch.globals[6]), [0.1, 0.0, 0.0]);
    }

    #[test]
    fn when_both_feet_are_lower_the_pelvis_sinks_by_the_larger_drop() {
        let skeleton = leg_skeleton();
        let scratch = posed(&skeleton, |at| Some(if at[0] < 10.0 { -0.1 } else { -0.3 }));
        assert!((point_of(&scratch.globals[0])[2] - 0.7).abs() < 1e-5);
        let left = point_of(&scratch.globals[3]);
        assert!((left[2] + 0.1).abs() < 1e-3, "{left:?}");
    }

    #[test]
    fn a_foot_with_no_ground_under_it_stays_as_the_motion_left_it() {
        let skeleton = leg_skeleton();
        let scratch = posed(&skeleton, |_| None);
        assert_eq!(point_of(&scratch.globals[3]), [-0.1, 0.0, 0.0]);
    }

    #[test]
    fn a_drop_past_a_step_under_one_foot_is_no_support_for_it_and_does_not_sink_the_pelvis() {
        let skeleton = leg_skeleton();
        let scratch = posed(&skeleton, |at| Some(if at[0] < 10.0 { -3.0 } else { 0.0 }));
        assert_eq!(
            point_of(&scratch.globals[0])[2],
            1.0,
            "таз остался на опоре правой ступни"
        );
        assert_eq!(point_of(&scratch.globals[3]), [-0.1, 0.0, 0.0]);
        let scratch = posed(&skeleton, |_| Some(5.0));
        assert_eq!(
            point_of(&scratch.globals[0])[2],
            1.0,
            "обе ступни без опоры: таз там, где его поставило движение"
        );
    }

    #[test]
    fn a_blend_halfway_between_two_clips_lies_between_their_poses() {
        let mut skeleton = leg_skeleton();
        let pelvis_at = |z: f32| Clip {
            name: format!("at{z}"),
            start: 0.0,
            length: 1.0,
            tracks: vec![Track {
                bone: 0,
                translation: Some(channel(
                    &[0.0, 1.0],
                    &[[0.0, 0.0, z, 0.0], [0.0, 0.0, z, 0.0]],
                    Interpolation::Linear,
                )),
                rotation: None,
                scale: None,
            }],
            retouch: None,
        };
        skeleton.clips = vec![pelvis_at(1.0), pelvis_at(2.0)];
        let playing = |clip| Playing {
            clip: Some(clip),
            time: 0.0,
            looped: true,
        };
        let blend = Blend::from_layers([
            Layer {
                playing: playing(1),
                weight: 0.25,
            },
            Layer {
                playing: playing(0),
                weight: 0.75,
            },
        ]);
        let mut scratch = PoseScratch::default();
        skeleton.pose(&blend, &mut scratch);
        assert!((point_of(&scratch.globals[0])[2] - 1.25).abs() < 1e-5);
    }

    #[test]
    fn a_blend_of_three_clips_weighs_each_by_its_share() {
        let mut skeleton = leg_skeleton();
        let pelvis_at = |z: f32| Clip {
            name: format!("at{z}"),
            start: 0.0,
            length: 1.0,
            tracks: vec![Track {
                bone: 0,
                translation: Some(channel(
                    &[0.0, 1.0],
                    &[[0.0, 0.0, z, 0.0], [0.0, 0.0, z, 0.0]],
                    Interpolation::Linear,
                )),
                rotation: None,
                scale: None,
            }],
            retouch: None,
        };
        skeleton.clips = vec![pelvis_at(1.0), pelvis_at(2.0), pelvis_at(4.0)];
        let layer = |clip, weight| Layer {
            playing: Playing {
                clip: Some(clip),
                time: 0.0,
                looped: true,
            },
            weight,
        };
        let blend = Blend::from_layers([layer(2, 0.5), layer(1, 0.25), layer(0, 0.25)]);
        let mut scratch = PoseScratch::default();
        skeleton.pose(&blend, &mut scratch);
        assert!((point_of(&scratch.globals[0])[2] - 2.75).abs() < 1e-5);
    }

    #[test]
    fn a_clip_without_keys_or_length_leaves_the_rest_pose() {
        let mut skeleton = leg_skeleton();
        skeleton.clips = vec![Clip {
            name: "empty".to_string(),
            start: 0.0,
            length: 0.0,
            tracks: Vec::new(),
            retouch: None,
        }];
        let blend = Blend::single(Playing {
            clip: Some(0),
            time: 3.0,
            looped: true,
        });
        let mut scratch = PoseScratch::default();
        skeleton.pose(&blend, &mut scratch);
        assert_eq!(point_of(&scratch.globals[0]), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn a_clip_that_plays_once_freezes_on_its_last_pose_and_a_looped_one_wraps() {
        let mut skeleton = leg_skeleton();
        skeleton.clips = vec![Clip {
            name: "lift".to_string(),
            start: 0.0,
            length: 1.0,
            tracks: vec![Track {
                bone: 0,
                translation: Some(channel(
                    &[0.0, 1.0],
                    &[[0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 4.0, 0.0]],
                    Interpolation::Linear,
                )),
                rotation: None,
                scale: None,
            }],
            retouch: None,
        }];
        let height_at = |time: f32, looped: bool| {
            let blend = Blend::single(Playing {
                clip: Some(0),
                time,
                looped,
            });
            let mut scratch = PoseScratch::default();
            skeleton.pose(&blend, &mut scratch);
            point_of(&scratch.globals[0])[2]
        };
        assert_eq!(height_at(5.0, false), 4.0);
        assert!((height_at(5.5, true) - 2.0).abs() < 1e-5);
    }
}
