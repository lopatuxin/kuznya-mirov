//! «Трёхмерная сцена» → «Модели и движения»: модели игры на видеокарте. Сетки частей, картинки материалов
//! со ступенями уменьшения и буфер экземпляров, в котором объекты каждой модели лежат подряд: каждая часть
//! модели рисуется одной командой на все объекты своей модели. Картинки моделей в атлас игры не идут: у
//! моделей свои текстуры.

use std::collections::HashMap;

use wgpu::util::{DeviceExt, TextureDataOrder};

use crate::core::model::{Image, Material, Model, ModelTable, Sampler, Texture, Wrap};

use super::gpu3d::MaterialOptions;
use super::models::{ModelInstance, mip_chain};

/// Байт на вершину части: место, нормаль, точка карты, касательная и цвет.
pub const VERTEX_STRIDE: u64 = 64;

/// Байт на объект: `ModelInstance`.
pub const INSTANCE_STRIDE: u64 = 32;

/// Вершина части, как её читает `scene3d.wgsl`.
pub const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 5] = [
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
        format: wgpu::VertexFormat::Float32x4,
        offset: 48,
        shader_location: 4,
    },
];

pub const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 2] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 0,
        shader_location: 5,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 16,
        shader_location: 6,
    },
];

/// Часть модели на видеокарте: сетка и привязка её материала.
struct PartGpu {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    material: wgpu::BindGroup,
}

struct ModelGpu {
    parts: Vec<PartGpu>,
}

/// Все модели игры на видеокарте и буфер экземпляров кадра.
pub struct ModelsGpu {
    material_layout: wgpu::BindGroupLayout,
    models: Vec<ModelGpu>,
    instances: wgpu::Buffer,
    capacity: usize,
    /// Сколько объектов каждой модели лежит в `instances`, подряд по моделям.
    counts: Vec<u32>,
    scratch: Vec<u8>,
}

fn texture_entry(binding: u32) -> [wgpu::BindGroupLayoutEntry; 2] {
    [
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: binding + 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}

fn material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries = vec![wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }];
    for binding in [1, 3, 5, 7] {
        entries.extend(texture_entry(binding));
    }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("model_material_layout"),
        entries: &entries,
    })
}

fn address_mode(wrap: Wrap) -> wgpu::AddressMode {
    match wrap {
        Wrap::Repeat => wgpu::AddressMode::Repeat,
        Wrap::Mirror => wgpu::AddressMode::MirrorRepeat,
        Wrap::Clamp => wgpu::AddressMode::ClampToEdge,
    }
}

fn sampler(device: &wgpu::Device, options: MaterialOptions, sampler: Sampler) -> wgpu::Sampler {
    let filter = |linear: bool| {
        if linear {
            wgpu::FilterMode::Linear
        } else {
            wgpu::FilterMode::Nearest
        }
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("model_sampler"),
        address_mode_u: address_mode(sampler.wrap[0]),
        address_mode_v: address_mode(sampler.wrap[1]),
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: filter(sampler.mag_linear),
        min_filter: filter(sampler.min_linear),
        mipmap_filter: if sampler.min_linear {
            wgpu::MipmapFilterMode::Linear
        } else {
            wgpu::MipmapFilterMode::Nearest
        },
        anisotropy_clamp: if options.anisotropic && sampler.mag_linear && sampler.min_linear {
            16
        } else {
            1
        },
        ..Default::default()
    })
}

/// Картинка модели на видеокарте со всеми ступенями уменьшения, что влезают в `max_side`.
fn image_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &Image,
    srgb: bool,
) -> wgpu::TextureView {
    let chain = mip_chain(image, srgb, device.limits().max_texture_dimension_2d);
    let (width, height) = chain.first().map_or((1, 1), |top| (top.width, top.height));
    let data: Vec<u8> = chain
        .iter()
        .flat_map(|level| level.pixels.iter().copied())
        .collect();
    device
        .create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("model_image"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: chain.len().max(1) as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: if srgb {
                    wgpu::TextureFormat::Rgba8UnormSrgb
                } else {
                    wgpu::TextureFormat::Rgba8Unorm
                },
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            TextureDataOrder::LayerMajor,
            &data,
        )
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// Карта из одной точки: у материала нет своей — шейдер читает её вместо неё.
fn solid_view(device: &wgpu::Device, queue: &wgpu::Queue, pixel: [u8; 4]) -> wgpu::TextureView {
    let image = Image {
        width: 1,
        height: 1,
        pixels: pixel.to_vec(),
    };
    image_view(device, queue, &image, false)
}

fn floats_to_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Что материалу нужно от видеокарты: картинки по ролям и как их брать.
struct Maps<'a> {
    white: &'a wgpu::TextureView,
    flat_normal: &'a wgpu::TextureView,
    default_sampler: &'a wgpu::Sampler,
}

struct ModelBuilder<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    options: MaterialOptions,
    layout: &'a wgpu::BindGroupLayout,
    maps: Maps<'a>,
}

impl ModelBuilder<'_> {
    fn model(&self, model: &Model) -> ModelGpu {
        let mut views: HashMap<(usize, bool), wgpu::TextureView> = HashMap::new();
        let materials: Vec<wgpu::BindGroup> = model
            .materials
            .iter()
            .map(|material| self.material(model, material, &mut views))
            .collect();
        let parts = model
            .parts
            .iter()
            .map(|part| {
                let floats: Vec<f32> = part
                    .vertices
                    .iter()
                    .flat_map(|v| {
                        v.position
                            .into_iter()
                            .chain(v.normal)
                            .chain(v.uv)
                            .chain(v.tangent)
                            .chain(v.color)
                    })
                    .collect();
                PartGpu {
                    vertices: self
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("model_vertices"),
                            contents: &floats_to_bytes(&floats),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                    indices: self
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("model_indices"),
                            contents: &part
                                .indices
                                .iter()
                                .flat_map(|index| index.to_le_bytes())
                                .collect::<Vec<u8>>(),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                    index_count: part.indices.len() as u32,
                    material: materials[part.material].clone(),
                }
            })
            .collect();
        ModelGpu { parts }
    }

    /// Привязка материала: числа и четыре карты с их выборками.
    fn material(
        &self,
        model: &Model,
        material: &Material,
        views: &mut HashMap<(usize, bool), wgpu::TextureView>,
    ) -> wgpu::BindGroup {
        let mut view_of =
            |texture: Option<&Texture>, srgb: bool, fallback: &wgpu::TextureView| match texture {
                Some(texture) => {
                    let view = views
                        .entry((texture.image, srgb))
                        .or_insert_with(|| {
                            image_view(self.device, self.queue, &model.images[texture.image], srgb)
                        })
                        .clone();
                    (view, sampler(self.device, self.options, texture.sampler))
                }
                None => (fallback.clone(), self.maps.default_sampler.clone()),
            };
        let (normal_texture, normal_scale) = match &material.normal_texture {
            Some((texture, scale)) => (Some(texture), *scale),
            None => (None, 1.0),
        };
        let (occlusion_texture, occlusion_strength) = match &material.occlusion_texture {
            Some((texture, strength)) => (Some(texture), *strength),
            None => (None, 0.0),
        };
        let base = view_of(material.base_texture.as_ref(), true, self.maps.white);
        let metal = view_of(
            material.metal_rough_texture.as_ref(),
            false,
            self.maps.white,
        );
        let normal = view_of(normal_texture, false, self.maps.flat_normal);
        let occlusion = view_of(occlusion_texture, false, self.maps.white);
        let factors = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("model_material"),
                contents: &floats_to_bytes(&[
                    material.base_color[0],
                    material.base_color[1],
                    material.base_color[2],
                    material.base_color[3],
                    material.metallic,
                    material.roughness,
                    normal_scale,
                    occlusion_strength,
                ]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let entry = |binding: u32, resource| wgpu::BindGroupEntry { binding, resource };
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model_material_bind_group"),
            layout: self.layout,
            entries: &[
                entry(0, factors.as_entire_binding()),
                entry(1, wgpu::BindingResource::TextureView(&base.0)),
                entry(2, wgpu::BindingResource::Sampler(&base.1)),
                entry(3, wgpu::BindingResource::TextureView(&metal.0)),
                entry(4, wgpu::BindingResource::Sampler(&metal.1)),
                entry(5, wgpu::BindingResource::TextureView(&normal.0)),
                entry(6, wgpu::BindingResource::Sampler(&normal.1)),
                entry(7, wgpu::BindingResource::TextureView(&occlusion.0)),
                entry(8, wgpu::BindingResource::Sampler(&occlusion.1)),
            ],
        })
    }
}

fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("model_instances"),
        size: capacity as u64 * INSTANCE_STRIDE,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl ModelsGpu {
    /// Отправляет на видеокарту все модели `table`: сетки, картинки и материалы. Без моделей — только
    /// раскладка привязки материала, по которой строятся конвейеры.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        options: MaterialOptions,
        table: &ModelTable,
    ) -> ModelsGpu {
        let layout = material_layout(device);
        let white = solid_view(device, queue, [255; 4]);
        let flat_normal = solid_view(device, queue, [128, 128, 255, 255]);
        let default_sampler = sampler(device, options, Sampler::default());
        let builder = ModelBuilder {
            device,
            queue,
            options,
            layout: &layout,
            maps: Maps {
                white: &white,
                flat_normal: &flat_normal,
                default_sampler: &default_sampler,
            },
        };
        let models: Vec<ModelGpu> = table.iter().map(|model| builder.model(model)).collect();
        let capacity = 256;
        ModelsGpu {
            counts: vec![0; models.len()],
            models,
            instances: instance_buffer(device, capacity),
            capacity,
            scratch: Vec::new(),
            material_layout: layout,
        }
    }

    pub fn material_layout(&self) -> &wgpu::BindGroupLayout {
        &self.material_layout
    }

    /// Пишет объекты кадра в буфер: объекты каждой модели подряд, по порядку моделей.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[Vec<ModelInstance>],
    ) {
        let lists = instances.iter().take(self.models.len());
        let total: usize = lists.clone().map(Vec::len).sum();
        if total > self.capacity {
            self.capacity = total.next_power_of_two();
            self.instances = instance_buffer(device, self.capacity);
        }
        self.scratch.clear();
        self.counts.fill(0);
        for (count, list) in self.counts.iter_mut().zip(lists) {
            *count = list.len() as u32;
            for instance in list {
                for value in instance.placement.into_iter().chain(instance.base) {
                    self.scratch.extend(value.to_le_bytes());
                }
            }
        }
        if !self.scratch.is_empty() {
            queue.write_buffer(&self.instances, 0, &self.scratch);
        }
    }

    /// Рисует каждую часть каждой модели одной командой на все объекты модели; `materials` — привязывать ли
    /// материалы частей (в проходе теней их нет).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, materials: bool) {
        let mut first_byte = 0u64;
        for (model, &count) in self.models.iter().zip(&self.counts) {
            let bytes = u64::from(count) * INSTANCE_STRIDE;
            if count > 0 {
                for part in &model.parts {
                    pass.set_vertex_buffer(0, part.vertices.slice(..));
                    pass.set_vertex_buffer(1, self.instances.slice(first_byte..first_byte + bytes));
                    pass.set_index_buffer(part.indices.slice(..), wgpu::IndexFormat::Uint32);
                    if materials {
                        pass.set_bind_group(1, &part.material, &[]);
                    }
                    pass.draw_indexed(0..part.index_count, 0, 0..count);
                }
            }
            first_byte += bytes;
        }
    }
}
