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

/// Видео игры, как его отдаёт загрузка: проигрыватель, размер файла (оба слоя вместе) и место одного
/// кадра в атласе.
pub struct VideoSource {
    pub player: HtmlVideoElement,
    pub width: u32,
    pub height: u32,
    pub place: AtlasRect,
}

/// Что стало с последней просьбой играть. Пока браузер не ответил, новая не шлётся; после отказа
/// видео стоит, пока часы кадров не встанут и не пойдут снова.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlayRequest {
    None,
    Pending,
    Refused,
}

struct Video {
    player: HtmlVideoElement,
    play_request: Rc<Cell<PlayRequest>>,
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
}

impl Videos {
    /// Заменяет видео игры на `sources`; прежние останавливаются. `atlas` — текстура атласа, в слои
    /// которой пишет проход.
    pub fn set(&mut self, device: &wgpu::Device, atlas: &wgpu::Texture, sources: Vec<VideoSource>) {
        self.release();
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
        }
    }

    /// Раз в отрисовку: кадр каждого видео, у которого он уже есть, копируется в текстуру файла и
    /// пишется в атлас. Видео без кадра остаётся прозрачным.
    pub fn refresh(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let Some(pass) = &self.pass else {
            return;
        };
        let ready: Vec<&Video> = self
            .items
            .iter()
            .filter(|video| video.has_frame())
            .collect();
        if ready.is_empty() {
            return;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("video_frames"),
        });
        for video in ready {
            video.copy_frame(queue);
            video.encode_into_atlas(&mut encoder, pass);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Часы кадров идут (`playing`) — проигрыватели играют, стоят — на паузе. Браузер мог
    /// остановить видео сам (скрытая вкладка): на первом кадре с идущими часами оно запускается снова.
    /// Отказ браузера играть — не ошибка, видео стоит на текущем кадре; снова его просят играть, когда
    /// часы встанут и пойдут.
    pub fn set_playing(&self, playing: bool) {
        for video in &self.items {
            let paused = video.player.paused();
            if playing && paused {
                video.start();
            } else if !playing {
                if !paused {
                    let _ = video.player.pause();
                }
                if video.play_request.get() == PlayRequest::Refused {
                    video.play_request.set(PlayRequest::None);
                }
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
        Video {
            player: source.player,
            play_request: Rc::new(Cell::new(PlayRequest::None)),
            width: source.width,
            height: source.height,
            place: source.place,
            file,
            file_bind_group,
            place_layer,
        }
    }

    /// Проигрыватель уже отдаёт кадр своего размера. Копировать видео без кадра браузер не даёт —
    /// `wgpu` на WebGPU упал бы на этом.
    fn has_frame(&self) -> bool {
        self.player.ready_state() >= HtmlMediaElement::HAVE_CURRENT_DATA
            && self.player.video_width() == self.width
            && self.player.video_height() == self.height
    }

    fn copy_frame(&self, queue: &wgpu::Queue) {
        queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source: wgpu::ExternalImageSource::HTMLVideoElement(self.player.clone()),
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

    /// Просьба играть, если прошлая не ждёт ответа и не получила отказ. Ответ браузера дожидается,
    /// чтобы отказ не стал необработанным и запомнился.
    fn start(&self) {
        if self.play_request.get() != PlayRequest::None {
            return;
        }
        let Ok(promise) = self.player.play() else {
            self.play_request.set(PlayRequest::Refused);
            return;
        };
        self.play_request.set(PlayRequest::Pending);
        let play_request = Rc::clone(&self.play_request);
        wasm_bindgen_futures::spawn_local(async move {
            let answer = match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => PlayRequest::None,
                // Просьбу прервала пауза самого движка: часы встали, пока браузер готовился играть.
                Err(error)
                    if error
                        .dyn_ref::<js_sys::Error>()
                        .is_some_and(|error| error.name() == "AbortError") =>
                {
                    PlayRequest::None
                }
                Err(_) => PlayRequest::Refused,
            };
            play_request.set(answer);
        });
    }
}
