//! 文档模型：Body 列表、参数化特征记录、快照式 undo/redo。
//!
//! 与 Super3D 原型一致：每个 body 携带轻量 `feature` 参数记录
//! （草图数据 / 拉伸参数），几何由内核 ShapeId 持有。undo/redo 为
//! 整文档 JSON 快照（原型方案，工程可靠；命令级升级留待后续）。

use crate::sketch::SketchEntity;
use s33d_kernel::{PlaneDef, ShapeId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub type BodyId = u64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BodyKind {
    Curve,
    Surface,
    /// 保留在平面上的草图（可继续选区域拉伸）。
    Sketch,
    /// 3D 实体。
    Solid,
}

/// 拉伸特征参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtrudeParams {
    pub depth: f64,
    pub symmetric: bool,
    pub bevel: f64,
}

/// 旋转特征参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevolveParams {
    /// 旋转轴（草图平面局部坐标）。
    pub axis_point: [f64; 2],
    pub axis_dir: [f64; 2],
    pub angle_deg: f64,
}

/// 草图特征（sketch body 的数据，Solid body 的来源草图）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SketchFeature {
    pub plane: PlaneDef,
    pub entities: Vec<SketchEntity>,
    /// A missing entry uses the first placed point. An explicit null unlocks it.
    /// Polygon point zero is the center and subsequent indices are its vertices.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vertex_locks: BTreeMap<usize, Option<usize>>,
    /// Display all three world-space coordinates while editing the sketch.
    /// Equation-defined planes use this because no pair of XYZ axes fully
    /// describes an oblique plane. Older project files default to the usual
    /// two in-plane datum axes.
    #[serde(default)]
    pub world_xyz_coordinates: bool,
    /// 该草图创建于哪个 body 的表面（表面 sketch 时记录；基准面 sketch 为 None）。
    /// Pocket 用它自动确定目标体。
    #[serde(default)]
    pub on_body: Option<BodyId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Primitive3DKind {
    Box,
    Cylinder,
    Sphere,
    Cone,
    Torus,
}

impl Primitive3DKind {
    pub const ALL: [Self; 5] = [
        Self::Box,
        Self::Cylinder,
        Self::Sphere,
        Self::Cone,
        Self::Torus,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Box => "Box",
            Self::Cylinder => "Cylinder",
            Self::Sphere => "Sphere",
            Self::Cone => "Cone",
            Self::Torus => "Torus",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Primitive3DParams {
    pub kind: Primitive3DKind,
    /// 底部中心；Sphere/Torus 同样以最低点落在该 Y 高度。
    pub position: [f64; 3],
    /// 可选的基础体局部坐标系。None 保持 Draw 3D 的默认世界朝向；
    /// 从 Extrude/Revolve 识别时保存原始空间方向，参数编辑不会把对象转回世界轴。
    #[serde(default)]
    pub frame: Option<PlaneDef>,
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    pub radius: f64,
    pub top_radius: f64,
    pub major_radius: f64,
    pub minor_radius: f64,
}

/// A rebuildable operation behind a [`Feature::Derived`] body.
///
/// Older project files only stored a human-readable `source` string. Keeping
/// this metadata optional preserves those files while new Boolean results can
/// participate in the parametric dependency graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DerivedOperation {
    Boolean {
        op: DerivedBooleanOp,
        operands: Vec<BodyId>,
    },
    /// A mirrored result follows its source geometry while retaining the
    /// world-space mirror plane chosen when the feature was created.
    Mirror { source: BodyId, plane: PlaneDef },
    /// A translated duplicate. Duplicate, Paste and Array all use the same
    /// structured recipe so upstream parametric edits remain connected.
    Copy {
        source: BodyId,
        offset: [f64; 3],
        /// Frozen primitive provenance keeps a copied cylinder identifiable
        /// after its source is removed. Consumers must still verify the
        /// current geometry; these parameters do not override the Copy recipe.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        primitive_snapshot: Option<Primitive3DParams>,
    },
    /// One deterministic piece from a planar split. `piece_index` follows the
    /// kernel's positive-side then negative-side component ordering.
    Split {
        source: BodyId,
        plane: PlaneDef,
        piece_index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DerivedBooleanOp {
    Union,
    Subtract,
    Intersect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GearKind {
    Spur,
    Helical,
    Bevel,
    Internal,
}

impl GearKind {
    pub const ALL: [Self; 4] = [Self::Spur, Self::Helical, Self::Bevel, Self::Internal];

    pub fn name(self) -> &'static str {
        match self {
            Self::Spur => "Spur Gear",
            Self::Helical => "Helical Gear",
            Self::Bevel => "Bevel Gear",
            Self::Internal => "Internal Gear",
        }
    }
}

/// Parametric gear generated by the built-in Gear Generator plug-in.
/// Dimensions are stored in the document's canonical millimeters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GearParams {
    pub kind: GearKind,
    pub module: f64,
    pub teeth: u32,
    pub pressure_angle_deg: f64,
    pub face_width: f64,
    pub bore_diameter: f64,
    pub backlash: f64,
    pub helix_angle_deg: f64,
    pub bevel_angle_deg: f64,
    pub ring_wall_thickness: f64,
    pub position: [f64; 3],
}

impl Default for GearParams {
    fn default() -> Self {
        Self {
            kind: GearKind::Spur,
            module: 2.0,
            teeth: 24,
            pressure_angle_deg: 20.0,
            face_width: 10.0,
            bore_diameter: 8.0,
            backlash: 0.05,
            helix_angle_deg: 20.0,
            bevel_angle_deg: 30.0,
            ring_wall_thickness: 6.0,
            position: [0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TireTreadPattern {
    HighwayRib,
    Block,
    Chevron,
    AllTerrain,
}

impl TireTreadPattern {
    pub const ALL: [Self; 4] = [
        Self::HighwayRib,
        Self::Block,
        Self::Chevron,
        Self::AllTerrain,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::HighwayRib => "Highway Rib",
            Self::Block => "Block Tread",
            Self::Chevron => "Chevron Tread",
            Self::AllTerrain => "All-Terrain",
        }
    }
}

/// Parametric tire generated by the built-in Tire Generator plug-in.
/// `position` is the center of the wheel and dimensions are millimeters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TireParams {
    pub outer_diameter: f64,
    pub section_width: f64,
    pub rim_diameter: f64,
    pub tread_depth: f64,
    pub tread_repeats: u32,
    pub pattern: TireTreadPattern,
    pub position: [f64; 3],
}

impl Default for TireParams {
    fn default() -> Self {
        Self {
            outer_diameter: 660.0,
            section_width: 235.0,
            rim_diameter: 457.2,
            tread_depth: 8.0,
            tread_repeats: 36,
            pattern: TireTreadPattern::HighwayRib,
            position: [0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WheelRimStyle {
    FiveSpoke,
    SixSpoke,
    SplitFive,
    MultiSpoke,
}

impl WheelRimStyle {
    pub const ALL: [Self; 4] = [
        Self::FiveSpoke,
        Self::SixSpoke,
        Self::SplitFive,
        Self::MultiSpoke,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::FiveSpoke => "Five Spoke",
            Self::SixSpoke => "Six Spoke",
            Self::SplitFive => "Split Five Spoke",
            Self::MultiSpoke => "Multi Spoke",
        }
    }
}

/// Parametric road-wheel rim generated by the Wheel Rim Generator plug-in.
/// `position` is the wheel center and the rotation axis is world Y.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WheelRimParams {
    pub diameter: f64,
    pub width: f64,
    pub rim_thickness: f64,
    pub hub_diameter: f64,
    pub center_bore_diameter: f64,
    pub spoke_width: f64,
    pub style: WheelRimStyle,
    pub bolt_count: u32,
    pub bolt_circle_diameter: f64,
    pub bolt_hole_diameter: f64,
    pub position: [f64; 3],
}

impl Default for WheelRimParams {
    fn default() -> Self {
        Self {
            diameter: 457.2,
            width: 203.2,
            rim_thickness: 14.0,
            hub_diameter: 150.0,
            center_bore_diameter: 66.6,
            spoke_width: 28.0,
            style: WheelRimStyle::FiveSpoke,
            bolt_count: 5,
            bolt_circle_diameter: 114.3,
            bolt_hole_diameter: 14.0,
            position: [0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceTexturePattern {
    Straight,
    DiagonalLeft,
    DiagonalRight,
    Diamond,
}

impl SurfaceTexturePattern {
    pub const ALL: [Self; 4] = [
        Self::Straight,
        Self::DiagonalLeft,
        Self::DiagonalRight,
        Self::Diamond,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Straight => "Straight Knurl",
            Self::DiagonalLeft => "Left Diagonal",
            Self::DiagonalRight => "Right Diagonal",
            Self::Diamond => "Diamond Knurl",
        }
    }
}

/// Stable geometric description of the surface selected for a texture.
/// Planar textures use an oriented rectangle. Cylindrical textures cover the
/// continuous lateral surface around `axis`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SurfaceTextureSurface {
    Planar {
        origin: [f64; 3],
        normal: [f64; 3],
        x_axis: [f64; 3],
        width: f64,
        height: f64,
    },
    Cylinder {
        center: [f64; 3],
        axis: [f64; 3],
        x_axis: [f64; 3],
        radius: f64,
        length: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SurfaceTextureParams {
    pub pattern: SurfaceTexturePattern,
    pub pitch: f64,
    pub groove_width: f64,
    pub depth: f64,
    pub angle_deg: f64,
    pub margin: f64,
}

impl Default for SurfaceTextureParams {
    fn default() -> Self {
        Self {
            pattern: SurfaceTexturePattern::Diamond,
            pitch: 4.0,
            groove_width: 1.0,
            depth: 0.6,
            angle_deg: 45.0,
            margin: 1.0,
        }
    }
}

impl Primitive3DParams {
    pub fn defaults(kind: Primitive3DKind) -> Self {
        Self {
            kind,
            position: [0.0, 0.0, 0.0],
            frame: None,
            width: 20.0,
            depth: 20.0,
            height: 20.0,
            radius: 10.0,
            top_radius: 0.0,
            major_radius: 15.0,
            minor_radius: 4.0,
        }
    }
}

/// 特征记录（与原型 `feature: {kind, sketch, params}` 对应）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EquationCurveParams {
    pub expressions: [String; 3],
    pub spherical: bool,
    pub degrees: bool,
    pub domain: [f64; 2],
    /// Length of one expression unit in canonical millimeters.
    pub unit_mm: f64,
    pub transform: [f64; 16],
}
impl Default for EquationCurveParams {
    fn default() -> Self {
        Self {
            expressions: ["20*cos(t)".into(), "20*sin(t)".into(), "2*t".into()],
            spherical: false,
            degrees: false,
            domain: [0.0, std::f64::consts::TAU * 2.0],
            unit_mm: 1.0,
            transform: glam::DMat4::IDENTITY.to_cols_array(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EquationSurfaceParams {
    pub expressions: [String; 3],
    pub spherical: bool,
    pub degrees: bool,
    pub u_domain: [f64; 2],
    pub v_domain: [f64; 2],
    pub unit_mm: f64,
    pub transform: [f64; 16],
    /// Interpolation subdivisions per parameter, bounded by the evaluator.
    pub resolution: u32,
    /// Only an already closed surface can be converted to a volume.
    pub fill: bool,
}

impl Default for EquationSurfaceParams {
    fn default() -> Self {
        Self {
            expressions: ["u".into(), "v".into(), "0.02*(u^2-v^2)".into()],
            spherical: false,
            degrees: false,
            u_domain: [-10.0, 10.0],
            v_domain: [-10.0, 10.0],
            unit_mm: 1.0,
            transform: glam::DMat4::IDENTITY.to_cols_array(),
            resolution: 24,
            fill: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Feature {
    Fastener {
        params: crate::fasteners::FastenerParams,
    },
    EquationSurface {
        params: EquationSurfaceParams,
    },
    EquationCurve {
        params: EquationCurveParams,
        points: Vec<[f64; 3]>,
    },
    /// Exact offset owns an immutable input snapshot; subsequent source edits
    /// do not silently retarget its topology-face indices.
    ExactOffset {
        input_brep: String,
        distance: f64,
        tolerance: f64,
        faces: Vec<u32>,
        mode: s33d_kernel::OffsetMode,
    },
    Sketch(SketchFeature),
    Extrude {
        sketch: SketchFeature,
        params: ExtrudeParams,
        /// Some when this feature was created from a persistent Sketch body.
        /// This is the stable dependency edge used for parametric rebuilds.
        #[serde(default)]
        source_sketch: Option<BodyId>,
        /// Sketches consumed by Pockets. Selection provenance only: unlike
        /// source_sketch, this does not rebuild a Boolean result as an extrusion.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pocket_source_sketches: Vec<BodyId>,
        /// Deterministic indices from `compute_regions`; empty means legacy
        /// single-profile data and is accepted only when exactly one region exists.
        #[serde(default)]
        region_indices: Vec<usize>,
    },
    Revolve {
        sketch: SketchFeature,
        params: RevolveParams,
        #[serde(default)]
        source_sketch: Option<BodyId>,
        /// None means legacy single-profile data.
        #[serde(default)]
        region_index: Option<usize>,
    },
    /// Draw 3D 创建的可回选编辑基础体。
    Primitive3D {
        params: Primitive3DParams,
    },
    /// Gear Generator plug-in result; retained parametrically for re-editing.
    Gear {
        params: GearParams,
    },
    /// Tire Generator plug-in result; retained parametrically for re-editing.
    Tire {
        params: TireParams,
    },
    /// Wheel Rim Generator plug-in result; retained parametrically for re-editing.
    WheelRim {
        params: WheelRimParams,
    },
    /// Non-destructive knurl operation. The source body remains hidden and the
    /// selected surface is stored as stable construction geometry.
    SurfaceTexture {
        source: BodyId,
        surface: SurfaceTextureSurface,
        params: SurfaceTextureParams,
    },
    /// 布尔/镜像等网格级操作的结果（保留来源引用便于重建）。
    Derived {
        /// 描述性来源（如 "union of #1 and #2"）。
        source: String,
        /// Structured dependency data used for downstream rebuilds. Legacy
        /// projects deserialize this as `None` and keep their frozen geometry.
        #[serde(default)]
        operation: Option<DerivedOperation>,
    },
    Imported {
        path: String,
    },
    /// 倒角/圆角操作（可点选改参数重算）。
    Chamfer {
        /// 目标体。
        target: BodyId,
        /// 基础体（该操作的差集来源：原目标体或上一个 chamfer 结果，链式累积）。
        #[serde(default)]
        base: Option<BodyId>,
        /// 棱链（世界坐标段 + 相邻面法线）。
        edges: Vec<ChamferEdgeSeg>,
        /// Bounds of the base shape when `edges` were selected. Together with
        /// the edge-chain geometry this forms a stable geometric selector used
        /// to find the corresponding chain after an upstream rebuild.
        #[serde(default)]
        reference_bounds: Option<[[f64; 3]; 2]>,
        /// 倒角距离（与 fillet 二选一）。
        chamfer: Option<f64>,
        /// 圆角半径。
        fillet: Option<f64>,
    },
}

/// 倒角/圆角棱链的一段（世界坐标）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChamferEdgeSeg {
    pub a: [f64; 3],
    pub b: [f64; 3],
    pub n1: [f64; 3],
    pub n2: [f64; 3],
}

impl Feature {
    pub fn describe(&self) -> String {
        match self {
            Feature::Fastener { params } => format!(
                "{} M{} × {}",
                params.kind.name(),
                params.diameter,
                params.pitch
            ),
            Feature::EquationCurve { .. } => "3D equation curve".into(),
            Feature::EquationSurface { .. } => "Equation surface".into(),
            Feature::ExactOffset { distance, .. } => format!("Exact offset / shell {distance} mm"),
            Feature::Sketch(_) => "Sketch".into(),
            Feature::Extrude { params, .. } => format!("Extrude {} mm", params.depth),
            Feature::Revolve { params, .. } => format!("Revolve {}°", params.angle_deg),
            Feature::Primitive3D { params } => format!("Draw 3D {}", params.kind.name()),
            Feature::Gear { params } => {
                format!(
                    "{} (m{}, {}T)",
                    params.kind.name(),
                    params.module,
                    params.teeth
                )
            }
            Feature::Tire { params } => format!(
                "Tire {} / {} mm ({})",
                params.outer_diameter,
                params.rim_diameter,
                params.pattern.name()
            ),
            Feature::WheelRim { params } => {
                format!("Wheel Rim {} mm ({})", params.diameter, params.style.name())
            }
            Feature::SurfaceTexture { params, .. } => format!(
                "{} (pitch {} mm, depth {} mm)",
                params.pattern.name(),
                params.pitch,
                params.depth
            ),
            Feature::Derived { source, .. } => source.clone(),
            Feature::Imported { path } => format!("Import {path}"),
            Feature::Chamfer {
                chamfer, fillet, ..
            } => {
                if let Some(d) = chamfer {
                    format!("Chamfer {d} mm")
                } else if let Some(r) = fillet {
                    format!("Fillet {r} mm")
                } else {
                    "Chamfer".into()
                }
            }
        }
    }

    pub fn sketch(&self) -> Option<&SketchFeature> {
        match self {
            Feature::Sketch(s)
            | Feature::Extrude { sketch: s, .. }
            | Feature::Revolve { sketch: s, .. } => Some(s),
            _ => None,
        }
    }
}

/// 材质（与 Super3D 属性面板一致）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub color: [f32; 4],
    pub metalness: f32,
    pub roughness: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            color: [0.60, 0.65, 0.72, 1.0], // #9aa7b8
            metalness: 0.1,
            roughness: 0.6,
        }
    }
}

/// 一个可渲染对象：sketch 平面图或 3D 实体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Body {
    pub id: BodyId,
    pub name: String,
    pub kind: BodyKind,
    pub feature: Feature,
    /// 内核 ShapeId（几何真相在 kernel）。
    pub shape: Option<ShapeId>,
    pub material: Material,
    pub visible: bool,
}

impl Body {
    pub fn is_sketch(&self) -> bool {
        self.kind == BodyKind::Sketch
    }

    pub fn sketch(&self) -> Option<&SketchFeature> {
        self.feature.sketch()
    }
}

/// 整文档快照（undo/redo 与工程保存共用）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectGroup {
    pub id: u64,
    pub name: String,
    pub members: Vec<BodyId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSnapshot {
    pub bodies: Vec<Body>,
    #[serde(default)]
    pub groups: Vec<ObjectGroup>,
    pub next_body_id: u64,
    pub name: String,
}

/// `.s33d` 工程中的完整文档状态：当前 objects 以及 undo/redo 快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentArchive {
    pub current: DocumentSnapshot,
    pub undo_stack: Vec<DocumentSnapshot>,
    pub redo_stack: Vec<DocumentSnapshot>,
}

impl DocumentArchive {
    /// 工程及其历史中仍可能被引用的全部内核 shape。
    pub fn shape_ids(&self) -> Vec<ShapeId> {
        let mut ids = HashSet::new();
        for snapshot in std::iter::once(&self.current)
            .chain(self.undo_stack.iter())
            .chain(self.redo_stack.iter())
        {
            for body in &snapshot.bodies {
                if let Some(shape) = body.shape {
                    ids.insert(shape);
                }
            }
        }
        let mut ids: Vec<_> = ids.into_iter().collect();
        ids.sort_unstable();
        ids
    }

    /// 打开工程时把文件内的旧 ShapeId 映射到新 kernel 中的 ShapeId。
    pub fn remap_shapes(&mut self, map: &HashMap<ShapeId, ShapeId>) -> Result<(), String> {
        for snapshot in std::iter::once(&mut self.current)
            .chain(self.undo_stack.iter_mut())
            .chain(self.redo_stack.iter_mut())
        {
            for body in &mut snapshot.bodies {
                if let Some(old) = body.shape {
                    body.shape =
                        Some(map.get(&old).copied().ok_or_else(|| {
                            format!("project is missing geometry for shape {old}")
                        })?);
                }
            }
        }
        Ok(())
    }
}

/// 文档：body 集合 + 撤销历史。
#[derive(Debug, Clone, Default)]
pub struct Document {
    // Session-only change token, deliberately excluded from archived snapshots.
    mutation_serial: u64,
    pub bodies: Vec<Body>,
    pub groups: Vec<ObjectGroup>,
    next_body_id: u64,
    name: String,
    undo_stack: Vec<DocumentSnapshot>,
    redo_stack: Vec<DocumentSnapshot>,
}

fn next_mutation_serial() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Document {
    pub fn mutation_serial(&self) -> u64 {
        self.mutation_serial
    }
    fn touch(&mut self) {
        self.mutation_serial = next_mutation_serial();
    }
    /// Undo snapshots are intentionally bounded. Shapes that fall out of this
    /// window become collectible by the application-level reference GC.
    pub const MAX_UNDO_STEPS: usize = 200;

    pub fn new(name: &str) -> Self {
        Self {
            mutation_serial: next_mutation_serial(),
            bodies: Vec::new(),
            groups: Vec::new(),
            next_body_id: 1,
            name: name.to_string(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn rename(&mut self, name: &str) {
        self.touch();
        self.name = name.to_string();
    }

    /// 当前快照。
    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            bodies: self.bodies.clone(),
            groups: self.groups.clone(),
            next_body_id: self.next_body_id,
            name: self.name.clone(),
        }
    }

    pub fn archive(&self) -> DocumentArchive {
        DocumentArchive {
            current: self.snapshot(),
            undo_stack: self.undo_stack.clone(),
            redo_stack: self.redo_stack.clone(),
        }
    }

    /// Shape references held by current objects and both history stacks,
    /// without cloning the full archive.
    pub fn referenced_shape_ids(&self) -> HashSet<ShapeId> {
        let mut ids = HashSet::new();
        for body in &self.bodies {
            if let Some(shape) = body.shape {
                ids.insert(shape);
            }
        }
        for snapshot in self.undo_stack.iter().chain(self.redo_stack.iter()) {
            for body in &snapshot.bodies {
                if let Some(shape) = body.shape {
                    ids.insert(shape);
                }
            }
        }
        ids
    }

    pub fn from_archive(archive: DocumentArchive) -> Self {
        Self {
            mutation_serial: next_mutation_serial(),
            bodies: archive.current.bodies,
            groups: archive.current.groups,
            next_body_id: archive.current.next_body_id,
            name: archive.current.name,
            undo_stack: archive.undo_stack,
            redo_stack: archive.redo_stack,
        }
    }

    fn restore(&mut self, snap: DocumentSnapshot) {
        self.touch();
        self.bodies = snap.bodies;
        self.groups = snap.groups;
        self.next_body_id = snap.next_body_id;
        self.name = snap.name;
    }

    /// 记录修改前状态（每个可变操作的第一步调用）。
    pub fn begin_change(&mut self) {
        self.touch();
        self.undo_stack.push(self.snapshot());
        if self.undo_stack.len() > Self::MAX_UNDO_STEPS {
            let excess = self.undo_stack.len() - Self::MAX_UNDO_STEPS;
            self.undo_stack.drain(0..excess);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> bool {
        if let Some(snap) = self.undo_stack.pop() {
            self.redo_stack.push(self.snapshot());
            self.restore(snap);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(snap) = self.redo_stack.pop() {
            self.undo_stack.push(self.snapshot());
            self.restore(snap);
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn add_body(&mut self, body: Body) -> BodyId {
        self.touch();
        let id = body.id;
        self.bodies.push(body);
        id
    }

    pub fn alloc_id(&mut self) -> BodyId {
        let id = self.next_body_id;
        self.next_body_id += 1;
        id
    }

    pub fn body(&self, id: BodyId) -> Option<&Body> {
        self.bodies.iter().find(|b| b.id == id)
    }

    pub fn body_mut(&mut self, id: BodyId) -> Option<&mut Body> {
        let index = self.bodies.iter().position(|b| b.id == id)?;
        self.touch();
        self.bodies.get_mut(index)
    }

    pub fn remove_body(&mut self, id: BodyId) -> Option<Body> {
        if let Some(idx) = self.bodies.iter().position(|b| b.id == id) {
            self.touch();
            for group in &mut self.groups {
                group.members.retain(|member| *member != id);
            }
            self.groups.retain(|group| !group.members.is_empty());
            Some(self.bodies.remove(idx))
        } else {
            None
        }
    }

    /// Flat, disjoint groups: regrouping moves members without copying bodies.
    /// The caller owns the transaction, just like add_body/remove_body.
    pub fn group_objects(&mut self, ids: &[BodyId]) -> Option<u64> {
        let members: Vec<_> = self
            .bodies
            .iter()
            .filter(|b| ids.contains(&b.id))
            .map(|b| b.id)
            .collect();
        if members.len() < 2 {
            return None;
        }
        self.touch();
        let id = self.groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        for group in &mut self.groups {
            group.members.retain(|id| !members.contains(id));
        }
        self.groups.retain(|g| !g.members.is_empty());
        self.groups.push(ObjectGroup {
            id,
            name: format!("Group {id}"),
            members,
        });
        Some(id)
    }

    /// Ungroup any group touched by this selection, retaining all objects.
    pub fn ungroup_objects(&mut self, ids: &[BodyId]) -> bool {
        let old = self.groups.len();
        self.groups
            .retain(|g| !g.members.iter().any(|id| ids.contains(id)));
        let changed = old != self.groups.len();
        if changed {
            self.touch();
        }
        changed
    }

    pub fn group_for(&self, id: BodyId) -> Option<&ObjectGroup> {
        self.groups.iter().find(|g| g.members.contains(&id))
    }

    /// 全部可见 solid 的 shape 列表。
    pub fn solid_shapes(&self) -> Vec<ShapeId> {
        self.bodies
            .iter()
            .filter(|b| b.visible && b.kind == BodyKind::Solid)
            .filter_map(|b| b.shape)
            .collect()
    }

    /// 序列化为 JSON（工程保存）。
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.snapshot())
    }

    /// 从 JSON 恢复（工程加载）。
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let snap: DocumentSnapshot = serde_json::from_str(json)?;
        let mut doc = Self::new(&snap.name);
        doc.bodies = snap.bodies;
        doc.groups = snap.groups;
        doc.next_body_id = snap.next_body_id;
        doc.undo_stack.clear();
        doc.redo_stack.clear();
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sketch::SketchEntity;

    fn sample_body(id: BodyId) -> Body {
        Body {
            id,
            name: format!("Body {id}"),
            kind: BodyKind::Solid,
            feature: Feature::Extrude {
                sketch: SketchFeature {
                    vertex_locks: Default::default(),
                    plane: PlaneDef::from_normal([0.0, 0.0, 1.0]),
                    entities: vec![SketchEntity::Rect {
                        c1: [0.0, 0.0],
                        c2: [10.0, 5.0],
                    }],
                    world_xyz_coordinates: false,
                    on_body: None,
                },
                params: ExtrudeParams {
                    depth: 20.0,
                    symmetric: false,
                    bevel: 0.0,
                },
                source_sketch: None,
                pocket_source_sketches: Vec::new(),
                region_indices: Vec::new(),
            },
            shape: Some(ShapeId(1)),
            material: Material::default(),
            visible: true,
        }
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut doc = Document::new("test");
        doc.begin_change();
        let id = doc.alloc_id();
        doc.add_body(sample_body(id));
        assert_eq!(doc.bodies.len(), 1);
        assert!(doc.undo());
        assert_eq!(doc.bodies.len(), 0);
        assert!(doc.redo());
        assert_eq!(doc.bodies.len(), 1);
        // 仍可撤销（回到 s0）
        assert!(doc.undo());
        assert_eq!(doc.bodies.len(), 0);
    }

    #[test]
    fn json_roundtrip() {
        let mut doc = Document::new("proj");
        doc.begin_change();
        let id = doc.alloc_id();
        doc.add_body(sample_body(id));
        let json = doc.to_json().unwrap();
        let doc2 = Document::from_json(&json).unwrap();
        assert_eq!(doc2.bodies.len(), 1);
        assert_eq!(doc2.name(), "proj");
        assert_eq!(doc2.bodies[0].feature.describe(), "Extrude 20 mm");
    }

    #[test]
    fn sketch_dependency_metadata_survives_project_json() {
        let mut doc = Document::new("dependent");
        let mut body = sample_body(2);
        if let Feature::Extrude {
            source_sketch,
            region_indices,
            ..
        } = &mut body.feature
        {
            *source_sketch = Some(1);
            *region_indices = vec![0, 2];
        }
        doc.add_body(body);

        let restored = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert!(matches!(
            &restored.bodies[0].feature,
            Feature::Extrude {
                source_sketch: Some(1),
                region_indices,
                ..
            } if region_indices == &vec![0, 2]
        ));
    }

    #[test]
    fn pocket_selection_provenance_is_backward_compatible_and_not_a_rebuild_link() {
        let mut body = sample_body(2);
        let legacy = serde_json::to_value(&body).unwrap();
        assert!(legacy["feature"]["Extrude"]
            .get("pocket_source_sketches")
            .is_none());
        let restored: Body = serde_json::from_value(legacy).unwrap();
        assert_eq!(body, restored);
        if let Feature::Extrude {
            pocket_source_sketches,
            ..
        } = &mut body.feature
        {
            *pocket_source_sketches = vec![1, 3];
        }
        let restored: Body = serde_json::from_str(&serde_json::to_string(&body).unwrap()).unwrap();
        assert!(matches!(restored.feature,
            Feature::Extrude { source_sketch: None, pocket_source_sketches, .. }
                if pocket_source_sketches == vec![1, 3]));
    }

    #[test]
    fn draw_3d_parameters_survive_json_roundtrip() {
        let params = Primitive3DParams {
            kind: Primitive3DKind::Torus,
            position: [1.0, 2.0, 3.0],
            major_radius: 24.0,
            minor_radius: 5.0,
            ..Primitive3DParams::defaults(Primitive3DKind::Torus)
        };
        let feature = Feature::Primitive3D { params };
        let json = serde_json::to_string(&feature).unwrap();
        let restored: Feature = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, feature);
        assert_eq!(restored.describe(), "Draw 3D Torus");
    }

    #[test]
    fn gear_parameters_survive_json_roundtrip() {
        let params = GearParams {
            kind: GearKind::Helical,
            module: 1.25,
            teeth: 31,
            pressure_angle_deg: 20.0,
            face_width: 12.0,
            bore_diameter: 6.0,
            backlash: 0.04,
            helix_angle_deg: -18.0,
            bevel_angle_deg: 30.0,
            ring_wall_thickness: 5.0,
            position: [4.0, 5.0, 6.0],
        };
        let feature = Feature::Gear { params };
        let json = serde_json::to_string(&feature).unwrap();
        let restored: Feature = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, feature);
        assert_eq!(restored.describe(), "Helical Gear (m1.25, 31T)");
    }

    #[test]
    fn structured_boolean_dependencies_survive_json_roundtrip() {
        let feature = Feature::Derived {
            source: "Union of 2, 3, 5".into(),
            operation: Some(DerivedOperation::Boolean {
                op: DerivedBooleanOp::Union,
                operands: vec![2, 3, 5],
            }),
        };
        let json = serde_json::to_string(&feature).unwrap();
        let restored: Feature = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, feature);
    }

    #[test]
    fn structured_single_source_dependencies_survive_json_roundtrip() {
        let mut plane = PlaneDef::from_normal([1.0, 0.0, 0.0]);
        plane.origin = [4.0, 0.0, 0.0];
        for operation in [
            DerivedOperation::Mirror { source: 7, plane },
            DerivedOperation::Copy {
                source: 7,
                offset: [20.0, -5.0, 2.0],
                primitive_snapshot: None,
            },
            DerivedOperation::Split {
                source: 7,
                plane,
                piece_index: 1,
            },
        ] {
            let feature = Feature::Derived {
                source: "structured result".into(),
                operation: Some(operation),
            };
            let json = serde_json::to_string(&feature).unwrap();
            let restored: Feature = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, feature);
        }
    }

    #[test]
    fn copy_primitive_snapshot_is_optional_and_survives_json_roundtrip() {
        let legacy: DerivedOperation =
            serde_json::from_str(r#"{"Copy":{"source":7,"offset":[20.0,-5.0,2.0]}}"#).unwrap();
        assert_eq!(
            legacy,
            DerivedOperation::Copy {
                source: 7,
                offset: [20.0, -5.0, 2.0],
                primitive_snapshot: None,
            }
        );
        assert!(!serde_json::to_string(&legacy)
            .unwrap()
            .contains("primitive_snapshot"));

        let operation = DerivedOperation::Copy {
            source: 7,
            offset: [20.0, -5.0, 2.0],
            primitive_snapshot: Some(Primitive3DParams {
                radius: 3.0,
                height: 30.0,
                ..Primitive3DParams::defaults(Primitive3DKind::Cylinder)
            }),
        };
        let json = serde_json::to_string(&operation).unwrap();
        assert_eq!(
            serde_json::from_str::<DerivedOperation>(&json).unwrap(),
            operation
        );
    }

    #[test]
    fn legacy_derived_feature_loads_as_frozen_geometry() {
        let restored: Feature =
            serde_json::from_str(r#"{"Derived":{"source":"legacy union"}}"#).unwrap();
        assert!(matches!(
            restored,
            Feature::Derived {
                source,
                operation: None
            } if source == "legacy union"
        ));
    }

    #[test]
    fn remove_and_find() {
        let mut doc = Document::new("t");
        doc.begin_change();
        let id = doc.alloc_id();
        doc.add_body(sample_body(id));
        assert!(doc.body(id).is_some());
        doc.begin_change();
        doc.remove_body(id);
        assert!(doc.body(id).is_none());
    }

    #[test]
    fn group_membership_roundtrips_history_and_legacy_documents() {
        let mut doc = Document::new("groups");
        for id in 1..=4 {
            doc.add_body(sample_body(id));
        }
        let bodies = doc.bodies.clone();
        doc.begin_change();
        let first = doc.group_objects(&[3, 1, 3, 999]).unwrap();
        assert_eq!(doc.group_for(1).unwrap().members, vec![1, 3]);
        assert_eq!(doc.bodies, bodies, "grouping must not edit geometry or IDs");
        let mut restored = Document::from_archive(doc.archive());
        assert!(restored.undo());
        assert!(restored.groups.is_empty());
        assert!(restored.redo());
        assert_eq!(restored.groups, doc.groups);
        doc.begin_change();
        let second = doc.group_objects(&[3, 4]).unwrap();
        assert_ne!(first, second);
        assert_eq!(doc.group_for(1).unwrap().members, vec![1]);
        assert_eq!(doc.group_for(3).unwrap().members, vec![3, 4]);
        doc.begin_change();
        doc.remove_body(3);
        assert_eq!(doc.group_for(4).unwrap().members, vec![4]);
        assert!(doc.undo());
        assert_eq!(doc.group_for(4).unwrap().members, vec![3, 4]);
        doc.begin_change();
        assert!(doc.ungroup_objects(&[4]));
        assert!(doc.group_for(3).is_none());
        assert_eq!(doc.bodies, bodies);
        let json = doc.to_json().unwrap();
        assert_eq!(Document::from_json(&json).unwrap().groups, doc.groups);
        let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
        legacy.as_object_mut().unwrap().remove("groups");
        assert!(Document::from_json(&legacy.to_string())
            .unwrap()
            .groups
            .is_empty());
    }

    #[test]
    fn hidden_solids_are_excluded_from_visible_shape_list() {
        let mut doc = Document::new("visibility");
        let mut visible = sample_body(1);
        visible.shape = Some(ShapeId(11));
        let mut hidden = sample_body(2);
        hidden.shape = Some(ShapeId(22));
        hidden.visible = false;
        doc.add_body(visible);
        doc.add_body(hidden);

        assert_eq!(doc.solid_shapes(), vec![ShapeId(11)]);
    }

    #[test]
    fn archive_roundtrip_preserves_undo_redo_and_remaps_shapes() {
        let mut doc = Document::new("archive");
        doc.begin_change();
        let id = doc.alloc_id();
        doc.add_body(sample_body(id));
        doc.begin_change();
        doc.body_mut(id).unwrap().name = "Renamed".into();

        let mut archive = doc.archive();
        assert_eq!(archive.shape_ids(), vec![ShapeId(1)]);
        archive
            .remap_shapes(&HashMap::from([(ShapeId(1), ShapeId(77))]))
            .unwrap();
        let mut restored = Document::from_archive(archive);
        assert_eq!(restored.body(id).unwrap().shape, Some(ShapeId(77)));
        assert_eq!(restored.body(id).unwrap().name, "Renamed");
        assert!(restored.undo());
        assert_eq!(restored.body(id).unwrap().name, "Body 1");
        assert_eq!(restored.body(id).unwrap().shape, Some(ShapeId(77)));
        assert!(restored.redo());
        assert_eq!(restored.body(id).unwrap().name, "Renamed");
    }

    #[test]
    fn history_window_bounds_shape_references() {
        let mut doc = Document::new("bounded-history");
        let id = doc.alloc_id();
        doc.add_body(sample_body(id));

        for shape in 2..=(Document::MAX_UNDO_STEPS as u64 + 21) {
            doc.begin_change();
            doc.body_mut(id).unwrap().shape = Some(ShapeId(shape));
        }

        assert_eq!(doc.undo_stack.len(), Document::MAX_UNDO_STEPS);
        let live = doc.referenced_shape_ids();
        assert!(
            !live.contains(&ShapeId(1)),
            "expired history must not pin shapes"
        );
        assert!(
            live.contains(&ShapeId(22)),
            "oldest retained snapshot is live"
        );
        assert!(live.contains(&ShapeId(Document::MAX_UNDO_STEPS as u64 + 21)));
    }
}
