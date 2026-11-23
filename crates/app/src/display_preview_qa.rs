use super::*;

#[test]
fn display_texture_preview_lines_are_clipped_to_the_allocated_preview() {
    for pattern in SurfaceTexturePattern::ALL {
        for width in [160., 286., 390., 600.] {
            let ctx = egui::Context::default();
            let mut preview = egui::Rect::NOTHING;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 360.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        preview = egui::Rect::from_min_size(
                            ui.next_widget_position(),
                            egui::vec2(ui.available_width(), 92.),
                        );
                        draw_texture_preview(ui, pattern);
                        ui.label("Parameter inputs must not be painted over");
                    });
                },
            );
            fn inspect(
                shape: &egui::Shape,
                clip: egui::Rect,
                preview: egui::Rect,
                count: &mut usize,
            ) {
                match shape {
                    egui::Shape::LineSegment { stroke, .. }
                        if stroke.color == egui::Color32::from_rgb(76, 111, 145) =>
                    {
                        assert!(
                            preview.contains_rect(clip),
                            "preview {preview:?}, line clip {clip:?}"
                        );
                        *count += 1;
                    }
                    egui::Shape::Vec(v) => {
                        for shape in v {
                            inspect(shape, clip, preview, count)
                        }
                    }
                    _ => {}
                }
            }
            let mut count = 0;
            for shape in output.shapes {
                inspect(&shape.shape, shape.clip_rect, preview, &mut count);
            }
            assert!(count >= 25, "preview must actually be drawn");
        }
    }
}
