use std::sync::Arc;

use iced::wgpu;
use iced::widget::shader::{self, Pipeline, Primitive, Viewport};
use iced::{Color, Element, Length, Rectangle, mouse};

use crate::app::{Message, MpvNe};

/// Cheaply-cloneable frame payload. The Arc-wrapped pixel buffer flows from
/// the player thread, into app state, then into the shader Program each draw.
#[derive(Clone)]
pub struct VideoFrame {
    pub pixels: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

impl std::fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pixels", &format_args!("<{} bytes>", self.pixels.len()))
            .finish()
    }
}

pub fn view(app: &MpvNe) -> Element<'_, Message> {
    if let Some(frame) = &app.current_frame {
        if app.player.path.is_some() && !app.stopped {
            return iced::widget::shader(VideoProgram { frame: frame.clone() })
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }
    }
    {
        let bg = Color::from_rgb(0.075, 0.085, 0.110);

        let logo = iced::widget::image(app.img_logo.clone())
            .width(Length::Fixed(160.0));

        iced::widget::container(logo)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .style(move |_| iced::widget::container::Style {
                background: Some(iced::Background::Color(bg)),
                ..Default::default()
            })
            .into()
    }
}

// ── shader Program ──────────────────────────────────────────────────────────

struct VideoProgram {
    frame: VideoFrame,
}

impl shader::Program<Message> for VideoProgram {
    type State = ();
    type Primitive = VideoPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        VideoPrimitive { frame: self.frame.clone() }
    }
}

// ── Primitive ───────────────────────────────────────────────────────────────

#[derive(Debug)]
struct VideoPrimitive {
    frame: VideoFrame,
}

impl Primitive for VideoPrimitive {
    type Pipeline = VideoPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        pipeline.upload(device, queue, &self.frame);
        pipeline.update_uniforms(queue, bounds, &self.frame);
    }

    /// iced sets the render pass viewport + scissor to our widget bounds
    /// before calling this, so a full-screen NDC quad maps to the widget.
    fn draw(
        &self,
        pipeline: &Self::Pipeline,
        render_pass: &mut wgpu::RenderPass<'_>,
    ) -> bool {
        pipeline.draw_into(render_pass)
    }
}

// ── Pipeline (created once, shared across all VideoPrimitive instances) ─────

pub struct VideoPipeline {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    texture: Option<TextureBundle>,
}

struct TextureBundle {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

impl Pipeline for VideoPipeline {
    fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("video shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_WGSL.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("video bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("video uniforms"),
            size: 16, // vec2<f32> padded to 16 bytes
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("video pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("video pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader_mod,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_mod,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("video sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self { pipeline, bgl, sampler, uniforms, texture: None }
    }
}

impl VideoPipeline {
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &VideoFrame) {
        let needs_recreate = match &self.texture {
            Some(t) => t.width != frame.width || t.height != frame.height,
            None => true,
        };

        if needs_recreate {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("video texture"),
                size: wgpu::Extent3d {
                    width: frame.width,
                    height: frame.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("video bg"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.uniforms.as_entire_binding(),
                    },
                ],
            });
            self.texture = Some(TextureBundle {
                texture,
                bind_group,
                width: frame.width,
                height: frame.height,
            });
        }

        let t = self.texture.as_ref().unwrap();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &t.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.width * 4),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Compute aspect-ratio-preserving scale factors and upload them.
    /// The fragment shader uses these to letterbox the video inside the widget.
    fn update_uniforms(
        &self,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        frame: &VideoFrame,
    ) {
        if frame.width == 0 || frame.height == 0 || bounds.width <= 0.0 || bounds.height <= 0.0
        {
            return;
        }
        let widget_aspect = bounds.width / bounds.height;
        let tex_aspect = frame.width as f32 / frame.height as f32;
        let (sx, sy) = if widget_aspect > tex_aspect {
            // Widget wider than video → pillarbox (black bars left/right).
            (tex_aspect / widget_aspect, 1.0)
        } else {
            // Widget taller than video → letterbox (black bars top/bottom).
            (1.0, widget_aspect / tex_aspect)
        };
        let data: [f32; 4] = [sx, sy, 0.0, 0.0];
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        queue.write_buffer(&self.uniforms, 0, &bytes);
    }

    fn draw_into(&self, pass: &mut wgpu::RenderPass<'_>) -> bool {
        let Some(t) = &self.texture else { return false };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &t.bind_group, &[]);
        pass.draw(0..6, 0..1);
        true
    }
}

// ── WGSL: passthrough textured fullscreen quad ──────────────────────────────

const SHADER_WGSL: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );
    var out: VsOut;
    out.pos = vec4<f32>(positions[i], 0.0, 1.0);
    out.uv = uvs[i];
    return out;
}

@group(0) @binding(0) var t_diffuse: texture_2d<f32>;
@group(0) @binding(1) var s_diffuse: sampler;

struct Uniforms {
    scale: vec2<f32>,
};
@group(0) @binding(2) var<uniform> u: Uniforms;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Map the widget UV into the video's UV space, centered.
    let tex_uv = (in.uv - vec2<f32>(0.5, 0.5)) / u.scale + vec2<f32>(0.5, 0.5);
    if (tex_uv.x < 0.0 || tex_uv.x > 1.0 || tex_uv.y < 0.0 || tex_uv.y > 1.0) {
        // BG_DEEPEST - matches the outer container so letterbox blends.
        return vec4<f32>(0.075, 0.085, 0.110, 1.0);
    }
    // Force alpha=1.0: iced uses PostMultiplied alpha on Windows DXGI, so any
    // alpha<1 pixel shows the desktop through. mpv may leave alpha unset in
    // letterbox regions of its own internal render.
    let c = textureSample(t_diffuse, s_diffuse, tex_uv);
    return vec4<f32>(c.rgb, 1.0);
}
"#;
