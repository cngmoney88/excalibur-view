//! The drawing itself: a wgpu device of the view's own, the pipelines, and
//! reading the finished picture back.

use wgpu::util::DeviceExt;

const COLOUR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const PICK: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
const SAMPLES: u32 = 4;
/// Parts per row of the palette texture.
const PALETTE_WIDTH: u32 = 2048;

/// One corner of a triangle, as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    /// The normal, scaled to -127..127; zero when the mesh gave none.
    pub normal: [i8; 4],
    pub part: u32,
}

/// What every draw needs to know besides the triangles.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub eye: [f32; 4],
    pub forward: [f32; 4],
    pub key: [f32; 4],
    pub fill: [f32; 4],
    pub sky: [f32; 4],
    pub ground: [f32; 4],
    pub top: [f32; 4],
    pub bottom: [f32; 4],
    pub cut: [f32; 4],
    pub edge: [f32; 4],
}

/// A few hundred parts' triangles and edges, uploaded together.
pub struct Chunk {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    edges: Option<wgpu::Buffer>,
    edge_count: u32,
}

/// Render targets for one size, kept while the view stays that size.
struct Targets {
    width: u32,
    height: u32,
    many: wgpu::TextureView,
    one: wgpu::Texture,
    depth: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_row: u32,
}

struct PickTargets {
    width: u32,
    height: u32,
    ids: wgpu::Texture,
    depth: wgpu::Texture,
    readback: wgpu::Buffer,
}

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub adapter: String,
    pub largest: u32,
    part: wgpu::RenderPipeline,
    edge: wgpu::RenderPipeline,
    backdrop: wgpu::RenderPipeline,
    pick: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    palette: wgpu::Texture,
    bind: wgpu::BindGroup,
    targets: Option<Targets>,
    pick_targets: Option<PickTargets>,
}

impl Gpu {
    pub fn new() -> Result<Gpu, String> {
        // Direct3D, Metal or Vulkan first, and OpenGL only when none of those
        // has an adapter: the same order the window itself prefers.
        let ask = |backends: wgpu::Backends, fallback: bool| {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends, ..Default::default() });
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: fallback,
                compatible_surface: None,
            }))
        };
        let adapter = ask(wgpu::Backends::PRIMARY, false)
            .or_else(|_| ask(wgpu::Backends::PRIMARY, true))
            .or_else(|_| ask(wgpu::Backends::GL, false))
            .map_err(|e| format!("There is no graphics adapter to draw the model with ({e})."))?;
        let info = adapter.get_info();
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("3D view"),
            required_features: wgpu::Features::empty(),
            required_limits: limits.clone(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| format!("The graphics adapter wouldn't start for the 3D view ({e})."))?;
        // A mistake in a draw is logged, not a crash: wgpu's default is to
        // panic, which would take the view's thread down with it.
        device.on_uncaptured_error(Box::new(|error| log::error!("3D view: {error}")));

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3D view"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D view"),
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
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3D view"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Snorm8x4, 2 => Uint32],
        };
        let pipeline = |label: &str,
                        fragment: &str,
                        format: wgpu::TextureFormat,
                        topology: wgpu::PrimitiveTopology,
                        depth: wgpu::DepthStencilState,
                        samples: u32,
                        vertices: bool| {
            let buffers = [vertex_layout.clone()];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(if vertices { "vs_part" } else { "vs_backdrop" }),
                    compilation_options: Default::default(),
                    buffers: if vertices { &buffers } else { &[] },
                },
                primitive: wgpu::PrimitiveState { topology, cull_mode: None, ..Default::default() },
                depth_stencil: Some(depth),
                multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let depth = |write: bool, compare: wgpu::CompareFunction, bias: wgpu::DepthBiasState| wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: write,
            depth_compare: compare,
            stencil: Default::default(),
            bias,
        };
        // Depth is reversed, so nearer is greater. Faces are pushed back a
        // touch so the edges drawn along them win.
        let behind = wgpu::DepthBiasState { constant: -4, slope_scale: -1.5, clamp: 0.0 };
        let part = pipeline(
            "parts",
            "fs_part",
            COLOUR,
            wgpu::PrimitiveTopology::TriangleList,
            depth(true, wgpu::CompareFunction::Greater, behind),
            SAMPLES,
            true,
        );
        let edge = pipeline(
            "edges",
            "fs_edge",
            COLOUR,
            wgpu::PrimitiveTopology::LineList,
            depth(false, wgpu::CompareFunction::GreaterEqual, Default::default()),
            SAMPLES,
            true,
        );
        let backdrop = pipeline(
            "backdrop",
            "fs_backdrop",
            COLOUR,
            wgpu::PrimitiveTopology::TriangleList,
            depth(false, wgpu::CompareFunction::Always, Default::default()),
            SAMPLES,
            false,
        );
        let pick = pipeline(
            "picking",
            "fs_pick",
            PICK,
            wgpu::PrimitiveTopology::TriangleList,
            depth(true, wgpu::CompareFunction::Greater, Default::default()),
            1,
            true,
        );
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3D view globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let palette = palette_texture(&device, 1);
        let bind = bind_group(&device, &layout, &globals, &palette);
        Ok(Gpu {
            device,
            queue,
            adapter: format!("{} ({:?})", info.name, info.backend),
            largest: limits.max_texture_dimension_2d.min(8192),
            part,
            edge,
            backdrop,
            pick,
            layout,
            globals,
            palette,
            bind,
            targets: None,
            pick_targets: None,
        })
    }

    /// Every part's colour, by part number. Alpha 0 hides a part.
    pub fn set_palette(&mut self, colours: &[[u8; 4]]) {
        let rows = (colours.len() as u32).div_ceil(PALETTE_WIDTH).max(1);
        if self.palette.height() != rows {
            self.palette = palette_texture(&self.device, rows);
            self.bind = bind_group(&self.device, &self.layout, &self.globals, &self.palette);
        }
        let mut bytes = vec![0u8; (PALETTE_WIDTH * rows * 4) as usize];
        bytes[..colours.len() * 4].copy_from_slice(bytemuck::cast_slice(colours));
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.palette,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(PALETTE_WIDTH * 4), rows_per_image: Some(rows) },
            wgpu::Extent3d { width: PALETTE_WIDTH, height: rows, depth_or_array_layers: 1 },
        );
    }

    pub fn chunk(&self, vertices: &[Vertex], indices: &[u32], edges: &[u32]) -> Chunk {
        let buffer = |label: &str, contents: &[u8], usage: wgpu::BufferUsages| {
            self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents, usage })
        };
        let placeholder = [0u8; 4];
        Chunk {
            vertices: buffer(
                "vertices",
                if vertices.is_empty() { &placeholder } else { bytemuck::cast_slice(vertices) },
                wgpu::BufferUsages::VERTEX,
            ),
            indices: buffer(
                "triangles",
                if indices.is_empty() { &placeholder } else { bytemuck::cast_slice(indices) },
                wgpu::BufferUsages::INDEX,
            ),
            index_count: indices.len() as u32,
            edges: (!edges.is_empty()).then(|| buffer("edges", bytemuck::cast_slice(edges), wgpu::BufferUsages::INDEX)),
            edge_count: edges.len() as u32,
        }
    }

    /// Draws the chunks and reads the picture back: RGBA, sRGB, alpha
    /// premultiplied, top row first.
    pub fn draw(
        &mut self,
        chunks: &[Chunk],
        globals: &Globals,
        width: u32,
        height: u32,
        backdrop: bool,
        edges: bool,
    ) -> Result<Vec<u8>, String> {
        let (width, height) = (width.clamp(1, self.largest), height.clamp(1, self.largest));
        if !self.targets.as_ref().is_some_and(|t| t.width == width && t.height == height) {
            self.targets = Some(self.make_targets(width, height));
        }
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(globals));
        let targets = self.targets.as_ref().expect("made above");
        let resolved = targets.one.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3D view"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.many,
                    resolve_target: Some(&resolved),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.bind, &[]);
            if backdrop {
                pass.set_pipeline(&self.backdrop);
                pass.draw(0..3, 0..1);
            }
            pass.set_pipeline(&self.part);
            for chunk in chunks.iter().filter(|c| c.index_count > 0) {
                pass.set_vertex_buffer(0, chunk.vertices.slice(..));
                pass.set_index_buffer(chunk.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..chunk.index_count, 0, 0..1);
            }
            if edges {
                pass.set_pipeline(&self.edge);
                for chunk in chunks {
                    if let Some(edges) = &chunk.edges {
                        pass.set_vertex_buffer(0, chunk.vertices.slice(..));
                        pass.set_index_buffer(edges.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..chunk.edge_count, 0, 0..1);
                    }
                }
            }
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &targets.one,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &targets.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(targets.padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);
        let bytes = self.read(&targets.readback)?;
        let row = (width * 4) as usize;
        let mut out = Vec::with_capacity(row * height as usize);
        for y in 0..height as usize {
            let at = y * targets.padded_row as usize;
            out.extend_from_slice(&bytes[at..at + row]);
        }
        Ok(out)
    }

    /// The part at a pixel, if any.
    pub fn pick(&mut self, chunks: &[Chunk], globals: &Globals, width: u32, height: u32, x: u32, y: u32) -> Option<u32> {
        let (width, height) = (width.clamp(1, self.largest), height.clamp(1, self.largest));
        if x >= width || y >= height {
            return None;
        }
        if !self.pick_targets.as_ref().is_some_and(|t| t.width == width && t.height == height) {
            self.pick_targets = Some(self.make_pick_targets(width, height));
        }
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(globals));
        let targets = self.pick_targets.as_ref().expect("made above");
        let ids = targets.ids.create_view(&Default::default());
        let depth = targets.depth.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("picking"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &ids,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.bind, &[]);
            pass.set_pipeline(&self.pick);
            // Only the pixel asked about matters.
            pass.set_scissor_rect(x, y, 1, 1);
            for chunk in chunks.iter().filter(|c| c.index_count > 0) {
                pass.set_vertex_buffer(0, chunk.vertices.slice(..));
                pass.set_index_buffer(chunk.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..chunk.index_count, 0, 0..1);
            }
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &targets.ids,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &targets.readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(1) },
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);
        let bytes = self.read(&targets.readback).ok()?;
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]).checked_sub(1)
    }

    fn read(&self, buffer: &wgpu::Buffer) -> Result<Vec<u8>, String> {
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::PollType::Wait).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let bytes = slice.get_mapped_range().to_vec();
        buffer.unmap();
        Ok(bytes)
    }

    fn make_targets(&self, width: u32, height: u32) -> Targets {
        let texture = |label: &str, format: wgpu::TextureFormat, samples: u32, usage: wgpu::TextureUsages| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let padded_row = (width * 4).div_ceil(256) * 256;
        Targets {
            width,
            height,
            many: texture("picture, sampled", COLOUR, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT)
                .create_view(&Default::default()),
            one: texture(
                "picture",
                COLOUR,
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            ),
            depth: texture("depth", DEPTH, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT)
                .create_view(&Default::default()),
            readback: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("picture readback"),
                size: padded_row as u64 * height as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            padded_row,
        }
    }

    fn make_pick_targets(&self, width: u32, height: u32) -> PickTargets {
        let texture = |label: &str, format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        PickTargets {
            width,
            height,
            ids: texture("part numbers", PICK, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC),
            depth: texture("picking depth", DEPTH, wgpu::TextureUsages::RENDER_ATTACHMENT),
            readback: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("picking readback"),
                size: 256,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }

    /// Lets go of the render targets, after a picture far bigger than the
    /// view.
    pub fn forget_targets(&mut self) {
        self.targets = None;
    }
}

fn palette_texture(device: &wgpu::Device, rows: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("palette"),
        size: wgpu::Extent3d { width: PALETTE_WIDTH, height: rows, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    palette: &wgpu::Texture,
) -> wgpu::BindGroup {
    let view = palette.create_view(&Default::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("3D view"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
        ],
    })
}
