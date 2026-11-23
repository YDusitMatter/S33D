// SPDX-License-Identifier: MPL-2.0
//! Source-colored selection inset. Cached CPU triangles, no native draw calls.
use super::*;
#[derive(Default)]
pub(super) struct Preview {
    key: Vec<(u64, u64, [u32; 4], String)>,
    meshes: Vec<(MeshData, [f32; 4])>,
    pub(super) names: Vec<String>,
    triangles: Vec<([Vec3; 3], Color32)>,
    min: Vec2,
    max: Vec2,
    pub warning: String,
    image: Option<egui::ColorImage>,
    texture: std::cell::RefCell<Option<egui::TextureHandle>>,
}
pub(super) fn sync(app: &mut S33DApp) {
    let bodies: Vec<_> = app
        .doc
        .bodies
        .iter()
        .filter(|b| {
            b.shape.is_some()
                && if app.selected.is_empty() {
                    b.visible
                } else {
                    app.selected.contains(&b.id)
                }
        })
        .collect();
    let key: Vec<_> = bodies
        .iter()
        .map(|b| {
            (
                b.id,
                b.shape.unwrap().0,
                b.material.color.map(f32::to_bits),
                b.name.clone(),
            )
        })
        .collect();
    if key == app.drawing.preview.key {
        return;
    }
    let mut p = Preview {
        key,
        ..Default::default()
    };
    let mut count = 0;
    for b in bodies {
        let shape = b.shape.unwrap();
        let mesh = app
            .mesh_cache
            .get(&shape)
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| app.kernel.tessellate(shape));
        match mesh {
            Ok(mesh) => {
                count += mesh.triangle_count();
                if count > 200_000 {
                    p.warning = app
                        .settings
                        .language
                        .text(
                            "Preview limit exceeded; select fewer objects.",
                            "预览超出面数限制，请减少选择对象。",
                        )
                        .into();
                    break;
                }
                p.meshes.push((mesh, b.material.color));
                p.names.push(b.name.clone());
            }
            Err(e) => p.warning = e.to_string(),
        }
    }
    p.prepare();
    app.drawing.preview = p;
}
pub(super) fn paint(p: &Preview, ui: &egui::Ui, rect: egui::Rect, zh: bool) {
    let paint = ui.painter_at(rect);
    paint.rect_filled(rect, 3., Color32::from_rgb(247, 249, 250));
    paint.text(
        rect.center_top() + egui::vec2(0., 9.),
        egui::Align2::CENTER_TOP,
        format!(
            "{} ({})",
            if zh {
                "当前选择 · 三维预览"
            } else {
                "Selection · 3D preview"
            },
            p.names.len()
        ),
        egui::FontId::proportional(12.),
        Color32::DARK_GRAY,
    );
    let area = egui::Rect::from_min_max(
        rect.min + egui::vec2(10., 28.),
        rect.max - egui::vec2(10., 28.),
    );
    if let Some(image) = &p.image {
        let mut texture = p.texture.borrow_mut();
        let texture = texture.get_or_insert_with(|| {
            ui.ctx().load_texture(
                "drawing-selection-preview",
                image.clone(),
                egui::TextureOptions::LINEAR,
            )
        });
        let factor = (area.width() / image.width() as f32)
            .min(area.height() / image.height() as f32)
            .max(0.);
        let fitted = egui::Rect::from_center_size(
            area.center(),
            egui::vec2(image.width() as f32, image.height() as f32) * factor,
        );
        paint.image(
            texture.id(),
            fitted,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            Color32::WHITE,
        );
    }
    let label = if !p.warning.is_empty() {
        p.warning.clone()
    } else if p.names.len() == 1 {
        p.names[0].clone()
    } else {
        if zh {
            "选择变化即时更新；图纸需点击刷新"
        } else {
            "Selection updates here; refresh the sheet separately"
        }
        .into()
    };
    paint.text(
        rect.center_bottom() - egui::vec2(0., 5.),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(10.),
        Color32::DARK_GRAY,
    );
}

impl Preview {
    // Prepare once per selection/shape/color change, never sort source meshes
    // on every UI frame. Rasterize every face; skipping faces creates holes.
    fn prepare(&mut self) {
        let rotate = glam::Mat3::from_rotation_x(0.48) * glam::Mat3::from_rotation_y(-0.62);
        let center = self
            .meshes
            .iter()
            .flat_map(|(m, _)| m.positions.iter())
            .fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(lo, hi), p| (lo.min(Vec3::from_array(*p)), hi.max(Vec3::from_array(*p))),
            );
        let center = (center.0 + center.1) * 0.5;
        self.min = Vec2::splat(f32::INFINITY);
        self.max = -self.min;
        for (mesh, color) in &self.meshes {
            for t in mesh.indices.chunks_exact(3) {
                let Some(points) = t
                    .iter()
                    .map(|i| {
                        mesh.positions
                            .get(*i as usize)
                            .copied()
                            .map(Vec3::from_array)
                    })
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                let pts: [Vec3; 3] = std::array::from_fn(|i| rotate * (points[i] - center));
                let normal = (pts[1] - pts[0]).cross(pts[2] - pts[0]).normalize_or_zero();
                if !pts.iter().all(|p| p.is_finite()) || normal == Vec3::ZERO {
                    continue;
                }
                for v in pts {
                    self.min = self.min.min(v.truncate());
                    self.max = self.max.max(v.truncate());
                }
                let light = 0.35 + 0.65 * normal.dot(Vec3::new(-0.3, 0.5, 0.8).normalize()).abs();
                let rgb = color.map(|c| (c * light * 255.).clamp(0., 255.) as u8);
                self.triangles
                    .push((pts, Color32::from_rgb(rgb[0], rgb[1], rgb[2])));
            }
        }
        let (w, h) = (420usize, 280usize);
        let mut image = egui::ColorImage::new([w, h], Color32::from_rgb(247, 249, 250));
        let mut depth = vec![f32::NEG_INFINITY; w * h];
        let extent = (self.max - self.min).max(Vec2::splat(1e-5));
        let scale = ((w as f32 - 24.) / extent.x).min((h as f32 - 24.) / extent.y);
        let middle = (self.min + self.max) * 0.5;
        for (pts, color) in &self.triangles {
            let q = pts.map(|p| {
                let xy = (p.truncate() - middle) * Vec2::new(scale, -scale)
                    + Vec2::new(w as f32, h as f32) * 0.5;
                Vec3::new(xy.x, xy.y, p.z)
            });
            let area = cross(
                q[1].truncate() - q[0].truncate(),
                q[2].truncate() - q[0].truncate(),
            );
            if area.abs() < 1e-8 {
                continue;
            }
            let lo = q
                .iter()
                .fold(Vec2::splat(f32::INFINITY), |a, p| a.min(p.truncate()))
                .max(Vec2::ZERO);
            let hi = q
                .iter()
                .fold(Vec2::splat(f32::NEG_INFINITY), |a, p| a.max(p.truncate()))
                .min(Vec2::new(w as f32 - 1., h as f32 - 1.));
            for y in lo.y.floor() as usize..=hi.y.ceil() as usize {
                for x in lo.x.floor() as usize..=hi.x.ceil() as usize {
                    if x >= w || y >= h {
                        continue;
                    }
                    let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let a = cross(q[1].truncate() - p, q[2].truncate() - p) / area;
                    let b = cross(q[2].truncate() - p, q[0].truncate() - p) / area;
                    let c = 1. - a - b;
                    if a >= -1e-5 && b >= -1e-5 && c >= -1e-5 {
                        let z = a * q[0].z + b * q[1].z + c * q[2].z;
                        if z > depth[y * w + x] {
                            depth[y * w + x] = z;
                            image.pixels[y * w + x] = *color;
                        }
                    }
                }
            }
        }
        self.image = Some(image);
        self.triangles.clear();
        self.meshes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_mesh_preview_fits_far_from_origin_and_contains_no_sampling_holes() {
        let mut mesh = super::super::tests::box_mesh([10., 6., 12.]);
        for p in &mut mesh.positions {
            for v in p {
                *v += 100_000.;
            }
        }
        let mut p = Preview {
            meshes: vec![(mesh, [0.3, 0.6, 0.9, 1.])],
            ..Default::default()
        };
        p.prepare();
        let image = p.image.unwrap();
        if let Some(path) = std::env::var_os("S33D_DRAWING_PREVIEW_OUTPUT") {
            let bytes: Vec<_> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            image::save_buffer(path, &bytes, 420, 280, image::ColorType::Rgba8).unwrap();
        }
        let bg = Color32::from_rgb(247, 249, 250);
        let painted: Vec<_> = image
            .pixels
            .iter()
            .enumerate()
            .filter(|(_, c)| **c != bg)
            .map(|(i, _)| (i % 420, i / 420))
            .collect();
        let lo = painted
            .iter()
            .fold((420, 280), |a, p| (a.0.min(p.0), a.1.min(p.1)));
        let hi = painted
            .iter()
            .fold((0, 0), |a, p| (a.0.max(p.0), a.1.max(p.1)));
        assert!(lo.0 >= 10 && lo.1 >= 10 && hi.0 < 410 && hi.1 < 270);
        assert!(hi.0 - lo.0 > 150 && hi.1 - lo.1 > 150);
        assert_ne!(image.pixels[140 * 420 + 210], bg);
    }
}
