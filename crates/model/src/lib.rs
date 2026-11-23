//! S33D 模型层：草图实体、平面分割、文档与特征。
//!
//! 本层为纯数据/算法（无 UI、无渲染），供 app 层与测试驱动。

pub mod document;
pub mod fasteners;
pub use fasteners::{FastenerKind, FastenerParams};
mod region_mesh;
pub mod regions;
pub mod sketch;
pub mod trim;

pub use document::{
    Body, BodyId, BodyKind, ChamferEdgeSeg, DerivedBooleanOp, DerivedOperation, Document,
    DocumentArchive, ExtrudeParams, Feature, GearKind, GearParams, Material, ObjectGroup,
    Primitive3DKind, Primitive3DParams, RevolveParams, SketchFeature, SurfaceTextureParams,
    SurfaceTexturePattern, SurfaceTextureSurface, TireParams, TireTreadPattern, WheelRimParams,
    WheelRimStyle,
};
pub use region_mesh::{
    checked_triangles, TriangulationError, TriangulationErrorKind, ValidatedRegionMesh,
    REGION_TRIANGULATION_BACKEND,
};
pub use regions::{
    compute_regions, entities_fingerprint, entity_segments, point_in_region, polygonize,
    triangulate_region, Face,
};
pub use sketch::SketchEntity;
