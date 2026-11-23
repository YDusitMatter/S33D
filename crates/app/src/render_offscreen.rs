//! Native model-image output. Shares the viewport shader and GPU draw path.
use super::*;
use std::future::Future;
use std::task::{Context, Poll, Wake, Waker};

fn wait<F: Future>(future: F) -> F::Output {
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
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park_timeout(std::time::Duration::from_secs(1)),
        }
    }
}

pub fn render_png(
    scene: &SceneData,
    width: u32,
    height: u32,
    background: [f32; 3],
) -> Result<Vec<u8>, String> {
    if width < 128
        || height < 128
        || width > 2048
        || height > 2048
        || u64::from(width) * u64::from(height) > 3_145_728
    {
        return Err("Offscreen output exceeds image bounds".into());
    }
    let camera = scene.camera.as_ref().ok_or("Missing render camera")?;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = wait(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .ok_or("No native rendering adapter is available")?;
    let (device, queue) = wait(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("S33D model image"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: Default::default(),
        },
        None,
    ))
    .map_err(|e| format!("Native render device: {e}"))?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = GpuRenderer::new(&device, &queue, format);
    let frame = RenderFrame::for_camera(camera);
    renderer.frame = frame;
    renderer.rebuild(&device, scene);
    renderer.update_uniform(
        &queue,
        &(camera.proj_matrix(width as f32 / height as f32)
            * camera.view_matrix_relative_to(frame.origin)),
        (camera.eye.as_dvec3() - frame.origin).as_vec3(),
        0.,
    );
    let extent = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("S33D image color"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("S33D image depth"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let stride = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("S33D image readback"),
        size: u64::from(stride) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let view = texture.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("S33D model image pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: background[0] as f64,
                        g: background[1] as f64,
                        b: background[2] as f64,
                        a: 1.,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.),
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
                bytes_per_row: Some(stride),
                rows_per_image: Some(height),
            },
        },
        extent,
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::Maintain::Wait);
    rx.recv_timeout(std::time::Duration::from_secs(30))
        .map_err(|_| "Native image readback timed out")?
        .map_err(|e| e.to_string())?;
    let rgba = {
        let mapped = buffer.slice(..).get_mapped_range();
        mapped
            .chunks_exact(stride as usize)
            .flat_map(|row| row[..(width * 4) as usize].iter().copied())
            .collect::<Vec<_>>()
    };
    buffer.unmap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_raw(width, height, rgba)
        .ok_or("Invalid image readback size")?
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(png.into_inner())
}
