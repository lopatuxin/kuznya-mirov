//! «Трёхмерная сцена» → «Отрисовка»: видеокарта для рельефа, фигур, земли и теней. Проходы одного
//! кадра (`encode`): карта теней 2048 × 2048 от солнца (фигуры и рельеф), затем рельеф с водой,
//! фигуры и плитки с плоскими объектами на земле — с буфером глубины и тенью. Надписи, полоски и
//! интерфейс рисует прежний проход без глубины — его собирает `super::gpu`. Ресурсы создаются при
//! первом кадре трёхмерной сцены, так что плоская игра их не платит вовсе.

use wgpu::util::DeviceExt;

use crate::core::shapes::{self, MeshVertex};
use crate::core::value::Shape;

use super::relief::TerrainMesh;

/// Карта теней — предел `downlevel_webgl2_defaults`.
const SHADOW_SIZE: u32 = 2048;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Общие данные кадра: камера, солнце, тень. Раскладка — как `Globals3d` в `scene3d.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals3d {
    pub view_proj: [[f32; 4]; 4],
    pub light_view_proj: [[f32; 4]; 4],
    pub sun: [f32; 4],
    pub shade: [f32; 4],
}

/// Одна фигура кадра: `placement` — середина на земле и косинус с синусом поворота, `dims` —
/// ширина, глубина, высота и высота полушария капсулы, `base` — высота основания над нулём сцены.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShapeInstance {
    pub placement: [f32; 4],
    pub dims: [f32; 4],
    pub color: [f32; 3],
    pub base: f32,
}

/// Вершина треугольника плитки или плоского объекта на земле — те же поля, что у
/// `super::relief::SurfaceVertex`.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GroundVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub sheet: [f32; 2],
}

// Раскладки, которые читает `scene3d.wgsl`: уникальный размер в байтах и смещения атрибутов ниже.
const _: () = assert!(std::mem::size_of::<Globals3d>() == 160);
const _: () = assert!(std::mem::size_of::<ShapeInstance>() == 48);
const _: () = assert!(std::mem::size_of::<GroundVertex>() == 72);

/// Кадр трёхмерной сцены для видеокарты; `shapes` — по видам фигур в порядке `Shape::ALL`. `terrain` —
/// сетка рельефа сцены, если в ней есть файл высот; буфер пересоздаётся, только когда сменился её
/// номер.
pub struct Scene3dFrame<'a> {
    pub globals: Globals3d,
    pub terrain: Option<&'a TerrainMesh>,
    pub ground: &'a [GroundVertex],
    pub shapes: [&'a [ShapeInstance]; 4],
}

struct MeshBuffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

pub struct Scene3d {
    shadow_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    depth_size: [u32; 2],
    globals_buffer: wgpu::Buffer,
    shadow_bind_group: wgpu::BindGroup,
    main_bind_group: wgpu::BindGroup,
    shadow_pipeline: wgpu::RenderPipeline,
    terrain_shadow_pipeline: wgpu::RenderPipeline,
    terrain_pipeline: wgpu::RenderPipeline,
    shape_pipeline: wgpu::RenderPipeline,
    ground_pipeline: wgpu::RenderPipeline,
    meshes: [MeshBuffers; 4],
    shape_buffer: wgpu::Buffer,
    shape_capacity: usize,
    ground_buffer: wgpu::Buffer,
    ground_capacity: usize,
    /// Сетка рельефа и воды и номер той сетки, что лежит в буфере.
    terrain: Option<TerrainBuffer>,
    /// Сколько фигур каждого вида лежит в `shape_buffer`, подряд по видам.
    shape_counts: [u32; 4],
    ground_count: u32,
}

struct TerrainBuffer {
    id: u64,
    buffer: wgpu::Buffer,
    /// Вершин земли, затем воды.
    land: u32,
    total: u32,
}

fn depth_texture_view(
    device: &wgpu::Device,
    size: [u32; 2],
    label: &str,
    sampled: bool,
) -> wgpu::TextureView {
    let usage = if sampled {
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
    } else {
        wgpu::TextureUsages::RENDER_ATTACHMENT
    };
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size[0].max(1),
                height: size[1].max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn make_mesh_buffers(device: &wgpu::Device, shape: Shape) -> MeshBuffers {
    let mesh = shapes::unit_mesh(shape);
    let floats: Vec<f32> = mesh
        .vertices
        .iter()
        .flat_map(|v: &MeshVertex| v.position.into_iter().chain(v.normal).chain(v.cap))
        .collect();
    MeshBuffers {
        vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shape_vertices"),
            contents: bytemuck::cast_slice(&floats),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shape_indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        }),
        index_count: mesh.indices.len() as u32,
    }
}

fn make_instance_buffer<T>(device: &wgpu::Device, label: &str, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (capacity * std::mem::size_of::<T>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

const SHAPE_ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 12,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 24,
        shader_location: 2,
    },
];

const SHAPE_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 0,
        shader_location: 3,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 16,
        shader_location: 4,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 32,
        shader_location: 5,
    },
];

const TERRAIN_ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 12,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 24,
        shader_location: 2,
    },
];

/// Байт на вершину сетки рельефа: место, нормаль и цвет.
const TERRAIN_STRIDE: u64 = 36;

const GROUND_ATTRIBUTES: [wgpu::VertexAttribute; 7] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 12,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 24,
        shader_location: 2,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 32,
        shader_location: 3,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 48,
        shader_location: 4,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 56,
        shader_location: 5,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 64,
        shader_location: 6,
    },
];

fn layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    ty: wgpu::BindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    }
}

impl Scene3d {
    /// Собирает ресурсы: карту теней, буфер глубины под `surface_size`, сетки фигур, три конвейера
    /// и две раскладки — теневая видит только общие данные (карта теней в ней — цель записи, а не
    /// текстура), основная — общие данные, атлас, его выборку и тень со сравнением.
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        atlas_view: &wgpu::TextureView,
        atlas_sampler: &wgpu::Sampler,
        surface_size: [u32; 2],
    ) -> Scene3d {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene3d"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/scene3d.wgsl").into()),
        });

        let shadow_view = depth_texture_view(device, [SHADOW_SIZE; 2], "shadow_map", true);
        let depth_view = depth_texture_view(device, surface_size, "scene_depth", false);
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene3d_globals"),
            size: std::mem::size_of::<Globals3d>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_entry = |visibility| {
            layout_entry(
                0,
                visibility,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            )
        };
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow_layout"),
            entries: &[globals_entry(wgpu::ShaderStages::VERTEX)],
        });
        let main_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene3d_layout"),
            entries: &[
                globals_entry(wgpu::ShaderStages::VERTEX_FRAGMENT),
                layout_entry(
                    1,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                ),
                layout_entry(
                    2,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                layout_entry(
                    3,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                layout_entry(
                    4,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                ),
            ],
        });

        let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow_bind_group"),
            layout: &shadow_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let main_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene3d_bind_group"),
            layout: &main_layout,
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
                    resource: wgpu::BindingResource::Sampler(atlas_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });

        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shadow_pipeline_layout"),
                bind_group_layouts: &[Some(&shadow_layout)],
                immediate_size: 0,
            });
        let main_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene3d_pipeline_layout"),
            bind_group_layouts: &[Some(&main_layout)],
            immediate_size: 0,
        });

        let shape_buffers = [
            wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &SHAPE_ATTRIBUTES,
            },
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ShapeInstance>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &SHAPE_INSTANCE_ATTRIBUTES,
            },
        ];
        let terrain_buffers = [wgpu::VertexBufferLayout {
            array_stride: TERRAIN_STRIDE,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &TERRAIN_ATTRIBUTES,
        }];
        let ground_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GroundVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &GROUND_ATTRIBUTES,
        }];
        let depth_state = |write: bool, compare: wgpu::CompareFunction| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let color_target = |blend| {
            [Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })]
        };

        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow_pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                buffers: &shape_buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less)),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let shape_targets = color_target(wgpu::BlendState::REPLACE);
        let terrain_shadow_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("terrain_shadow_pipeline"),
                layout: Some(&shadow_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_shadow_terrain"),
                    buffers: &terrain_buffers,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: None,
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less)),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain_pipeline"),
            layout: Some(&main_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_terrain"),
                buffers: &terrain_buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_terrain"),
                targets: &shape_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less)),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let shape_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shape_pipeline"),
            layout: Some(&main_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shape"),
                buffers: &shape_buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_shape"),
                targets: &shape_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less)),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        // Плитки и плоские объекты лежат на поверхности рельефа или настила: без сдвига к камере
        // их глубина спорила бы с глубиной поверхности под ними.
        let ground_depth = wgpu::DepthStencilState {
            bias: wgpu::DepthBiasState {
                constant: -2,
                slope_scale: -2.0,
                clamp: 0.0,
            },
            ..depth_state(false, wgpu::CompareFunction::LessEqual)
        };
        let ground_targets = color_target(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let ground_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ground_pipeline"),
            layout: Some(&main_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_ground"),
                buffers: &ground_buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_ground"),
                targets: &ground_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(ground_depth),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let shape_capacity = 256;
        let ground_capacity = 8192;
        Scene3d {
            shadow_view,
            depth_view,
            depth_size: surface_size,
            globals_buffer,
            shadow_bind_group,
            main_bind_group,
            shadow_pipeline,
            terrain_shadow_pipeline,
            terrain_pipeline,
            shape_pipeline,
            ground_pipeline,
            meshes: Shape::ALL.map(|shape| make_mesh_buffers(device, shape)),
            shape_buffer: make_instance_buffer::<ShapeInstance>(
                device,
                "shape_instances",
                shape_capacity,
            ),
            shape_capacity,
            ground_buffer: make_instance_buffer::<GroundVertex>(
                device,
                "ground_vertices",
                ground_capacity,
            ),
            ground_capacity,
            terrain: None,
            shape_counts: [0; 4],
            ground_count: 0,
        }
    }

    /// Буфер глубины следует за размером холста.
    fn fit_depth(&mut self, device: &wgpu::Device, surface_size: [u32; 2]) {
        if self.depth_size != surface_size {
            self.depth_view = depth_texture_view(device, surface_size, "scene_depth", false);
            self.depth_size = surface_size;
        }
    }

    /// Пишет данные кадра в буферы, при нужде увеличивая их.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_size: [u32; 2],
        frame: &Scene3dFrame<'_>,
    ) {
        self.fit_depth(device, surface_size);
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&frame.globals));

        let shape_total: usize = frame.shapes.iter().map(|s| s.len()).sum();
        if shape_total > self.shape_capacity {
            self.shape_capacity = shape_total.next_power_of_two();
            self.shape_buffer = make_instance_buffer::<ShapeInstance>(
                device,
                "shape_instances",
                self.shape_capacity,
            );
        }
        let mut offset = 0u64;
        for (count, instances) in self.shape_counts.iter_mut().zip(frame.shapes) {
            *count = instances.len() as u32;
            if !instances.is_empty() {
                queue.write_buffer(&self.shape_buffer, offset, bytemuck::cast_slice(instances));
            }
            offset += std::mem::size_of_val(instances) as u64;
        }

        if frame.ground.len() > self.ground_capacity {
            self.ground_capacity = frame.ground.len().next_power_of_two();
            self.ground_buffer = make_instance_buffer::<GroundVertex>(
                device,
                "ground_vertices",
                self.ground_capacity,
            );
        }
        self.ground_count = frame.ground.len() as u32;
        if !frame.ground.is_empty() {
            queue.write_buffer(&self.ground_buffer, 0, bytemuck::cast_slice(frame.ground));
        }

        self.sync_terrain(device, frame.terrain);
    }

    /// Держит в буфере сетку `mesh`: пишет её заново, только когда сменился номер.
    fn sync_terrain(&mut self, device: &wgpu::Device, mesh: Option<&TerrainMesh>) {
        let Some(mesh) = mesh else {
            self.terrain = None;
            return;
        };
        if self.terrain.as_ref().is_some_and(|held| held.id == mesh.id) {
            return;
        }
        let floats: Vec<f32> = mesh
            .vertices
            .iter()
            .flat_map(|v| v.position.into_iter().chain(v.normal).chain(v.color))
            .collect();
        self.terrain = Some(TerrainBuffer {
            id: mesh.id,
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain_vertices"),
                contents: bytemuck::cast_slice(&floats),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            land: mesh.land as u32,
            total: mesh.vertices.len() as u32,
        });
    }

    /// Рисует фигуры каждого вида одной командой: сетка вида и его экземпляры подряд в буфере.
    fn draw_shapes(&self, pass: &mut wgpu::RenderPass<'_>) {
        let mut first_byte = 0u64;
        for (mesh, &count) in self.meshes.iter().zip(&self.shape_counts) {
            let bytes = count as u64 * std::mem::size_of::<ShapeInstance>() as u64;
            if count > 0 {
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_vertex_buffer(1, self.shape_buffer.slice(first_byte..first_byte + bytes));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..mesh.index_count, 0, 0..count);
            }
            first_byte += bytes;
        }
    }

    /// Проход теней (фигуры и рельеф) и основной проход: рельеф с водой, фигуры, затем плитки и
    /// плоские объекты — они лежат на поверхностях, которые уже записали глубину. Цвет очищается
    /// фоном сцены; следующий за ними проход интерфейса берёт цвет как есть.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        background: [f32; 4],
    ) {
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &self.shadow_bind_group, &[]);
            self.draw_shapes(&mut pass);
            if let Some(terrain) = &self.terrain {
                pass.set_pipeline(&self.terrain_shadow_pipeline);
                pass.set_vertex_buffer(0, terrain.buffer.slice(..));
                pass.draw(0..terrain.land, 0..1);
            }
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene3d"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: background[0] as f64,
                        g: background[1] as f64,
                        b: background[2] as f64,
                        a: background[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.main_bind_group, &[]);
        if let Some(terrain) = &self.terrain {
            pass.set_pipeline(&self.terrain_pipeline);
            pass.set_vertex_buffer(0, terrain.buffer.slice(..));
            pass.draw(0..terrain.total, 0..1);
        }
        pass.set_pipeline(&self.shape_pipeline);
        self.draw_shapes(&mut pass);
        if self.ground_count > 0 {
            pass.set_pipeline(&self.ground_pipeline);
            pass.set_vertex_buffer(0, self.ground_buffer.slice(..));
            pass.draw(0..self.ground_count, 0..1);
        }
    }
}
