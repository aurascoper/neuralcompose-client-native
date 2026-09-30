use crate::recording::Recorder;
use eframe::{
    egui,
    egui_wgpu::{self, wgpu},
};
use neuralcompose_viz::signal::{Geometry, Vertex};
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniform {
    pub mvp: [[f32; 4]; 4],
    pub params: [f32; 4],
}

pub struct Resources {
    line: wgpu::RenderPipeline,
    particles: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampled: wgpu::BindGroup,
    sample_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    size: [u32; 2],
}
impl Resources {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("phase scene"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let sample_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let line_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&sample_layout)],
            immediate_size: 0,
        });
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        let make = |entry: &str,
                    fragment: &str,
                    topology,
                    fmt,
                    pl: &wgpu::PipelineLayout,
                    vertex: bool,
                    additive: bool| {
            let attrs = wgpu::vertex_attr_array![0=>Float32x4,1=>Float32];
            let buffers = [Some(wgpu::VertexBufferLayout {
                array_stride: 20,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attrs,
            })];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(entry),
                    buffers: if vertex { &buffers } else { &[] },
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: fmt,
                        blend: if additive {
                            Some(blend)
                        } else {
                            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING)
                        },
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let line = make(
            "line_vertex",
            "line_fragment",
            wgpu::PrimitiveTopology::LineStrip,
            wgpu::TextureFormat::Rgba16Float,
            &line_layout,
            true,
            true,
        );
        let particles = make(
            "particle_vertex",
            "particle_fragment",
            wgpu::PrimitiveTopology::TriangleList,
            wgpu::TextureFormat::Rgba16Float,
            &line_layout,
            false,
            true,
        );
        let composite = make(
            "screen_vertex",
            "bloom_fragment",
            wgpu::PrimitiveTopology::TriangleList,
            format,
            &composite_layout,
            false,
            false,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (texture, view, sampled) = Self::target(device, [1280, 800], &sample_layout, &sampler);
        let vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rolling samples"),
            size: (4096 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            line,
            particles,
            composite,
            vertices,
            uniform,
            bind,
            texture,
            view,
            sampled,
            sample_layout,
            sampler,
            size: [1280, 800],
        }
    }
    fn target(
        device: &wgpu::Device,
        size: [u32; 2],
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("linear light accumulation"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampled = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        (texture, view, sampled)
    }
}

pub struct Scene {
    pub geometry: Arc<Geometry>,
    pub uniform: Uniform,
    pub size: [u32; 2],
    pub particles: bool,
    pub recorder: Arc<Mutex<Recorder>>,
    pub epoch: Instant,
    pub received_ms: Option<u64>,
    pub received: u64,
    pub live: bool,
    pub skipped: u64,
}
impl egui_wgpu::CallbackTrait for Scene {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _: &egui_wgpu::ScreenDescriptor,
        _: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let r = resources.get_mut::<Resources>().unwrap();
        if r.size != self.size {
            (r.texture, r.view, r.sampled) =
                Resources::target(device, self.size, &r.sample_layout, &r.sampler);
            r.size = self.size;
        }
        queue.write_buffer(&r.uniform, 0, bytemuck::bytes_of(&self.uniform));
        if !self.geometry.vertices.is_empty() {
            queue.write_buffer(
                &r.vertices,
                0,
                bytemuck::cast_slice(&self.geometry.vertices),
            );
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("phase-space submission"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("phase-space"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &r.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &r.bind, &[]);
            if self.particles {
                pass.set_pipeline(&r.particles);
                pass.draw(0..6, 0..256);
            }
            pass.set_pipeline(&r.line);
            pass.set_vertex_buffer(0, r.vertices.slice(..));
            for strip in &self.geometry.strips {
                if strip.len() > 1 {
                    pass.draw(strip.clone(), 0..1);
                }
            }
        }
        // This timestamp follows an actual queue submission, not a UI callback.
        queue.submit([encoder.finish()]);
        let now = self.epoch.elapsed().as_secs_f64();
        let age = self.received_ms.map(|t| (now * 1000.0 - t as f64).max(0.0));
        let mut recorder = self.recorder.lock().unwrap();
        recorder.framebuffer = self.size;
        recorder.submit(
            now,
            age,
            self.received,
            self.live,
            self.geometry.clipped,
            self.geometry.examined,
            self.skipped,
        );
        Vec::new()
    }
    fn paint(
        &self,
        _: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let r = resources.get::<Resources>().unwrap();
        pass.set_pipeline(&r.composite);
        pass.set_bind_group(0, &r.sampled, &[]);
        pass.draw(0..3, 0..1);
    }
}
