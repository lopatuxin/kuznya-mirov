//! «Картинки» → «Видео»: браузерные проигрыватели видео игры и перенос их кадров в атлас. Перед
//! отрисовкой кадра текущий кадр каждого проигрывателя копируется в текстуру файла видео, и проход
//! `shaders/video.wgsl` пишет его в место видео в слое атласа. Всё остальное в отрисовке видео от
//! картинки не отличает.

use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use web_sys::{HtmlMediaElement, HtmlVideoElement};

use super::atlas::AtlasRect;
use super::gpu::ATLAS_FORMAT;
use super::video_play::{
    self, LoopCourse, LoopStep, PanelCourse, PanelStep, PlayRequest, PlayerAction, PlayerReading,
    Roll, Slot,
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
    /// «Видео на панели»: `hovering` — курсор над местом наведения панели, `rest_at` — секунда ролика с
    /// позой последнего кадра.
    Hover {
        hovering: bool,
        rest_at: Option<f32>,
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
}

struct VideoPass {
    pipeline: wgpu::RenderPipeline,
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
            .filter(|(video, player)| video.has_frame(player))
            .collect();
        if ready.is_empty() {
            return;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("video_frames"),
        });
        for (video, player) in ready {
            video.copy_frame(queue, player);
            video.encode_into_atlas(&mut encoder, pass);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    fn drive_panels(&mut self) {
        for video in &mut self.items {
            let input = self
                .panel_inputs
                .iter()
                .find(|input| input.image == video.image)
                .map(|input| input.play);
            if video.looped.is_some() {
                drive_loop(video, input);
                continue;
            }
            let Some(panel) = &mut video.panel else {
                continue;
            };
            let Some(PanelPlay::Hover { hovering, rest_at }) = input else {
                panel.show = None;
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
            apply_panel_step(&step, video);
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
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("video_pipeline"),
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
                    blend: None,
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
        });
        VideoPass { pipeline, layout }
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
        }
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
