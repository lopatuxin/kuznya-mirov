//! «Ветер и частицы» → «Облака», «Время и случайность»: облака неба — раскладка по высоте с глубиной,
//! снос ровным ветром, кольцо вокруг камеры и перестановка ушедших за его край, проступание и таяние,
//! первая сборка мира и своя случайность. Ни видеокарты, ни браузера: `Motion` ведёт часы и зовёт
//! [`Clouds::update`] раз в кадр, `atlas` берёт отсюда рисунки.

use crate::core::rng::Rng;
use crate::core::scene::LayerView;
use crate::core::value::{ImageId, Vec2};

use super::particles::Rect;

const STEPS_PER_SECOND: f64 = 60.0;
const STEP_SECONDS: f64 = 1.0 / STEPS_PER_SECOND;
/// Начальное число счётчика случайности облаков, одно и то же в каждом движке.
const SEED: u64 = 0xC10D_5CA9_E5EE_D001;

// Числа облаков начальные: их подбирают глазами на картинках облаков.

/// Середина облака по высоте — в полосе неба от этой доли его высоты до этой.
const BAND: (f64, f64) = (0.27, 0.4);
/// `parallax` облака — `parallax` неба плюс это, умноженное на близость.
const PARALLAX_GAIN: f64 = 0.12;
/// Размер облака — основной × (первое + второе × близость) × поправка.
const SIZE_BY_NEARNESS: (f64, f64) = (0.5, 0.5);
/// Поправка размера облака наугад — от и до.
const SIZE_FIX: (f64, f64) = (0.85, 1.15);
/// Просвечивание облака — первое плюс второе × близость.
const OPACITY_BY_NEARNESS: (f64, f64) = (0.55, 0.45);
/// Скорость облака, клеток в секунду, — `max(|ветер x| × первое, второе)` × (третье + четвёртое × близость).
const WIND_SHARE: f64 = 0.1;
const WIND_FLOOR: f64 = 0.03;
const SPEED_BY_NEARNESS: (f64, f64) = (0.3, 0.7);
/// Кольцо облаков — столько ширин окна плюс столько ширин самого широкого облака.
const RING_WINDOWS: f64 = 3.0;
const RING_WIDEST: f64 = 2.0;
/// Облаков на небе при `clouds` 1.
const CLOUDS_AT_FULL: f64 = 24.0;
/// Из стольких случайных мест в кольце новое облако берёт самое свободное.
const PLACE_CANDIDATES: usize = 3;
/// Секунд, за которые облако проступает или тает.
const FADE_SECONDS: f64 = 4.0;

/// Небо на этот кадр: объект с `position`, `size`, `repeat_x` и свойствами облаков.
#[derive(Debug, Clone, Copy)]
pub struct Sky<'a> {
    pub id: u32,
    pub generation: u32,
    pub rect: Rect,
    pub layer: i32,
    pub parallax: f64,
    /// `clouds`, от 0 до 1.
    pub clouds: f64,
    pub images: &'a [ImageId],
}

/// Рисунок одного облака: середина без сдвига слоя глубины, размер в клетках, отражение, просвечивание
/// с проступанием и таянием и `parallax` облака.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub image: ImageId,
    pub mirrored: bool,
    pub center: Vec2,
    pub size: Vec2,
    pub opacity: f64,
    pub parallax: f64,
}

#[derive(Debug, Clone, Copy)]
struct Cloud {
    image: ImageId,
    mirrored: bool,
    /// Близость: 1 у верха полосы, 0 у низа.
    near: f64,
    x: f64,
    y: f64,
    size: Vec2,
    parallax: f64,
    /// Доля силы, от 0 до 1: проступание растит её, таяние сносит.
    fade: f64,
    leaving: bool,
}

impl Cloud {
    fn sprite(&self) -> Sprite {
        Sprite {
            image: self.image,
            mirrored: self.mirrored,
            center: [self.x, self.y],
            size: self.size,
            opacity: (OPACITY_BY_NEARNESS.0 + OPACITY_BY_NEARNESS.1 * self.near) * self.fade,
            parallax: self.parallax,
        }
    }

    /// Один шаг по 1/60 секунды: снос ветром и проступание или таяние. `false`, когда растаяло.
    fn step(&mut self, wind: f64) -> bool {
        let direction = if wind < 0.0 { -1.0 } else { 1.0 };
        let speed = (wind.abs() * WIND_SHARE).max(WIND_FLOOR)
            * (SPEED_BY_NEARNESS.0 + SPEED_BY_NEARNESS.1 * self.near);
        self.x += direction * speed * STEP_SECONDS;
        let fade_step = STEP_SECONDS / FADE_SECONDS;
        if self.leaving {
            self.fade -= fade_step;
            self.fade > 0.0
        } else {
            self.fade = (self.fade + fade_step).min(1.0);
            true
        }
    }
}

/// Что облако берёт наугад, в одном и том же порядке всегда — счётчик случайности не зависит от данных.
struct Rolls {
    image: f64,
    mirror: f64,
    height: f64,
    fix: f64,
    places: [f64; PLACE_CANDIDATES],
}

impl Rolls {
    fn draw(rng: &mut Rng) -> Rolls {
        Rolls {
            image: rng.next_unit(),
            mirror: rng.next_unit(),
            height: rng.next_unit(),
            fix: rng.next_unit(),
            places: std::array::from_fn(|_| rng.next_unit()),
        }
    }
}

/// Кольцо облаков неба на этот кадр: его середина для облака с `parallax` `q` — место, что рисуется в
/// середине окна.
#[derive(Debug, Clone, Copy)]
struct Ring {
    scene_middle: f64,
    shift: f64,
    length: f64,
}

impl Ring {
    fn middle(&self, parallax: f64) -> f64 {
        self.scene_middle + self.shift * parallax
    }

    fn offset(&self, cloud: &Cloud) -> f64 {
        cloud.x - self.middle(cloud.parallax)
    }
}

#[derive(Debug)]
struct SkyState {
    id: u32,
    generation: u32,
    layer: i32,
    parallax: f64,
    /// Неба больше нет среди тех, что отданы движку: его облака тают там, где были.
    gone: bool,
    touched: u32,
    /// От дальних к ближним.
    clouds: Vec<Cloud>,
}

impl SkyState {
    fn is(&self, sky: &Sky) -> bool {
        !self.gone && self.id == sky.id && self.generation == sky.generation
    }

    fn follow(&mut self, sky: &Sky, pass: u32) {
        self.layer = sky.layer;
        if self.parallax != sky.parallax {
            self.parallax = sky.parallax;
            for cloud in &mut self.clouds {
                cloud.parallax = sky.parallax + PARALLAX_GAIN * cloud.near;
            }
        }
        self.touched = pass;
    }

    fn active(&self) -> usize {
        self.clouds.iter().filter(|cloud| !cloud.leaving).count()
    }

    fn insert(&mut self, cloud: Cloud) {
        let at = self
            .clouds
            .partition_point(|other| other.near <= cloud.near);
        self.clouds.insert(at, cloud);
    }

    fn melt_all(&mut self) {
        for cloud in &mut self.clouds {
            cloud.leaving = true;
        }
    }

    /// Облака с картинкой, которой в списке больше нет, тают.
    fn melt_unlisted(&mut self, images: &[ImageId]) {
        for cloud in &mut self.clouds {
            cloud.leaving |= !images.contains(&cloud.image);
        }
    }

    /// Тает облако, что дальше всех от середины окна.
    fn melt_farthest(&mut self, ring: &Ring) {
        let farthest = self
            .clouds
            .iter_mut()
            .filter(|cloud| !cloud.leaving)
            .max_by(|a, b| ring.offset(a).abs().total_cmp(&ring.offset(b).abs()));
        if let Some(cloud) = farthest {
            cloud.leaving = true;
        }
    }
}

/// Сколько облаков на небе: ни одного без картинок.
fn target_count(sky: &Sky) -> usize {
    if sky.images.is_empty() {
        return 0;
    }
    (CLOUDS_AT_FULL * sky.clouds.clamp(0.0, 1.0)).round() as usize
}

/// Новое облако без места по ширине: картинка, отражение, высота и размер наугад. `None`, если у
/// картинки нет основного размера.
fn roll_cloud(rolls: &Rolls, sky: &Sky, sizes: &[Option<Vec2>]) -> Option<Cloud> {
    let pick = ((rolls.image * sky.images.len() as f64) as usize).min(sky.images.len() - 1);
    let image = sky.images[pick];
    let base = sizes.get(image).copied().flatten()?;
    let (top, height) = (sky.rect.0[1], sky.rect.1[1]);
    let near = 1.0 - rolls.height;
    let fix = SIZE_FIX.0 + (SIZE_FIX.1 - SIZE_FIX.0) * rolls.fix;
    let scale = (SIZE_BY_NEARNESS.0 + SIZE_BY_NEARNESS.1 * near) * fix;
    Some(Cloud {
        image,
        mirrored: rolls.mirror >= 0.5,
        near,
        x: 0.0,
        y: top + height * (BAND.0 + (BAND.1 - BAND.0) * rolls.height),
        size: base.map(|side| side * scale),
        parallax: sky.parallax + PARALLAX_GAIN * near,
        fade: 0.0,
        leaving: false,
    })
}

/// Камера кадра: середина сцены и то, что нужно кольцу от неё.
#[derive(Debug, Clone, Copy)]
pub struct Camera<'a> {
    pub scene_middle: f64,
    pub view: &'a LayerView,
}

#[derive(Debug)]
pub struct Clouds {
    rng: Rng,
    /// Основной размер картинки облака в клетках, по номеру картинки.
    base_sizes: Vec<Option<Vec2>>,
    skies: Vec<SkyState>,
    integrated_steps: u64,
    /// Ближайшая сборка мира застанет небо — оно будет в облаках сразу.
    warm: bool,
    pass: u32,
}

impl Default for Clouds {
    fn default() -> Clouds {
        Clouds {
            rng: Rng::new(SEED),
            base_sizes: Vec::new(),
            skies: Vec::new(),
            integrated_steps: 0,
            warm: true,
            pass: 0,
        }
    }
}

impl Clouds {
    /// «Облака»: небо при каждом запуске своё — страница даёт облакам своё зерно случайности; без него
    /// зерно постоянное, и раскладка повторяется (так в тестах).
    pub fn seed(&mut self, seed: u64) {
        self.rng = Rng::new(seed);
    }

    /// Основные размеры картинок загруженной игры. Те же, что были, — облака остаются: файлы
    /// перечитаны, а мир нет; другие — номера картинок могли сдвинуться, и небо соберётся заново, облака
    /// проступят.
    pub(crate) fn set_base_sizes(&mut self, sizes: Vec<Option<Vec2>>) {
        if self.base_sizes != sizes {
            self.base_sizes = sizes;
            self.skies.clear();
        }
    }

    /// Часы ушли назад или далеко вперёд, партия началась заново — облаков нет, небо соберётся на
    /// ближайшем кадре. Счётчик случайности не трогается.
    pub fn restart(&mut self, steps: f64) {
        self.skies.clear();
        self.integrated_steps = steps.floor() as u64;
        self.warm = true;
    }

    /// Рисунки облаков неба объекта, от дальних к ближним.
    pub fn live_sprites(&self, id: u32, generation: u32) -> impl Iterator<Item = Sprite> + '_ {
        self.skies
            .iter()
            .filter(move |sky| !sky.gone && sky.id == id && sky.generation == generation)
            .flat_map(|sky| sky.clouds.iter().map(Cloud::sprite))
    }

    /// Рисунки облаков неба, которого больше нет, со `layer` неба на момент ухода.
    pub fn orphan_sprites(&self) -> impl Iterator<Item = (i32, Sprite)> + '_ {
        self.skies
            .iter()
            .filter(|sky| sky.gone)
            .flat_map(|sky| sky.clouds.iter().map(|cloud| (sky.layer, cloud.sprite())))
    }

    /// Доводит облака до часов `clock_steps` шагами по 1/60 секунды, затем приводит небо к тому, что
    /// сейчас в `skies`: недостающие облака встают, лишние и с убранной картинкой тают, ушедшие за край
    /// кольца переставляются за камеру. `wind` — ровный ветер сцены по `x`. Небо, которого нет среди
    /// `skies`, тает там, где было. Небо, что встало в мире только что собранном (`restart`, первая
    /// сборка) и с `world_exists`, — сразу в облаках в полную силу; позже — без облаков, они проступают.
    pub fn update<'a>(
        &mut self,
        clock_steps: f64,
        world_exists: bool,
        wind: f64,
        camera: Camera,
        skies: impl Iterator<Item = Sky<'a>>,
    ) {
        let wind = if wind.is_finite() { wind } else { 0.0 };
        self.pass = self.pass.wrapping_add(1);
        let due = clock_steps.floor() as u64;
        while self.integrated_steps < due {
            self.integrated_steps += 1;
            for sky in &mut self.skies {
                sky.clouds.retain_mut(|cloud| cloud.step(wind));
            }
        }

        let warm = self.warm && world_exists;
        for sky in skies {
            self.tend(&sky, camera, warm);
        }
        let pass = self.pass;
        for state in self.skies.iter_mut().filter(|state| state.touched != pass) {
            state.gone = true;
            state.melt_all();
        }
        self.skies
            .retain(|state| !state.gone || !state.clouds.is_empty());
        if world_exists {
            self.warm = false;
        }
    }

    fn tend(&mut self, sky: &Sky, camera: Camera, warm: bool) {
        let target = target_count(sky);
        let Clouds {
            rng,
            base_sizes,
            skies,
            pass,
            ..
        } = self;
        let (state, strength) = match skies.iter().position(|state| state.is(sky)) {
            Some(index) => {
                skies[index].follow(sky, *pass);
                (&mut skies[index], 0.0)
            }
            None if target == 0 => return,
            None => {
                skies.push(SkyState {
                    id: sky.id,
                    generation: sky.generation,
                    layer: sky.layer,
                    parallax: sky.parallax,
                    gone: false,
                    touched: *pass,
                    clouds: Vec::new(),
                });
                let state = skies.last_mut().expect("только что добавлено");
                (state, if warm { 1.0 } else { 0.0 })
            }
        };
        let widest = sky
            .images
            .iter()
            .filter_map(|&image| base_sizes.get(image).copied().flatten())
            .map(|[width, _]| width * SIZE_FIX.1)
            .fold(0.0, f64::max);
        let ring = Ring {
            scene_middle: camera.scene_middle,
            shift: camera.view.shift[0],
            length: RING_WINDOWS * camera.view.window_cells + RING_WIDEST * widest,
        };
        state.melt_unlisted(sky.images);
        reseat_outside(state, rng, base_sizes, sky, &ring);
        while state.active() > target {
            state.melt_farthest(&ring);
        }
        while state.active() < target {
            let rolls = Rolls::draw(rng);
            let Some(mut cloud) = roll_cloud(&rolls, sky, base_sizes) else {
                break;
            };
            cloud.x = ring.middle(cloud.parallax) + freest_offset(state, &rolls, &ring);
            cloud.fade = strength;
            state.insert(cloud);
        }
    }
}

/// Облака, середина которых вышла за край своего кольца, получают всё наугад заново и встают у
/// противоположного края нового кольца — на столько внутрь, на сколько вышли (по кругу, если дальше
/// целого кольца).
fn reseat_outside(
    state: &mut SkyState,
    rng: &mut Rng,
    sizes: &[Option<Vec2>],
    sky: &Sky,
    ring: &Ring,
) {
    if ring.length <= 0.0 {
        return;
    }
    let half = ring.length / 2.0;
    let mut moved = false;
    for index in 0..state.clouds.len() {
        let cloud = state.clouds[index];
        let offset = ring.offset(&cloud);
        if cloud.leaving || offset.abs() <= half {
            continue;
        }
        let rolls = Rolls::draw(rng);
        let Some(mut fresh) = roll_cloud(&rolls, sky, sizes) else {
            continue;
        };
        let beyond = (offset.abs() - half) % ring.length;
        let inside = half - beyond;
        let opposite = if offset < 0.0 { inside } else { -inside };
        fresh.x = ring.middle(fresh.parallax) + opposite;
        fresh.fade = 1.0;
        state.clouds[index] = fresh;
        moved = true;
    }
    if moved {
        state
            .clouds
            .sort_unstable_by(|a, b| a.near.total_cmp(&b.near));
    }
}

/// Из случайных мест в кольце — то, что дальше всех от остальных облаков неба по ширине: смещение от
/// середины кольца.
fn freest_offset(state: &SkyState, rolls: &Rolls, ring: &Ring) -> f64 {
    let mut best = (f64::NEG_INFINITY, 0.0);
    for roll in rolls.places {
        let offset = (roll - 0.5) * ring.length;
        let gap = state
            .clouds
            .iter()
            .filter(|cloud| !cloud.leaving)
            .map(|cloud| (ring.offset(cloud) - offset).abs())
            .fold(f64::INFINITY, f64::min);
        if gap > best.0 {
            best = (gap, offset);
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE_MIDDLE: f64 = 50.0;
    const SKY: Rect = ([0.0, 0.0], [100.0, 20.0]);
    const SIZES: [Option<Vec2>; 2] = [Some([4.0, 2.0]), Some([6.0, 3.0])];

    fn near(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    fn view(shift: f64) -> LayerView {
        LayerView {
            shift: [shift, 0.0],
            window_cells: 30.0,
        }
    }

    fn sky<'a>(clouds: f64, images: &'a [ImageId]) -> Sky<'a> {
        Sky {
            id: 3,
            generation: 0,
            rect: SKY,
            layer: 0,
            parallax: 0.0,
            clouds,
            images,
        }
    }

    fn make() -> Clouds {
        let mut clouds = Clouds::default();
        clouds.set_base_sizes(SIZES.to_vec());
        clouds
    }

    /// Кадр: часы на `seconds`, камера сдвинута на `shift`.
    fn frame(clouds: &mut Clouds, seconds: f64, wind: f64, shift: f64, input: Sky) {
        let view = view(shift);
        let camera = Camera {
            scene_middle: SCENE_MIDDLE,
            view: &view,
        };
        clouds.update(
            seconds * STEPS_PER_SECOND,
            true,
            wind,
            camera,
            std::iter::once(input),
        );
    }

    fn sprites(clouds: &Clouds) -> Vec<Sprite> {
        clouds.live_sprites(3, 0).collect()
    }

    fn standing(count: f64) -> Clouds {
        let mut clouds = make();
        frame(&mut clouds, 0.0, 0.0, 0.0, sky(count, &[0, 1]));
        clouds
    }

    /// Те же облака, все — в середине кольца: за край ни одно не выйдет.
    fn centred(count: f64) -> Clouds {
        let mut clouds = standing(count);
        for cloud in &mut clouds.skies[0].clouds {
            cloud.x = SCENE_MIDDLE;
        }
        clouds
    }

    #[test]
    fn the_sky_gets_round_of_24_times_the_clouds() {
        for (amount, count) in [(0.3, 7), (1.0, 24), (0.0, 0), (0.01, 0), (0.03, 1)] {
            let clouds = standing(amount);
            assert_eq!(sprites(&clouds).len(), count, "clouds {amount}");
        }
    }

    #[test]
    fn a_sky_without_pictures_has_none_and_a_picture_without_a_size_gives_none() {
        let mut clouds = make();
        frame(&mut clouds, 0.0, 0.0, 0.0, sky(1.0, &[]));
        assert!(sprites(&clouds).is_empty());

        let mut clouds = make();
        frame(&mut clouds, 0.0, 0.0, 0.0, sky(1.0, &[5]));
        assert!(sprites(&clouds).is_empty(), "размера у картинки нет");
    }

    #[test]
    fn the_height_is_inside_the_band_and_the_closeness_follows_it() {
        let clouds = standing(1.0);
        for sprite in sprites(&clouds) {
            assert!((1.0..=12.0).contains(&sprite.center[1]), "{sprite:?}");
            let nearness = 1.0 - (sprite.center[1] / 20.0 - BAND.0) / (BAND.1 - BAND.0);
            assert!((-1e-9..=1.0 + 1e-9).contains(&nearness), "{nearness}");
            assert!((-1e-9..=0.12 + 1e-9).contains(&sprite.parallax));
            assert!(near(sprite.parallax, 0.12 * nearness, 1e-9));
            assert!(near(sprite.opacity, 0.55 + 0.45 * nearness, 1e-9));
        }
    }

    /// Облако у верха полосы и у её низа с одной картинкой и поправкой 1: размер, плотность и скорость.
    #[test]
    fn a_high_cloud_is_bigger_denser_and_faster_than_a_low_one() {
        let high = roll_with(0.0, 0.5);
        let low = roll_with(0.999, 0.5);
        assert!(high.size[0] > low.size[0]);
        assert!(high.sprite().opacity > low.sprite().opacity);
        assert!(high.parallax > low.parallax);
        assert!(near(high.parallax, 0.12, 1e-12));
        assert!(near(low.parallax, 0.0, 0.001));
        let (mut a, mut b) = (high, low);
        a.step(1.0);
        b.step(1.0);
        assert!(a.x - high.x > b.x - low.x);
    }

    fn roll_with(height: f64, fix: f64) -> Cloud {
        let rolls = Rolls {
            image: 0.0,
            mirror: 0.0,
            height,
            fix,
            places: [0.5; PLACE_CANDIDATES],
        };
        let mut cloud = roll_cloud(&rolls, &sky(1.0, &[0]), &SIZES).unwrap();
        cloud.fade = 1.0;
        cloud
    }

    #[test]
    fn the_size_is_the_base_times_the_closeness_and_the_fix() {
        let high = roll_with(0.0, 0.5);
        assert!(near(high.size[0], 4.0, 1e-12), "{:?}", high.size);
        assert!(near(high.size[1], 2.0, 1e-12));
        let low = roll_with(1.0, 0.0);
        assert!(near(low.size[0], 4.0 * 0.5 * 0.85, 1e-12), "{:?}", low.size);
        let fat = roll_with(0.0, 1.0);
        assert!(near(fat.size[0], 4.0 * 1.15, 1e-12));
    }

    fn top_cloud_x(clouds: &Clouds) -> (f64, usize) {
        let sprites = sprites(clouds);
        let (index, top) = sprites
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.parallax.total_cmp(&b.1.parallax))
            .unwrap();
        (top.center[0], index)
    }

    #[test]
    fn the_wind_carries_a_cloud_by_the_worked_numbers() {
        for (wind, moved) in [(1.5, 1.5), (-1.5, -1.5), (0.0, 0.3)] {
            let mut cloud = roll_with(0.0, 0.5);
            let start = cloud.x;
            for _ in 0..600 {
                cloud.step(wind);
            }
            assert!(near(cloud.x - start, moved, 1e-9), "ветер {wind}");
        }
    }

    #[test]
    fn the_wind_over_y_does_not_change_the_height_and_a_changed_wind_acts_at_once() {
        let mut clouds = centred(0.3);
        let before = sprites(&clouds);
        frame(&mut clouds, 5.0, 1.5, 0.0, sky(0.3, &[0, 1]));
        let right = sprites(&clouds);
        frame(&mut clouds, 10.0, -1.5, 0.0, sky(0.3, &[0, 1]));
        let left = sprites(&clouds);
        for ((start, rightward), leftward) in before.iter().zip(&right).zip(&left) {
            assert_eq!(start.center[1], leftward.center[1]);
            assert!(rightward.center[0] > start.center[0]);
            assert!(
                leftward.center[0] < rightward.center[0],
                "ветер развернулся — облака плывут обратно"
            );
        }
    }

    #[test]
    fn a_cloud_that_leaves_the_ring_stands_at_the_opposite_edge_with_a_new_height() {
        let mut clouds = standing(0.3);
        let ring_half = (3.0 * 30.0 + 2.0 * 6.0 * 1.15) / 2.0;
        let (_, index) = top_cloud_x(&clouds);
        let before = sprites(&clouds)[index];
        {
            let state = &mut clouds.skies[0];
            let cloud = state
                .clouds
                .iter_mut()
                .find(|cloud| cloud.y == before.center[1])
                .unwrap();
            cloud.x = SCENE_MIDDLE + cloud.parallax * 0.0 + ring_half + 0.2;
        }
        frame(&mut clouds, 0.0, 0.0, 0.0, sky(0.3, &[0, 1]));
        let after = sprites(&clouds);
        assert_eq!(after.len(), 7);
        let ring = Ring {
            scene_middle: SCENE_MIDDLE,
            shift: 0.0,
            length: 2.0 * ring_half,
        };
        let pushed = clouds.skies[0]
            .clouds
            .iter()
            .filter(|cloud| (ring.offset(cloud) + ring_half).abs() < 0.3)
            .count();
        assert_eq!(pushed, 1, "встало у левого края: {after:?}");
        assert!(
            clouds.skies[0]
                .clouds
                .iter()
                .all(|cloud| { ring.offset(cloud).abs() <= ring_half + 1e-9 && cloud.fade == 1.0 })
        );
    }

    #[test]
    fn a_camera_that_went_a_hundred_cells_right_leaves_every_cloud_in_the_new_ring() {
        let mut clouds = standing(1.0);
        frame(&mut clouds, 0.0, 0.0, 100.0, sky(1.0, &[0, 1]));
        let half = (3.0 * 30.0 + 2.0 * 6.0 * 1.15) / 2.0;
        let state = &clouds.skies[0];
        assert_eq!(state.active(), 24);
        for cloud in &state.clouds {
            let middle = SCENE_MIDDLE + 100.0 * cloud.parallax;
            assert!((cloud.x - middle).abs() <= half + 1e-9, "{cloud:?}");
            assert_eq!(cloud.fade, 1.0, "встали сразу в полную силу");
        }
        let distinct = state
            .clouds
            .iter()
            .map(|cloud| (cloud.x * 1000.0) as i64)
            .collect::<std::collections::HashSet<_>>();
        assert!(distinct.len() > 12, "не сбились в кучу у одного края");
    }

    #[test]
    fn a_sky_from_the_first_assembly_stands_in_full_strength_and_a_later_one_grows_in_four_seconds()
    {
        let warm = standing(0.3);
        assert!(warm.skies[0].clouds.iter().all(|cloud| cloud.fade == 1.0));
        assert!(sprites(&warm).iter().all(|sprite| sprite.opacity >= 0.55));

        let mut late = make();
        late.update(
            0.0,
            true,
            0.0,
            Camera {
                scene_middle: SCENE_MIDDLE,
                view: &view(0.0),
            },
            std::iter::empty(),
        );
        frame(&mut late, 0.0, 0.0, 0.0, sky(0.3, &[0, 1]));
        assert_eq!(sprites(&late).len(), 7);
        assert!(sprites(&late).iter().all(|sprite| sprite.opacity == 0.0));
        frame(&mut late, 2.0, 0.0, 0.0, sky(0.3, &[0, 1]));
        assert!(
            late.skies[0]
                .clouds
                .iter()
                .all(|cloud| near(cloud.fade, 0.5, 0.02))
        );
        frame(&mut late, 4.1, 0.0, 0.0, sky(0.3, &[0, 1]));
        assert!(late.skies[0].clouds.iter().all(|cloud| cloud.fade == 1.0));
    }

    #[test]
    fn a_world_that_does_not_exist_yet_keeps_the_first_assembly_for_later() {
        let mut clouds = make();
        let view = view(0.0);
        let camera = Camera {
            scene_middle: SCENE_MIDDLE,
            view: &view,
        };
        clouds.update(0.0, false, 0.0, camera, std::iter::empty());
        clouds.update(0.0, true, 0.0, camera, std::iter::once(sky(0.3, &[0, 1])));
        assert!(clouds.skies[0].clouds.iter().all(|cloud| cloud.fade == 1.0));
    }

    #[test]
    fn fewer_clouds_melt_the_farthest_from_the_middle_and_vanish_after_four_seconds() {
        let mut clouds = standing(1.0);
        frame(&mut clouds, 0.0, 0.0, 0.0, sky(0.3, &[0, 1]));
        let ring = Ring {
            scene_middle: SCENE_MIDDLE,
            shift: 0.0,
            length: 0.0,
        };
        let state = &clouds.skies[0];
        assert_eq!(state.active(), 7);
        let kept_farthest = state
            .clouds
            .iter()
            .filter(|cloud| !cloud.leaving)
            .map(|cloud| ring.offset(cloud).abs())
            .fold(0.0, f64::max);
        let melted_nearest = state
            .clouds
            .iter()
            .filter(|cloud| cloud.leaving)
            .map(|cloud| ring.offset(cloud).abs())
            .fold(f64::INFINITY, f64::min);
        assert_eq!(state.clouds.len(), 24);
        assert!(melted_nearest >= kept_farthest - 1e-9);

        frame(&mut clouds, 2.0, 0.0, 0.0, sky(0.3, &[0, 1]));
        assert_eq!(sprites(&clouds).len(), 24, "ещё тают");
        frame(&mut clouds, 4.2, 0.0, 0.0, sky(0.3, &[0, 1]));
        assert_eq!(sprites(&clouds).len(), 7);
    }

    #[test]
    fn a_count_of_zero_a_removed_picture_list_or_a_missing_sky_melts_everything() {
        for survivors in [sky(0.0, &[0, 1]), sky(0.5, &[])] {
            let mut clouds = standing(0.5);
            frame(&mut clouds, 0.0, 0.0, 0.0, survivors);
            assert!(clouds.skies[0].clouds.iter().all(|cloud| cloud.leaving));
            frame(&mut clouds, 4.2, 0.0, 0.0, survivors);
            assert!(sprites(&clouds).is_empty());
        }

        let mut clouds = standing(0.5);
        let view = view(0.0);
        let camera = Camera {
            scene_middle: SCENE_MIDDLE,
            view: &view,
        };
        clouds.update(0.0, true, 0.0, camera, std::iter::empty());
        assert!(sprites(&clouds).is_empty(), "небо ушло");
        assert_eq!(clouds.orphan_sprites().count(), 12);
        clouds.update(4.2 * 60.0, true, 0.0, camera, std::iter::empty());
        assert_eq!(clouds.orphan_sprites().count(), 0);
        assert!(clouds.skies.is_empty());
    }

    #[test]
    fn a_picture_taken_out_of_the_list_melts_its_clouds_and_new_ones_grow_in_their_place() {
        let mut clouds = standing(0.5);
        frame(&mut clouds, 0.0, 0.0, 0.0, sky(0.5, &[1]));
        let state = &clouds.skies[0];
        assert!(
            state
                .clouds
                .iter()
                .filter(|cloud| cloud.image == 0)
                .all(|cloud| cloud.leaving)
        );
        assert_eq!(state.active(), 12);
        assert!(
            state
                .clouds
                .iter()
                .filter(|cloud| !cloud.leaving)
                .all(|cloud| cloud.image == 1)
        );
        frame(&mut clouds, 4.2, 0.0, 0.0, sky(0.5, &[1]));
        assert!(sprites(&clouds).iter().all(|sprite| sprite.image == 1));
        assert_eq!(sprites(&clouds).len(), 12);
    }

    #[test]
    fn a_picture_added_to_the_list_leaves_the_old_clouds_alone() {
        let mut clouds = standing(0.5);
        let before: Vec<_> = sprites(&clouds).iter().map(|sprite| sprite.image).collect();
        frame(&mut clouds, 0.0, 0.0, 0.0, sky(0.5, &[0, 1, 1]));
        let after: Vec<_> = sprites(&clouds).iter().map(|sprite| sprite.image).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn the_clouds_are_ordered_from_far_to_near_and_half_of_them_are_mirrored() {
        let clouds = standing(1.0);
        let sprites = sprites(&clouds);
        assert!(
            sprites
                .windows(2)
                .all(|pair| pair[0].parallax <= pair[1].parallax)
        );
        let mirrored = sprites.iter().filter(|sprite| sprite.mirrored).count();
        assert!((6..=18).contains(&mirrored), "{mirrored}");
    }

    #[test]
    fn a_new_cloud_takes_the_freest_of_three_places() {
        let mut state = SkyState {
            id: 0,
            generation: 0,
            layer: 0,
            parallax: 0.0,
            gone: false,
            touched: 0,
            clouds: vec![Cloud {
                near: 0.0,
                ..roll_with(0.999, 0.5)
            }],
        };
        let ring = Ring {
            scene_middle: 0.0,
            shift: 0.0,
            length: 100.0,
        };
        state.clouds[0].x = 10.0;
        let rolls = Rolls {
            image: 0.0,
            mirror: 0.0,
            height: 0.0,
            fix: 0.0,
            places: [0.55, 0.05, 0.95],
        };
        assert!(near(freest_offset(&state, &rolls, &ring), -45.0, 1e-9));
    }

    #[test]
    fn a_restart_empties_the_sky_and_warms_the_next_assembly() {
        let mut clouds = standing(0.3);
        clouds.restart(0.0);
        assert!(sprites(&clouds).is_empty());
        frame(&mut clouds, 0.0, 0.0, 0.0, sky(0.3, &[0, 1]));
        assert!(clouds.skies[0].clouds.iter().all(|cloud| cloud.fade == 1.0));
    }

    #[test]
    fn the_clock_stands_the_clouds_stand() {
        let mut clouds = standing(0.3);
        let before = sprites(&clouds);
        frame(&mut clouds, 0.0, 1.5, 0.0, sky(0.3, &[0, 1]));
        assert_eq!(before, sprites(&clouds));
    }

    #[test]
    fn two_skies_keep_their_own_clouds() {
        let mut clouds = make();
        let view = view(0.0);
        let camera = Camera {
            scene_middle: SCENE_MIDDLE,
            view: &view,
        };
        let first = Sky {
            id: 3,
            ..sky(0.3, &[0])
        };
        let second = Sky {
            id: 4,
            ..sky(1.0, &[1])
        };
        clouds.update(0.0, true, 0.0, camera, [first, second].into_iter());
        assert_eq!(clouds.live_sprites(3, 0).count(), 7);
        assert_eq!(clouds.live_sprites(4, 0).count(), 24);
        assert!(clouds.live_sprites(3, 0).all(|sprite| sprite.image == 0));
        assert!(clouds.live_sprites(4, 0).all(|sprite| sprite.image == 1));
    }

    #[test]
    fn a_new_generation_in_the_same_slot_is_another_sky() {
        let mut clouds = standing(0.3);
        let view = view(0.0);
        let camera = Camera {
            scene_middle: SCENE_MIDDLE,
            view: &view,
        };
        let reborn = Sky {
            generation: 1,
            ..sky(0.3, &[0, 1])
        };
        clouds.update(0.0, true, 0.0, camera, std::iter::once(reborn));
        assert_eq!(clouds.live_sprites(3, 1).count(), 7);
        assert_eq!(clouds.live_sprites(3, 0).count(), 0);
        assert_eq!(clouds.orphan_sprites().count(), 7);
    }

    #[test]
    fn the_same_clock_gives_the_same_clouds_however_it_is_sliced() {
        let run = |slices: &[f64]| {
            let mut clouds = centred(0.3);
            for seconds in slices {
                frame(&mut clouds, *seconds, 1.5, 0.0, sky(0.3, &[0, 1]));
            }
            sprites(&clouds)
        };
        let whole = run(&[10.0]);
        let sliced = run(&[2.5, 5.0, 7.5, 10.0]);
        assert_eq!(whole.len(), sliced.len());
        for (a, b) in whole.iter().zip(&sliced) {
            assert!(near(a.center[0], b.center[0], 1e-9));
        }
    }
}
