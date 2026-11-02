//! Drawing the owned particles once the world is drawn: one shared atlas,
//! one pipeline per colour format, one vertex buffer sized for the hard
//! particle limit, one draw call. Depth is tested in the shader against the
//! world's (reversed) depth, so nothing shows through terrain, whatever the
//! world's render scale.

use crate::assets::{Atlas, Orientation};
use crate::config::HARD_MAX_ACTIVE;
use crate::diagnostics::Counters;
use crate::particles::Instance;
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;

const MAX_QUADS: usize = HARD_MAX_ACTIVE as usize;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    uv: [f32; 2],
    rgba: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [f32; 16],
    /// depth size / colour size, for reading the world's depth.
    depth_scale: [f32; 2],
    depth_max: [f32; 2],
    /// x: 1 tests the world's depth, 0 skips it (debug only).
    mode: [f32; 4],
}

const SHADER: &str = r#"
struct U {
    view_proj: mat4x4<f32>,
    depth_scale: vec2<f32>,
    depth_max: vec2<f32>,
    mode: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
@group(0) @binding(3) var scene_depth: texture_depth_2d;

struct V {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) rgba: vec4<f32>,
};

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) rgba: vec4<f32>) -> V {
    var out: V;
    out.pos = u.view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.rgba = rgba;
    return out;
}

@fragment
fn fs(in: V) -> @location(0) vec4<f32> {
    let texel = textureSample(atlas, atlas_sampler, in.uv);
    let c = texel * in.rgba;
    if (c.a < 0.02) {
        discard;
    }
    // Reversed depth: the world's nearer surfaces are larger.
    let d = vec2<i32>(clamp(floor(in.pos.xy * u.depth_scale), vec2<f32>(0.0), u.depth_max));
    let scene = textureLoad(scene_depth, d, 0);
    if (u.mode.x > 0.5 && in.pos.z < scene) {
        discard;
    }
    return c;
}
"#;

struct Resources {
    device: wgpu::Device,
    atlas_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    module: wgpu::ShaderModule,
    pipelines: Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    uniforms: wgpu::Buffer,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    group: Option<(wgpu::TextureView, wgpu::BindGroup)>,
}

/// GPU state, rebuilt when the device changes.
#[derive(Default)]
pub struct Gpu {
    res: Option<Resources>,
    scratch: Vec<Vertex>,
    order: Vec<(f32, u32)>,
    last_log: Option<std::time::Instant>,
    started: Option<std::time::Instant>,
    pub debug: Debug,
}

/// Developer diagnostics, from `[diagnostics] collect_frame_trace`: log the
/// first quad's clip position every two seconds, and alternate the world
/// depth test off and on every two seconds to tell a depth problem from a
/// geometry one. Off in every normal game.
#[derive(Clone, Copy, Default)]
pub struct Debug {
    pub trace: bool,
}

fn mul(m: &[f32; 16], v: [f32; 4]) -> [f32; 4] {
    let mut o = [0.0; 4];
    for (r, out) in o.iter_mut().enumerate() {
        *out = m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2] + m[12 + r] * v[3];
    }
    o
}

/// The frame's targets, copied out of the game's `WorldDrawn`.
pub struct Target<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub color_view: &'a wgpu::TextureView,
    pub color_format: wgpu::TextureFormat,
    pub size: [u32; 2],
    pub depth: &'a wgpu::TextureView,
    pub depth_size: [u32; 2],
    /// Camera-relative world to clip, column-major, without TAA jitter.
    pub view_proj: [f32; 16],
    pub camera: [f64; 3],
}

impl Gpu {
    fn depth_off_now(&mut self) -> bool {
        if !self.debug.trace {
            return false;
        }
        let start = *self.started.get_or_insert_with(std::time::Instant::now);
        (start.elapsed().as_secs() / 2) % 2 == 1
    }

    /// Drops every GPU object (world left, device lost).
    pub fn release(&mut self) {
        self.res = None;
    }

    pub fn draw(
        &mut self,
        atlas: &Arc<Atlas>,
        instances: &[Instance],
        t: Target<'_>,
        c: &mut Counters,
    ) {
        if instances.is_empty() || t.size[0] == 0 || t.size[1] == 0 || t.depth_size[0] == 0 {
            return;
        }
        let depth_off = self.depth_off_now();
        if self.res.as_ref().is_some_and(|r| r.device != *t.device) {
            self.res = None;
        }
        let res = self
            .res
            .get_or_insert_with(|| Resources::new(t.device, t.queue, atlas, c));
        let pipeline = res.pipeline(t.color_format, c);
        res.prepare_group(t.depth);
        let res = &*res;
        let pipeline = &res.pipelines[pipeline].1;
        let group = &res.group.as_ref().expect("prepared").1;
        let n = instances.len().min(MAX_QUADS);

        // Back to front, so translucent edges blend over what is behind.
        let cam = t.camera;
        self.order.clear();
        self.order
            .extend(instances[..n].iter().enumerate().map(|(i, p)| {
                let d = [
                    (p.pos[0] - cam[0]) as f32,
                    (p.pos[1] - cam[1]) as f32,
                    (p.pos[2] - cam[2]) as f32,
                ];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2], i as u32)
            }));
        self.order
            .sort_unstable_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let m = t.view_proj;
        let right = normalize([m[0], m[4], m[8]]);
        let up = normalize([m[1], m[5], m[9]]);
        self.scratch.clear();
        for &(_, i) in &self.order {
            let p = &instances[i as usize];
            let center = [
                (p.pos[0] - cam[0]) as f32,
                (p.pos[1] - cam[1]) as f32,
                (p.pos[2] - cam[2]) as f32,
            ];
            if !center.iter().all(|v| v.is_finite()) || !p.half.is_finite() || !p.rot.is_finite() {
                continue;
            }
            let (s, co) = p.rot.sin_cos();
            let (ax, ay) = match p.orientation {
                Orientation::Billboard => (
                    add(scale(right, co), scale(up, s)),
                    add(scale(right, -s), scale(up, co)),
                ),
                Orientation::Surface => ([co, 0.0, s], [-s, 0.0, co]),
            };
            let (ax, ay) = (scale(ax, p.half), scale(ay, p.half));
            let [u0, v0, u1, v1] = p.uv;
            let corner = |sx: f32, sy: f32, u: f32, v: f32| Vertex {
                pos: add(center, add(scale(ax, sx), scale(ay, sy))),
                uv: [u, v],
                rgba: p.rgba,
            };
            self.scratch.extend_from_slice(&[
                corner(-1.0, -1.0, u0, v1),
                corner(1.0, -1.0, u1, v1),
                corner(1.0, 1.0, u1, v0),
                corner(-1.0, 1.0, u0, v0),
            ]);
        }
        if self.scratch.is_empty() {
            return;
        }
        if self.debug.trace {
            let now = std::time::Instant::now();
            if self
                .last_log
                .is_none_or(|t| now.duration_since(t).as_secs() >= 2)
            {
                self.last_log = Some(now);
                let v = self.scratch[0].pos;
                let clip = mul(&m, [v[0], v[1], v[2], 1.0]);
                log::warn!(
                    target: crate::runtime::LOG_TARGET,
                    "[visuality_horizon] trace: depth test {}, {} quads, first rel {:?} clip {:?} ndc_z {:.6} size {:?} depth {:?} fmt {:?} tick {}us frame {}us",
                    if depth_off { "off" } else { "on" },
                    self.scratch.len() / 4,
                    v,
                    clip,
                    clip[2] / clip[3],
                    t.size,
                    t.depth_size,
                    t.color_format,
                    c.last_tick_us,
                    c.last_frame_us
                );
            }
        }
        let uniforms = Uniforms {
            view_proj: t.view_proj,
            depth_scale: [
                t.depth_size[0] as f32 / t.size[0] as f32,
                t.depth_size[1] as f32 / t.size[1] as f32,
            ],
            depth_max: [t.depth_size[0] as f32 - 1.0, t.depth_size[1] as f32 - 1.0],
            mode: [if depth_off { 0.0 } else { 1.0 }, 0.0, 0.0, 0.0],
        };
        t.queue
            .write_buffer(&res.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let bytes: &[u8] = bytemuck::cast_slice(&self.scratch);
        t.queue.write_buffer(&res.vertices, 0, bytes);
        c.uploaded_bytes += (bytes.len() + std::mem::size_of::<Uniforms>()) as u64;
        c.vertices += self.scratch.len() as u64;
        c.draws += 1;

        let mut pass = t.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Visuality particles"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: t.color_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.set_vertex_buffer(0, res.vertices.slice(..bytes.len() as u64));
        pass.set_index_buffer(res.indices.slice(..), wgpu::IndexFormat::Uint16);
        let quads = (self.scratch.len() / 4) as u32;
        pass.draw_indexed(0..quads * 6, 0, 0..1);
    }
}

impl Resources {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, atlas: &Atlas, c: &mut Counters) -> Self {
        let size = wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        };
        // The atlas holds sRGB values and the frame target is gamma-space
        // 8-bit: sample and write them unchanged.
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Visuality atlas"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            &atlas.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas.width * 4),
                rows_per_image: Some(atlas.height),
            },
            size,
        );
        c.atlas_uploads += 1;
        c.uploaded_bytes += atlas.rgba.len() as u64;
        let atlas_view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Visuality nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Visuality"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Visuality"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Visuality"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Visuality uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Visuality vertices"),
            size: (MAX_QUADS * 4 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let quad_indices: Vec<u16> = (0..MAX_QUADS as u16)
            .flat_map(|q| {
                let b = q * 4;
                [b, b + 1, b + 2, b, b + 2, b + 3]
            })
            .collect();
        let indices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Visuality indices"),
            size: (quad_indices.len() * 2) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&indices, 0, bytemuck::cast_slice(&quad_indices));
        Resources {
            device: device.clone(),
            atlas_view,
            sampler,
            layout,
            pipeline_layout,
            module,
            pipelines: Vec::new(),
            uniforms,
            vertices,
            indices,
            group: None,
        }
    }

    fn pipeline(&mut self, format: wgpu::TextureFormat, c: &mut Counters) -> usize {
        if let Some(i) = self.pipelines.iter().position(|(f, _)| *f == format) {
            return i;
        }
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Unorm8x4];
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Visuality particles"),
                layout: Some(&self.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attrs,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        c.pipelines_built += 1;
        self.pipelines.push((format, pipeline));
        self.pipelines.len() - 1
    }

    /// The bind group for this depth view, rebuilt only when the view changes.
    fn prepare_group(&mut self, depth: &wgpu::TextureView) {
        let stale = self.group.as_ref().is_none_or(|(v, _)| v != depth);
        if stale {
            let g = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Visuality"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.atlas_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(depth),
                    },
                ],
            });
            self.group = Some((depth.clone(), g));
        }
    }
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if l > 1e-6 && l.is_finite() {
        scale(a, 1.0 / l)
    } else {
        [1.0, 0.0, 0.0]
    }
}
