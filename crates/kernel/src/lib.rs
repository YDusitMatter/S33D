//! S33D 几何内核抽象层。
//!
//! 隔离 truck 基础建模、Manifold 网格布尔和 OCCT 精确曲面后端。
//! STEP / 精确操作保存 B-Rep，网格仅用于显示；旧几何路径继续保留。

pub mod export_file;
mod facet_distance;
mod faceted;
pub mod mesh;
pub mod mesh_bool;
pub mod mesh_io;
pub mod occt;
pub mod region;
pub mod step_out;
pub mod three_mf;
pub mod truck_kernel;

pub use mesh::MeshData;
pub use occt::OffsetMode;
pub use region::{Axis2D, DatumPlane, PlaneDef, Region2D};
pub use truck_kernel::{EdgeCutterSegment, TruckKernel};

use glam::Mat4;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

/// 不可变 shape 句柄（特征重算产生新 id，利于 undo/redo）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ShapeId(pub u64);

impl std::fmt::Display for ShapeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// 内核错误。
#[derive(Debug, Error)]
pub enum KernelError {
    #[error("geometry error: {0}")]
    Geometry(String),
    #[error("boolean operation failed: {0}")]
    Boolean(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unknown shape id {0}")]
    UnknownShape(ShapeId),
    #[error("step io error: {0}")]
    Step(String),
}

pub type Result<T> = std::result::Result<T, KernelError>;

/// 几何内核抽象接口（OCCT 语义）。
///
/// 所有方法返回新的 `ShapeId`（不可变语义）；`tessellate` 是唯一通向
/// 渲染层的出口。坐标单位：毫米；内部 Y-up，STEP 边界处转 Z-up。
pub trait GeometryKernel {
    /// 在指定平面上沿法线方向拉伸一个 2D 区域。
    fn extrude_region(
        &mut self,
        region: &Region2D,
        plane: &PlaneDef,
        depth: f64,
    ) -> Result<ShapeId>;

    /// 在指定平面上绕轴旋转一个 2D 区域。
    fn revolve_region(
        &mut self,
        region: &Region2D,
        plane: &PlaneDef,
        axis: &Axis2D,
        angle_deg: f64,
    ) -> Result<ShapeId>;

    fn boolean_union(&mut self, a: ShapeId, b: ShapeId) -> Result<ShapeId>;
    fn boolean_subtract(&mut self, a: ShapeId, b: ShapeId) -> Result<ShapeId>;
    fn boolean_intersect(&mut self, a: ShapeId, b: ShapeId) -> Result<ShapeId>;

    /// 应用 4x4 变换矩阵，返回新 shape。
    fn transform(&mut self, id: ShapeId, mat: &Mat4) -> Result<ShapeId>;

    /// 跨平面镜像，返回新 shape。
    fn mirror(&mut self, id: ShapeId, plane: &PlaneDef) -> Result<ShapeId>;

    /// 沿平面切分 shape，返回法线正向侧 / 反向侧切出的全部独立连通件
    /// （每件都是带切口封盖的闭合体，数量为两个或多个）。
    fn split_by_plane(&mut self, id: ShapeId, plane: &PlaneDef) -> Result<Vec<ShapeId>>;

    /// 网格化（渲染与选择的唯一入口）。
    fn tessellate(&self, id: ShapeId) -> Result<MeshData>;

    /// 导入 STEP 文件，返回导入的 shape 列表（网格体）。
    fn import_step(&mut self, path: &str) -> Result<Vec<ShapeId>>;
    /// Import IGES B-rep/surfaces with the same assembly placement as STEP.
    fn import_iges(&mut self, path: &str) -> Result<Vec<ShapeId>>;

    /// 导出 faceted B-rep STEP（AP214，毫米）。
    fn export_step(&self, id: ShapeId, path: &str) -> Result<()>;

    /// 导入 STL（二进制/ASCII），返回导入的 shape。
    fn import_stl(&mut self, path: &str) -> Result<Vec<ShapeId>>;

    /// 导入 Wavefront OBJ，返回导入的 shape。
    fn import_obj(&mut self, path: &str) -> Result<Vec<ShapeId>>;

    /// 导入 PLY（ASCII/二进制），返回导入的 shape。
    fn import_ply(&mut self, path: &str) -> Result<Vec<ShapeId>>;

    /// 导入 GLB / glTF，返回导入的 shape 列表。
    fn import_glb(&mut self, path: &str) -> Result<Vec<ShapeId>>;

    /// Import the build geometry of a 3MF package.
    fn import_3mf(&mut self, path: &str) -> Result<Vec<ShapeId>>;

    fn export_stl(&self, id: ShapeId, path: &str) -> Result<()>;
    fn export_obj(&self, id: ShapeId, path: &str) -> Result<()>;
    fn export_ply(&self, id: ShapeId, path: &str) -> Result<()>;
    fn export_glb(&self, id: ShapeId, path: &str) -> Result<()>;

    /// 删除 shape。
    fn delete(&mut self, id: ShapeId);

    /// 引用追踪 GC：只保留 Document 当前状态、undo/redo 和活动预览仍引用的
    /// shape。返回被回收的 shape 数量。
    fn retain_shapes(&mut self, live: &HashSet<ShapeId>) -> usize;

    /// 当前内核中持有的 shape 数量（诊断与 GC 测试）。
    fn shape_count(&self) -> usize;

    /// 全部 shape 的世界包围盒 (min, max)，用于 fit view。
    fn bounds(&self) -> Option<(glam::Vec3, glam::Vec3)>;
}
#[cfg(test)]
mod export_qa;
