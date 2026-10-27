//! «Картинки» → «Видео»: браузерные проигрыватели видео игры и перенос их кадров в атлас. Перед
//! отрисовкой кадра текущий кадр каждого проигрывателя копируется в текстуру файла видео, и проход
//! `shaders/video.wgsl` пишет его в место видео в слое атласа. Всё остальное в отрисовке видео от
//! картинки не отличает.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::{Function, Reflect};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlMediaElement, HtmlVideoElement};

use super::atlas::AtlasRect;
use super::gpu::ATLAS_FORMAT;
use super::video_play::{
    self, IdleFade, LoopCourse, LoopStep, PanelCourse, PanelStep, PlayRequest, PlayerAction,
    PlayerReading, Roll, Slot,
};

/// Кто играет видео игры: часы кадров мира, панель по наведению или панель по кругу.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoRole {
    World,
    /// Панель с `play_on_hover`: её ведёт ход видео панели, а не часы кадров, и у видео есть второй
    /// проигрыватель — файл `reverse`.
    Hover,
    /// Панель без `play_on_hover`: идёт по кругу временем окна, пока экран с панелью активен. Второй
    /// проигрыватель того же файла — `reverse` источника, который страница прочитала вторым разом: они
    /// играют по очереди.
    Loop,
}

/// Видео игры, как его отдаёт загрузка: проигрыватель, размер файла (оба слоя вместе) и место одного
/// кадра в атласе.
pub struct VideoSource {
    pub image: usize,
    pub player: HtmlVideoElement,
    pub reverse: Option<HtmlVideoElement>,
    pub role: VideoRole,
    pub width: u32,
    pub height: u32,
    pub place: AtlasRect,
}

/// Как панель играет своё видео в этом кадре.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanelPlay {
    /// «Видео на панели»: `hovering` — место наведения панели горит (курсор над ним или оно выбрано), `held` —
    /// горит, потому что выбрано: на экране, открытом заново, такое видео сразу стоит на последнем кадре.
    /// `rest_at` — секунда ролика с позой последнего кадра, `idle` — ролик покоя панели, `now` — время окна в
    /// секундах.
    Hover {
        hovering: bool,
        held: bool,
        rest_at: Option<f32>,
        idle: Option<usize>,
        now: f64,
    },
    /// «Видео на панели»: по кругу; `now` — время окна в секундах, `opened` — сколько раз открывали экран:
    /// открыли заново — видео снова с первого кадра.
    Loop { now: f64, opened: u64 },
}

/// «Видео на панели»: панель с видео `image` стоит на экране.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelInput {
    pub image: usize,
    pub play: PanelPlay,
}

/// Ход видео панели и второй проигрыватель — тот же ролик задом наперёд.
struct PanelVideo {
    reverse: HtmlVideoElement,
    reverse_request: Rc<Cell<PlayRequest>>,
    course: PanelCourse,
    /// Чей кадр писать в атлас в этом кадре; `None` — оставить показанный.
    show: Option<Roll>,
    /// «Видео на панели», `idle`: ролик покоя панели, покой и вес кадра ролика покоя поверх видео.
    idle: Option<usize>,
    fade: IdleFade,
    idle_weight: f32,
    /// В месте атласа видео лежит смесь с кадром покоя, а не чистый кадр видео.
    mixed: Cell<bool>,
}

/// Ход видео по кругу и второй проигрыватель того же файла.
struct LoopVideo {
    twin: HtmlVideoElement,
    twin_request: Rc<Cell<PlayRequest>>,
    course: LoopCourse,
    /// Чей кадр писать в атлас в этом кадре; `None` — оставить показанный.
    show: Option<Slot>,
}

struct Video {
    image: usize,
    player: HtmlVideoElement,
    play_request: Rc<Cell<PlayRequest>>,
    panel: Option<PanelVideo>,
    looped: Option<LoopVideo>,
    width: u32,
    height: u32,
    place: AtlasRect,
    file: wgpu::Texture,
    file_bind_group: wgpu::BindGroup,
    place_layer: wgpu::TextureView,
    /// Какой кадр лежит в текстуре файла; `None` — ещё никакого.
    copied: RefCell<Option<FrameMark>>,
    /// Счёт показанных кадров каждого проигрывателя видео.
    presented: Vec<PresentedFrames>,
}

/// Кадр проигрывателя: сам проигрыватель и его место. Идущий проигрыватель сменил кадр, когда браузер показал
/// новый (`presented`): время у него идёт и между кадрами ролика. Стоящий — когда сменилось время или
/// закончился переход на новое место.
#[derive(Clone, PartialEq)]
struct FrameMark {
    player: HtmlVideoElement,
    paused: bool,
    seeking: bool,
    time: f64,
    /// Сколько кадров идущий проигрыватель показал; `None` — браузер этого не сообщает, и кадр считается новым
    /// всегда.
    presented: Option<u32>,
}

impl FrameMark {
    fn of(player: &HtmlVideoElement, presented: Option<u32>) -> FrameMark {
        let paused = player.paused();
        FrameMark {
            player: player.clone(),
            paused,
            seeking: player.seeking(),
            time: if paused { player.current_time() } else { 0.0 },
            presented: if paused { Some(0) } else { presented },
        }
    }
}

/// Обработчик `requestVideoFrameCallback`: время окна и сведения о кадре ему не нужны.
type FrameTick = Closure<dyn FnMut(JsValue, JsValue)>;

/// Счёт кадров, которые проигрыватель показал: браузер зовёт `requestVideoFrameCallback` на каждый новый кадр
/// видео — это и есть «кадр сменился». Счёт разжатых кадров (`getVideoPlaybackQuality`) для этого не годится:
/// разжатие идёт на несколько кадров впереди показа, и последние кадры перед концом ролика замерли бы. Браузер
/// без `requestVideoFrameCallback` счёта не получает.
struct PresentedFrames {
    player: HtmlVideoElement,
    count: Rc<Cell<u32>>,
    request: Rc<Cell<f64>>,
    callback: Rc<RefCell<Option<FrameTick>>>,
}

impl PresentedFrames {
    fn watch(player: &HtmlVideoElement) -> Option<PresentedFrames> {
        let ask = Reflect::get(player, &JsValue::from_str("requestVideoFrameCallback"))
            .ok()?
            .dyn_into::<Function>()
            .ok()?;
        let count = Rc::new(Cell::new(0u32));
        let request = Rc::new(Cell::new(0.0));
        let callback: Rc<RefCell<Option<FrameTick>>> = Rc::new(RefCell::new(None));
        let again = {
            let (player, ask, request, callback) = (
                player.clone(),
                ask.clone(),
                Rc::clone(&request),
                Rc::downgrade(&callback),
            );
            move || {
                let Some(callback) = callback.upgrade() else {
                    return;
                };
                if let Some(callback) = callback.borrow().as_ref()
                    && let Ok(id) = ask.call1(&player, callback.as_ref().unchecked_ref())
                {
                    request.set(id.as_f64().unwrap_or(0.0));
                }
            }
        };
        let tick = {
            let count = Rc::clone(&count);
            let again = again.clone();
            Closure::new(move |_now: JsValue, _metadata: JsValue| {
                count.set(count.get().wrapping_add(1));
                again();
            })
        };
        callback.replace(Some(tick));
        again();
        Some(PresentedFrames {
            player: player.clone(),
            count,
            request,
            callback,
        })
    }
}

impl Drop for PresentedFrames {
    /// Просьба снимается до того, как уходит её обработчик: браузер не позовёт то, чего уже нет.
    fn drop(&mut self) {
        if let Ok(cancel) =
            Reflect::get(&self.player, &JsValue::from_str("cancelVideoFrameCallback"))
            && let Ok(cancel) = cancel.dyn_into::<Function>()
        {
            let _ = cancel.call1(&self.player, &JsValue::from_f64(self.request.get()));
        }
        self.callback.replace(None);
    }
}

struct VideoPass {
    pipeline: wgpu::RenderPipeline,
    /// «Видео на панели», `idle`: тот же проход, но кадр смешивается с тем, что уже в месте атласа, с весом —
    /// постоянной смешения.
    mix_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

#[derive(Default)]
pub(super) struct Videos {
    pass: Option<VideoPass>,
    items: Vec<Video>,
    panel_inputs: Vec<PanelInput>,
}

impl Videos {
    /// Заменяет видео игры на `sources`; прежние останавливаются. `atlas` — текстура атласа, в слои
    /// которой пишет проход.
    pub fn set(&mut self, device: &wgpu::Device, atlas: &wgpu::Texture, sources: Vec<VideoSource>) {
        self.release();
        self.panel_inputs.clear();
        if sources.is_empty() {
            return;
        }
        let pass = self.pass.get_or_insert_with(|| VideoPass::new(device));
        self.items = sources
            .into_iter()
            .map(|source| Video::new(device, pass, atlas, source))
            .collect();
    }

    /// Останавливает проигрыватели и забывает их.
    pub fn release(&mut self) {
        for video in self.items.drain(..) {
            let _ = video.player.pause();
            if let Some(panel) = &video.panel {
                let _ = panel.reverse.pause();
            }
            if let Some(looped) = &video.looped {
                let _ = looped.twin.pause();
            }
        }
    }

    /// «Видео на панели»: панели с видео, что стоят на экране этого кадра, и над какими местами курсор. Видео
    /// панели, которой в списке нет, возвращается на первый кадр.
    pub fn set_panel_inputs(&mut self, inputs: &[PanelInput]) {
        self.panel_inputs.clear();
        self.panel_inputs.extend_from_slice(inputs);
    }

    /// Раз в отрисовку: кадр каждого видео, у которого он уже есть, копируется в текстуру файла и
    /// пишется в атлас. Видео без кадра остаётся прозрачным. Видео панели сначала делает свой шаг.
    pub fn refresh(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.drive_panels();
        let Some(pass) = &self.pass else {
            return;
        };
        let ready: Vec<(&Video, &HtmlVideoElement)> = self
            .items
            .iter()
            .filter_map(|video| Some((video, video.shown_player()?)))
            .filter(|(video, player)| video.has_frame(player) && video.frame_changed(player))
            .collect();
        let mixing = self.items.iter().any(|video| {
            video
                .panel
                .as_ref()
                .is_some_and(|panel| panel.idle_weight > 0.0 || panel.mixed.get())
        });
        if ready.is_empty() && !mixing {
            return;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("video_frames"),
        });
        let written: Vec<usize> = ready.iter().map(|(video, _)| video.image).collect();
        for (video, player) in ready {
            video.copy_frame(queue, player);
            video.encode_into_atlas(&mut encoder, pass);
        }
        self.mix_idle_frames(&mut encoder, pass, &written);
        queue.submit(std::iter::once(encoder.finish()));
    }

    /// «Видео на панели», `idle`: кадр ролика покоя смешивается с кадром видео по наведению в месте атласа
    /// этого видео, с весом таяния — так панель рисует одну картинку, и край фигуры, где силуэты расходятся, тает
    /// вместе со всем кадром. Смесь ложится только на чистый кадр видео, иначе легла бы на прошлую смесь: видео
    /// не записано в этот кадр (проигрыватель переходит на новое место, экран открыт заново) — чистый кадр
    /// заново пишется из текстуры файла, где лежит последний кадр видео. Так смесь не застывает на время перехода,
    /// а место, где лежала смесь, при весе 0 снова становится кадром видео.
    fn mix_idle_frames(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        pass: &VideoPass,
        written: &[usize],
    ) {
        for video in &self.items {
            let Some(panel) = &video.panel else {
                continue;
            };
            let weight = panel.idle_weight;
            if weight <= 0.0 && !panel.mixed.get() {
                continue;
            }
            if !written.contains(&video.image) {
                video.encode_into_atlas(encoder, pass);
            }
            let source = panel
                .idle
                .and_then(|idle| self.items.iter().find(|item| item.image == idle));
            match source {
                Some(source) if weight > 0.0 => {
                    video.encode_mix(encoder, pass, source, weight);
                    panel.mixed.set(true);
                }
                _ => panel.mixed.set(false),
            }
        }
    }

    /// Видео по наведению делают шаг первыми: ролик покоя идёт, только пока его видео в покое, и его круг ведёт
    /// номер покоя, а не открытие экрана. Затем — видео по кругу, затем вес кадра покоя: записал ли ролик
    /// покоя кадр, видно только после его шага.
    fn drive_panels(&mut self) {
        let mut idle_plays: Vec<(usize, PanelPlay)> = Vec::new();
        let mut idle_of: Vec<(usize, usize, f64)> = Vec::new();
        for video in &mut self.items {
            let input = self
                .panel_inputs
                .iter()
                .find(|input| input.image == video.image)
                .map(|input| input.play);
            if let Some(PanelPlay::Hover { held: true, .. }) = input
                && let Some(start) = video
                    .panel
                    .as_mut()
                    .and_then(|panel| panel.course.start_held())
            {
                apply_panel_step(&start, video);
            }
            let Some(panel) = &mut video.panel else {
                continue;
            };
            let Some(PanelPlay::Hover {
                hovering,
                rest_at,
                idle,
                now,
                ..
            }) = input
            else {
                panel.show = None;
                panel.fade.leave();
                panel.idle_weight = 0.0;
                if panel.course.reset() {
                    let _ = video.player.pause();
                    let _ = panel.reverse.pause();
                    video.play_request.set(PlayRequest::None);
                    panel.reverse_request.set(PlayRequest::None);
                    seek(&video.player, 0.0);
                }
                continue;
            };
            let current = match panel.course.current() {
                Roll::Forward => &video.player,
                Roll::Backward => &panel.reverse,
            };
            let step = panel
                .course
                .step(hovering, rest_at.map(f64::from), reading_of(current));
            panel.show = step.show;
            panel.idle = idle;
            if let Some(idle) = idle {
                if let Some(opened) = panel.fade.begin(panel.course.at_rest(hovering)) {
                    idle_plays.push((idle, PanelPlay::Loop { now, opened }));
                }
                idle_of.push((video.image, idle, now));
            }
            apply_panel_step(&step, video);
        }
        let mut written = Vec::new();
        for video in &mut self.items {
            if video.looped.is_none() {
                continue;
            }
            let input = self
                .panel_inputs
                .iter()
                .find(|input| input.image == video.image)
                .map(|input| input.play)
                .or_else(|| {
                    idle_plays
                        .iter()
                        .find(|(idle, _)| *idle == video.image)
                        .map(|(_, play)| *play)
                });
            drive_loop(video, input);
            if video
                .shown_player()
                .is_some_and(|player| video.has_frame(player))
            {
                written.push(video.image);
            }
        }
        for (image, idle, now) in idle_of {
            let Some(panel) = self
                .items
                .iter_mut()
                .find(|video| video.image == image)
                .and_then(|video| video.panel.as_mut())
            else {
                continue;
            };
            panel.idle_weight = panel.fade.finish(now, written.contains(&idle)) as f32;
        }
    }

    /// Часы кадров идут (`playing`) — проигрыватели играют, стоят — на паузе; когда просить играть,
    /// решает [`video_play::next_step`]. Видео панелей ведут их ходы.
    pub fn set_playing(&self, playing: bool) {
        for video in self
            .items
            .iter()
            .filter(|video| video.panel.is_none() && video.looped.is_none())
        {
            let (action, request) =
                video_play::next_step(playing, video.player.paused(), video.play_request.get());
            video.play_request.set(request);
            match action {
                PlayerAction::Play => video.start(),
                PlayerAction::Pause => {
                    let _ = video.player.pause();
                }
                PlayerAction::Nothing => {}
            }
        }
    }
}

impl VideoPass {
    fn new(device: &wgpu::Device) -> VideoPass {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("video"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/video.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("video_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("video_pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let make = |label: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: ATLAS_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..wgpu::PrimitiveState::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = make("video_pipeline", None);
        let weighted = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Constant,
            dst_factor: wgpu::BlendFactor::OneMinusConstant,
            operation: wgpu::BlendOperation::Add,
        };
        let mix_pipeline = make(
            "video_mix_pipeline",
            Some(wgpu::BlendState {
                color: weighted,
                alpha: weighted,
            }),
        );
        VideoPass {
            pipeline,
            mix_pipeline,
            layout,
        }
    }
}

impl Video {
    fn new(
        device: &wgpu::Device,
        pass: &VideoPass,
        atlas: &wgpu::Texture,
        source: VideoSource,
    ) -> Video {
        let file = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("video_file"),
            size: wgpu::Extent3d {
                width: source.width,
                height: source.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ATLAS_FORMAT,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let file_view = file.create_view(&wgpu::TextureViewDescriptor::default());
        let file_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("video_file_bind_group"),
            layout: &pass.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&file_view),
            }],
        });
        let place_layer = atlas.create_view(&wgpu::TextureViewDescriptor {
            label: Some("video_place_layer"),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: source.place.sheet,
            array_layer_count: Some(1),
            ..Default::default()
        });
        let presented = std::iter::once(&source.player)
            .chain(source.reverse.as_ref())
            .filter_map(PresentedFrames::watch)
            .collect();
        let (hover_reverse, loop_twin) = match source.role {
            VideoRole::Hover => (source.reverse, None),
            VideoRole::Loop => (None, source.reverse),
            VideoRole::World => (None, None),
        };
        let panel = hover_reverse.map(|reverse| {
            source.player.set_loop(false);
            reverse.set_loop(false);
            PanelVideo {
                reverse,
                reverse_request: Rc::new(Cell::new(PlayRequest::None)),
                course: PanelCourse::new(source.player.duration()),
                show: None,
                idle: None,
                fade: IdleFade::default(),
                idle_weight: 0.0,
                mixed: Cell::new(false),
            }
        });
        let looped = loop_twin.map(|twin| {
            let length = source.player.duration();
            // Стык ведут два проигрывателя по очереди. Ролик без длины браузер крутит сам.
            source
                .player
                .set_loop(!(length.is_finite() && length > 0.0));
            twin.set_loop(false);
            LoopVideo {
                twin,
                twin_request: Rc::new(Cell::new(PlayRequest::None)),
                course: LoopCourse::new(length),
                show: None,
            }
        });
        Video {
            image: source.image,
            player: source.player,
            play_request: Rc::new(Cell::new(PlayRequest::None)),
            panel,
            looped,
            width: source.width,
            height: source.height,
            place: source.place,
            file,
            file_bind_group,
            place_layer,
            copied: RefCell::new(None),
            presented,
        }
    }

    /// Кадр `player` ещё не лежит в текстуре файла. Видео идут 24 кадра в секунду, а экран рисуется 60 раз и
    /// чаще, видео на паузе не меняется вовсе: перелить в видеокарту тот же кадр снова — пустая работа, а с
    /// десятком видео начального экрана это миллионы точек на каждый кадр экрана.
    fn frame_changed(&self, player: &HtmlVideoElement) -> bool {
        let mark = self.mark_of(player);
        mark.presented.is_none() || self.copied.borrow().as_ref() != Some(&mark)
    }

    fn mark_of(&self, player: &HtmlVideoElement) -> FrameMark {
        let presented = self
            .presented
            .iter()
            .find(|watch| watch.player == *player)
            .map(|watch| watch.count.get());
        FrameMark::of(player, presented)
    }

    /// Проигрыватель, чей кадр писать в атлас в этом кадре: у обычного видео свой, у видео панели тот, что ход
    /// выбрал, а пока он переходит на новое место, — никакой.
    fn shown_player(&self) -> Option<&HtmlVideoElement> {
        if let Some(looped) = &self.looped {
            return match looped.show? {
                0 => Some(&self.player),
                _ => Some(&looped.twin),
            };
        }
        match &self.panel {
            None => Some(&self.player),
            Some(panel) => match panel.show? {
                Roll::Forward => Some(&self.player),
                Roll::Backward => Some(&panel.reverse),
            },
        }
    }

    /// Проигрыватель уже отдаёт кадр своего размера. Копировать видео без кадра браузер не даёт —
    /// `wgpu` на WebGPU упал бы на этом.
    fn has_frame(&self, player: &HtmlVideoElement) -> bool {
        player.ready_state() >= HtmlMediaElement::HAVE_CURRENT_DATA
            && player.video_width() == self.width
            && player.video_height() == self.height
    }

    fn copy_frame(&self, queue: &wgpu::Queue, player: &HtmlVideoElement) {
        self.copied.replace(Some(self.mark_of(player)));
        queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source: wgpu::ExternalImageSource::HTMLVideoElement(player.clone()),
                origin: wgpu::Origin2d::ZERO,
                flip_y: false,
            },
            wgpu::CopyExternalImageDestInfo {
                texture: &self.file,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
                color_space: wgpu::PredefinedColorSpace::Srgb,
                premultiplied_alpha: false,
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn encode_into_atlas(&self, encoder: &mut wgpu::CommandEncoder, pass: &VideoPass) {
        let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("video_into_atlas"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.place_layer,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        render.set_pipeline(&pass.pipeline);
        render.set_bind_group(0, &self.file_bind_group, &[]);
        render.set_viewport(
            self.place.x as f32,
            self.place.y as f32,
            self.place.w as f32,
            self.place.h as f32,
            0.0,
            1.0,
        );
        render.draw(0..4, 0..1);
    }

    /// Кадр из текстуры файла `source` (того же размера) смешивается с тем, что уже лежит в месте этого видео:
    /// `weight` от кадра `source`, остальное от прежнего. Атлас хранит цвет, умноженный на прозрачность, поэтому
    /// смесь цвета и прозрачности одним весом — честная смесь двух кадров.
    fn encode_mix(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        pass: &VideoPass,
        source: &Video,
        weight: f32,
    ) {
        let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("idle_into_atlas"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.place_layer,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        render.set_pipeline(&pass.mix_pipeline);
        render.set_bind_group(0, &source.file_bind_group, &[]);
        let weight = f64::from(weight);
        render.set_blend_constant(wgpu::Color {
            r: weight,
            g: weight,
            b: weight,
            a: weight,
        });
        render.set_viewport(
            self.place.x as f32,
            self.place.y as f32,
            self.place.w as f32,
            self.place.h as f32,
            0.0,
            1.0,
        );
        render.draw(0..4, 0..1);
    }

    /// Просьба играть. Ответ браузера дожидается, чтобы отказ не стал необработанным и запомнился;
    /// просьбу, которую прервала пауза самого движка (`AbortError`), отказом не считают.
    fn start(&self) {
        request_play(&self.player, &self.play_request);
    }
}

/// Просьба играть `player`; ответ браузера оседает в `request`.
fn request_play(player: &HtmlVideoElement, request: &Rc<Cell<PlayRequest>>) {
    let Ok(promise) = player.play() else {
        request.set(video_play::after_answer(true));
        return;
    };
    let request = Rc::clone(request);
    wasm_bindgen_futures::spawn_local(async move {
        let refused = match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(_) => false,
            Err(error) => !error
                .dyn_ref::<js_sys::Error>()
                .is_some_and(|error| error.name() == "AbortError"),
        };
        request.set(video_play::after_answer(refused));
    });
}

/// Шаг видео по кругу: панель на экране — ход следит за двумя проигрывателями по времени окна `now` и счёту
/// открытий экрана `opened`, панели нет — видео стоит на первом кадре и ждёт следующего открытия экрана.
fn drive_loop(video: &mut Video, input: Option<PanelPlay>) {
    let Some(looped) = &video.looped else {
        return;
    };
    let ready = [
        video.has_frame(&video.player),
        video.has_frame(&looped.twin),
    ];
    let readings = [reading_of(&video.player), reading_of(&looped.twin)];
    let Some(looped) = video.looped.as_mut() else {
        return;
    };
    let step = match input {
        Some(PanelPlay::Loop { now, opened }) => looped.course.step(now, opened, ready, readings),
        _ => looped.course.leave().unwrap_or_default(),
    };
    looped.show = step.show;
    apply_loop_step(&step, video);
}

fn reading_of(player: &HtmlVideoElement) -> PlayerReading {
    PlayerReading {
        time: player.current_time(),
        seeking: player.seeking(),
        paused: player.paused(),
    }
}

/// Переход проигрывателя на `time` секунд своего файла, не дальше его конца.
fn seek(player: &HtmlVideoElement, time: f64) {
    let end = player.duration();
    player.set_current_time(if end.is_finite() { time.min(end) } else { time });
}

/// Забывает отказ браузера играть: проигрыватель встал или ушёл на другое место, и просьбу можно повторить.
fn forget_refusal(request: &Rc<Cell<PlayRequest>>) {
    if request.get() == PlayRequest::Refused {
        request.set(PlayRequest::None);
    }
}

fn pause_player(player: &HtmlVideoElement, request: &Rc<Cell<PlayRequest>>) {
    let _ = player.pause();
    forget_refusal(request);
}

fn seek_player(player: &HtmlVideoElement, request: &Rc<Cell<PlayRequest>>, time: f64) {
    seek(player, time);
    forget_refusal(request);
}

/// Просьбу, на которую браузер ещё не ответил или отказал, не повторяют.
fn play_player(player: &HtmlVideoElement, request: &Rc<Cell<PlayRequest>>) {
    if request.get() == PlayRequest::None {
        request.set(PlayRequest::Pending);
        request_play(player, request);
    }
}

/// Выполняет шаг хода видео панели: пауза, переход на место, просьба играть.
fn apply_panel_step(step: &PanelStep, video: &Video) {
    let Some(panel) = &video.panel else {
        return;
    };
    let part = |roll: Roll| match roll {
        Roll::Forward => (&video.player, &video.play_request),
        Roll::Backward => (&panel.reverse, &panel.reverse_request),
    };
    if let Some(roll) = step.pause {
        let (player, request) = part(roll);
        pause_player(player, request);
    }
    if let Some((roll, time)) = step.seek {
        let (player, request) = part(roll);
        seek_player(player, request, time);
    }
    if let Some(roll) = step.play {
        let (player, request) = part(roll);
        play_player(player, request);
    }
}

/// Выполняет шаг хода видео по кругу с каждым из двух проигрывателей.
fn apply_loop_step(step: &LoopStep, video: &Video) {
    let Some(looped) = &video.looped else {
        return;
    };
    let players = [
        (&video.player, &video.play_request),
        (&looped.twin, &looped.twin_request),
    ];
    for (order, (player, request)) in step.orders.iter().zip(players) {
        if order.pause {
            pause_player(player, request);
        }
        if let Some(time) = order.seek {
            seek_player(player, request, time);
        }
        if order.play {
            play_player(player, request);
        }
    }
}
