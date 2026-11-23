//! wgpu 3D Ã¦Â¸Â²Ã¦Å¸â€œÃ¯Â¼Å¡Ã©â‚¬Å¡Ã¨Â¿â€¡ egui Ã§Å¡â€ž PaintCallback Ã¥Å“Â¨Ã¨Â§â€ Ã¥ÂÂ£Ã¥Å’ÂºÃ¥Å¸Å¸Ã§Â»ËœÃ¥Ë†Â¶Ã£â‚¬â€š
//!
//! Ã¥Å“ÂºÃ¦â„¢Â¯Ã§â€Â± app Ã¥Â±â€šÃ¦Â¯ÂÃ¥Â¸Â§Ã¦â€ºÂ´Ã¦â€“Â°Ã¯Â¼Ë†`SceneData` + Ã§â€°Ë†Ã¦Å“Â¬Ã¥ÂÂ·Ã¯Â¼â€°Ã¯Â¼â€ºGPU Ã§Â¼â€œÃ¥â€ Â²Ã¥Å“Â¨Ã§â€°Ë†Ã¦Å“Â¬Ã¥ÂËœÃ¥Å’â€“Ã¦â€”Â¶Ã©â€¡ÂÃ¥Â»ÂºÃ£â‚¬â€š
//! Ã¦Â¸Â²Ã¦Å¸â€œÃ¥â€ â€¦Ã¥Â®Â¹Ã¯Â¼Å¡Ã¤Â¸â€°Ã¨Â§â€™Ã§Â½â€˜Ã¦Â Â¼Ã¯Â¼Ë†Phong Ã¥â€¦â€°Ã§â€¦Â§Ã¯Â¼â€°+ Ã§ÂºÂ¿Ã¦Â¡â€ Ã¯Â¼Ë†Ã§Â½â€˜Ã¦Â Â¼Ã¥Å“Â°Ã©ÂÂ¢/Ã¥ÂÂÃ¦Â â€¡Ã¨Â½Â´/Ã¨Ââ€°Ã¥â€ºÂ¾Ã§ÂºÂ¿Ã¯Â¼â€°Ã£â‚¬â€š

use crate::camera::Camera;
use egui_wgpu;
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;
use glam::{DVec3, Mat4, Vec3};
use std::sync::{Arc, Mutex};
#[path = "render_offscreen.rs"]
mod offscreen;
pub use offscreen::render_png;

/// Ã¤Â¸â‚¬Ã¤Â¸ÂªÃ¥ÂÂ¯Ã¦Â¸Â²Ã¦Å¸â€œÃ§Â½â€˜Ã¦Â Â¼Ã¤Â½â€œÃ¯Â¼Ë†CPU Ã¤Â¾Â§Ã¯Â¼Å’world Ã¥ÂÂÃ¦Â â€¡Ã¯Â¼Å’Ã¦Â³â€¢Ã§ÂºÂ¿Ã¥Â·Â²Ã§Â®â€”Ã¥Â¥Â½Ã¯Â¼â€°Ã£â‚¬â€š
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SceneMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub color: [f32; 3],
    /// Ã§ÂºÂ¿Ã¦Â¡â€ Ã©Â«ËœÃ¤ÂºÂ®Ã¯Â¼Ë†Ã©â‚¬â€°Ã¤Â¸Â­/Ã¦â€šÂ¬Ã¥ÂÅ“Ã¦â€”Â¶Ã¥ÂÂ Ã¥Å Â Ã¯Â¼â€°Ã£â‚¬â€š
    pub highlight: bool,
}

/// Ã§ÂºÂ¿Ã¦Â®ÂµÃ©â€ºâ€ Ã¯Â¼Ë†Ã¦Ë†ÂÃ¥Â¯Â¹Ã©Â¡Â¶Ã§â€šÂ¹Ã¯Â¼â€°Ã£â‚¬â€š
#[derive(Debug, Clone, Default)]
pub struct SceneLines {
    pub positions: Vec<[f32; 3]>,
    pub color: [f32; 3],
}

#[derive(Debug, Clone, Default)]
pub struct ScenePlane {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub color: [f32; 3],
    pub opacity: f32,
}

/// Keep sketch geometry in world f64 until the shared render origin is removed.
/// This is deliberately separate from legacy tessellation, whose f32 positions
/// have already lost precision before they reach the renderer.
#[derive(Debug, Clone, Default)]
pub struct SceneRegionSurface {
    pub positions: Vec<[f64; 3]>,
    pub indices: Vec<u32>,
    pub color: [f32; 3],
    pub opacity: f32,
}

#[derive(Debug, Clone, Default)]
pub struct SceneRegionLines {
    pub positions: Vec<[f64; 3]>,
    pub color: [f32; 3],
}

/// One frame for every scene layer and the camera. Never rebase just an overlay.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct RenderFrame {
    origin: DVec3,
}

impl RenderFrame {
    fn for_camera(camera: &Camera) -> Self {
        // A cell is about 1% of the view distance: stable while navigating, but
        // small enough that rebased sketch details retain viewport precision.
        // A fixed large cell would still erase tiny features when zoomed in.
        let target = camera.target.as_dvec3();
        let cell = 2.0_f64.powf(
            (f64::from(camera.distance()) * 0.01)
                .max(1e-6)
                .log2()
                .floor(),
        );
        let origin = if target.is_finite() {
            (target / cell).round() * cell
        } else {
            DVec3::ZERO
        };
        Self { origin }
    }

    fn world64(self, point: [f64; 3]) -> [f32; 3] {
        (DVec3::from_array(point) - self.origin)
            .as_vec3()
            .to_array()
    }

    fn world32(self, point: [f32; 3]) -> [f32; 3] {
        self.world64(point.map(f64::from))
    }
}

fn region_depth_bias(
    scene: &SceneData,
    frame: RenderFrame,
    view_proj: Mat4,
    viewport: [f32; 2],
) -> f32 {
    // A region selection belongs to one sketch plane. Derive that plane from
    // precise geometry, including the outline-only case after a fill diagnostic.
    let points = scene
        .region_surfaces
        .iter()
        .find(|s| s.positions.len() >= 3)
        .map(|s| s.positions.as_slice())
        .unwrap_or(&scene.region_lines.positions);
    let Some(first) = points.first().copied().map(DVec3::from_array) else {
        return 0.0;
    };
    let Some(second) = points
        .iter()
        .copied()
        .map(DVec3::from_array)
        .find(|p| (*p - first).length_squared() > 0.0)
    else {
        return 0.0;
    };
    let edge = second - first;
    let Some(normal) = points
        .iter()
        .copied()
        .map(DVec3::from_array)
        .map(|p| edge.cross(p - first))
        .find(|n| n.length_squared() > 0.0)
        .and_then(|n| n.try_normalize())
    else {
        return 0.0;
    };
    let point = (first - frame.origin).as_vec3();
    let normal = normal.as_vec3();
    let clip_plane = view_proj.inverse().transpose() * normal.extend(-normal.dot(point));
    if !clip_plane.is_finite() || clip_plane.z.abs() <= 1e-8 {
        return 0.0;
    }
    // z_ndc = -(a*x_ndc + b*y_ndc + d)/c. A 0.75-pixel
    // Manhattan bound covers line coverage and fixed-point vertex snapping.
    let slope = (clip_plane.x / clip_plane.z).abs() * 2.0 / viewport[0].max(1.0)
        + (clip_plane.y / clip_plane.z).abs() * 2.0 / viewport[1].max(1.0);
    if slope.is_finite() {
        slope * 0.75
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct SceneData {
    pub meshes: Vec<SceneMesh>,
    /// Two grid weights plus independent axes reproduce the layered CAD
    /// workspace used by Super3D without coupling grid styling to geometry.
    pub grid_minor_lines: SceneLines,
    pub grid_major_lines: SceneLines,
    pub grid_axis_lines: Vec<SceneLines>,
    /// Subtle sharp-edge outlines; selected bodies use the orange CAD accent.
    pub body_edge_lines: Vec<SceneLines>,
    pub sketch_lines: SceneLines,
    /// Orange dashed geometry shown while a sketch entity is being placed/edited.
    pub sketch_preview_lines: SceneLines,
    /// Sketch endpoints/midpoints/centers, rendered as compact red markers.
    pub sketch_point_lines: SceneLines,
    pub highlight_lines: SceneLines,
    /// Selected regions use identical depth-tested coverage for fill and outline.
    pub region_surfaces: Vec<SceneRegionSurface>,
    pub region_lines: SceneRegionLines,
    /// Unselected sketch boundaries can retain f64 through the same frame.
    pub world_sketch_lines: SceneRegionLines,
    /// 3D 棱选择高亮：参与深度测试，避免小孔背面轮廓穿透实体“泄露”。
    pub edge_lines: SceneLines,
    /// Rotate 工具中已拾取的自定义轴线（红色虚线）。
    pub rotation_axis_lines: SceneLines,
    /// 半透明平面（PickPlane 模式的基准面）。
    pub planes: Vec<ScenePlane>,
    /// Draw3D previews blend over the grid, but beneath geometry outlines.
    pub preview_surfaces: Vec<ScenePlane>,
    /// 基准面边框线（Y 轴界线等）。
    pub plane_lines: SceneLines,
    /// 每个基准面的边框（各自颜色）。
    pub plane_borders: Vec<SceneLines>,
    /// Ã¥ÂËœÃ¥Å’â€“Ã¦â€”Â¶Ã©â€¡ÂÃ¥Â»Âº GPU Ã§Â¼â€œÃ¥â€ Â²Ã£â‚¬â€š
    pub version: u64,
    pub show_grid: bool,
    pub camera: Option<Camera>,
}

const SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    camera_pos: vec4<f32>,
    region_bias: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct MeshVSIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};
struct MeshVSOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

@vertex
fn mesh_vs(in: MeshVSIn) -> MeshVSOut {
    var out: MeshVSOut;
    out.clip = u.view_proj * vec4<f32>(in.pos, 1.0);
    out.world = in.pos;
    out.normal = in.normal;
    out.color = in.color;
    return out;
}

@fragment
fn mesh_fs(in: MeshVSOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let l = normalize(u.light_dir.xyz);
    let v = normalize(u.camera_pos.xyz - in.world);
    let diffuse = max(dot(n, l), 0.0);
    let fill_dir = normalize(vec3<f32>(-0.55, 0.35, -0.65));
    let fill = max(dot(n, fill_dir), 0.0);
    let hemi_t = clamp(n.y * 0.5 + 0.5, 0.0, 1.0);
    let hemi = mix(
        vec3<f32>(0.10, 0.11, 0.14),
        vec3<f32>(0.38, 0.44, 0.56),
        hemi_t,
    );
    let h = normalize(l + v);
    let spec = pow(max(dot(n, h), 0.0), 42.0) * 0.22;
    let rim = pow(1.0 - max(dot(n, v), 0.0), 2.5) * 0.10;
    let lighting = 0.34 + diffuse * 0.78 + fill * 0.20;
    let c = in.color * lighting
        + hemi * 0.12
        + vec3<f32>(spec)
        + vec3<f32>(0.10, 0.14, 0.22) * rim;
    return vec4<f32>(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

struct LineVSIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec3<f32>,
};
struct LineVSOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn line_vs(in: LineVSIn) -> LineVSOut {
    var out: LineVSOut;
    out.clip = u.view_proj * vec4<f32>(in.pos, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn line_fs(in: LineVSOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
// 半透明平面（基准面拾取）
struct PlaneVSIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
};
struct PlaneVSOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn plane_vs(in: PlaneVSIn) -> PlaneVSOut {
    var out: PlaneVSOut;
    out.clip = u.view_proj * vec4<f32>(in.pos, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn plane_fs(in: PlaneVSOut) -> @location(0) vec4<f32> {
    return in.color;
}

struct RegionFSOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

// A line covers pixels whose centers are up to half a pixel off its geometric
// centerline. Use the sketch plane's shared pixel depth gradient, rather than
// derivatives of the line (which omit the cross-line gradient). Both fills and
// outlines receive the same subpixel tolerance and four depth ULPs. They retain
// LessEqual, do not write depth, and do not move in world space.
fn region_depth(depth: f32) -> f32 {
    let ulp_depth = bitcast<f32>(max(bitcast<u32>(depth), 4u) - 4u);
    return max(0.0, min(ulp_depth, depth - u.region_bias.x));
}

@fragment
fn region_plane_fs(in: PlaneVSOut) -> RegionFSOut {
    var out: RegionFSOut;
    out.color = in.color;
    out.depth = region_depth(in.clip.z);
    return out;
}

@fragment
fn region_line_fs(in: LineVSOut) -> RegionFSOut {
    var out: RegionFSOut;
    out.color = vec4<f32>(in.color, 1.0);
    out.depth = region_depth(in.clip.z);
    return out;
}"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    camera_pos: [f32; 4],
    region_bias: [f32; 4],
}

impl Uniforms {
    fn new(view_proj: Mat4, camera_pos: Vec3, region_bias: f32) -> Self {
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: [0.45, 0.8, 0.4, 0.0],
            camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z, 1.0],
            region_bias: [region_bias, 0.0, 0.0, 0.0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LineVertex {
    pos: [f32; 3],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PlaneVertex {
    pos: [f32; 3],
    color: [f32; 4],
}

struct GpuMesh {
    source: SceneMesh,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    color: [f32; 3],
    highlight: bool,
}

struct GpuLines {
    vbuf: wgpu::Buffer,
    vertex_count: u32,
    /// 高亮线框（选中）：深度 Always，总可见（不被实体遮挡）。
    highlight: bool,
}

struct GpuPlane {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    preview: bool,
}

/// Ã¤Â¸â‚¬Ã¦Â¬Â¡Ã¦Â¸Â²Ã¦Å¸â€œÃ¥â€ºÅ¾Ã¨Â°Æ’Ã§Å¡â€ž GPU Ã¨Âµâ€žÃ¦ÂºÂÃ£â‚¬â€š
pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target_format: wgpu::TextureFormat,
    mesh_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    highlight_pipeline: wgpu::RenderPipeline,
    plane_pipeline: wgpu::RenderPipeline,
    region_plane_pipeline: wgpu::RenderPipeline,
    region_line_pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    uniform_bind: wgpu::BindGroup,
    meshes: Vec<GpuMesh>,
    lines: Vec<GpuLines>,
    planes: Vec<GpuPlane>,
    region_planes: Vec<GpuPlane>,
    region_lines: Vec<GpuLines>,
    background_line_count: usize,
    scene_version: u64,
    frame: RenderFrame,
}

/// egui Ã¤Â¾Â§Ã§Å¡â€žÃ¥â€ºÅ¾Ã¨Â°Æ’Ã¥Â¯Â¹Ã¨Â±Â¡Ã¯Â¼Ë†Ã¦Â¯ÂÃ¥Â¸Â§Ã§â€Â± app Ã¦â€Â¾Ã¥â€¦Â¥ painterÃ¯Â¼â€°Ã£â‚¬â€š
/// Supplied by eframe's render state, whose presentation format depends on the backend.
pub(crate) struct ViewportTargetFormat(pub wgpu::TextureFormat);

pub struct ViewportCallback {
    pub scene: Arc<Mutex<SceneData>>,
    pub renderer: Arc<Mutex<Option<GpuRenderer>>>,
    pub viewport_rect: egui::Rect,
}

impl ViewportCallback {
    pub fn new(
        scene: Arc<Mutex<SceneData>>,
        renderer: Arc<Mutex<Option<GpuRenderer>>>,
        viewport_rect: egui::Rect,
    ) -> Self {
        Self {
            scene,
            renderer,
            viewport_rect,
        }
    }
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let target_format = callback_resources
            .get::<ViewportTargetFormat>()
            .expect("S33D must register eframe's render target format")
            .0;
        let mut guard = self.renderer.lock().unwrap();
        if guard
            .as_ref()
            .is_some_and(|renderer| renderer.target_format != target_format)
        {
            *guard = None;
        }
        let renderer = guard.get_or_insert_with(|| GpuRenderer::new(device, queue, target_format));
        let scene = self.scene.lock().unwrap();

        // uniformÃ¯Â¼Å¡view_proj
        let rect = self.viewport_rect;
        let aspect = (rect.width() / rect.height().max(1.0)).max(1e-4);
        let camera = scene.camera.as_ref().cloned().unwrap_or_default();
        let frame = RenderFrame::for_camera(&camera);
        let vp = camera.proj_matrix(aspect) * camera.view_matrix_relative_to(frame.origin);
        renderer.update_uniform(
            queue,
            &vp,
            (camera.eye.as_dvec3() - frame.origin).as_vec3(),
            region_depth_bias(
                &scene,
                frame,
                vp,
                [
                    rect.width() * screen_descriptor.pixels_per_point,
                    rect.height() * screen_descriptor.pixels_per_point,
                ],
            ),
        );

        // Ã¥Å“ÂºÃ¦â„¢Â¯Ã§â€°Ë†Ã¦Å“Â¬Ã¥ÂËœÃ¥Å’â€“Ã¦â€”Â¶Ã©â€¡ÂÃ¥Â»ÂºÃ¥â€¡Â Ã¤Â½â€¢Ã§Â¼â€œÃ¥â€ Â²
        if scene.version != renderer.scene_version || frame != renderer.frame {
            if frame != renderer.frame {
                renderer.meshes.clear();
            }
            renderer.frame = frame;
            renderer.rebuild(device, &scene);
            renderer.scene_version = scene.version;
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::epaint::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        _callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let mut guard = self.renderer.lock().unwrap();
        if let Some(renderer) = guard.as_mut() {
            renderer.draw(render_pass);
        }
    }
}

impl GpuRenderer {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("s33d shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("s33d uniform"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &uniform_buf,
            0,
            bytemuck::bytes_of(&Uniforms::new(Mat4::IDENTITY, Vec3::ZERO, 0.0)),
        );

        // Ã¥â€¦Â±Ã¤ÂºÂ« uniform Ã¥Â¸Æ’Ã¥Â±â‚¬Ã¯Â¼Å¡bind group Ã¤Â¸Å½Ã¤Â¸Â¤Ã¤Â¸Âª pipeline layout Ã©Æ’Â½Ã§â€Â¨Ã¥Â®Æ’
        let u_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("s33d uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("s33d uniform bind"),
            layout: &u_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        let mesh_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh layout"),
            bind_group_layouts: &[&u_layout],
            push_constant_ranges: &[],
        });
        let line_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("line layout"),
            bind_group_layouts: &[&u_layout],
            push_constant_ranges: &[],
        });

        let mesh_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh pipeline"),
            layout: Some(&mesh_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("mesh_vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<MeshVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
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
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                // Solid bodies must populate the depth buffer so background grid lines
                // and ordinary scene lines cannot show through the model.
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("mesh_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let plane_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("plane pipeline"),
            layout: Some(&line_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("plane_vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<PlaneVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 12,
                            shader_location: 1,
                        },
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("plane_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("line pipeline"),
            layout: Some(&line_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("line_vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
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
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("line_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let highlight_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("highlight line pipeline"),
            layout: Some(&line_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("line_vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
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
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                // 选中高亮线框总可见（即使被其它实体包住）
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("line_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let region_pipeline = |lines: bool| {
            let attributes = [
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: if lines {
                        wgpu::VertexFormat::Float32x3
                    } else {
                        wgpu::VertexFormat::Float32x4
                    },
                    offset: 12,
                    shader_location: 1,
                },
            ];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if lines {
                    "region outline pipeline"
                } else {
                    "region fill pipeline"
                }),
                layout: Some(&line_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(if lines { "line_vs" } else { "plane_vs" }),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: if lines {
                            std::mem::size_of::<LineVertex>()
                        } else {
                            std::mem::size_of::<PlaneVertex>()
                        } as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    }],
                },
                primitive: wgpu::PrimitiveState {
                    topology: if lines {
                        wgpu::PrimitiveTopology::LineList
                    } else {
                        wgpu::PrimitiveTopology::TriangleList
                    },
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(if lines {
                        "region_line_fs"
                    } else {
                        "region_plane_fs"
                    }),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let region_plane_pipeline = region_pipeline(false);
        let region_line_pipeline = region_pipeline(true);

        Self {
            device: device.clone(),
            queue: queue.clone(),
            target_format,
            mesh_pipeline,
            line_pipeline,
            highlight_pipeline,
            plane_pipeline,
            region_plane_pipeline,
            region_line_pipeline,
            uniform_buf,
            uniform_bind,
            meshes: Vec::new(),
            lines: Vec::new(),
            planes: Vec::new(),
            region_planes: Vec::new(),
            region_lines: Vec::new(),
            background_line_count: 0,
            scene_version: u64::MAX,
            frame: RenderFrame::default(),
        }
    }

    fn update_uniform(
        &self,
        queue: &wgpu::Queue,
        view_proj: &Mat4,
        camera_pos: Vec3,
        region_bias: f32,
    ) {
        queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::bytes_of(&Uniforms::new(*view_proj, camera_pos, region_bias)),
        );
    }

    fn rebuild(&mut self, device: &wgpu::Device, scene: &SceneData) {
        let _ = &self.queue;
        // meshes
        let mut previous = std::mem::take(&mut self.meshes).into_iter();
        for m in &scene.meshes {
            if m.positions.is_empty() || m.normals.is_empty() || m.indices.is_empty() {
                continue;
            }
            // Camera/overlay changes must not reallocate unchanged solid buffers.
            // Full equality avoids stale rendering from a hash collision.
            if let Some(gpu) = previous.next() {
                if gpu.source == *m {
                    self.meshes.push(gpu);
                    continue;
                }
            }
            let vertices: Vec<MeshVertex> = m
                .positions
                .iter()
                .zip(m.normals.iter())
                .map(|(p, n)| MeshVertex {
                    pos: self.frame.world32(*p),
                    normal: *n,
                    color: if m.highlight {
                        // Super3D-style blue emissive lift: retain the body's
                        // material identity instead of replacing it with blue.
                        [
                            (m.color[0] * 0.78 + 0.10).min(1.0),
                            (m.color[1] * 0.82 + 0.16).min(1.0),
                            (m.color[2] * 0.88 + 0.24).min(1.0),
                        ]
                    } else {
                        m.color
                    },
                })
                .collect();
            if vertices.is_empty() || m.indices.is_empty() {
                continue;
            }
            let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mesh vbuf"),
                size: (std::mem::size_of::<MeshVertex>() * vertices.len()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue
                .write_buffer(&vbuf, 0, bytemuck::cast_slice(&vertices));
            let ibuf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mesh ibuf"),
                size: (std::mem::size_of::<u32>() * m.indices.len()) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue
                .write_buffer(&ibuf, 0, bytemuck::cast_slice(&m.indices));
            self.meshes.push(GpuMesh {
                source: m.clone(),
                vbuf,
                ibuf,
                index_count: m.indices.len() as u32,
                color: m.color,
                highlight: m.highlight,
            });
        }

        // linesÃ¯Â¼Ë†Ã§Â½â€˜Ã¦Â Â¼Ã¥Å“Â°Ã©ÂÂ¢ + Ã¨Ââ€°Ã¥â€ºÂ¾ + Ã©Â«ËœÃ¤ÂºÂ®Ã¯Â¼â€°
        self.lines.clear();
        let mut line_sets: Vec<(Vec<[f32; 3]>, [f32; 3], bool)> = Vec::new();
        if !scene.plane_lines.positions.is_empty() {
            line_sets.push((
                scene.plane_lines.positions.clone(),
                scene.plane_lines.color,
                false,
            ));
        }
        for border in &scene.plane_borders {
            if !border.positions.is_empty() {
                line_sets.push((border.positions.clone(), border.color, false));
            }
        }
        if scene.show_grid {
            if !scene.grid_minor_lines.positions.is_empty() {
                line_sets.push((
                    scene.grid_minor_lines.positions.clone(),
                    scene.grid_minor_lines.color,
                    false,
                ));
            }
            if !scene.grid_major_lines.positions.is_empty() {
                line_sets.push((
                    scene.grid_major_lines.positions.clone(),
                    scene.grid_major_lines.color,
                    false,
                ));
            }
            for axis in &scene.grid_axis_lines {
                if !axis.positions.is_empty() {
                    line_sets.push((axis.positions.clone(), axis.color, false));
                }
            }
        }
        self.background_line_count = line_sets.len();
        for edges in &scene.body_edge_lines {
            if !edges.positions.is_empty() {
                line_sets.push((edges.positions.clone(), edges.color, false));
            }
        }
        if !scene.sketch_lines.positions.is_empty() {
            line_sets.push((
                scene.sketch_lines.positions.clone(),
                scene.sketch_lines.color,
                false,
            ));
        }
        if !scene.sketch_preview_lines.positions.is_empty() {
            line_sets.push((
                scene.sketch_preview_lines.positions.clone(),
                scene.sketch_preview_lines.color,
                false,
            ));
        }
        if !scene.sketch_point_lines.positions.is_empty() {
            line_sets.push((
                scene.sketch_point_lines.positions.clone(),
                scene.sketch_point_lines.color,
                false,
            ));
        }
        if !scene.highlight_lines.positions.is_empty() {
            line_sets.push((
                scene.highlight_lines.positions.clone(),
                scene.highlight_lines.color,
                true,
            ));
        }
        if !scene.edge_lines.positions.is_empty() {
            line_sets.push((
                scene.edge_lines.positions.clone(),
                scene.edge_lines.color,
                false,
            ));
        }
        if !scene.rotation_axis_lines.positions.is_empty() {
            line_sets.push((
                scene.rotation_axis_lines.positions.clone(),
                scene.rotation_axis_lines.color,
                false,
            ));
        }
        for (positions, color, highlight) in line_sets {
            if positions.is_empty() {
                continue;
            }
            let vertices: Vec<LineVertex> = positions
                .iter()
                .map(|p| LineVertex {
                    pos: self.frame.world32(*p),
                    color,
                })
                .collect();
            let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("line vbuf"),
                size: (std::mem::size_of::<LineVertex>() * vertices.len()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue
                .write_buffer(&vbuf, 0, bytemuck::cast_slice(&vertices));
            self.lines.push(GpuLines {
                vbuf,
                vertex_count: vertices.len() as u32,
                highlight,
            });
        }

        if !scene.world_sketch_lines.positions.is_empty() {
            let vertices: Vec<_> = scene
                .world_sketch_lines
                .positions
                .iter()
                .map(|p| LineVertex {
                    pos: self.frame.world64(*p),
                    color: scene.world_sketch_lines.color,
                })
                .collect();
            self.lines.push(GpuLines {
                vbuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("precise sketch lines"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                vertex_count: vertices.len() as u32,
                highlight: false,
            });
        }

        self.region_lines.clear();
        if !scene.region_lines.positions.is_empty() {
            let vertices: Vec<_> = scene
                .region_lines
                .positions
                .iter()
                .map(|p| LineVertex {
                    pos: self.frame.world64(*p),
                    color: scene.region_lines.color,
                })
                .collect();
            self.region_lines.push(GpuLines {
                vbuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("region outlines"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                vertex_count: vertices.len() as u32,
                highlight: false,
            });
        }

        // planes（半透明基准面）
        self.planes.clear();
        for (index, p) in scene
            .planes
            .iter()
            .chain(&scene.preview_surfaces)
            .enumerate()
        {
            if p.positions.is_empty() || p.indices.is_empty() {
                continue;
            }
            let vertices: Vec<PlaneVertex> = p
                .positions
                .iter()
                .map(|pos| PlaneVertex {
                    pos: self.frame.world32(*pos),
                    color: [p.color[0], p.color[1], p.color[2], p.opacity],
                })
                .collect();
            let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("plane vbuf"),
                size: (std::mem::size_of::<PlaneVertex>() * vertices.len()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue
                .write_buffer(&vbuf, 0, bytemuck::cast_slice(&vertices));
            let ibuf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("plane ibuf"),
                size: (std::mem::size_of::<u32>() * p.indices.len()) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue
                .write_buffer(&ibuf, 0, bytemuck::cast_slice(&p.indices));
            self.planes.push(GpuPlane {
                vbuf,
                ibuf,
                index_count: p.indices.len() as u32,
                preview: index >= scene.planes.len(),
            });
        }

        self.region_planes.clear();
        for surface in &scene.region_surfaces {
            if surface.positions.is_empty() || surface.indices.is_empty() {
                continue;
            }
            let vertices: Vec<_> = surface
                .positions
                .iter()
                .map(|p| PlaneVertex {
                    pos: self.frame.world64(*p),
                    color: [
                        surface.color[0],
                        surface.color[1],
                        surface.color[2],
                        surface.opacity,
                    ],
                })
                .collect();
            self.region_planes.push(GpuPlane {
                vbuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("region fill vertices"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                ibuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("region fill indices"),
                    contents: bytemuck::cast_slice(&surface.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                index_count: surface.indices.len() as u32,
                preview: false,
            });
        }
    }

    fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        for m in &self.meshes {
            render_pass.set_pipeline(&self.mesh_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind, &[]);
            render_pass.set_vertex_buffer(0, m.vbuf.slice(..));
            render_pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..m.index_count, 0, 0..1);
        }
        for p in self.planes.iter().filter(|p| !p.preview) {
            render_pass.set_pipeline(&self.plane_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind, &[]);
            render_pass.set_vertex_buffer(0, p.vbuf.slice(..));
            render_pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..p.index_count, 0, 0..1);
        }
        // Draw the grid before Draw3D previews so alpha actually softens it.
        // Existing solid-body depth testing is unchanged.
        for l in self.lines.iter().take(self.background_line_count) {
            render_pass.set_pipeline(&self.line_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind, &[]);
            render_pass.set_vertex_buffer(0, l.vbuf.slice(..));
            render_pass.draw(0..l.vertex_count, 0..1);
        }
        for p in self.planes.iter().filter(|p| p.preview) {
            render_pass.set_pipeline(&self.plane_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind, &[]);
            render_pass.set_vertex_buffer(0, p.vbuf.slice(..));
            render_pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..p.index_count, 0, 0..1);
        }
        // Keep outlines and resize handles crisp above the preview fill.
        for l in self
            .lines
            .iter()
            .skip(self.background_line_count)
            .filter(|l| !l.highlight)
        {
            render_pass.set_pipeline(&self.line_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind, &[]);
            render_pass.set_vertex_buffer(0, l.vbuf.slice(..));
            render_pass.draw(0..l.vertex_count, 0..1);
        }
        // Selection coverage is double-sided, depth tested and never writes
        // depth. Fill and outline share exactly the same bounded depth bias.
        for surface in &self.region_planes {
            render_pass.set_pipeline(&self.region_plane_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind, &[]);
            render_pass.set_vertex_buffer(0, surface.vbuf.slice(..));
            render_pass.set_index_buffer(surface.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..surface.index_count, 0, 0..1);
        }
        for lines in &self.region_lines {
            render_pass.set_pipeline(&self.region_line_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind, &[]);
            render_pass.set_vertex_buffer(0, lines.vbuf.slice(..));
            render_pass.draw(0..lines.vertex_count, 0..1);
        }
        // 高亮线框（选中）：最后画，深度 Always——被实体包住也可见
        for l in self.lines.iter().filter(|l| l.highlight) {
            render_pass.set_pipeline(&self.highlight_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind, &[]);
            render_pass.set_vertex_buffer(0, l.vbuf.slice(..));
            render_pass.draw(0..l.vertex_count, 0..1);
        }
    }
}

/// Build a two-weight grid on any modeling/sketch plane. `u` and `v` are the
/// plane axes; every `major_every` minor intervals becomes a major line.
pub fn make_grid_lines(
    origin: [f32; 3],
    u: [f32; 3],
    v: [f32; 3],
    half_size: f32,
    step: f32,
    major_every: i32,
) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let origin = Vec3::from_array(origin);
    let u = Vec3::from_array(u).normalize_or_zero();
    let v = Vec3::from_array(v).normalize_or_zero();
    let mut minor = Vec::new();
    let mut major = Vec::new();
    if step <= 0.0 || half_size <= 0.0 || u.length_squared() == 0.0 || v.length_squared() == 0.0 {
        return (minor, major);
    }
    let count = (half_size / step).floor() as i32;
    let major_every = major_every.max(1);
    for i in -count..=count {
        let offset = i as f32 * step;
        let target = if i % major_every == 0 {
            &mut major
        } else {
            &mut minor
        };
        target.push((origin + u * offset - v * half_size).to_array());
        target.push((origin + u * offset + v * half_size).to_array());
        target.push((origin + v * offset - u * half_size).to_array());
        target.push((origin + v * offset + u * half_size).to_array());
    }
    (minor, major)
}

#[cfg(test)]
#[path = "render3d_qa.rs"]
mod tests;
