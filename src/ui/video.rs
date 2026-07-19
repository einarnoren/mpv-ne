use std::sync::Arc;

use iced::wgpu;
use iced::widget::shader::{self, Pipeline, Primitive, Viewport};
use iced::{Color, Element, Length, Rectangle, mouse};

use crate::app::{FrameMode, Message, MpvNe, Projection, StereoOutput, StereoSource};

/// Cheaply-cloneable frame payload. The Arc-wrapped pixel buffer flows from
/// the player thread, into app state, then into the shader Program each draw.
#[derive(Clone)]
pub struct VideoFrame {
    pub pixels: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    /// Monotonic id assigned when the frame is created (see
    /// `MpvNe::next_frame_seq`) - lets the GPU pipeline tell whether it's
    /// already uploaded this exact frame without relying on pointer
    /// identity, which a freed-then-reused `Arc` allocation could alias.
    pub seq: u64,
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
            return iced::widget::shader(VideoProgram {
                frame: frame.clone(),
                mode: app.frame_mode,
                stereo_source: app.video_stereo_source,
                stereo_output: app.video_stereo_output,
                projection: app.video_projection,
                vr_yaw: app.vr_yaw,
                vr_pitch: app.vr_pitch,
                vr_fov_deg: app.vr_fov_deg,
            })
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
    mode: FrameMode,
    stereo_source: StereoSource,
    stereo_output: StereoOutput,
    projection: Projection,
    vr_yaw: f32,
    vr_pitch: f32,
    vr_fov_deg: f32,
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
        VideoPrimitive {
            frame: self.frame.clone(),
            mode: self.mode,
            stereo_source: self.stereo_source,
            stereo_output: self.stereo_output,
            projection: self.projection,
            vr_yaw: self.vr_yaw,
            vr_pitch: self.vr_pitch,
            vr_fov_deg: self.vr_fov_deg,
        }
    }
}

// ── Primitive ───────────────────────────────────────────────────────────────

#[derive(Debug)]
struct VideoPrimitive {
    frame: VideoFrame,
    mode: FrameMode,
    stereo_source: StereoSource,
    stereo_output: StereoOutput,
    projection: Projection,
    vr_yaw: f32,
    vr_pitch: f32,
    vr_fov_deg: f32,
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
        pipeline.update_uniforms(
            queue, bounds, &self.frame, self.mode, self.stereo_source, self.stereo_output,
            self.projection, self.vr_yaw, self.vr_pitch, self.vr_fov_deg,
        );
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
    /// `seq` of the last frame actually uploaded to the GPU texture.
    /// `view()` re-clones the current `VideoFrame` on every redraw (cursor
    /// moves, menu opens, anything), not just when mpv delivers a new
    /// frame - without this, every one of those redraws re-uploaded the
    /// same still-unchanged pixel buffer to the GPU for nothing.
    last_uploaded: Option<u64>,
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
            // v0: scale (vec2<f32>) + stereo_source + stereo_output
            // v1: yaw, pitch, tan_half_fov, widget_aspect
            // v2: projection, unused x3
            size: 48,
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

        Self { pipeline, bgl, sampler, uniforms, texture: None, last_uploaded: None }
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

        // Skip the upload entirely if this exact frame (by Arc identity) is
        // already sitting in the texture - `view()` clones the VideoFrame on
        // every redraw regardless of whether mpv delivered a new one, so
        // most redraws (cursor moves, menu opens, etc.) would otherwise
        // re-upload identical pixel data to the GPU for nothing.
        if !needs_recreate && self.last_uploaded == Some(frame.seq) {
            return;
        }
        self.last_uploaded = Some(frame.seq);

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
    #[allow(clippy::too_many_arguments)]
    fn update_uniforms(
        &self,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        frame: &VideoFrame,
        mode: FrameMode,
        stereo_source: StereoSource,
        stereo_output: StereoOutput,
        projection: Projection,
        vr_yaw: f32,
        vr_pitch: f32,
        vr_fov_deg: f32,
    ) {
        if frame.width == 0 || frame.height == 0 || bounds.width <= 0.0 || bounds.height <= 0.0
        {
            return;
        }
        let widget_aspect = bounds.width / bounds.height;
        // A stereoscopic source packs two eyes into one frame - the fit/fill
        // math below needs to letterbox against a single eye's aspect ratio,
        // not the whole packed frame's.
        let (eye_w, eye_h) = match stereo_source {
            StereoSource::Mono => (frame.width as f32, frame.height as f32),
            StereoSource::SideBySide => (frame.width as f32 / 2.0, frame.height as f32),
            StereoSource::OverUnder => (frame.width as f32, frame.height as f32 / 2.0),
        };
        let tex_aspect = eye_w / eye_h;
        let wider = widget_aspect > tex_aspect;
        // The shader samples tex_uv = (uv - 0.5) / scale + 0.5: scale < 1
        // letterboxes that axis, scale > 1 zooms in and crops it, scale == 1
        // maps the axis edge-to-edge.
        // A VR projection casts a perspective ray per pixel instead of
        // fitting/filling a flat frame - it always fills the whole widget,
        // so the letterbox scale is simply identity in that mode.
        let (sx, sy) = if projection != Projection::Flat {
            (1.0, 1.0)
        } else {
            match mode {
                // Fit (contain): whole frame visible, bars on the limiting axis.
                FrameMode::Fit if wider => (tex_aspect / widget_aspect, 1.0),
                FrameMode::Fit => (1.0, widget_aspect / tex_aspect),
                // Fill (cover): scale up to cover, crop overflow — branches swapped.
                FrameMode::Fill if wider => (1.0, widget_aspect / tex_aspect),
                FrameMode::Fill => (tex_aspect / widget_aspect, 1.0),
                // Stretch: map both axes edge-to-edge, distorting the aspect ratio.
                FrameMode::Stretch => (1.0, 1.0),
            }
        };
        let stereo_source_code = match stereo_source {
            StereoSource::Mono => 0.0,
            StereoSource::SideBySide => 1.0,
            StereoSource::OverUnder => 2.0,
        };
        let stereo_output_code = match stereo_output {
            StereoOutput::LeftEye => 0.0,
            StereoOutput::RightEye => 1.0,
            StereoOutput::AnaglyphRedCyan => 2.0,
        };
        let projection_code: f32 = match projection {
            Projection::Flat => 0.0,
            Projection::Equirect360 => 1.0,
            Projection::Equirect180 => 2.0,
        };
        let tan_half_fov = (vr_fov_deg.to_radians() * 0.5).tan();
        let data: [f32; 12] = [
            sx, sy, stereo_source_code, stereo_output_code,
            vr_yaw, vr_pitch, tan_half_fov, widget_aspect,
            projection_code, 0.0, 0.0, 0.0,
        ];
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
    // Packed as 3 explicit vec4s rather than individual scalar fields so
    // the WGSL/CPU-side layouts stay trivially in sync (a flat [f32; 12]
    // array on the Rust side, no implicit std140-style padding to reason
    // about).
    v0: vec4<f32>, // scale.x, scale.y, stereo_source, stereo_output
    v1: vec4<f32>, // vr_yaw, vr_pitch, tan_half_fov, widget_aspect
    v2: vec4<f32>, // projection, unused, unused, unused
};
@group(0) @binding(2) var<uniform> u: Uniforms;

const PI: f32 = 3.14159265359;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var eye_uv: vec2<f32>;
    let projection = i32(round(u.v2.x));

    if (projection == 0) {
        // Flat: map the widget UV into a single eye's UV space, centered -
        // letterbox bars use this same eye-space rectangle regardless of
        // stereo mode (update_uniforms already computed `scale` against one
        // eye's aspect).
        eye_uv = (in.uv - vec2<f32>(0.5, 0.5)) / u.v0.xy + vec2<f32>(0.5, 0.5);
        if (eye_uv.x < 0.0 || eye_uv.x > 1.0 || eye_uv.y < 0.0 || eye_uv.y > 1.0) {
            // BG_DEEPEST - matches the outer container so letterbox blends.
            return vec4<f32>(0.075, 0.085, 0.110, 1.0);
        }
    } else {
        // VR: cast a perspective ray through this pixel (a virtual camera
        // looking into the equirectangular panorama) and convert it to
        // spherical coordinates to sample the source. This fills the whole
        // widget - no letterboxing concept applies to a VR view.
        let ndc = in.uv * 2.0 - vec2<f32>(1.0, 1.0);
        let tan_half_fov = u.v1.z;
        let aspect = u.v1.w;
        let dir_cam = normalize(vec3<f32>(ndc.x * tan_half_fov * aspect, -ndc.y * tan_half_fov, -1.0));

        let yaw = u.v1.x;
        let pitch = u.v1.y;
        let cy = cos(yaw); let sy = sin(yaw);
        let cp = cos(pitch); let sp = sin(pitch);
        // Pitch (around X) then yaw (around Y) - standard FPS-camera order.
        let d1 = vec3<f32>(dir_cam.x, dir_cam.y * cp - dir_cam.z * sp, dir_cam.y * sp + dir_cam.z * cp);
        let dir = vec3<f32>(d1.x * cy + d1.z * sy, d1.y, -d1.x * sy + d1.z * cy);

        let theta = atan2(dir.x, -dir.z); // longitude, -PI..PI
        let phi = asin(clamp(dir.y, -1.0, 1.0)); // latitude, -PI/2..PI/2

        if (projection == 2) {
            // 180 half-sphere: only the front hemisphere has content -
            // looking behind the camera shows nothing.
            if (abs(theta) > PI * 0.5) {
                return vec4<f32>(0.075, 0.085, 0.110, 1.0);
            }
            eye_uv = vec2<f32>(theta / PI + 0.5, 0.5 - phi / PI);
        } else {
            // 360 full sphere: wrap horizontally with fract() rather than
            // relying on the sampler's address mode, so the wrap stays
            // inside this one eye's sub-image instead of bleeding into the
            // other eye's half for a stereoscopic 360 source.
            eye_uv = vec2<f32>(fract(theta / (2.0 * PI) + 0.5), 0.5 - phi / PI);
        }
    }

    // Force alpha=1.0: iced uses PostMultiplied alpha on Windows DXGI, so any
    // alpha<1 pixel shows the desktop through. mpv may leave alpha unset in
    // letterbox regions of its own internal render.
    let src = i32(round(u.v0.z));
    if (src == 0) {
        let c = textureSample(t_diffuse, s_diffuse, eye_uv);
        return vec4<f32>(c.rgb, 1.0);
    }

    // Stereoscopic: eye_uv addresses a single eye's frame - remap it into
    // the packed source texture's left/right half.
    var left_uv: vec2<f32>;
    var right_uv: vec2<f32>;
    if (src == 1) {
        left_uv = vec2<f32>(eye_uv.x * 0.5, eye_uv.y);
        right_uv = vec2<f32>(eye_uv.x * 0.5 + 0.5, eye_uv.y);
    } else {
        left_uv = vec2<f32>(eye_uv.x, eye_uv.y * 0.5);
        right_uv = vec2<f32>(eye_uv.x, eye_uv.y * 0.5 + 0.5);
    }

    let outm = i32(round(u.v0.w));
    if (outm == 2) {
        let l = textureSample(t_diffuse, s_diffuse, left_uv);
        let r = textureSample(t_diffuse, s_diffuse, right_uv);
        return vec4<f32>(l.r, r.g, r.b, 1.0);
    }
    let coord = select(right_uv, left_uv, outm == 0);
    let c = textureSample(t_diffuse, s_diffuse, coord);
    return vec4<f32>(c.rgb, 1.0);
}
"#;
