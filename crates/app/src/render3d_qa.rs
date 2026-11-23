//! Renderer tests use a hand-built annulus, independent of sketch triangulation.
use super::*;
use std::future::Future;
use std::task::{Context, Poll, Wake, Waker};

#[test]
fn shared_frame_preserves_small_world64_details() {
    let mut camera = Camera::default();
    camera.target = Vec3::splat(1_000_000.0);
    camera.eye = camera.target + Vec3::Z * 5.0;
    let frame = RenderFrame::for_camera(&camera);
    let a = [1_000_000.0, 1_000_000.0, 1_000_000.0];
    let b = [1_000_000.0001, 1_000_000.0, 1_000_000.0];
    assert_eq!(a.map(|v| v as f32), b.map(|v| v as f32));
    assert!((frame.world64(b)[0] - frame.world64(a)[0] - 0.0001).abs() < 1e-8);
    assert_eq!(frame.world32(a.map(|v| v as f32)), frame.world64(a));
}

#[test]
fn camera_and_every_scene_layer_use_same_render_frame() {
    let mut camera = Camera::default();
    camera.eye = Vec3::new(1_000_002.0, 1_000_001.5, 1_000_005.0);
    camera.target = Vec3::splat(1_000_000.0);
    let frame = RenderFrame::for_camera(&camera);
    let world = [1_000_000.25, 1_000_000.125, 1_000_000.0];
    let vp = camera.proj_matrix(1.0) * camera.view_matrix_relative_to(frame.origin);
    let projected = vp.project_point3(Vec3::from_array(frame.world64(world)));
    let mut local = camera.clone();
    local.eye -= Vec3::splat(1_000_000.0);
    local.target -= Vec3::splat(1_000_000.0);
    let expected =
        (local.proj_matrix(1.0) * local.view_matrix()).project_point3(Vec3::new(0.25, 0.125, 0.0));
    assert!((projected - expected).length() < 1e-6);
}

#[test]
fn region_bias_tracks_physical_pixels_and_survives_outline_only() {
    let scene = ring_scene(DVec3::ZERO, 1.0, Vec3::new(2.0, 1.5, 5.0), true, false);
    let camera = scene.camera.as_ref().unwrap();
    let frame = RenderFrame::for_camera(camera);
    let vp = camera.proj_matrix(1.0) * camera.view_matrix_relative_to(frame.origin);
    let bias = region_depth_bias(&scene, frame, vp, [128.0, 128.0]);
    assert!(bias > 0.0 && bias.is_finite());
    assert!((region_depth_bias(&scene, frame, vp, [256.0, 256.0]) * 2.0 - bias).abs() < 1e-9);
    let mut outline = scene;
    outline.region_surfaces.clear();
    assert!((region_depth_bias(&outline, frame, vp, [128.0, 128.0]) - bias).abs() < 1e-9);
}

fn block_on<F: Future>(future: F) -> F::Output {
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park(),
        }
    }
}

const WIDTH: u32 = 128;
const HEIGHT: u32 = 128;

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut GpuRenderer,
    scene: &SceneData,
) -> Vec<u8> {
    let camera = scene.camera.as_ref().unwrap();
    let frame = RenderFrame::for_camera(camera);
    if renderer.frame != frame {
        renderer.meshes.clear();
    }
    renderer.frame = frame;
    renderer.rebuild(device, scene);
    renderer.update_uniform(
        queue,
        &(camera.proj_matrix(1.0) * camera.view_matrix_relative_to(frame.origin)),
        (camera.eye.as_dvec3() - frame.origin).as_vec3(),
        region_depth_bias(
            scene,
            frame,
            camera.proj_matrix(1.0) * camera.view_matrix_relative_to(frame.origin),
            [WIDTH as f32, HEIGHT as f32],
        ),
    );
    let extent = wgpu::Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("region QA color"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: renderer.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("region QA depth"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("region QA readback"),
        size: u64::from(WIDTH * HEIGHT * 4),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let color_view = texture.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("region QA pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.draw(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 4),
                rows_per_image: Some(HEIGHT),
            },
        },
        extent,
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
    let _ = device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().unwrap();
    let result = buffer.slice(..).get_mapped_range().to_vec();
    buffer.unmap();
    result
}

fn ring_scene(origin: DVec3, scale: f64, eye: Vec3, ortho: bool, occluded: bool) -> SceneData {
    let world = |x: f64, y: f64| (origin + DVec3::new(x * scale, y * scale, 0.0)).to_array();
    let mut fill = SceneRegionSurface {
        color: [1.0, 0.5, 0.05],
        opacity: 0.45,
        ..Default::default()
    };
    // Four disjoint quads: no bridge, no ear clipping, no overdraw at a hole.
    for [x0, y0, x1, y1] in [
        [-1.0, -1.0, 1.0, -0.35],
        [-1.0, 0.35, 1.0, 1.0],
        [-1.0, -0.35, -0.35, 0.35],
        [0.35, -0.35, 1.0, 0.35],
    ] {
        let i = fill.positions.len() as u32;
        fill.positions
            .extend([world(x0, y0), world(x1, y0), world(x1, y1), world(x0, y1)]);
        fill.indices.extend([i, i + 1, i + 2, i, i + 2, i + 3]);
    }
    let mut lines = SceneRegionLines {
        color: fill.color,
        ..Default::default()
    };
    for half in [1.0, 0.35] {
        let corners = [
            world(-half, -half),
            world(half, -half),
            world(half, half),
            world(-half, half),
        ];
        for i in 0..4 {
            lines.positions.extend([corners[i], corners[(i + 1) % 4]]);
        }
    }
    let mut camera = Camera::default();
    camera.target = origin.as_vec3();
    camera.eye = camera.target + eye * scale as f32;
    camera.up = Vec3::Y;
    camera.near = 0.01 * scale as f32;
    camera.far = 100.0 * scale as f32;
    camera.fov_y_deg = 30.0;
    camera.ortho = ortho;
    camera.ortho_half = 1.4 * scale as f32;
    let mut scene = SceneData {
        region_surfaces: vec![fill],
        region_lines: lines,
        camera: Some(camera),
        ..Default::default()
    };
    if occluded {
        let z = if eye.z > 0.0 { 0.05 } else { -0.05 } * scale;
        scene.meshes.push(SceneMesh {
            positions: [(-2.0, -2.0), (0.0, -2.0), (0.0, 2.0), (-2.0, 2.0)]
                .map(|(x, y)| {
                    (origin + DVec3::new(x * scale, y * scale, z))
                        .as_vec3()
                        .to_array()
                })
                .to_vec(),
            normals: vec![[0.0, 0.0, 1.0]; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            color: [0.05, 0.15, 0.8],
            highlight: false,
        });
    }
    scene
}

fn orange(pixel: &[u8]) -> bool {
    // Texture readback is BGRA.
    i32::from(pixel[2]) > i32::from(pixel[1]) + 25 && i32::from(pixel[1]) > i32::from(pixel[0]) + 15
}

fn save_example(name: &str, pixels: &[u8]) {
    if let Some(directory) = std::env::var_os("S33D_RENDER_QA_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let mut rgba = pixels.to_vec();
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        image::save_buffer(
            directory.join(name),
            &rgba,
            WIDTH,
            HEIGHT,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

fn orange_near(pixels: &[u8], scene: &SceneData, world: DVec3, radius: i32) -> bool {
    let camera = scene.camera.as_ref().unwrap();
    let frame = RenderFrame::for_camera(camera);
    let p = (camera.proj_matrix(1.0) * camera.view_matrix_relative_to(frame.origin))
        .project_point3(Vec3::from_array(frame.world64(world.to_array())));
    let cx = ((p.x + 1.0) * 0.5 * WIDTH as f32).floor() as i32;
    let cy = ((1.0 - p.y) * 0.5 * HEIGHT as f32).floor() as i32;
    (-radius..=radius).any(|dy| {
        (-radius..=radius).any(|dx| {
            let (x, y) = (cx + dx, cy + dy);
            x >= 0
                && y >= 0
                && x < WIDTH as i32
                && y < HEIGHT as i32
                && orange(&pixels[((y as u32 * WIDTH + x as u32) * 4) as usize..][..4])
        })
    })
}

#[test]
#[ignore = "requires an actual wgpu adapter; run explicitly for renderer acceptance"]
fn gpu_region_holes_sides_scales_origins_and_occlusion() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("renderer acceptance requires a wgpu adapter");
    println!("region GPU QA adapter: {:?}", adapter.get_info());
    let (device, queue) = block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("S33D region QA"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: Default::default(),
        },
        None,
    ))
    .unwrap();
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Bgra8Unorm);
    let mut count = 0;
    for ortho in [false, true] {
        for (view, eye) in [
            ("front", Vec3::new(0.0, 0.0, 5.0)),
            ("back", Vec3::new(0.0, 0.0, -5.0)),
            ("slanted", Vec3::new(2.0, 1.5, 5.0)),
        ] {
            for scale in [0.125, 1.0, 64.0] {
                let near = ring_scene(DVec3::ZERO, scale, eye, ortho, false);
                let local = render(&device, &queue, &mut renderer, &near);
                assert!(
                    !orange_near(&local, &near, DVec3::ZERO, 2),
                    "hole filled: {view}/{ortho}/{scale}"
                );
                assert!(
                    orange_near(&local, &near, DVec3::new(0.75 * scale, 0.0, 0.0), 1),
                    "fill absent: {view}/{ortho}/{scale}"
                );
                assert!(
                    orange_near(&local, &near, DVec3::new(scale, 0.0, 0.0), 2),
                    "outline absent: {view}/{ortho}/{scale}"
                );
                let origin = DVec3::new(1_000_000.0, -1_000_000.0, 1_000_000.0);
                let far = ring_scene(origin, scale, eye, ortho, false);
                let translated = render(&device, &queue, &mut renderer, &far);
                if scale == 1.0 {
                    save_example(
                        &format!(
                            "region-{view}-{}-1e6.png",
                            if ortho { "ortho" } else { "perspective" }
                        ),
                        &translated,
                    );
                }
                assert_eq!(
                    local, translated,
                    "shared-origin image differs: {view}/{ortho}/{scale}"
                );
                let mut supported = near.clone();
                supported.meshes.push(SceneMesh {
                    positions: vec![
                        [-scale as f32, -scale as f32, 0.0],
                        [scale as f32, -scale as f32, 0.0],
                        [scale as f32, scale as f32, 0.0],
                        [-scale as f32, scale as f32, 0.0],
                    ],
                    normals: vec![[0.0, 0.0, 1.0]; 4],
                    indices: vec![0, 1, 2, 0, 2, 3],
                    color: [0.01, 0.01, 0.01],
                    highlight: false,
                });
                let coplanar = render(&device, &queue, &mut renderer, &supported);
                let missing = local
                    .chunks_exact(4)
                    .zip(coplanar.chunks_exact(4))
                    .filter(|(plain, supported)| orange(plain) && !orange(supported))
                    .count();
                if missing > 1 {
                    let details: Vec<_> = local
                        .chunks_exact(4)
                        .zip(coplanar.chunks_exact(4))
                        .enumerate()
                        .filter(|(_, (plain, supported))| orange(plain) && !orange(supported))
                        .take(8)
                        .map(|(i, (a, b))| (i % WIDTH as usize, i / WIDTH as usize, a, b))
                        .collect();
                    println!("missing details: {details:?}");
                }
                assert!(
                    missing <= 1,
                    "coplanar coverage lost {missing} pixels: {view}/{ortho}/{scale}"
                );
                save_example(
                    &format!("region-{view}-{ortho}-{scale}-coplanar.png"),
                    &coplanar,
                );
                assert!(
                    orange_near(&coplanar, &supported, DVec3::new(0.75 * scale, 0.0, 0.0), 1),
                    "coplanar fill missing: {view}/{ortho}/{scale}"
                );
                assert!(
                    orange_near(&coplanar, &supported, DVec3::new(scale, 0.0, 0.0), 2),
                    "coplanar outline missing"
                );
                let mut translated_supported = far.clone();
                translated_supported.meshes = supported
                    .meshes
                    .iter()
                    .cloned()
                    .map(|mut mesh| {
                        for point in &mut mesh.positions {
                            *point = (Vec3::from_array(*point).as_dvec3() + origin)
                                .as_vec3()
                                .to_array();
                        }
                        mesh
                    })
                    .collect();
                let translated_coplanar =
                    render(&device, &queue, &mut renderer, &translated_supported);
                assert_eq!(
                    coplanar, translated_coplanar,
                    "solid/region frame mismatch: {view}/{ortho}/{scale}"
                );
                count += 2;
                // Front/back tests use a solid in front of the left half; both
                // fill and its outer outline must be hidden by the same solid.
                if view != "slanted" {
                    let blocked = ring_scene(DVec3::ZERO, scale, eye, ortho, true);
                    let pixels = render(&device, &queue, &mut renderer, &blocked);
                    if scale == 1.0 {
                        save_example(
                            &format!(
                                "region-{view}-{}-occluded.png",
                                if ortho { "ortho" } else { "perspective" }
                            ),
                            &pixels,
                        );
                    }
                    assert!(
                        !orange_near(&pixels, &blocked, DVec3::new(-0.75 * scale, 0.0, 0.0), 1),
                        "fill leaked through solid: {view}/{ortho}/{scale}"
                    );
                    assert!(
                        !orange_near(&pixels, &blocked, DVec3::new(-scale, 0.0, 0.0), 1),
                        "outline leaked through solid: {view}/{ortho}/{scale}"
                    );
                    assert!(
                        orange_near(&pixels, &blocked, DVec3::new(0.75 * scale, 0.0, 0.0), 1),
                        "unoccluded half disappeared"
                    );
                    count += 1;
                }
                count += 2;
            }
        }
    }
    println!("region GPU QA: {count} rendered/read-back cases passed");
}

#[test]
#[ignore = "requires an actual wgpu adapter; run explicitly for renderer acceptance"]
fn gpu_viewport_accepts_rgba_and_bgra_targets() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("renderer acceptance requires a wgpu adapter");
    let (device, queue) = block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("S33D target-format QA"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: Default::default(),
        },
        None,
    ))
    .unwrap();
    let scene = ring_scene(DVec3::ZERO, 1.0, Vec3::new(0.0, 0.0, 5.0), true, false);
    for format in [
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8Unorm,
    ] {
        let mut renderer = GpuRenderer::new(&device, &queue, format);
        let mut pixels = render(&device, &queue, &mut renderer, &scene);
        if format == wgpu::TextureFormat::Rgba8Unorm {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        assert!(
            !orange_near(&pixels, &scene, DVec3::ZERO, 2),
            "hole filled: {format:?}"
        );
        assert!(
            orange_near(&pixels, &scene, DVec3::new(0.75, 0.0, 0.0), 1),
            "fill absent: {format:?}"
        );
        assert!(
            orange_near(&pixels, &scene, DVec3::new(1.0, 0.0, 0.0), 2),
            "outline absent: {format:?}"
        );
    }
}
