//! S33D 桌面应用入口。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![recursion_limit = "256"]

mod agent_protocol;
mod app;
mod appearance;
mod camera;
mod display;
mod drawing2d;
mod fastener_plugin;
mod fasteners;
mod gear;
mod identity;
mod mesh_cylinders;
mod mode;
mod plugins;
mod recent_projects;
mod reference_assets;
mod render3d;
mod sketch_session;
mod sketch_text;
mod startup;
mod surface_texture;
#[cfg(test)]
mod thread_fit_qa;
mod threads;
mod tire;
mod ui;
mod ui_icons;
mod wheel_rim;
mod workflow_guide;

fn main() -> eframe::Result {
    // Explorer's Open with command passes the filename as a single native OS
    // argument. Keep it intact (including spaces and non-ASCII characters).
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let viewport = egui::ViewportBuilder::default()
        .with_title(identity::PRODUCT_NAME)
        .with_decorations(false)
        .with_resizable(false)
        .with_inner_size([768.0, 1024.0])
        .with_min_inner_size([1.0, 1.0]);
    #[cfg(s33d_brand_icon)]
    let viewport = viewport.with_icon(
        eframe::icon_data::from_png_bytes(include_bytes!(
            "../../../assets/icons/s33d-sprouted.png"
        ))
        .expect("the embedded S33D application icon must be a valid PNG"),
    );
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        // 3D 视口需要深度缓冲（egui 默认 0）
        depth_buffer: 32,
        centered: true,
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        identity::PRODUCT_NAME,
        options,
        Box::new(move |cc| {
            if let Some(state) = &cc.wgpu_render_state {
                state
                    .renderer
                    .write()
                    .callback_resources
                    .insert(render3d::ViewportTargetFormat(state.target_format));
            }
            Ok(Box::new(startup::LaunchScreen::new(cc, arguments)))
        }),
    )
}
