use web_sys::HtmlCanvasElement;
use wgpu::util::DeviceExt;

use super::atlas::{self, ATLAS_SIZE};
use crate::core::screens::{Align, FontId};

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawRect {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    /// «Картинки» → «Атлас и отрисовка»: the rectangle in the atlas this instance samples, in
    /// atlas pixels — the shader divides by `ATLAS_SIZE` itself. A plain color fill uses
    /// `atlas::WHITE_PIXEL` here, stretched and multiplied by `color`, so a fill and an image go
    /// out through the very same instance and the very same draw call.
    pub atlas_pos: [f32; 2],
    pub atlas_size: [f32; 2],
}

/// One label or button caption to hand to `glyphon` this frame. `rect_px` is the element's
/// placement rectangle in the same screen (CSS) pixels `core::screens::Placement` resolves —
/// the renderer applies the device-pixel-ratio scale itself, the same as it does for `ui_rects`.
pub struct TextDraw {
    pub text: String,
    pub font: FontId,
    pub font_size_px: f32,
    pub color: [f32; 4],
    pub align: Align,
    pub rect_px: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    scale: [f32; 2],
    offset: [f32; 2],
    canvas_size_px: [f32; 2],
    _padding: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackend {
    WebGpu,
    WebGl2,
}

impl GpuBackend {
    pub fn label(self) -> &'static str {
        match self {
            GpuBackend::WebGpu => "webgpu",
            GpuBackend::WebGl2 => "webgl2",
        }
    }
}

fn cosmic_align(align: Align) -> glyphon::cosmic_text::Align {
    match align {
        Align::Left => glyphon::cosmic_text::Align::Left,
        Align::Center => glyphon::cosmic_text::Align::Center,
        Align::Right => glyphon::cosmic_text::Align::Right,
    }
}

fn glyphon_color(color: [f32; 4]) -> glyphon::Color {
    let to_u8 = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    glyphon::Color::rgba(
        to_u8(color[0]),
        to_u8(color[1]),
        to_u8(color[2]),
        to_u8(color[3]),
    )
}

/// Draws one frame in three passes over the same canvas: the world's colored rectangles (scene
/// cells, letterboxed into the window), the interface's panels and buttons (window pixels), and
/// the interface's text (`glyphon`, on top of both) — «Интерфейс игры» → «Отрисовка».
pub struct Renderer {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    rect_pipeline: wgpu::RenderPipeline,
    quad_buffer: wgpu::Buffer,

    world_globals_buffer: wgpu::Buffer,
    world_bind_group: wgpu::BindGroup,
    world_instance_buffer: wgpu::Buffer,
    world_instance_capacity: usize,

    ui_globals_buffer: wgpu::Buffer,
    ui_bind_group: wgpu::BindGroup,
    ui_instance_buffer: wgpu::Buffer,
    ui_instance_capacity: usize,

    /// «Картинки» → «Атлас и отрисовка»: one `ATLAS_SIZE`×`ATLAS_SIZE` texture, allocated once
    /// here and never resized — `build_atlas` only ever rewrites its contents, so neither bind
    /// group above ever needs recreating after this constructor returns.
    atlas_texture: wgpu::Texture,

    font_system: glyphon::FontSystem,
    swash_cache: glyphon::SwashCache,
    text_atlas: glyphon::TextAtlas,
    text_viewport: glyphon::Viewport,
    text_renderer: glyphon::TextRenderer,
    /// `FontId` (index into this table) → the family name `cosmic-text` shaping needs, resolved
    /// from the font file itself when it was loaded — see `load_font`.
    fonts: Vec<String>,

    background: [f32; 4],
    backend: GpuBackend,

    canvas_size_px: [f32; 2],
    scene_cells: [f32; 2],
    device_pixel_ratio: f32,
}

impl Renderer {
    pub async fn new(
        canvas: HtmlCanvasElement,
        width_px: u32,
        height_px: u32,
        scene_width: u32,
        scene_height: u32,
        background: [f32; 4],
    ) -> Result<Renderer, String> {
        // Deciding between WebGPU and WebGL2 has to happen before Instance::new (a sync call),
        // so wgpu's own helper probes `navigator.gpu` first and drops BROWSER_WEBGPU from the
        // descriptor when it is not there — this is the documented way to get the fallback.
        let instance_desc = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        };
        let instance = wgpu::util::new_instance_with_webgpu_detection(instance_desc).await;

        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| format!("не удалось создать поверхность отрисовки: {e}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| {
                "WebGPU и WebGL2 недоступны в этом браузере: рисовать нечем".to_string()
            })?;

        let backend = match adapter.get_info().backend {
            wgpu::Backend::BrowserWebGpu => GpuBackend::WebGpu,
            _ => GpuBackend::WebGl2,
        };

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("kuznya-mirov device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("не удалось создать устройство видеокарты: {e}"))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .first()
            .copied()
            .ok_or_else(|| "нет поддерживаемых форматов поверхности".to_string())?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width_px.max(1),
            height: height_px.max(1),
            present_mode: caps
                .present_modes
                .first()
                .copied()
                .unwrap_or(wgpu::PresentMode::Fifo),
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/rect.wgsl").into()),
        });

        // «Картинки» → «Атлас и отрисовка»: one fixed-size texture and sampler, shared by both
        // passes' bind groups — `Rgba8Unorm` carries alpha without premultiplying it, `Nearest`
        // keeps a scaled-up sprite blocky rather than smoothed, `ClampToEdge` keeps a stretched
        // fill from bleeding into a neighboring atlas entry.
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let canvas_size_px = [width_px.max(1) as f32, height_px.max(1) as f32];
        let scene_cells = [scene_width.max(1) as f32, scene_height.max(1) as f32];
        let world_globals = world_globals_data(canvas_size_px, scene_cells);
        let world_globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world_globals"),
            contents: bytemuck::bytes_of(&world_globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let world_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world_globals_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: world_globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        let ui_globals = ui_globals_data(canvas_size_px, 1.0);
        let ui_globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui_globals"),
            contents: bytemuck::bytes_of(&ui_globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let ui_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui_globals_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ui_globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        };
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<DrawRect>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 8,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 3,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 32,
                    shader_location: 4,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 40,
                    shader_location: 5,
                },
            ],
        };

        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rect_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout, instance_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let quad_vertices: [[f32; 2]; 6] = [
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ];
        let quad_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("unit_quad"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let world_instance_capacity = 1024usize;
        let world_instance_buffer = make_instance_buffer(&device, world_instance_capacity);
        let ui_instance_capacity = 256usize;
        let ui_instance_buffer = make_instance_buffer(&device, ui_instance_capacity);

        let text_cache = glyphon::Cache::new(&device);
        let mut text_atlas = glyphon::TextAtlas::new(&device, &queue, &text_cache, format);
        let text_viewport = glyphon::Viewport::new(&device, &text_cache);
        let text_renderer = glyphon::TextRenderer::new(
            &mut text_atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );

        Ok(Renderer {
            _instance: instance,
            surface,
            device,
            queue,
            config,
            rect_pipeline,
            quad_buffer,
            world_globals_buffer,
            world_bind_group,
            world_instance_buffer,
            world_instance_capacity,
            ui_globals_buffer,
            ui_bind_group,
            ui_instance_buffer,
            ui_instance_capacity,
            atlas_texture,
            font_system: glyphon::FontSystem::new(),
            swash_cache: glyphon::SwashCache::new(),
            text_atlas,
            text_viewport,
            text_renderer,
            fonts: Vec::new(),
            background,
            backend,
            canvas_size_px,
            scene_cells,
            device_pixel_ratio: 1.0,
        })
    }

    pub fn backend(&self) -> GpuBackend {
        self.backend
    }

    /// The window size in CSS pixels — the unit `screens.json`'s `anchor`/`offset`/`size` are
    /// written in, and what mouse hit-testing has to use as its viewport.
    pub fn window_size_css(&self) -> [f32; 2] {
        [
            self.canvas_size_px[0] / self.device_pixel_ratio,
            self.canvas_size_px[1] / self.device_pixel_ratio,
        ]
    }

    /// Loads one font's bytes into `cosmic-text`'s shared database and returns the `FontId`
    /// later `TextDraw`s reference. The family name shaping needs comes from the file itself —
    /// the caller's name for it (from `files.fonts`) is not a font-internal name, so this reads
    /// it back from the face `load_font_source` reports having just added.
    pub fn load_font(&mut self, bytes: Vec<u8>) -> FontId {
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(glyphon::fontdb::Source::Binary(std::sync::Arc::new(bytes)));
        let family = ids
            .first()
            .and_then(|&id| self.font_system.db().face(id))
            .and_then(|face| face.families.first())
            .map(|(name, _)| name.clone())
            .unwrap_or_default();
        let font_id = self.fonts.len();
        self.fonts.push(family);
        font_id
    }

    /// «Картинки» → «Атлас и отрисовка»: packs `images` (already checked, one whole strip per
    /// declared picture) into the fixed-size atlas and uploads it — one texture write, no bind
    /// group ever needs recreating. Returns the packed rectangles, indexed the same way `images`
    /// was — i.e. by `ImageId`; the packed bytes themselves are dropped right after the upload.
    pub fn build_atlas(
        &mut self,
        images: &[atlas::AtlasImage],
    ) -> Result<Vec<atlas::AtlasRect>, String> {
        let packed = atlas::pack(images)?;
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &packed.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_SIZE * 4),
                rows_per_image: Some(ATLAS_SIZE),
            },
            wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
        );
        Ok(packed.rects)
    }

    /// The game's scene size and background become known only once it has loaded, well after
    /// the GPU itself was set up — this rewrites both without touching anything else.
    pub fn set_scene(&mut self, scene_width: u32, scene_height: u32, background: [f32; 4]) {
        self.scene_cells = [scene_width.max(1) as f32, scene_height.max(1) as f32];
        self.background = background;
        self.write_world_globals();
    }

    /// Reconfigures the GPU surface for a new device-pixel canvas size. Call on resize; does
    /// not touch the game. `set_pixel_ratio` supplies the CSS/device-pixel split separately.
    pub fn resize(&mut self, width_px: u32, height_px: u32) {
        self.config.width = width_px.max(1);
        self.config.height = height_px.max(1);
        self.surface.configure(&self.device, &self.config);
        self.canvas_size_px = [self.config.width as f32, self.config.height as f32];
        self.write_world_globals();
        self.write_ui_globals();
        self.text_viewport.update(
            &self.queue,
            glyphon::Resolution {
                width: self.config.width,
                height: self.config.height,
            },
        );
    }

    /// «Интерфейс игры» → «Раскладка»: the ratio the interface's own pixel values get
    /// multiplied by; the world pass never needs it, since `canvas_size_px` already is device
    /// pixels regardless of density.
    pub fn set_pixel_ratio(&mut self, ratio: f32) {
        self.device_pixel_ratio = ratio.max(0.01);
        self.write_ui_globals();
    }

    fn write_world_globals(&mut self) {
        let globals = world_globals_data(self.canvas_size_px, self.scene_cells);
        self.queue
            .write_buffer(&self.world_globals_buffer, 0, bytemuck::bytes_of(&globals));
    }

    fn write_ui_globals(&mut self) {
        let globals = ui_globals_data(self.canvas_size_px, self.device_pixel_ratio);
        self.queue
            .write_buffer(&self.ui_globals_buffer, 0, bytemuck::bytes_of(&globals));
    }

    fn grow_world_instances(&mut self, needed: usize) {
        if needed <= self.world_instance_capacity {
            return;
        }
        self.world_instance_capacity = needed.next_power_of_two();
        self.world_instance_buffer =
            make_instance_buffer(&self.device, self.world_instance_capacity);
    }

    fn grow_ui_instances(&mut self, needed: usize) {
        if needed <= self.ui_instance_capacity {
            return;
        }
        self.ui_instance_capacity = needed.next_power_of_two();
        self.ui_instance_buffer = make_instance_buffer(&self.device, self.ui_instance_capacity);
    }

    fn acquire_frame(&mut self) -> Result<Option<wgpu::SurfaceTexture>, String> {
        use wgpu::CurrentSurfaceTexture as T;
        match self.surface.get_current_texture() {
            T::Success(tex) | T::Suboptimal(tex) => Ok(Some(tex)),
            T::Timeout | T::Occluded => Ok(None),
            T::Outdated | T::Lost => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    T::Success(tex) | T::Suboptimal(tex) => Ok(Some(tex)),
                    _ => Err("поверхность недоступна после пересоздания".to_string()),
                }
            }
            T::Validation => Err("ошибка проверки при получении текстуры поверхности".to_string()),
        }
    }

    /// One `cosmic-text` buffer per `TextDraw`, shaped to fit its element and clipped to it —
    /// «Интерфейс игры» → «Текст»: single line, no wrapping, aligned inside the element.
    fn build_text_buffers(
        &mut self,
        texts: &[TextDraw],
    ) -> Vec<(glyphon::Buffer, usize, [f32; 4], [f32; 4])> {
        let dpr = self.device_pixel_ratio;
        texts
            .iter()
            .map(|t| {
                let font_size_px = (t.font_size_px * dpr).max(1.0);
                let metrics = glyphon::Metrics::new(font_size_px, font_size_px * 1.2);
                let mut buffer = glyphon::Buffer::new(&mut self.font_system, metrics);
                buffer.set_wrap(&mut self.font_system, glyphon::cosmic_text::Wrap::None);
                let rect_px = [
                    t.rect_px[0] * dpr,
                    t.rect_px[1] * dpr,
                    t.rect_px[2] * dpr,
                    t.rect_px[3] * dpr,
                ];
                buffer.set_size(&mut self.font_system, Some(rect_px[2]), None);
                let family = self.fonts.get(t.font).map(String::as_str).unwrap_or("");
                let attrs = glyphon::Attrs::new().family(glyphon::Family::Name(family));
                buffer.set_text(
                    &mut self.font_system,
                    &t.text,
                    &attrs,
                    glyphon::Shaping::Advanced,
                    Some(cosmic_align(t.align)),
                );
                (buffer, t.font, rect_px, t.color)
            })
            .collect()
    }

    /// Draws one frame: `world_instances` (scene cells, letterboxed) and `ui_instances` (window
    /// pixels) each go out as one instanced draw call, then `texts` uploads to the GPU as a
    /// single `glyphon` command drawn on top of both — «Интерфейс игры» → «Отрисовка».
    pub fn render_frame(
        &mut self,
        world_instances: &[DrawRect],
        ui_instances: &[DrawRect],
        texts: &[TextDraw],
    ) -> Result<(), String> {
        self.grow_world_instances(world_instances.len());
        if !world_instances.is_empty() {
            self.queue.write_buffer(
                &self.world_instance_buffer,
                0,
                bytemuck::cast_slice(world_instances),
            );
        }
        self.grow_ui_instances(ui_instances.len());
        if !ui_instances.is_empty() {
            self.queue.write_buffer(
                &self.ui_instance_buffer,
                0,
                bytemuck::cast_slice(ui_instances),
            );
        }

        let buffers = self.build_text_buffers(texts);
        let text_areas: Vec<glyphon::TextArea> = buffers
            .iter()
            .map(|(buffer, _font, rect_px, color)| {
                let line_height = buffer.metrics().line_height;
                glyphon::TextArea {
                    buffer,
                    left: rect_px[0],
                    top: rect_px[1] + (rect_px[3] - line_height) / 2.0,
                    scale: 1.0,
                    bounds: glyphon::TextBounds {
                        left: rect_px[0] as i32,
                        top: rect_px[1] as i32,
                        right: (rect_px[0] + rect_px[2]) as i32,
                        bottom: (rect_px[1] + rect_px[3]) as i32,
                    },
                    default_color: glyphon_color(*color),
                    custom_glyphs: &[],
                }
            })
            .collect();
        if !text_areas.is_empty() {
            self.text_renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.font_system,
                    &mut self.text_atlas,
                    &self.text_viewport,
                    text_areas,
                    &mut self.swash_cache,
                )
                .map_err(|e| format!("не удалось подготовить текст: {e:?}"))?;
        }

        let Some(surface_texture) = self.acquire_frame()? else {
            return Ok(()); // occluded/timeout: skip this frame, try again next tick
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear_and_draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.background[0] as f64,
                            g: self.background[1] as f64,
                            b: self.background[2] as f64,
                            a: self.background[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
            if !world_instances.is_empty() {
                pass.set_bind_group(0, &self.world_bind_group, &[]);
                pass.set_vertex_buffer(1, self.world_instance_buffer.slice(..));
                pass.draw(0..6, 0..world_instances.len() as u32);
            }
            if !ui_instances.is_empty() {
                pass.set_bind_group(0, &self.ui_bind_group, &[]);
                pass.set_vertex_buffer(1, self.ui_instance_buffer.slice(..));
                pass.draw(0..6, 0..ui_instances.len() as u32);
            }
            if !texts.is_empty() {
                self.text_renderer
                    .render(&self.text_atlas, &self.text_viewport, &mut pass)
                    .map_err(|e| format!("не удалось нарисовать текст: {e:?}"))?;
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        Ok(())
    }
}

fn make_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("instances"),
        size: (capacity * std::mem::size_of::<DrawRect>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// World pass: scene cells scale up to fill the largest letterboxed rectangle of `canvas_size_px`
/// that keeps the scene's aspect ratio, centered — «Формат игры»: «сцена вписывается в окно
/// целиком, с сохранением пропорций».
fn world_globals_data(canvas_size_px: [f32; 2], scene_cells: [f32; 2]) -> Globals {
    let scale = (canvas_size_px[0] / scene_cells[0]).min(canvas_size_px[1] / scene_cells[1]);
    let viewport_size = [scene_cells[0] * scale, scene_cells[1] * scale];
    let offset = [
        (canvas_size_px[0] - viewport_size[0]) / 2.0,
        (canvas_size_px[1] - viewport_size[1]) / 2.0,
    ];
    Globals {
        scale: [scale, scale],
        offset,
        canvas_size_px,
        _padding: [0.0, 0.0],
    }
}

/// UI pass: window pixels (as `screens.json` and `core::screens::Placement` use them) scale up
/// by the device pixel ratio to the canvas's own device pixels — «Интерфейс игры» → «Раскладка».
fn ui_globals_data(canvas_size_px: [f32; 2], device_pixel_ratio: f32) -> Globals {
    Globals {
        scale: [device_pixel_ratio, device_pixel_ratio],
        offset: [0.0, 0.0],
        canvas_size_px,
        _padding: [0.0, 0.0],
    }
}
