use web_sys::HtmlCanvasElement;
use wgpu::util::DeviceExt;

use super::atlas::{self, ATLAS_SIZE};
use super::gpu_models::ModelsGpu;
use super::gpu3d;
use super::materials::Relief;
use super::text_area;
use super::video::{VideoSource, Videos};
use super::wind;
use crate::core::model::ModelTable;
use crate::core::screens::{Align, FontId};
use crate::core::terrain::Cover;
use crate::data::load::{CoverMask, FontSettings, MaterialDecl};

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawRect {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    /// «Картинки» → «Атлас»: the rectangle in the atlas this instance samples, in that sheet's
    /// own pixels — the shader divides by `ATLAS_SIZE` itself. A plain color fill uses
    /// `atlas::WHITE_PIXEL` here, stretched and multiplied by `color`, so a fill and an image go
    /// out through the very same instance and the very same draw call.
    pub atlas_pos: [f32; 2],
    pub atlas_size: [f32; 2],
    /// «Картинки», требование 24: quarter turns (0–3) the sampled image is rotated clockwise
    /// before it stretches to fill `position`/`size` — the interface's own rectangles (panels,
    /// buttons) always pass `0.0` here; only a world object's `rotation` ever sets it.
    pub rotation_quarters: f32,
    /// «Картинки» → «Атлас», требование 14: which layer of the `D2Array` atlas texture to sample
    /// — `atlas::AtlasRect::sheet`, carried straight through.
    pub atlas_layer: f32,
    /// «Картинки» → «Сглаживание»: `0.0`/`1.0` — the fragment shader samples this instance both as
    /// a texel fetch (crisp) and through the linear sampler (smoothed) and picks between them with
    /// `select`, never a branch (a texture sample needs the same path for every fragment in a
    /// quad). WGSL's own field for this is `smooth_flag`, not `smooth` — `smooth` is reserved.
    pub smooth: f32,
    /// «Картинки» → «Отражение», требование 9: `0.0`/`1.0` — mirrors the sampled point in the
    /// image's own axes, after the rotation above has already picked which corner maps where.
    pub flip_x: f32,
    /// «Картинки», «Таблица картинок»: `0.0`/`1.0` — светящаяся картинка прибавляет свет к тому, что
    /// под ней: фрагмент отдаёт `(цвет, 0)`.
    pub glow: f32,
    /// «Ветер и частицы» → «Качание», требование 14: на сколько клеток вбок ушёл верх прямоугольника;
    /// вершинный шейдер гнёт его по дуге по восьми полосам.
    pub lean: f32,
    /// «Ветер и частицы» → «Частица», требование 13: поворот вокруг середины прямоугольника на любой
    /// угол, градусов по часовой стрелке; 0 — не повёрнут.
    pub angle: f32,
    /// «Огонь»: `1.0` — экземпляр рисует пламя, `2.0` — угли, а не картинку: узор, форму языков и куч и
    /// полосы цвета считает фрагментный шейдер, цвет экземпляра — `fire_color`; у всего остального `0.0`.
    pub flame: f32,
    /// «Огонь»: `[сила (у углей — накал), зерно узора, целая часть хода узора, дробная часть]`; нули у
    /// всего, кроме пламени и углей.
    pub fire: [f32; 4],
    /// «Картинки» → «Растягивание рамки»: `[x, y, ширина, высота]` кадра в точках листа, к краю которого
    /// прижимается выборка; у всего, кроме кусков рамки, — тот же прямоугольник, что `atlas_pos`/`atlas_size`.
    pub clamp_rect: [f32; 4],
}

/// Интерфейс кадра: прямоугольники и надписи элементов экрана, а над ними, поверх текста всех элементов, —
/// слой подсказки → «Интерфейс игры» → «Отрисовка».
#[derive(Default)]
pub struct UiLayers {
    pub rects: Vec<DrawRect>,
    pub texts: Vec<TextDraw>,
    pub tooltip_rects: Vec<DrawRect>,
    pub tooltip_texts: Vec<TextDraw>,
}

/// Шрифт игры, как его просит раскладка текста: семейство из файла и настройки из `files.fonts`.
struct FontFace {
    family: String,
    weight: glyphon::Weight,
    stretch: glyphon::Stretch,
    features: glyphon::cosmic_text::FontFeatures,
}

/// The text attributes `font` is shaped with; an unknown `FontId` falls back to the default face.
fn font_attrs(fonts: &[FontFace], font: FontId) -> glyphon::Attrs<'_> {
    match fonts.get(font) {
        Some(face) => glyphon::Attrs::new()
            .family(glyphon::Family::Name(&face.family))
            .weight(face.weight)
            .stretch(face.stretch)
            .font_features(face.features.clone()),
        None => glyphon::Attrs::new().family(glyphon::Family::Name("")),
    }
}

/// «Текст»: ширина в процентах обычной — шаг CSS `font-stretch`, проверенный при загрузке.
fn font_stretch(width: f32) -> glyphon::Stretch {
    use glyphon::Stretch;
    match width {
        50.0 => Stretch::UltraCondensed,
        62.5 => Stretch::ExtraCondensed,
        75.0 => Stretch::Condensed,
        87.5 => Stretch::SemiCondensed,
        112.5 => Stretch::SemiExpanded,
        125.0 => Stretch::Expanded,
        150.0 => Stretch::ExtraExpanded,
        200.0 => Stretch::UltraExpanded,
        _ => Stretch::Normal,
    }
}

/// One label or button caption to hand to `glyphon` this frame. `rect_px` is the element's
/// placement rectangle in the same `screens.json` points `core::screens::Placement` resolves —
/// the renderer applies the device-pixel-ratio and interface scale itself, the same as it does for
/// `ui_rects`.
pub struct TextDraw {
    pub text: String,
    pub font: FontId,
    pub font_size_px: f32,
    pub color: [f32; 4],
    pub align: Align,
    /// «Текст»: перенос по словам по ширине `rect_px`, строки сверху; без него — одна строка посередине по
    /// высоте.
    pub wrap: bool,
    pub rect_px: [f32; 4],
}

/// One label in the world to hand to `glyphon` this frame — «Надписи и полоски в мире», требование
/// 17: `rect_cells`/`font_size_cells` are in scene cells, the same unit `core::world_elements`
/// computes in; the renderer converts them to device pixels itself, through the same
/// `world_scale`/`world_offset` the world rectangle pass already uses (`set_world_frame`), so a
/// world label always lands exactly where its object's own rectangle does.
pub struct WorldTextDraw {
    pub text: String,
    pub font: FontId,
    pub font_size_cells: f32,
    pub color: [f32; 4],
    pub align: Align,
    pub rect_cells: [f32; 4],
}

/// Один отрезок слоя мира в порядке рисования — «Надписи, полоски и картинки в мире», требование
/// 5: прямоугольники `WorldOverlay::instances` или надписи `WorldOverlay::texts`, по номерам.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldRun {
    Rects(std::ops::Range<usize>),
    Texts(std::ops::Range<usize>),
}

/// Всё, что кадр рисует между миром и интерфейсом: прямоугольники мира (в клетках сцены, у
/// трёхмерной — в точках окна) и надписи в мире, а `runs` говорит, в каком порядке их чередовать:
/// картинка, стоящая в списке `world_elements` после надписи, рисуется поверх неё.
#[derive(Default)]
pub struct WorldOverlay {
    pub instances: Vec<DrawRect>,
    pub texts: Vec<WorldTextDraw>,
    pub runs: Vec<WorldRun>,
}

impl WorldOverlay {
    /// Только прямоугольники — кадр без надписей и картинок в мире.
    pub fn of_rects(instances: Vec<DrawRect>) -> WorldOverlay {
        WorldOverlay {
            runs: vec![WorldRun::Rects(0..instances.len())],
            instances,
            texts: Vec::new(),
        }
    }
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

/// A no-op [`wgpu::rwh::HasDisplayHandle`] for `InstanceDescriptor::display` — `WebDisplayHandle`
/// carries no fields, so nothing here can dangle or mismatch; it only tags the handle as "web" so
/// `wgpu-core`'s `create_surface` (the WebGL2/GLES path) has a display to pair with the canvas's
/// window handle instead of rejecting the surface as `MissingDisplayHandle`. The `SurfaceTarget::
/// Canvas` helper always passes `raw_display_handle: None`, so this is never compared against a
/// different handle — see `wgpu-core` 29.0.4 `instance.rs`, `Instance::create_surface`.
#[derive(Debug)]
struct WebDisplay;

impl wgpu::rwh::HasDisplayHandle for WebDisplay {
    fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
        let raw = wgpu::rwh::RawDisplayHandle::Web(wgpu::rwh::WebDisplayHandle::new());
        // SAFETY: `WebDisplayHandle` has no fields to invalidate; there is nothing to uphold.
        Ok(unsafe { wgpu::rwh::DisplayHandle::borrow_raw(raw) })
    }
}

/// Draws one frame in five layers over the same canvas, back to front: the world's colored
/// rectangles (scene cells, letterboxed into the window — a world element's own bar rectangles go
/// out through this same pass), the world's own label text (`glyphon`, a separate pass — «Надписи
/// и полоски в мире»), the interface's panels and buttons (window pixels), and the interface's text
/// (`glyphon`, on top of everything) — «Интерфейс игры» → «Отрисовка».
pub struct Renderer {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    rect_pipeline: wgpu::RenderPipeline,
    strip_buffer: wgpu::Buffer,

    world_globals_buffer: wgpu::Buffer,
    world_bind_group: wgpu::BindGroup,
    world_instance_buffer: wgpu::Buffer,
    world_instance_capacity: usize,

    ui_globals_buffer: wgpu::Buffer,
    ui_bind_group: wgpu::BindGroup,
    ui_instance_buffer: wgpu::Buffer,
    ui_instance_capacity: usize,

    /// «Картинки» → «Атлас», требование 14: a `D2` texture with one layer per atlas sheet,
    /// sampled through a `D2Array` view — `build_atlas` recreates it (and both bind groups above,
    /// which reference its view) whenever the layer count a game needs changes, since a texture's
    /// own layer count is fixed at creation; it never grows or shrinks in place otherwise.
    atlas_texture: wgpu::Texture,
    /// The layer count `atlas_texture` currently has — compared against what the next
    /// `build_atlas` needs before deciding whether to recreate it.
    atlas_layers: u32,
    /// Shared by `world_bind_group`/`ui_bind_group`; kept so `build_atlas` can rebuild either one
    /// after recreating `atlas_texture`, without re-describing the layout every time.
    atlas_bind_group_layout: wgpu::BindGroupLayout,
    /// «Картинки» → «Сглаживание»: the atlas's crisp path is a texel fetch (no sampler — see the
    /// shader's own binding comment), so only the smoothed path needs one; `ClampToEdge` keeps a
    /// stretched fill from bleeding into a neighboring atlas entry.
    linear_sampler: wgpu::Sampler,

    font_system: glyphon::FontSystem,
    swash_cache: glyphon::SwashCache,
    text_cache: glyphon::Cache,
    text_atlas: glyphon::TextAtlas,
    text_viewport: glyphon::Viewport,
    text_renderer: glyphon::TextRenderer,
    /// «Надписи и полоски в мире», требование 14: `TextRenderer`s sharing the same atlas/cache/viewport
    /// as `text_renderer` — glyphon's own renderer holds one prepared batch at a time, so a world-text
    /// pass drawn *between* the world and interface rectangle passes (this module's own doc comment on
    /// `render_frame`) needs its own instance rather than a second `prepare`/`render` on `text_renderer`,
    /// which would just overwrite the interface's batch. «Картинки в мире», требование 5: по одному на
    /// каждый отрезок надписей кадра (`WorldRun::Texts`), растут по мере надобности.
    world_text_renderers: Vec<glyphon::TextRenderer>,
    /// «Подсказка»: свой рендерер текста — подсказка рисуется поверх текста всех элементов экрана.
    tooltip_text_renderer: glyphon::TextRenderer,
    /// `FontId` (index into this table) → the family name `cosmic-text` shaping needs, resolved
    /// from the font file itself when it was loaded — see `load_font_file` — and the font's own
    /// width, weight and OpenType features from `files.fonts`.
    fonts: Vec<FontFace>,

    background: [f32; 4],
    backend: GpuBackend,

    canvas_size_px: [f32; 2],
    /// «Камера»: масштаб и сдвиг мировой раскладки — считает не рендерер, а движковый слой
    /// (`core::game::Game::camera_frame`, вне партии — прежнее вписывание сцены целиком),
    /// `set_world_frame` их только принимает и пишет в буфер. `set_scene`/`resize` держат тут
    /// временную заглушку (то же вписывание целиком) до первого настоящего кадра.
    world_scale: f32,
    world_offset: [f32; 2],
    scene_cells: [f32; 2],
    device_pixel_ratio: f32,
    /// «Раскладка экрана»: масштаб интерфейса от `ui_reference`; перемножается с плотностью экрана.
    ui_scale: f32,
    /// «Трёхмерная сцена»: ресурсы создаются при первом её кадре; сброшены, когда атлас получил новую
    /// текстуру.
    scene3d: Option<gpu3d::Scene3d>,
    /// «Свет и материалы»: материалы и маски покрытий игры; без них — пустые текстуры.
    materials: gpu3d::MaterialGpu,
    material_options: gpu3d::MaterialOptions,
    /// «Трёхмерная сцена» → «Модели и движения»: модели игры и буфер их объектов в кадре.
    models: ModelsGpu,
    /// «Картинки» → «Видео»: проигрыватели видео игры; их кадры каждую отрисовку пишутся в места
    /// видео в `atlas_texture`.
    videos: Videos,
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
            ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(WebDisplay))
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

        let material_options = gpu3d::MaterialOptions {
            webgl2: backend == GpuBackend::WebGl2,
            anisotropic: adapter
                .get_downlevel_capabilities()
                .flags
                .contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING),
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

        // Shaders and glyphon (`ColorMode::Web`) write colors already in sRGB, so the surface must
        // not encode them again; wgpu-core lists sRGB formats first for the WebGL2 surface.
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|format| !format.is_srgb())
            .or_else(|| caps.formats.first())
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

        // «Картинки» → «Атлас», требования 14, 18–19: one `D2Array` texture and one sampler,
        // shared by both passes' bind groups — `Rgba8Unorm` carries alpha without premultiplying
        // it a second time (the atlas's own pixels already are, see `atlas::blit`); the crisp path
        // is a texel fetch (no sampler, see the shader's own binding comment), `Linear` smooths
        // the other, `ClampToEdge` keeps a stretched fill from bleeding into a neighboring atlas
        // entry.
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas_sampler_linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let atlas_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                            view_dimension: wgpu::TextureViewDimension::D2Array,
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

        // «Стек и инфраструктура» → «Ловушка WebGL2»: a game that only needs one sheet still
        // starts with a safe layer count for the backend it's actually drawing through — `load()`
        // hasn't called `build_atlas` yet at this point, so this is a placeholder exactly like
        // `world_scale`/`world_offset` below, replaced by the game's own real need on the first
        // `build_atlas`.
        let atlas_layers = match backend {
            GpuBackend::WebGpu => 1,
            GpuBackend::WebGl2 => atlas::webgl2_safe_layer_count(1),
        };
        let atlas_texture = create_atlas_texture(&device, atlas_layers);
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let canvas_size_px = [width_px.max(1) as f32, height_px.max(1) as f32];
        let scene_cells = [scene_width.max(1) as f32, scene_height.max(1) as f32];
        let (world_scale, world_offset) =
            crate::core::scene::letterbox(canvas_size_px, scene_cells);
        let world_globals = world_globals_data(canvas_size_px, world_scale, world_offset);
        let world_globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world_globals"),
            contents: bytemuck::bytes_of(&world_globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let world_bind_group = make_atlas_bind_group(
            &device,
            &atlas_bind_group_layout,
            &world_globals_buffer,
            &atlas_view,
            &linear_sampler,
            "world_globals_bind_group",
        );

        let ui_globals = ui_globals_data(canvas_size_px, 1.0);
        let ui_globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui_globals"),
            contents: bytemuck::bytes_of(&ui_globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let ui_bind_group = make_atlas_bind_group(
            &device,
            &atlas_bind_group_layout,
            &ui_globals_buffer,
            &atlas_view,
            &linear_sampler,
            "ui_globals_bind_group",
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect_pipeline_layout"),
            bind_group_layouts: &[Some(&atlas_bind_group_layout)],
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
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 48,
                    shader_location: 6,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 52,
                    shader_location: 7,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 56,
                    shader_location: 8,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 60,
                    shader_location: 9,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 64,
                    shader_location: 10,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 68,
                    shader_location: 11,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 72,
                    shader_location: 12,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 76,
                    shader_location: 13,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 80,
                    shader_location: 14,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 96,
                    shader_location: 15,
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
                    // «Картинки» → «Сглаживание», требование 5: the atlas's own points are
                    // already color-times-alpha (`atlas::blit`), so blending must not multiply by
                    // source alpha a second time — that's exactly what a non-premultiplied blend
                    // does, and it's what painted the dark/colored fringe requirement 5 rules out.
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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

        let strip_vertices = wind::strip_units();
        let strip_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("unit_strip"),
            contents: bytemuck::cast_slice(&strip_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let world_instance_capacity = 1024usize;
        let world_instance_buffer = make_instance_buffer(&device, world_instance_capacity);
        let ui_instance_capacity = 256usize;
        let ui_instance_buffer = make_instance_buffer(&device, ui_instance_capacity);

        let text_cache = glyphon::Cache::new(&device);
        let mut text_atlas = glyphon::TextAtlas::with_color_mode(
            &device,
            &queue,
            &text_cache,
            format,
            glyphon::ColorMode::Web,
        );
        let text_viewport = glyphon::Viewport::new(&device, &text_cache);
        let text_renderer = glyphon::TextRenderer::new(
            &mut text_atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );
        let tooltip_text_renderer = glyphon::TextRenderer::new(
            &mut text_atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );
        let materials = gpu3d::MaterialGpu::new(&device, &queue, material_options, None);
        let models = ModelsGpu::new(&device, &queue, material_options, &ModelTable::default());

        Ok(Renderer {
            _instance: instance,
            surface,
            device,
            queue,
            config,
            rect_pipeline,
            strip_buffer,
            world_globals_buffer,
            world_bind_group,
            world_instance_buffer,
            world_instance_capacity,
            ui_globals_buffer,
            ui_bind_group,
            ui_instance_buffer,
            ui_instance_capacity,
            atlas_texture,
            atlas_layers,
            atlas_bind_group_layout,
            linear_sampler,
            font_system: glyphon::FontSystem::new(),
            swash_cache: glyphon::SwashCache::new(),
            text_cache,
            text_atlas,
            text_viewport,
            text_renderer,
            world_text_renderers: Vec::new(),
            tooltip_text_renderer,
            fonts: Vec::new(),
            background,
            backend,
            canvas_size_px,
            world_scale,
            world_offset,
            scene_cells,
            device_pixel_ratio: 1.0,
            ui_scale: 1.0,
            scene3d: None,
            materials,
            material_options,
            models,
            videos: Videos::default(),
        })
    }

    pub fn backend(&self) -> GpuBackend {
        self.backend
    }

    /// The window size in CSS pixels — the unit mouse positions arrive in; `ScreensConfig::ui_frame`
    /// turns it into the `screens.json` points `anchor`/`offset`/`size` are written in.
    pub fn window_size_css(&self) -> [f32; 2] {
        [
            self.canvas_size_px[0] / self.device_pixel_ratio,
            self.canvas_size_px[1] / self.device_pixel_ratio,
        ]
    }

    /// Loads one font file's bytes into `cosmic-text`'s shared database and returns its family name.
    /// The family name shaping needs comes from the file itself — the caller's name for it (from
    /// `files.fonts`) is not a font-internal name, so this reads it back from the face
    /// `load_font_source` reports having just added. «Текст»: a file two names lead to is loaded once.
    pub fn load_font_file(&mut self, bytes: Vec<u8>) -> String {
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(glyphon::fontdb::Source::Binary(std::sync::Arc::new(bytes)));
        ids.first()
            .and_then(|&id| self.font_system.db().face(id))
            .and_then(|face| face.families.first())
            .map(|(name, _)| name.clone())
            .unwrap_or_default()
    }

    /// Adds the next `FontId` later `TextDraw`s reference: a family `load_font_file` returned and
    /// that name's own settings — «Интерфейс игры» → «Текст».
    pub fn add_font(&mut self, family: String, settings: &FontSettings) -> FontId {
        let mut features = glyphon::cosmic_text::FontFeatures::new();
        for tag in &settings.features {
            features.enable(glyphon::cosmic_text::FeatureTag::new(tag));
        }
        self.fonts.push(FontFace {
            family,
            weight: glyphon::Weight(settings.weight),
            stretch: font_stretch(settings.width),
            features,
        });
        self.fonts.len() - 1
    }

    /// «Подсказка»: ширина строки `text` шрифтом `font` размера `font_size` — в точках `screens.json`, как
    /// и сам размер.
    pub fn text_width(&mut self, text: &str, font: FontId, font_size: f32) -> f32 {
        let scale = self.device_pixel_ratio * self.ui_scale;
        let font_size_px = (font_size * scale).max(1.0);
        let metrics =
            glyphon::Metrics::new(font_size_px, text_area::line_height(font_size_px, false));
        let mut buffer = glyphon::Buffer::new(&mut self.font_system, metrics);
        buffer.set_wrap(&mut self.font_system, glyphon::cosmic_text::Wrap::None);
        let attrs = font_attrs(&self.fonts, font);
        buffer.set_text(
            &mut self.font_system,
            text,
            &attrs,
            glyphon::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);
        let width = buffer
            .layout_runs()
            .fold(0.0f32, |width, run| width.max(run.line_w));
        width / scale
    }

    /// «Редактор», требование 20: a repeated `load()` must not pile fonts up under the previous
    /// game's ones — dropped and rebuilt fresh, so the next game's `load_font` calls hand out
    /// `FontId`s starting at 0 again, matching its own `files.fonts` order. `glyphon`'s glyph
    /// atlas caches rasters by `CacheKey`, which embeds the `fontdb::ID` the fresh `FontSystem`
    /// hands out — the same IDs as the dropped one — so `swash_cache` and `text_atlas` are
    /// rebuilt too, or a new font's glyphs would come back as the old font's rasters.
    pub fn reset_fonts(&mut self) {
        self.font_system = glyphon::FontSystem::new();
        self.fonts.clear();
        self.swash_cache = glyphon::SwashCache::new();
        self.text_atlas = glyphon::TextAtlas::with_color_mode(
            &self.device,
            &self.queue,
            &self.text_cache,
            self.config.format,
            glyphon::ColorMode::Web,
        );
    }

    /// «Картинки» → «Атлас», требование 14: packs `images` (already checked, one whole strip per
    /// declared picture) into a stack of sheets and uploads each one its own texture layer —
    /// «Стек и инфраструктура» → «Ловушка WebGL2»: the WebGL2 backend gets a bumped layer count
    /// (`atlas::webgl2_safe_layer_count`) instead of the exact number of sheets, since `wgpu-hal`
    /// 29.0.4's GLES backend turns a one-layer `D2` into a plain `TEXTURE_2D` and a square one
    /// with a layer count divisible by 6 into a cube map, either of which breaks this module's
    /// `D2Array` view; WebGPU always gets exactly as many layers as there are sheets. Recreates
    /// the texture (and both bind groups, which reference its view) only when that layer count
    /// actually changes from one game to the next. Returns the packed rectangles, indexed the same
    /// way `images` was — i.e. by `ImageId`. The points go through one sheet-sized buffer, filled
    /// and uploaded sheet by sheet (`write_texture` copies it before returning), so no more than
    /// one sheet's bytes ever exist at once, and it is dropped right after the last upload.
    pub fn build_atlas(
        &mut self,
        images: &[atlas::AtlasImage],
    ) -> Result<Vec<atlas::AtlasRect>, String> {
        self.videos.release();
        let packed = atlas::pack(images)?;
        let layers = match self.backend {
            GpuBackend::WebGpu => packed.sheet_count,
            GpuBackend::WebGl2 => atlas::webgl2_safe_layer_count(packed.sheet_count),
        };
        if layers != self.atlas_layers {
            self.recreate_atlas_texture(layers);
        }
        let mut pixels = vec![0u8; atlas::SHEET_BYTES];
        for sheet in 0..packed.sheet_count {
            atlas::fill_sheet(&packed, images, sheet, &mut pixels);
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: sheet,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
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
        }
        Ok(packed.rects)
    }

    /// «Картинки» → «Видео»: видео игры, чьи места `build_atlas` только что отдал; пустой список
    /// останавливает и забывает прежние. Видео, чьё место уже не в атласе, рисовать нечем.
    pub fn set_videos(&mut self, sources: Vec<VideoSource>) {
        self.videos.set(&self.device, &self.atlas_texture, sources);
    }

    /// Кадры видео — в атлас; звать перед отрисовкой кадра плоской сцены.
    pub fn refresh_videos(&self) {
        self.videos.refresh(&self.device, &self.queue);
    }

    /// Часы кадров по времени идут — видео играют, стоят — на паузе.
    pub fn set_videos_playing(&self, playing: bool) {
        self.videos.set_playing(playing);
    }

    fn recreate_atlas_texture(&mut self, layers: u32) {
        let texture = create_atlas_texture(&self.device, layers);
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        self.world_bind_group = make_atlas_bind_group(
            &self.device,
            &self.atlas_bind_group_layout,
            &self.world_globals_buffer,
            &view,
            &self.linear_sampler,
            "world_globals_bind_group",
        );
        self.ui_bind_group = make_atlas_bind_group(
            &self.device,
            &self.atlas_bind_group_layout,
            &self.ui_globals_buffer,
            &view,
            &self.linear_sampler,
            "ui_globals_bind_group",
        );
        self.atlas_texture = texture;
        self.atlas_layers = layers;
        self.scene3d = None;
    }

    /// «Свет и материалы»: отправляет на видеокарту материалы и маски покрытий игры и заменяет ими
    /// прежние; без `relief` — пустые текстуры. Байты остаются у вызывающего: сюда они не копятся.
    /// Ресурсы трёхмерной сцены пересоздаются, как при новом атласе: их раскладка ссылается на текстуры.
    pub fn set_relief(&mut self, relief: Option<&Relief<'_>>) {
        self.materials =
            gpu3d::MaterialGpu::new(&self.device, &self.queue, self.material_options, relief);
        self.scene3d = None;
    }

    /// «Трёхмерная сцена» → «Модели и движения»: отправляет на видеокарту модели игры и заменяет ими прежние.
    /// Ресурсы трёхмерной сцены пересоздаются: конвейеры моделей ссылаются на раскладку материала.
    pub fn set_models(&mut self, table: &ModelTable) {
        self.models = ModelsGpu::new(&self.device, &self.queue, self.material_options, table);
        self.scene3d = None;
    }

    /// «Свет и материалы», `set_covers`: таблица слоёв и маски покрытий на видеокарте заменяются новыми,
    /// материалы и карта цвета остаются. Ресурсы трёхмерной сцены пересоздаются, только если массив
    /// масок создан заново: их раскладка ссылается на него.
    pub fn set_covers(
        &mut self,
        covers: &[Cover],
        decls: &[MaterialDecl],
        scene: [u32; 2],
        masks: &[CoverMask],
    ) {
        let rebuilt = self.materials.set_covers(
            &self.device,
            &self.queue,
            self.material_options,
            covers,
            decls,
            scene,
            masks,
        );
        if rebuilt {
            self.scene3d = None;
        }
    }

    /// The game's scene size and background become known only once it has loaded, well after
    /// the GPU itself was set up — rewrites both, and stands in a plain wall-to-wall letterbox of
    /// the new scene as `world_scale`/`world_offset`'s placeholder until the wasm layer's own
    /// next `set_world_frame` (every `draw()`/`tick()`) replaces it with the real one — «Камера».
    pub fn set_scene(&mut self, scene_width: u32, scene_height: u32, background: [f32; 4]) {
        self.scene_cells = [scene_width.max(1) as f32, scene_height.max(1) as f32];
        self.background = background;
        (self.world_scale, self.world_offset) =
            crate::core::scene::letterbox(self.canvas_size_px, self.scene_cells);
        self.write_world_globals();
    }

    /// «Камера»: масштаб и сдвиг мировой раскладки этого кадра, в оконных (CSS) пикселях — вне
    /// партии (или без `view_height`) это по-прежнему letterbox сцены целиком, в партии —
    /// камера ядра (`core::game::Game::camera_frame`); который из двух посчитать, решает
    /// вызывающий (`wasm::mod`), сам рендерер камеры не знает вовсе. Переводит их в пиксели
    /// устройства тем же `device_pixel_ratio`, что и интерфейсный проход, перед записью в
    /// буфер — `canvas_size_px`, на который шейдер делит, сам в пикселях устройства.
    pub fn set_world_frame(&mut self, scale: f32, offset: [f32; 2]) {
        let dpr = self.device_pixel_ratio;
        self.world_scale = scale * dpr;
        self.world_offset = [offset[0] * dpr, offset[1] * dpr];
        self.write_world_globals();
    }

    /// Reconfigures the GPU surface for a new device-pixel canvas size. Call on resize; does
    /// not touch the game. `set_pixel_ratio` supplies the CSS/device-pixel split separately.
    /// Recomputes the same placeholder letterbox `set_scene` does — «Камера»: it stands in for
    /// the new canvas size until the wasm layer's own next `set_world_frame`.
    pub fn resize(&mut self, width_px: u32, height_px: u32) {
        self.config.width = width_px.max(1);
        self.config.height = height_px.max(1);
        self.surface.configure(&self.device, &self.config);
        self.canvas_size_px = [self.config.width as f32, self.config.height as f32];
        (self.world_scale, self.world_offset) =
            crate::core::scene::letterbox(self.canvas_size_px, self.scene_cells);
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

    /// «Раскладка экрана»: масштаб интерфейса этого кадра от `ui_reference` — 1 без него.
    pub fn set_ui_scale(&mut self, scale: f32) {
        if scale != self.ui_scale {
            self.ui_scale = scale;
            self.write_ui_globals();
        }
    }

    fn write_world_globals(&mut self) {
        let globals = world_globals_data(self.canvas_size_px, self.world_scale, self.world_offset);
        self.queue
            .write_buffer(&self.world_globals_buffer, 0, bytemuck::bytes_of(&globals));
    }

    fn write_ui_globals(&mut self) {
        let globals = ui_globals_data(self.canvas_size_px, self.device_pixel_ratio * self.ui_scale);
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

    /// One shaped `cosmic-text` buffer, clipped to `width_px` — «Интерфейс игры» → «Текст»: a single
    /// line without wrapping, or with `wrap` lines broken at words to the width, each aligned inside the
    /// element. Shared by `build_text_buffers` (interface, window pixels) and `build_world_text_buffers`
    /// (world, scene cells) — everything past resolving the actual pixel rectangle is identical
    /// between the two.
    fn shape_text_buffer(
        &mut self,
        text: &str,
        font: FontId,
        font_size_px: f32,
        align: Align,
        width_px: f32,
        wrap: bool,
    ) -> glyphon::Buffer {
        let metrics =
            glyphon::Metrics::new(font_size_px, text_area::line_height(font_size_px, wrap));
        let mut buffer = glyphon::Buffer::new(&mut self.font_system, metrics);
        let wrapping = if wrap {
            glyphon::cosmic_text::Wrap::Word
        } else {
            glyphon::cosmic_text::Wrap::None
        };
        buffer.set_wrap(&mut self.font_system, wrapping);
        buffer.set_size(&mut self.font_system, Some(width_px), None);
        let attrs = font_attrs(&self.fonts, font);
        buffer.set_text(
            &mut self.font_system,
            text,
            &attrs,
            glyphon::Shaping::Advanced,
            Some(cosmic_align(align)),
        );
        buffer
    }

    /// One `cosmic-text` buffer per `TextDraw` — «Интерфейс игры»: `rect_px` is in `screens.json`
    /// points, scaled up here by the device pixel ratio and the interface scale, same as `ui_rects`.
    fn build_text_buffers(&mut self, texts: &[TextDraw]) -> Vec<ShapedText> {
        let dpr = self.device_pixel_ratio * self.ui_scale;
        texts
            .iter()
            .map(|t| {
                let font_size_px = (t.font_size_px * dpr).max(1.0);
                let rect_px = [
                    t.rect_px[0] * dpr,
                    t.rect_px[1] * dpr,
                    t.rect_px[2] * dpr,
                    t.rect_px[3] * dpr,
                ];
                let buffer = self.shape_text_buffer(
                    &t.text,
                    t.font,
                    font_size_px,
                    t.align,
                    rect_px[2],
                    t.wrap,
                );
                ShapedText {
                    buffer,
                    rect_px,
                    color: t.color,
                    wrap: t.wrap,
                }
            })
            .collect()
    }

    /// One `cosmic-text` buffer per `WorldTextDraw` — «Надписи и полоски в мире», требование 17:
    /// `rect_cells` is in scene cells, converted to device pixels through the same `world_scale`/
    /// `world_offset` the world rectangle pass uses (`set_world_frame`/`set_scene`), which already
    /// carries the device pixel ratio — unlike `build_text_buffers`, no separate multiply here.
    fn build_world_text_buffers(&mut self, texts: &[WorldTextDraw]) -> Vec<ShapedText> {
        let scale = self.world_scale;
        let offset = self.world_offset;
        texts
            .iter()
            .map(|t| {
                let font_size_px = (t.font_size_cells * scale).max(1.0);
                let rect_px = [
                    offset[0] + t.rect_cells[0] * scale,
                    offset[1] + t.rect_cells[1] * scale,
                    t.rect_cells[2] * scale,
                    t.rect_cells[3] * scale,
                ];
                let buffer = self.shape_text_buffer(
                    &t.text,
                    t.font,
                    font_size_px,
                    t.align,
                    rect_px[2],
                    false,
                );
                ShapedText {
                    buffer,
                    rect_px,
                    color: t.color,
                    wrap: false,
                }
            })
            .collect()
    }

    /// Draws one frame in layers, back to front — «Интерфейс игры» → «Отрисовка»: the world's colored
    /// rectangles (`world.instances`, scene cells, letterboxed — a world element's own bar and image
    /// rectangles are appended to this same list by the caller, so they draw over the objects beneath
    /// them for free), the world's own labels (`world.texts`, a separate `glyphon` pass so a screen
    /// panel drawn after it can still cover it) interleaved with the images by `world.runs`, the
    /// interface's panels and buttons (`ui_instances`, window pixels), then the interface's text
    /// (`texts`) on top of everything.
    pub fn render_frame(&mut self, world: &WorldOverlay, ui: &UiLayers) -> Result<(), String> {
        let (world_text_counts, ui_counts) = self.upload_overlay(world, ui)?;
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
            self.draw_overlay(&mut pass, world, &world_text_counts, ui_counts)?;
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        Ok(())
    }

    /// «Трёхмерная сцена»: кадр из трёх частей — тени и мир с глубиной (`gpu3d::Scene3d::encode`),
    /// затем тот же проход без глубины, что у плоской сцены, поверх готового цвета: полоски мира
    /// (`world.instances`, в точках окна), надписи и картинки в мире, интерфейс — надписи и полоски над
    /// фигурами не закрывает ничто.
    pub fn render_frame_3d(
        &mut self,
        frame: &gpu3d::Scene3dFrame<'_>,
        world: &WorldOverlay,
        ui: &UiLayers,
    ) -> Result<(), String> {
        let (world_text_counts, ui_counts) = self.upload_overlay(world, ui)?;
        let size = [self.config.width, self.config.height];
        let atlas_view = self
            .atlas_texture
            .create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
        let scene3d = self.scene3d.get_or_insert_with(|| {
            gpu3d::Scene3d::new(
                &self.device,
                self.config.format,
                &atlas_view,
                &self.linear_sampler,
                &self.materials,
                &self.models,
                size,
            )
        });
        scene3d.upload(&self.device, &self.queue, size, frame);
        self.models.upload(&self.device, &self.queue, &frame.models);

        let Some(surface_texture) = self.acquire_frame()? else {
            return Ok(());
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame_3d"),
            });
        if let Some(scene3d) = self.scene3d.as_ref() {
            scene3d.encode(&mut encoder, &view, self.background, &self.models);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            self.draw_overlay(&mut pass, world, &world_text_counts, ui_counts)?;
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        Ok(())
    }

    /// Записывает прямоугольники мира и интерфейса в буферы и готовит их тексты — общая часть кадра
    /// плоской и трёхмерной сцены. Отвечает, сколько надписей готово у каждого отрезка надписей мира
    /// (по порядку `WorldRun::Texts`) и сколько у интерфейса прямоугольников и надписей элементов и
    /// подсказки: надпись целиком вне окна не готовится и не считается.
    fn upload_overlay(
        &mut self,
        world: &WorldOverlay,
        ui: &UiLayers,
    ) -> Result<(Vec<usize>, UiCounts), String> {
        self.grow_world_instances(world.instances.len());
        if !world.instances.is_empty() {
            self.queue.write_buffer(
                &self.world_instance_buffer,
                0,
                bytemuck::cast_slice(&world.instances),
            );
        }
        self.grow_ui_instances(ui.rects.len() + ui.tooltip_rects.len());
        if !ui.rects.is_empty() {
            self.queue
                .write_buffer(&self.ui_instance_buffer, 0, bytemuck::cast_slice(&ui.rects));
        }
        if !ui.tooltip_rects.is_empty() {
            self.queue.write_buffer(
                &self.ui_instance_buffer,
                (ui.rects.len() * std::mem::size_of::<DrawRect>()) as u64,
                bytemuck::cast_slice(&ui.tooltip_rects),
            );
        }

        let mut world_text_counts = Vec::new();
        for run in &world.runs {
            let WorldRun::Texts(range) = run else {
                continue;
            };
            world_text_counts.push(
                self.prepare_world_text_run(world_text_counts.len(), &world.texts[range.clone()])?,
            );
        }

        let texts = self.prepare_ui_texts(&ui.texts, false)?;
        let tooltip_texts = self.prepare_ui_texts(&ui.tooltip_texts, true)?;
        Ok((
            world_text_counts,
            UiCounts {
                rects: ui.rects.len(),
                texts,
                tooltip_rects: ui.tooltip_rects.len(),
                tooltip_texts,
            },
        ))
    }

    /// Готовит надписи интерфейса рендерером элементов или, с `tooltip`, рендерером подсказки. Отвечает,
    /// сколько надписей готово к рисованию.
    fn prepare_ui_texts(&mut self, texts: &[TextDraw], tooltip: bool) -> Result<usize, String> {
        let buffers = self.build_text_buffers(texts);
        let text_areas = text_areas_from(&buffers, self.canvas_size_px);
        let count = text_areas.len();
        if count == 0 {
            return Ok(0);
        }
        let renderer = if tooltip {
            &mut self.tooltip_text_renderer
        } else {
            &mut self.text_renderer
        };
        renderer
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
        Ok(count)
    }

    /// Готовит надписи мира одного отрезка рендерером `index` — он заводится, когда отрезков в кадре
    /// стало больше, чем было. Отвечает, сколько надписей готово к рисованию.
    fn prepare_world_text_run(
        &mut self,
        index: usize,
        texts: &[WorldTextDraw],
    ) -> Result<usize, String> {
        while self.world_text_renderers.len() <= index {
            self.world_text_renderers.push(glyphon::TextRenderer::new(
                &mut self.text_atlas,
                &self.device,
                wgpu::MultisampleState::default(),
                None,
            ));
        }
        let buffers = self.build_world_text_buffers(texts);
        let areas = text_areas_from(&buffers, self.canvas_size_px);
        let count = areas.len();
        if count > 0 {
            self.world_text_renderers[index]
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.font_system,
                    &mut self.text_atlas,
                    &self.text_viewport,
                    areas,
                    &mut self.swash_cache,
                )
                .map_err(|e| format!("не удалось подготовить текст в мире: {e:?}"))?;
        }
        Ok(count)
    }

    /// Рисует в открытом проходе слои по порядку: отрезки мира (прямоугольники и текст), прямоугольники
    /// интерфейса, текст интерфейса, плашку и текст подсказки; `world_text_counts` — сколько надписей
    /// подготовил каждый отрезок текста, `ui_counts` — сколько прямоугольников и надписей интерфейса.
    fn draw_overlay(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        world: &WorldOverlay,
        world_text_counts: &[usize],
        ui_counts: UiCounts,
    ) -> Result<(), String> {
        let mut text_runs = 0;
        for run in &world.runs {
            match run {
                WorldRun::Rects(range) => {
                    if range.is_empty() {
                        continue;
                    }
                    // Рендерер текста, нарисовавший отрезок до этого, переставил свой конвейер и
                    // вершинный буфер на этом же проходе — здесь они возвращаются.
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_vertex_buffer(0, self.strip_buffer.slice(..));
                    pass.set_bind_group(0, &self.world_bind_group, &[]);
                    pass.set_vertex_buffer(1, self.world_instance_buffer.slice(..));
                    pass.draw(
                        0..wind::STRIP_VERTICES as u32,
                        range.start as u32..range.end as u32,
                    );
                }
                WorldRun::Texts(_) => {
                    if world_text_counts[text_runs] > 0 {
                        self.world_text_renderers[text_runs]
                            .render(&self.text_atlas, &self.text_viewport, pass)
                            .map_err(|e| format!("не удалось нарисовать текст в мире: {e:?}"))?;
                    }
                    text_runs += 1;
                }
            }
        }
        let tooltip_end = ui_counts.rects + ui_counts.tooltip_rects;
        self.draw_ui_rects(pass, 0..ui_counts.rects);
        if ui_counts.texts > 0 {
            self.text_renderer
                .render(&self.text_atlas, &self.text_viewport, pass)
                .map_err(|e| format!("не удалось нарисовать текст: {e:?}"))?;
        }
        self.draw_ui_rects(pass, ui_counts.rects..tooltip_end);
        if ui_counts.tooltip_texts > 0 {
            self.tooltip_text_renderer
                .render(&self.text_atlas, &self.text_viewport, pass)
                .map_err(|e| format!("не удалось нарисовать подсказку: {e:?}"))?;
        }
        Ok(())
    }

    /// Прямоугольники интерфейса `range` его буфера; рендерер текста, рисовавший до них, переставил
    /// конвейер и вершинный буфер — здесь они возвращаются.
    fn draw_ui_rects(&self, pass: &mut wgpu::RenderPass<'_>, range: std::ops::Range<usize>) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(&self.rect_pipeline);
        pass.set_vertex_buffer(0, self.strip_buffer.slice(..));
        pass.set_bind_group(0, &self.ui_bind_group, &[]);
        pass.set_vertex_buffer(1, self.ui_instance_buffer.slice(..));
        pass.draw(
            0..wind::STRIP_VERTICES as u32,
            range.start as u32..range.end as u32,
        );
    }
}

/// Сколько прямоугольников и надписей интерфейса кадра готово: элементов экрана и подсказки.
#[derive(Debug, Clone, Copy)]
struct UiCounts {
    rects: usize,
    texts: usize,
    tooltip_rects: usize,
    tooltip_texts: usize,
}

/// A shaped buffer with the rectangle it belongs to (device pixels), its color and whether it wraps.
struct ShapedText {
    buffer: glyphon::Buffer,
    rect_px: [f32; 4],
    color: [f32; 4],
    wrap: bool,
}

/// Shared by `render_frame`'s two `prepare` calls — one `TextArea` per shaped buffer, in its own
/// rectangle (vertically centered, or from the top when it wraps) and clipped to it and to the window:
/// a label past the window edge stays where it is and is cut by the edge, a label wholly outside is
/// left out. `window` is the canvas size in device pixels.
fn text_areas_from(buffers: &[ShapedText], window: [f32; 2]) -> Vec<glyphon::TextArea<'_>> {
    buffers
        .iter()
        .filter_map(|text| {
            let line_height = text.buffer.metrics().line_height;
            let place = text_area::place(text.rect_px, line_height, text.wrap, window)?;
            let [left, top, right, bottom] = place.bounds;
            Some(glyphon::TextArea {
                buffer: &text.buffer,
                left: text.rect_px[0],
                top: place.top,
                scale: 1.0,
                bounds: glyphon::TextBounds {
                    left,
                    top,
                    right,
                    bottom,
                },
                default_color: glyphon_color(text.color),
                custom_glyphs: &[],
            })
        })
        .collect()
}

fn make_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("instances"),
        size: (capacity * std::mem::size_of::<DrawRect>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// «Картинки» → «Атлас», требование 14: a `D2` texture of `layers` array layers, `ATLAS_SIZE`²
/// each — shared by `Renderer::new` (the placeholder texture before any game has loaded) and
/// `Renderer::recreate_atlas_texture` (a real game's own sheet count). `RENDER_ATTACHMENT` — в слои
/// атласа пишет проход видео.
fn create_atlas_texture(device: &wgpu::Device, layers: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas"),
        size: wgpu::Extent3d {
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            depth_or_array_layers: layers.max(1),
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ATLAS_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

/// Формат текстуры атласа и текстур файлов видео: `Rgba8Unorm` несёт прозрачность, не умножая цвет на
/// неё второй раз — точки атласа уже умножены (`atlas::blit`, `shaders/video.wgsl`).
pub(super) const ATLAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// One globals-uniform-plus-atlas bind group, shared by `Renderer::new` (`world_bind_group`/
/// `ui_bind_group`) and `Renderer::recreate_atlas_texture`, which rebuilds both against a new
/// atlas view.
fn make_atlas_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals_buffer: &wgpu::Buffer,
    atlas_view: &wgpu::TextureView,
    linear_sampler: &wgpu::Sampler,
    label: &str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(linear_sampler),
            },
        ],
    })
}

/// World pass: `scale`/`offset` place scene cells in the window — «Камера»: the wasm layer
/// computes them (the camera's own frame in a battle, the plain letterbox outside one) and hands
/// them here through `set_world_frame`/`set_scene`; this just assembles the uniform buffer.
fn world_globals_data(canvas_size_px: [f32; 2], scale: f32, offset: [f32; 2]) -> Globals {
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
