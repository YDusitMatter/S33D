//! 视口相机：轨道/平移/缩放、标准视图、透视/正交切换。

use glam::{Mat4, Vec3};

/// 标准视图方向（与 Super3D `viewport.js` VIEW_DIRS 一致，Y-up）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewDir {
    Top,
    Bottom,
    Front,
    Back,
    Right,
    Left,
    Iso,
}

impl ViewDir {
    pub fn dir(self) -> Vec3 {
        match self {
            ViewDir::Top => Vec3::new(0.0, 1.0, 0.0),
            ViewDir::Bottom => Vec3::new(0.0, -1.0, 0.0),
            ViewDir::Front => Vec3::new(0.0, 0.0, 1.0),
            ViewDir::Back => Vec3::new(0.0, 0.0, -1.0),
            ViewDir::Right => Vec3::new(1.0, 0.0, 0.0),
            ViewDir::Left => Vec3::new(-1.0, 0.0, 0.0),
            ViewDir::Iso => Vec3::new(1.0, 0.82, 1.0).normalize(),
        }
    }

    pub fn up(self) -> Vec3 {
        match self {
            ViewDir::Top => Vec3::new(0.0, 0.0, -1.0),
            ViewDir::Bottom => Vec3::new(0.0, 0.0, 1.0),
            _ => Vec3::new(0.0, 1.0, 0.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y_deg: f32,
    pub ortho: bool,
    /// 正交投影半高（世界单位）。
    pub ortho_half: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Vec3::new(260.0, 210.0, 260.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y_deg: 45.0,
            ortho: false,
            ortho_half: 250.0,
            near: 0.5,
            far: 200_000.0,
        }
    }
}

impl Camera {
    /// Fit both dimensions of an offscreen output, preserving projection/direction.
    pub fn fit_output(&mut self, min: Vec3, max: Vec3, aspect: f32) {
        if !self.ortho {
            self.recenter(min, max, aspect);
            return;
        }
        let center = min * 0.5 + max * 0.5;
        let half = (max - min) * 0.5;
        let forward = self.forward();
        let right = self.right();
        self.up = right.cross(forward).normalize();
        let mut half_height = 1e-5_f32;
        for x in [-1., 1.] {
            for y in [-1., 1.] {
                for z in [-1., 1.] {
                    let p = half * Vec3::new(x, y, z);
                    half_height = half_height
                        .max(p.dot(self.up).abs())
                        .max(p.dot(right).abs() / aspect);
                }
            }
        }
        let radius = half.length().max(1e-4);
        self.target = center;
        self.eye = center - forward * (radius * 3.0);
        self.ortho_half = half_height * 1.15;
        self.near = radius * 0.01;
        self.far = radius * 10.0;
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye, self.target, self.up)
    }

    /// Match the common scene upload frame without subtracting large f32 values
    /// inside the view matrix. Camera navigation itself still uses legacy f32.
    pub fn view_matrix_relative_to(&self, origin: glam::DVec3) -> Mat4 {
        Mat4::look_at_rh(
            (self.eye.as_dvec3() - origin).as_vec3(),
            (self.target.as_dvec3() - origin).as_vec3(),
            self.up,
        )
    }

    /// 投影矩阵（wgpu：z ∈ [0,1] 深度）。
    pub fn proj_matrix(&self, aspect: f32) -> Mat4 {
        if self.ortho {
            let h = self.ortho_half;
            let w = h * aspect;
            Mat4::orthographic_rh(-w, w, -h, h, self.near, self.far)
        } else {
            Mat4::perspective_rh(
                self.fov_y_deg.to_radians(),
                aspect.max(1e-4),
                self.near,
                self.far,
            )
        }
    }

    /// 前方方向。
    pub fn forward(&self) -> Vec3 {
        (self.target - self.eye).normalize()
    }

    /// 右方向。
    pub fn right(&self) -> Vec3 {
        self.forward().cross(self.up).normalize()
    }

    /// 相机到目标的距离。
    pub fn distance(&self) -> f32 {
        self.eye.distance(self.target)
    }

    /// 轨道旋转（屏幕 dx/dy，单位像素）。
    pub fn orbit(&mut self, dx: f32, dy: f32, pan_speed: f32) {
        let dir = self.forward();
        let right = self.right();
        let up = self.up;
        // 绕目标旋转：水平绕 up 轴，垂直绕 right 轴
        let horiz = glam::Quat::from_axis_angle(up, -dx * pan_speed);
        let vert = glam::Quat::from_axis_angle(right, -dy * pan_speed);
        let d = self.eye - self.target;
        let d = vert * (horiz * d);
        self.eye = self.target + d;
        // 维持 up 尽可能朝上
        let new_forward = (self.target - self.eye).normalize();
        let new_right = new_forward.cross(Vec3::Y).normalize();
        if new_right.length_squared() > 1e-4 {
            self.up = new_right.cross(new_forward).normalize();
        }
        let _ = dir;
    }

    /// 平移（屏幕 dx/dy）。
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let right = self.right();
        let up = self.up;
        let world_per_pixel = if self.ortho {
            self.ortho_half * 2.0 / 600.0
        } else {
            self.distance() * (self.fov_y_deg.to_radians() / 2.0).tan() * 2.0 / 600.0
        };
        let t = (-right * dx + up * dy) * world_per_pixel;
        self.eye += t;
        self.target += t;
    }

    /// 缩放（delta > 0 拉近）。
    pub fn zoom(&mut self, delta: f32) {
        if self.ortho {
            self.ortho_half = (self.ortho_half * (1.0 - delta * 0.1)).clamp(1.0, 1e6);
        } else {
            // eye 在视线反方向（target - forward*d），保持视角方向不变
            let dir = self.forward();
            let d = (self.distance() * (1.0 - delta * 0.12)).clamp(1.0, 1e6);
            self.eye = self.target - dir * d;
        }
    }

    /// 设置标准视图。
    pub fn set_view(&mut self, view: ViewDir) {
        self.up = view.up();
        let dist = self.distance().max(200.0);
        self.eye = self.target + view.dir() * dist;
    }

    /// 透视/正交切换。
    pub fn toggle_projection(&mut self) -> bool {
        if self.ortho {
            self.ortho = false;
        } else {
            self.ortho = true;
            // 正交半高 = 当前透视在目标距离处的可视半高
            self.ortho_half = self.distance() * (self.fov_y_deg.to_radians() / 2.0).tan();
        }
        self.ortho
    }

    /// 拟合包围盒。
    pub fn fit(&mut self, center: Vec3, radius: f32) {
        // dir = 视线方向（forward，从 eye 指向 target）；
        // eye 必须放在视线反方向：eye = center - dir * d
        let dir = if (self.eye - self.target).length_squared() < 1e-6 {
            Vec3::new(1.0, 0.8, 1.0).normalize()
        } else {
            self.forward()
        };
        self.target = center;
        let r = radius.max(30.0);
        // 自适应远裁剪面（超大/超小模型都可见）
        self.far = (r * 20.0).max(200_000.0);
        if self.ortho {
            self.ortho_half = r * 1.3;
            self.eye = center - dir * (r * 5.0);
        } else {
            let d = r / (self.fov_y_deg.to_radians() / 2.0).tan() * 1.35;
            self.eye = center - dir * d;
        }
    }

    /// Center visible bounds in perspective, fitting both viewport dimensions.
    pub fn recenter(&mut self, min: Vec3, max: Vec3, aspect: f32) {
        let center = min * 0.5 + max * 0.5;
        let half = (max - min) * 0.5;
        let forward = (self.target - self.eye)
            .try_normalize()
            .unwrap_or(-ViewDir::Iso.dir());
        let right = forward.cross(self.up).try_normalize().unwrap_or_else(|| {
            forward
                .cross(if forward.y.abs() < 0.9 {
                    Vec3::Y
                } else {
                    Vec3::Z
                })
                .normalize()
        });
        self.up = right.cross(forward).normalize();
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        let tan_y = (self.fov_y_deg.clamp(5.0, 150.0).to_radians() * 0.5).tan() * 0.85;
        let tan_x = tan_y * aspect;
        let padding = half.length().max(0.001) * 0.1;
        let mut distance = padding;
        let mut z_min = f32::INFINITY;
        let mut z_max = f32::NEG_INFINITY;
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    let p = half * Vec3::new(x, y, z);
                    let depth = p.dot(forward);
                    distance = distance
                        .max(p.dot(right).abs() / tan_x - depth)
                        .max(p.dot(self.up).abs() / tan_y - depth)
                        .max(padding - depth);
                    z_min = z_min.min(depth);
                    z_max = z_max.max(depth);
                }
            }
        }
        self.ortho = false;
        self.target = center;
        self.eye = center - forward * distance;
        self.near = ((distance + z_min) * 0.1).max(1e-6);
        self.far = ((distance + z_max) * 10.0).max(self.near * 100.0);
    }

    /// 进入草图时，将相机正对指定平面。
    pub fn align_to_plane(
        &mut self,
        origin: Vec3,
        normal: Vec3,
        x_axis: Vec3,
        y_axis: Vec3,
        half: f32,
    ) {
        let _ = x_axis;
        let _ = y_axis;
        self.up = y_axis;
        self.target = origin;
        self.ortho = true;
        self.ortho_half = half;
        self.eye = origin + normal * 500.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recenter_fits_small_large_and_narrow_views_in_perspective() {
        for aspect in [0.3, 1.0, 2.5] {
            for scale in [0.01, 1.0, 100_000.0] {
                let mut cam = Camera::default();
                cam.ortho = true;
                let forward = cam.forward();
                let center = Vec3::new(10.0, -20.0, 30.0) * scale;
                let half = Vec3::new(12.0, 3.0, 7.0) * scale;
                cam.recenter(center - half, center + half, aspect);
                assert!(!cam.ortho);
                assert!((cam.target - center).length() < scale * 1e-4);
                assert!(cam.forward().dot(forward) > 0.999);
                let matrix = cam.proj_matrix(aspect) * cam.view_matrix();
                for x in [-1.0, 1.0] {
                    for y in [-1.0, 1.0] {
                        for z in [-1.0, 1.0] {
                            let p = matrix.project_point3(center + half * Vec3::new(x, y, z));
                            assert!(p.x.abs() <= 0.851 && p.y.abs() <= 0.851, "{p:?}");
                            assert!(p.z > 0.0 && p.z < 1.0);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fit_adapts_far_plane_for_huge_models() {
        let mut cam = Camera::default();
        cam.fit(glam::Vec3::new(1000.0, 0.0, 0.0), 500_000.0);
        // 远裁剪面 ≥ 半径×20，超大模型不被裁剪
        assert!(cam.far >= 10_000_000.0, "far={}", cam.far);
        assert_eq!(cam.target, glam::Vec3::new(1000.0, 0.0, 0.0));
        // 相机距离足以容纳模型
        assert!(cam.distance() > 500_000.0);
    }

    #[test]
    fn fit_keeps_small_models_visible() {
        let mut cam = Camera::default();
        cam.fit(glam::Vec3::new(5.0, 0.0, 0.0), 1.0);
        // 最小半径 30mm
        assert_eq!(cam.target, glam::Vec3::new(5.0, 0.0, 0.0));
        assert!(cam.distance() > 30.0);
        assert!(cam.far >= 200_000.0);
    }

    #[test]
    fn fit_keeps_view_direction_no_flip() {
        // 回归：fit 不能把相机翻转到目标另一侧（导入后从下方看的根因）
        let mut cam = Camera::default();
        cam.eye = glam::Vec3::new(200.0, 150.0, 200.0);
        cam.target = glam::Vec3::ZERO;
        let before = cam.forward();
        assert!(before.y < 0.0, "setup: looking down");
        cam.fit(glam::Vec3::new(10.0, 0.0, 0.0), 50.0);
        let after = cam.forward();
        assert!(
            (before - after).length() < 1e-4,
            "direction flipped! before={before:?} after={after:?}"
        );
        // eye 必须仍在 target 上方（视线反方向）
        assert!(cam.eye.y > cam.target.y, "eye below target!");
        // eye 相对 target 的方向 = -forward
        let eye_dir = (cam.eye - cam.target).normalize();
        assert!((eye_dir + after).length() < 1e-4);
    }

    #[test]
    fn zoom_keeps_eye_on_same_side() {
        let mut cam = Camera::default();
        cam.eye = glam::Vec3::new(200.0, 150.0, 200.0);
        cam.target = glam::Vec3::ZERO;
        let before = cam.forward();
        cam.zoom(0.5);
        let after = cam.forward();
        assert!((before - after).length() < 1e-4, "zoom flipped camera");
        assert!(cam.eye.y > 0.0, "eye went below: {:?}", cam.eye);
    }
    #[test]
    fn ortho_fit_sets_half_height() {
        let mut cam = Camera::default();
        cam.ortho = true;
        cam.fit(glam::Vec3::ZERO, 100.0);
        assert!(
            (cam.ortho_half - 130.0).abs() < 1e-3,
            "half={}",
            cam.ortho_half
        );
        assert!(cam.far >= 200_000.0);
    }
}
