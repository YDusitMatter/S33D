//! 2D 区域、平面定义与草图坐标变换。
//!
//! 约定（与 Super3D 原型一致）：
//! - 内部坐标系为 Y-up 世界坐标（毫米），渲染友好。
//! - 草图平面用局部坐标系描述：`origin` 为平面上一点，
//!   `x_axis` / `y_axis` 为平面内两个正交单位向量，`normal = x × y`。
//! - `Region2D` 的点在平面局部 (u, v) 坐标下，`outer` 逆时针、`holes` 顺时针。

use glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize};

/// 平面局部坐标中的 2D 点。
pub type Point2 = [f64; 2];

/// 2D 封闭区域：一个外环 + 若干内孔。
/// `outer` 逆时针（从平面法线方向看），`holes` 顺时针。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region2D {
    pub outer: Vec<Point2>,
    pub holes: Vec<Vec<Point2>>,
}

impl Region2D {
    pub fn new(outer: Vec<Point2>) -> Self {
        Self {
            outer,
            holes: Vec::new(),
        }
    }

    /// 外环的带符号面积（兼容既有语义，不扣除孔洞；outer 应为正）。
    pub fn signed_area(&self) -> f64 {
        signed_area(&self.outer)
    }

    pub fn is_empty(&self) -> bool {
        self.outer.len() < 3
    }
}

/// 带符号面积（鞋带公式）。
pub fn signed_area(pts: &[Point2]) -> f64 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    // Absolute-coordinate products erase small profiles far from the origin.
    // A nearby reference and compensated summation keep the area local.
    let origin = pts[0];
    let mut a = 0.0;
    let mut correction = 0.0;
    for i in 0..n {
        let p = pts[i];
        let q = pts[(i + 1) % n];
        let value =
            (p[0] - origin[0]) * (q[1] - origin[1]) - (q[0] - origin[0]) * (p[1] - origin[1]);
        let adjusted = value - correction;
        let next = a + adjusted;
        correction = (next - a) - adjusted;
        a = next;
    }
    a / 2.0
}

/// 3D 空间中的平面定义（单位向量正交）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlaneDef {
    pub origin: [f64; 3],
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
}

impl PlaneDef {
    /// 构造一个标准平面：以 `normal` 为法线、过原点。
    /// 与 Super3D `makePlaneDef` 相同的 x_axis 选择规则。
    pub fn from_normal(normal: [f64; 3]) -> Self {
        let n = DVec3::from_array(normal).normalize();
        let x = if n.y.abs() > 0.999 {
            DVec3::new(n.y.signum(), 0.0, 0.0)
        } else {
            DVec3::new(0.0, 1.0, 0.0).cross(n).normalize()
        };
        let y = n.cross(x).normalize();
        Self {
            origin: [0.0, 0.0, 0.0],
            x_axis: x.to_array(),
            y_axis: y.to_array(),
        }
    }

    pub fn normal(&self) -> DVec3 {
        DVec3::from_array(self.x_axis)
            .cross(DVec3::from_array(self.y_axis))
            .normalize()
    }

    pub fn x_axis(&self) -> DVec3 {
        DVec3::from_array(self.x_axis).normalize()
    }

    pub fn y_axis(&self) -> DVec3 {
        DVec3::from_array(self.y_axis).normalize()
    }

    pub fn origin(&self) -> DVec3 {
        DVec3::from_array(self.origin)
    }

    /// 平面局部坐标 (u, v) → 世界坐标。
    pub fn to_world(&self, u: f64, v: f64) -> [f64; 3] {
        (self.origin() + u * self.x_axis() + v * self.y_axis()).to_array()
    }

    /// 世界坐标 → 平面局部坐标 (u, v)。
    pub fn to_local(&self, p: [f64; 3]) -> (f64, f64) {
        let rel = DVec3::from_array(p) - self.origin();
        (rel.dot(self.x_axis()), rel.dot(self.y_axis()))
    }

    /// 保证环的方向：`outer_ccw=true` 时返回逆时针环（从法线方向看），否则顺时针。
    pub fn with_orientation(&self, ring: &[Point2], outer_ccw: bool) -> Vec<Point2> {
        let a = signed_area(ring);
        let want_ccw = outer_ccw;
        let is_ccw = a >= 0.0;
        if is_ccw == want_ccw {
            ring.to_vec()
        } else {
            ring.iter().rev().copied().collect()
        }
    }
}

/// 草图平面内的旋转轴（局部坐标）：过 `point`、方向为 `dir`。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Axis2D {
    pub point: Point2,
    pub dir: Point2,
}

impl Axis2D {
    pub fn is_valid(&self) -> bool {
        let l = (self.dir[0] * self.dir[0] + self.dir[1] * self.dir[1]).sqrt();
        l > 1e-12
    }
}

/// 标准基准面（与 Super3D 的 planeSpecs 一致，Y-up）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatumPlane {
    Top,   // XZ 平面（ZX），法线 +Y
    Front, // XY 平面，法线 +Z
    Right, // YZ 平面，法线 +X
}

impl DatumPlane {
    /// 全部三个基准面（顺序：Top/Front/Right）。
    pub const ALL: [DatumPlane; 3] = [DatumPlane::Top, DatumPlane::Front, DatumPlane::Right];

    pub fn def(&self) -> PlaneDef {
        let n = match self {
            DatumPlane::Top => [0.0, 1.0, 0.0],
            DatumPlane::Front => [0.0, 0.0, 1.0],
            DatumPlane::Right => [1.0, 0.0, 0.0],
        };
        PlaneDef::from_normal(n)
    }

    pub fn name(&self) -> &'static str {
        match self {
            DatumPlane::Top => "Top (XZ)",
            DatumPlane::Front => "Front (XY)",
            DatumPlane::Right => "Right (YZ)",
        }
    }
}

impl std::fmt::Display for DatumPlane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// 平面局部坐标包围盒。
pub fn local_bounds(region: &Region2D) -> (Point2, Point2) {
    let mut min = [f64::INFINITY, f64::INFINITY];
    let mut max = [f64::NEG_INFINITY, f64::NEG_INFINITY];
    for p in region.outer.iter().chain(region.holes.iter().flatten()) {
        min[0] = min[0].min(p[0]);
        min[1] = min[1].min(p[1]);
        max[0] = max[0].max(p[0]);
        max[1] = max[1].max(p[1]);
    }
    (min, max)
}

/// glam::Vec3 的便捷构造（从数组）。
pub fn v3(a: [f64; 3]) -> Vec3 {
    Vec3::new(a[0] as f32, a[1] as f32, a[2] as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_def_orthonormal() {
        for n in [
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.3, 0.7, 0.5],
        ] {
            let p = PlaneDef::from_normal(n);
            let x = p.x_axis();
            let y = p.y_axis();
            assert!((x.dot(y)).abs() < 1e-9, "x·y ≈ 0");
            assert!((x.length() - 1.0).abs() < 1e-9);
            assert!((y.length() - 1.0).abs() < 1e-9);
            assert!((p.normal() - DVec3::from_array(n).normalize()).length() < 1e-9);
        }
    }

    #[test]
    fn to_world_roundtrip() {
        let p = PlaneDef::from_normal([0.0, 0.0, 1.0]);
        let w = p.to_world(3.0, 4.0);
        let (u, v) = p.to_local(w);
        assert!((u - 3.0).abs() < 1e-9);
        assert!((v - 4.0).abs() < 1e-9);
    }

    #[test]
    fn signed_area_ccw_positive() {
        let rect = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]];
        assert!(signed_area(&rect) > 0.0);
        let rev: Vec<Point2> = rect.iter().rev().copied().collect();
        assert!(signed_area(&rev) < 0.0);
    }

    #[test]
    fn signed_area_remains_local_after_large_translation() {
        for offset in [0., 1e6, 1e9] {
            let ring = vec![
                [offset, offset],
                [offset + 10., offset],
                [offset + 10., offset + 10.],
                [offset, offset + 10.],
            ];
            assert_eq!(signed_area(&ring), 100.);
            let region = Region2D {
                outer: ring.clone(),
                holes: vec![vec![
                    [offset + 4., offset + 4.],
                    [offset + 6., offset + 4.],
                    [offset + 6., offset + 6.],
                    [offset + 4., offset + 6.],
                ]],
            };
            assert_eq!(region.signed_area(), 100., "outer-only contract changed");
            assert_eq!(
                signed_area(&ring.into_iter().rev().collect::<Vec<_>>()),
                -100.
            );
        }
    }
}
