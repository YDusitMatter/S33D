//! Owned, immutable exact geometry. Native exceptions are caught in the C++
//! bridge; only copied data crosses FFI. The B-Rep, not its display mesh, is
//! the persistent modeling source of truth.
use crate::{KernelError, MeshData, Result};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{c_char, c_void, CStr, CString},
    ptr::NonNull,
    sync::Mutex,
};

static NATIVE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OffsetMode {
    SurfaceOffset,
    HollowSolid,
    ThickenSurface,
    SymmetricThickenSurface,
}

#[derive(Debug, Clone)]
pub struct ExactShape {
    pub brep: String,
    pub mesh: MeshData,
    pub triangle_faces: Vec<u32>,
    /// Original surface classification, not a tessellation-based guess.
    pub planar_faces: Vec<bool>,
    pub face_count: u32,
    pub solid_count: u32,
}

extern "C" {
    fn s33d_occt_revolve(
        points: *const f64,
        sizes: *const u32,
        loops: usize,
        origin: *const f64,
        direction: *const f64,
        angle: f64,
    ) -> *mut c_void;
    fn s33d_occt_surface(points: *const f64, nu: usize, nv: usize, fill: bool) -> *mut c_void;
    fn s33d_occt_read(brep: *const c_char) -> *mut c_void;
    fn s33d_occt_cylinders(brep: *const c_char) -> *mut c_void;
    fn s33d_occt_mesh(
        positions: *const f64,
        vertices: usize,
        indices: *const u32,
        count: usize,
    ) -> *mut c_void;
    fn s33d_occt_import(text: *const c_char, separate: bool, normalize: bool) -> *mut c_void;
    fn s33d_occt_mesh_step(
        positions: *const f64,
        vertices: usize,
        indices: *const u32,
        count: usize,
    ) -> *mut c_void;
    fn s33d_occt_export(brep: *const c_char, z_up: bool) -> *mut c_void;
    fn s33d_occt_import_iges(path: *const c_char, normalize: bool) -> *mut c_void;
    fn s33d_occt_export_iges(brep: *const c_char, unit: *const c_char) -> *mut c_void;
    fn s33d_occt_split(brep: *const c_char, origin: *const f64, normal: *const f64) -> *mut c_void;
    fn s33d_occt_transform(brep: *const c_char, matrix: *const f64) -> *mut c_void;
    fn s33d_occt_offset(
        brep: *const c_char,
        distance: f64,
        tolerance: f64,
        faces: *const u32,
        count: usize,
        mode: i32,
    ) -> *mut c_void;
    fn s33d_occt_boolean(a: *const c_char, b: *const c_char, mode: i32) -> *mut c_void;
    fn s33d_occt_boolean_refined(a: *const c_char, b: *const c_char, mode: i32) -> *mut c_void;
    fn s33d_occt_extrude_face_boolean(
        brep: *const c_char,
        face: u32,
        normal: *const f64,
        depth: f64,
        start: f64,
        cut: bool,
    ) -> *mut c_void;
    fn s33d_occt_extrude_face(
        brep: *const c_char,
        face: u32,
        normal: *const f64,
        depth: f64,
        start: f64,
    ) -> *mut c_void;
    #[cfg(test)]
    fn s33d_occt_fixture(kind: i32) -> *mut c_void;
    fn s33d_occt_free(result: *mut c_void);
    fn s33d_occt_error(result: *const c_void) -> *const c_char;
    fn s33d_occt_text(result: *const c_void) -> *const c_char;
    fn s33d_occt_count(result: *const c_void) -> usize;
    fn s33d_occt_brep(result: *const c_void, i: usize) -> *const c_char;
    fn s33d_occt_vertices(result: *const c_void, i: usize) -> usize;
    fn s33d_occt_triangles(result: *const c_void, i: usize) -> usize;
    fn s33d_occt_positions(result: *const c_void, i: usize) -> *const f32;
    fn s33d_occt_normals(result: *const c_void, i: usize) -> *const f32;
    fn s33d_occt_indices(result: *const c_void, i: usize) -> *const u32;
    fn s33d_occt_faces(result: *const c_void, i: usize) -> *const u32;
    fn s33d_occt_planar_faces(result: *const c_void, i: usize) -> *const u8;
    fn s33d_occt_face_count(result: *const c_void, i: usize) -> u32;
    fn s33d_occt_solid_count(result: *const c_void, i: usize) -> u32;
}

fn error(message: impl Into<String>) -> KernelError {
    KernelError::Geometry(message.into())
}
fn cstring(text: &str) -> Result<CString> {
    CString::new(text).map_err(|_| error("Geometry text contains a NUL byte"))
}
struct NativeResult(NonNull<c_void>);
impl Drop for NativeResult {
    fn drop(&mut self) {
        unsafe { s33d_occt_free(self.0.as_ptr()) }
    }
}
impl NativeResult {
    // Only native functions declared above construct this owned result.
    unsafe fn take(pointer: *mut c_void) -> Result<Self> {
        let result =
            Self(NonNull::new(pointer).ok_or_else(|| error("Native geometry allocation failed"))?);
        let message = CStr::from_ptr(s33d_occt_error(pointer)).to_string_lossy();
        if !message.is_empty() {
            return Err(error(message.into_owned()));
        }
        Ok(result)
    }
    fn shapes(&self) -> Result<Vec<ExactShape>> {
        let r = self.0.as_ptr();
        let mut shapes = Vec::new();
        // Native buffers remain owned by self until all slices have been copied.
        unsafe {
            for i in 0..s33d_occt_count(r) {
                let vertices = s33d_occt_vertices(r, i);
                let triangles = s33d_occt_triangles(r, i);
                if vertices == 0
                    || triangles == 0
                    || vertices > 10_000_000
                    || triangles > 20_000_000
                {
                    return Err(error("Invalid or oversized display mesh"));
                }
                let positions = std::slice::from_raw_parts(s33d_occt_positions(r, i), vertices * 3)
                    .chunks_exact(3)
                    .map(|p| [p[0], p[1], p[2]])
                    .collect::<Vec<_>>();
                let indices =
                    std::slice::from_raw_parts(s33d_occt_indices(r, i), triangles * 3).to_vec();
                if positions.iter().flatten().any(|v| !v.is_finite())
                    || indices.iter().any(|v| *v as usize >= vertices)
                {
                    return Err(error("Invalid tessellation coordinates"));
                }
                let mut mesh = MeshData::new(positions, indices);
                let fallback = mesh.vertex_normals();
                mesh.vertex_normals = Some(
                    std::slice::from_raw_parts(s33d_occt_normals(r, i), vertices * 3)
                        .chunks_exact(3)
                        .zip(fallback)
                        .map(|(n, fallback)| {
                            let normal = glam::Vec3::new(n[0], n[1], n[2]);
                            if normal.is_finite() && normal.length_squared() > 1e-12 {
                                normal.normalize().to_array()
                            } else {
                                fallback
                            }
                        })
                        .collect(),
                );
                let triangle_faces =
                    std::slice::from_raw_parts(s33d_occt_faces(r, i), triangles).to_vec();
                let face_count = s33d_occt_face_count(r, i);
                if triangle_faces.iter().any(|&face| face >= face_count) {
                    return Err(error("Invalid topology-face map"));
                }
                shapes.push(ExactShape {
                    brep: CStr::from_ptr(s33d_occt_brep(r, i))
                        .to_str()
                        .map_err(|_| error("Invalid B-Rep encoding"))?
                        .to_owned(),
                    mesh,
                    triangle_faces,
                    planar_faces: std::slice::from_raw_parts(
                        s33d_occt_planar_faces(r, i),
                        face_count as usize,
                    )
                    .iter()
                    .map(|&value| value != 0)
                    .collect(),
                    face_count,
                    solid_count: s33d_occt_solid_count(r, i),
                });
            }
        }
        Ok(shapes)
    }
    fn one(&self) -> Result<ExactShape> {
        let mut shapes = self.shapes()?;
        if shapes.len() != 1 {
            return Err(error("Expected exactly one native result"));
        }
        Ok(shapes.remove(0))
    }
}

impl ExactShape {
    /// Representative face id, origin XYZ, axis XYZ, radius, axial length,
    /// internal flag. Recognizes spline-encoded cylinders and joins connected
    /// coaxial faces only when they form a complete wall between two end rings.
    pub fn cylinders(&self) -> Result<Vec<[f64; 10]>> {
        let text = cstring(&self.brep)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        let result = unsafe { NativeResult::take(s33d_occt_cylinders(text.as_ptr()))? };
        let value = unsafe { CStr::from_ptr(s33d_occt_text(result.0.as_ptr())) }.to_string_lossy();
        value
            .lines()
            .map(|line| {
                let values: Vec<f64> = line
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<std::result::Result<_, _>>()
                    .map_err(|_| error("Invalid cylinder query response"))?;
                values
                    .try_into()
                    .map_err(|_| error("Invalid cylinder query length"))
            })
            .collect()
    }
    pub(crate) fn revolve(
        rings: &[Vec<[f64; 3]>],
        origin: [f64; 3],
        direction: [f64; 3],
        angle: f64,
    ) -> Result<Self> {
        if rings.is_empty()
            || rings.len() > 1024
            || rings.iter().any(|r| {
                r.len() < 3 || r.len() > 100000 || r.iter().flatten().any(|x| !x.is_finite())
            })
            || origin
                .iter()
                .chain(direction.iter())
                .any(|x| !x.is_finite())
            || glam::DVec3::from_array(direction).length_squared() < 1e-16
            || !angle.is_finite()
            || angle.abs() < 1e-9
        {
            return Err(error("Invalid revolve parameters"));
        }
        let points: Vec<f64> = rings.iter().flatten().flatten().copied().collect();
        let sizes: Vec<u32> = rings.iter().map(|r| r.len() as u32).collect();
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe {
            NativeResult::take(s33d_occt_revolve(
                points.as_ptr(),
                sizes.as_ptr(),
                sizes.len(),
                origin.as_ptr(),
                direction.as_ptr(),
                angle,
            ))?
            .one()
        }
    }
    /// Interpolate bounded equation samples as a smooth B-spline B-Rep.
    /// This approximates the equation between samples; it is not symbolic CAD.
    pub fn from_surface_grid(
        points: &[[f64; 3]],
        nu: usize,
        nv: usize,
        fill: bool,
    ) -> Result<Self> {
        if !(4..=65).contains(&nu)
            || !(4..=65).contains(&nv)
            || points.len() != nu * nv
            || points
                .iter()
                .flatten()
                .any(|p| !p.is_finite() || p.abs() > 1e7)
        {
            return Err(error("Invalid surface sample grid (4..65 points per axis)"));
        }
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe {
            NativeResult::take(s33d_occt_surface(points.as_ptr().cast(), nu, nv, fill))?.one()
        }
    }
    /// Preserve a closed polygonal tool as planar B-Rep faces, not a recovered
    /// smooth design surface. Used only when combining legacy tools with B-Rep.
    pub fn from_faceted_mesh(mesh: &MeshData) -> Result<Self> {
        let mesh = crate::faceted::prepare(mesh)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe {
            NativeResult::take(s33d_occt_mesh(
                mesh.positions.as_ptr().cast(),
                mesh.positions.len(),
                mesh.indices.as_ptr(),
                mesh.indices.len(),
            ))?
            .one()
        }
    }
    pub fn from_brep(text: &str) -> Result<Self> {
        let text = cstring(text)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe { NativeResult::take(s33d_occt_read(text.as_ptr()))?.one() }
    }
    pub fn import_step(text: &str) -> Result<Vec<Self>> {
        Self::import_step_placed(text, false)
    }
    /// UI/Agent file imports share the same atomic assembly placement.
    pub fn import_step_normalized(text: &str) -> Result<Vec<Self>> {
        Self::import_step_placed(text, true)
    }
    fn import_step_placed(text: &str, normalize: bool) -> Result<Vec<Self>> {
        let text = cstring(text)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe { NativeResult::take(s33d_occt_import(text.as_ptr(), true, normalize))?.shapes() }
    }
    pub(crate) fn import_step_compound(text: &str) -> Result<Self> {
        let text = cstring(text)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe { NativeResult::take(s33d_occt_import(text.as_ptr(), false, false))?.one() }
    }
    /// Export a faceted solid directly, without re-tessellating every face twice.
    pub(crate) fn export_mesh_step(mesh: &MeshData) -> Result<String> {
        let mesh = crate::faceted::prepare(mesh)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe {
            let result = NativeResult::take(s33d_occt_mesh_step(
                mesh.positions.as_ptr().cast(),
                mesh.positions.len(),
                mesh.indices.as_ptr(),
                mesh.indices.len(),
            ))?;
            Ok(CStr::from_ptr(s33d_occt_text(result.0.as_ptr()))
                .to_string_lossy()
                .into_owned())
        }
    }
    pub fn export_step(&self) -> Result<String> {
        self.export_step_placed(false)
    }
    pub(crate) fn export_step_z_up(&self) -> Result<String> {
        self.export_step_placed(true)
    }
    fn export_step_placed(&self, z_up: bool) -> Result<String> {
        let text = cstring(&self.brep)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe {
            let result = NativeResult::take(s33d_occt_export(text.as_ptr(), z_up))?;
            Ok(CStr::from_ptr(s33d_occt_text(result.0.as_ptr()))
                .to_string_lossy()
                .into_owned())
        }
    }
    /// OCCT's IGES reader accepts a UTF-8 filename (including Windows paths).
    /// Lengths are transferred in millimetres, independent of display units.
    pub fn import_iges(path: &str, normalize: bool) -> Result<Vec<Self>> {
        let path = cstring(path)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe { NativeResult::take(s33d_occt_import_iges(path.as_ptr(), normalize))?.shapes() }
    }
    pub fn export_iges(&self) -> Result<String> {
        self.export_iges_unit("MM")
    }
    fn export_iges_unit(&self, unit: &str) -> Result<String> {
        let text = cstring(&self.brep)?;
        let unit = cstring(unit)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe {
            let result = NativeResult::take(s33d_occt_export_iges(text.as_ptr(), unit.as_ptr()))?;
            Ok(CStr::from_ptr(s33d_occt_text(result.0.as_ptr()))
                .to_string_lossy()
                .into_owned())
        }
    }
    pub fn split(&self, origin: [f64; 3], normal: [f64; 3]) -> Result<Vec<Self>> {
        if origin.iter().chain(normal.iter()).any(|v| !v.is_finite())
            || glam::DVec3::from_array(normal).length() < 1e-9
        {
            return Err(error("Invalid split plane"));
        }
        let text = cstring(&self.brep)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe {
            NativeResult::take(s33d_occt_split(
                text.as_ptr(),
                origin.as_ptr(),
                normal.as_ptr(),
            ))?
            .shapes()
        }
    }
    pub fn transform(&self, matrix: &glam::Mat4) -> Result<Self> {
        if !matrix.is_finite()
            || matrix.determinant().abs() < 1e-12
            || matrix.x_axis.w != 0.0
            || matrix.y_axis.w != 0.0
            || matrix.z_axis.w != 0.0
            || matrix.w_axis.w != 1.0
        {
            return Err(error("Expected a finite invertible affine transform"));
        }
        let text = cstring(&self.brep)?;
        let m = matrix.to_cols_array().map(f64::from);
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe { NativeResult::take(s33d_occt_transform(text.as_ptr(), m.as_ptr()))?.one() }
    }
    pub fn offset(
        &self,
        distance: f64,
        tolerance: f64,
        faces: &[u32],
        mode: OffsetMode,
    ) -> Result<Self> {
        let text = cstring(&self.brep)?;
        let mut faces = faces.to_vec();
        faces.sort_unstable();
        faces.dedup();
        let mode = match mode {
            OffsetMode::SurfaceOffset => 0,
            OffsetMode::HollowSolid => 1,
            OffsetMode::ThickenSurface => 2,
            OffsetMode::SymmetricThickenSurface => 3,
        };
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe {
            NativeResult::take(s33d_occt_offset(
                text.as_ptr(),
                distance,
                tolerance,
                faces.as_ptr(),
                faces.len(),
                mode,
            ))?
            .one()
        }
    }
    /// Sweep the original face wires, including analytic arcs and inner loops.
    /// Display-mesh chords must not replace the boundary of an exact surface.
    pub fn extrude_planar_face(
        &self,
        face: u32,
        normal: [f64; 3],
        depth: f64,
        symmetric: bool,
    ) -> Result<Self> {
        self.extrude_planar_face_range(
            face,
            normal,
            depth,
            if symmetric { -depth / 2.0 } else { 0.0 },
        )
    }
    /// Extrude an original trimmed face over an explicit signed axial range.
    /// Used by Pocket to overrun the surface without faceting its boundaries.
    pub fn extrude_planar_face_range(
        &self,
        face: u32,
        normal: [f64; 3],
        depth: f64,
        start: f64,
    ) -> Result<Self> {
        if face >= self.face_count
            || !depth.is_finite()
            || !start.is_finite()
            || depth.abs() < 1e-9
            || normal.iter().any(|v| !v.is_finite())
            || glam::DVec3::from_array(normal).length() < 1e-9
        {
            return Err(error("Invalid exact face extrusion parameters"));
        }
        let text = cstring(&self.brep)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe {
            NativeResult::take(s33d_occt_extrude_face(
                text.as_ptr(),
                face,
                normal.as_ptr(),
                depth,
                start,
            ))?
            .one()
        }
    }

    /// One native transaction; the intermediate validated tool has no display mesh.
    pub fn extrude_planar_face_boolean(
        &self,
        face: u32,
        normal: [f64; 3],
        depth: f64,
        start: f64,
        cut: bool,
    ) -> Result<Self> {
        if face >= self.face_count
            || !depth.is_finite()
            || !start.is_finite()
            || depth.abs() < 1e-9
            || normal.iter().any(|v| !v.is_finite())
            || glam::DVec3::from_array(normal).length() < 1e-9
        {
            return Err(error("Invalid exact face extrusion parameters"));
        }
        let text = cstring(&self.brep)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe {
            NativeResult::take(s33d_occt_extrude_face_boolean(
                text.as_ptr(),
                face,
                normal.as_ptr(),
                depth,
                start,
                cut,
            ))?
            .one()
        }
    }
    pub fn boolean(&self, other: &Self, mode: i32) -> Result<Self> {
        let a = cstring(&self.brep)?;
        let b = cstring(&other.brep)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe { NativeResult::take(s33d_occt_boolean(a.as_ptr(), b.as_ptr(), mode))?.one() }
    }
    /// Unify tangent/coplanar Boolean faces and validate the persisted B-rep.
    /// Used for local edits of certified faceted walls; other Booleans retain
    /// their existing face partitioning.
    pub fn boolean_refined(&self, other: &Self, mode: i32) -> Result<Self> {
        if !(0..=2).contains(&mode) {
            return Err(error("Invalid Boolean mode"));
        }
        let a = cstring(&self.brep)?;
        let b = cstring(&other.brep)?;
        let _lock = NATIVE_LOCK
            .lock()
            .map_err(|_| error("Native geometry lock poisoned"))?;
        unsafe {
            NativeResult::take(s33d_occt_boolean_refined(a.as_ptr(), b.as_ptr(), mode))?.one()
        }
    }
    #[cfg(test)]
    pub(crate) fn fixture(kind: i32) -> Self {
        let _lock = NATIVE_LOCK.lock().unwrap();
        unsafe {
            NativeResult::take(s33d_occt_fixture(kind))
                .unwrap()
                .one()
                .unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn iges_import_preserves_brep_units_and_unicode_paths() {
        let source = ExactShape::fixture(0);
        let path = std::env::temp_dir().join(format!("s33d-IGES-中文-{}.IGS", std::process::id()));
        std::fs::write(&path, source.export_iges().unwrap()).unwrap();
        let raw = ExactShape::import_iges(path.to_str().unwrap(), false).unwrap();
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].solid_count, 1);
        assert_eq!(raw[0].face_count, 6);
        assert_eq!(raw[0].mesh.bounds(), source.mesh.bounds());
        std::fs::write(&path, source.export_iges_unit("INCH").unwrap()).unwrap();
        let inches = ExactShape::import_iges(path.to_str().unwrap(), false).unwrap();
        let (min, max) = inches[0].mesh.bounds().unwrap();
        let (expected_min, expected_max) = source.mesh.bounds().unwrap();
        assert!((min - expected_min).length() < 1e-4);
        assert!((max - expected_max).length() < 1e-4);
        let placed = ExactShape::import_iges(path.to_str().unwrap(), true).unwrap();
        let (min, max) = placed[0].mesh.bounds().unwrap();
        assert!((min - glam::Vec3::new(-10.0, 0.0, -15.0)).length() < 1e-4);
        assert!((max - glam::Vec3::new(10.0, 40.0, 15.0)).length() < 1e-4);
        let second = source
            .transform(&glam::Mat4::from_translation(glam::vec3(50., 0., 0.)))
            .unwrap();
        let assembly = source.boolean(&second, 0).unwrap();
        std::fs::write(&path, assembly.export_iges().unwrap()).unwrap();
        let parts = ExactShape::import_iges(path.to_str().unwrap(), true).unwrap();
        assert_eq!(parts.len(), 2);
        let centers: Vec<_> = parts
            .iter()
            .map(|s| {
                let (min, max) = s.mesh.bounds().unwrap();
                (min.x + max.x) * 0.5
            })
            .collect();
        assert!(((centers[0] - centers[1]).abs() - 50.0).abs() < 1e-4);
        assert!((centers[0] + centers[1]).abs() < 1e-4);
        std::fs::write(&path, ExactShape::fixture(2).export_iges().unwrap()).unwrap();
        let surface = ExactShape::import_iges(path.to_str().unwrap(), true).unwrap();
        assert_eq!(surface.len(), 1);
        assert_eq!(surface[0].solid_count, 0);
        std::fs::write(&path, "not an IGES file").unwrap();
        assert!(ExactShape::import_iges(path.to_str().unwrap(), true).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    #[ignore = "Requires a user-supplied STEP fixture path in S33D_CAD_FIXTURE"]
    fn solidworks_export_fixture_imports_exact_geometry() {
        let path = std::env::var("S33D_CAD_FIXTURE").expect("Set S33D_CAD_FIXTURE");
        let text = std::fs::read_to_string(&path).unwrap();
        let shapes = ExactShape::import_step_normalized(&text).unwrap();
        assert!(!shapes.is_empty());
        for shape in &shapes {
            assert!(shape.face_count > 0 && !shape.mesh.indices.is_empty());
            assert!(!shape.brep.is_empty());
            assert!(shape.mesh.bounds().is_some());
            ExactShape::from_brep(&shape.brep).unwrap();
        }
        eprintln!(
            "Verified {}: {} exact object(s), {} solids",
            path,
            shapes.len(),
            shapes.iter().map(|s| s.solid_count).sum::<u32>()
        );
    }
    #[test]
    fn thread_cylinders_recognize_spline_screw_and_split_tube_but_not_open_arc() {
        assert!(ExactShape::fixture(1).cylinders().unwrap().is_empty());
        assert!(ExactShape::fixture(2).cylinders().unwrap().is_empty());
        for (kind, expected) in [(3, 2), (4, 2), (5, 0)] {
            let shape = ExactShape::fixture(kind);
            let restored = ExactShape::import_step(&shape.export_step().unwrap())
                .unwrap()
                .remove(0);
            assert!(restored.export_step().unwrap().contains("B_SPLINE_SURFACE"));
            let walls = restored.cylinders().unwrap();
            assert_eq!(walls.len(), expected, "fixture {kind}: {walls:?}");
            if kind == 3 {
                assert!(walls.iter().all(|c| c[9] == 0.));
                assert!(walls
                    .iter()
                    .any(|c| (c[7] - 1.5).abs() < 1e-4 && (c[8] - 10.).abs() < 1e-4));
                assert!(walls
                    .iter()
                    .any(|c| (c[7] - 2.8).abs() < 1e-4 && (c[8] - 2.).abs() < 1e-4));
            } else if kind == 4 {
                assert_eq!(walls.iter().filter(|c| c[9] != 0.).count(), 1);
                assert!(walls.iter().all(|c| (c[8] - 10.).abs() < 1e-4));
            }
        }
    }
    #[test]
    fn thread_cylinders_survive_step_roundtrip_and_transform() {
        let ring = vec![[2., 0., 0.], [4., 0., 0.], [4., 0., 10.], [2., 0., 10.]];
        let tube =
            ExactShape::revolve(&[ring], [0.; 3], [0., 0., 1.], std::f64::consts::TAU).unwrap();
        let moved = tube
            .transform(&glam::Mat4::from_rotation_translation(
                glam::Quat::from_rotation_x(0.7),
                glam::Vec3::new(5., 12., 3.),
            ))
            .unwrap();
        let restored = ExactShape::import_step(&moved.export_step().unwrap())
            .unwrap()
            .remove(0);
        let cylinders = restored.cylinders().unwrap();
        assert_eq!(cylinders.len(), 2);
        assert_eq!(cylinders.iter().filter(|c| c[9] != 0.).count(), 1);
        for c in cylinders {
            assert!((c[8] - 10.).abs() < 1e-6);
            assert!((c[5].abs() - 0.7_f64.sin()).abs() < 1e-6);
            assert!((c[7] - if c[9] != 0. { 2. } else { 4. }).abs() < 1e-6);
        }
    }
    #[test]
    fn fused_face_commit_matches_separate_prism_and_boolean() {
        let source = ExactShape::fixture(0);
        let original = source.brep.clone();
        let t = &source.mesh.indices[..3];
        let p = |i: u32| glam::DVec3::from_array(source.mesh.positions[i as usize].map(f64::from));
        let normal = (p(t[1]) - p(t[0]))
            .cross(p(t[2]) - p(t[0]))
            .normalize()
            .to_array();
        let face = source.triangle_faces[0];
        for (depth, start, cut) in [(2.0, 0.0, false), (-4.0, 1.0, true)] {
            let tool = source
                .extrude_planar_face_range(face, normal, depth, start)
                .unwrap();
            let reference = source.boolean(&tool, if cut { 1 } else { 0 }).unwrap();
            let fused = source
                .extrude_planar_face_boolean(face, normal, depth, start, cut)
                .unwrap();
            assert_eq!(fused.solid_count, 1);
            assert_eq!(fused.face_count, reference.face_count);
            assert_eq!(fused.mesh.bounds(), reference.mesh.bounds());
            let restored = ExactShape::from_brep(&fused.brep).unwrap();
            assert_eq!(fused.triangle_faces, restored.triangle_faces);
        }
        assert!(source
            .extrude_planar_face_boolean(source.face_count, normal, 2.0, 0.0, false)
            .is_err());
        assert!(source
            .extrude_planar_face_boolean(face, normal, f64::NAN, 0.0, false)
            .is_err());
        assert_eq!(source.brep, original);
    }

    #[test]
    fn normalized_step_preserves_topology_and_assembly_bounds() {
        let source = ExactShape::fixture(0);
        let step = source.export_step().unwrap();
        let imported = ExactShape::import_step_normalized(&step).unwrap();
        assert_eq!(imported.len(), 1);
        let shape = &imported[0];
        assert_eq!(shape.face_count, 6);
        let (min, max) = shape.mesh.bounds().unwrap();
        assert!((min - glam::Vec3::new(-10.0, 0.0, -15.0)).length() < 1e-4);
        assert!((max - glam::Vec3::new(10.0, 40.0, 15.0)).length() < 1e-4);
        let restored = ExactShape::from_brep(&shape.brep).unwrap();
        assert_eq!(shape.triangle_faces, restored.triangle_faces);
    }

    #[test]
    fn native_face_pocket_preserves_curves_and_positive_cutter_orientation() {
        let shape = ExactShape::fixture(0);
        let t = &shape.mesh.indices[0..3];
        let p = |i: u32| glam::DVec3::from_array(shape.mesh.positions[i as usize].map(f64::from));
        let normal = (p(t[1]) - p(t[0]))
            .cross(p(t[2]) - p(t[0]))
            .normalize()
            .to_array();
        let face = shape.triangle_faces[0];
        let cutter = shape
            .extrude_planar_face_range(face, normal, -4.0, 1.0)
            .unwrap();
        assert_eq!(cutter.solid_count, 1);
        let result = shape.boolean(&cutter, 1).unwrap();
        assert_eq!(result.solid_count, 1);
        assert_eq!(result.face_count, 6);
        let (min, max) = result.mesh.bounds().unwrap();
        let extent = max - min;
        assert!((extent.x * extent.y * extent.z - 17.0 * 30.0 * 40.0).abs() < 0.1);
        assert!(shape
            .extrude_planar_face_range(face, normal, -1.0, f64::NAN)
            .is_err());
    }

    #[test]
    #[ignore = "Read-only private STEP regression; set S33D_STEP_FIXTURE to run"]
    fn user_step_import_and_planar_operations() {
        let path = std::env::var("S33D_STEP_FIXTURE").expect("S33D_STEP_FIXTURE");
        let fused = std::env::var_os("S33D_STEP_FUSED").is_some();
        eprintln!(
            "face commit path: {}",
            if fused {
                "fused"
            } else {
                "separate prism/boolean"
            }
        );
        let text = std::fs::read_to_string(path).unwrap();
        let started = std::time::Instant::now();
        let shapes = ExactShape::import_step_normalized(&text).unwrap();
        eprintln!(
            "import {:.3}s, {} parts",
            started.elapsed().as_secs_f64(),
            shapes.len()
        );
        for shape in shapes {
            eprintln!(
                "{} faces, {} triangles",
                shape.face_count,
                shape.mesh.indices.len() / 3
            );
            // Exercise each planar face, not only the visually simple outer cap.
            for face in 0..shape.face_count {
                let triangles: Vec<_> = shape
                    .mesh
                    .indices
                    .chunks_exact(3)
                    .zip(&shape.triangle_faces)
                    .filter(|(_, f)| **f == face)
                    .map(|(t, _)| t)
                    .collect();
                let Some(t) = triangles.first() else { continue };
                let p = |i: u32| {
                    glam::DVec3::from_array(shape.mesh.positions[i as usize].map(f64::from))
                };
                let origin = p(t[0]);
                let normal = (p(t[1]) - origin).cross(p(t[2]) - origin).normalize();
                if !normal.is_finite()
                    || triangles
                        .iter()
                        .flat_map(|t| t.iter())
                        .any(|&i| (p(i) - origin).dot(normal).abs() > 1e-4)
                {
                    continue;
                }
                let started = std::time::Instant::now();
                eprintln!("face {face}: start ({} triangles)", triangles.len());
                let extended = if fused {
                    shape
                        .extrude_planar_face_boolean(face, normal.to_array(), 2.0, 0.0, false)
                        .unwrap()
                } else {
                    let prism = shape
                        .extrude_planar_face(face, normal.to_array(), 2.0, false)
                        .unwrap();
                    eprintln!("face {face}: prism {:.3}s", started.elapsed().as_secs_f64());
                    shape.boolean(&prism, 0).unwrap()
                };
                assert!(extended.solid_count > 0);
                eprintln!("face {face}: union {:.3}s", started.elapsed().as_secs_f64());
                let cut = if fused {
                    shape
                        .extrude_planar_face_boolean(face, normal.to_array(), -3.0, 1.0, true)
                        .unwrap()
                } else {
                    let cutter = shape
                        .extrude_planar_face_range(face, normal.to_array(), -3.0, 1.0)
                        .unwrap();
                    shape.boolean(&cutter, 1).unwrap()
                };
                assert!(cut.solid_count > 0);
                eprintln!("face {face}: cut {:.3}s", started.elapsed().as_secs_f64());
            }
        }
    }
    #[test]
    fn mixed_faceted_tool_preserves_exact_operand_surfaces() {
        let sphere = ExactShape::fixture(1);
        let cube = ExactShape::fixture(0);
        let faceted = ExactShape::from_faceted_mesh(&cube.mesh).unwrap();
        let cut = sphere.boolean(&faceted, 1).unwrap();
        assert_eq!(cut.solid_count, 1);
        assert!(cut.export_step().unwrap().contains("SPHERICAL_SURFACE"));
    }
    #[test]
    fn exact_split_and_mirror_keep_curved_surfaces() {
        let sphere = ExactShape::fixture(1);
        let parts = sphere.split([0.0; 3], [1.0, 0.0, 0.0]).unwrap();
        assert_eq!(parts.len(), 2);
        for part in parts {
            assert!(part.export_step().unwrap().contains("SPHERICAL_SURFACE"));
        }
        let mirror = sphere
            .transform(&glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0)))
            .unwrap();
        assert!(mirror.export_step().unwrap().contains("SPHERICAL_SURFACE"));
    }
    #[test]
    fn open_spline_surface_survives_step_and_thickens() {
        let surface = ExactShape::fixture(2);
        assert_eq!(surface.solid_count, 0);
        let step = surface.export_step().unwrap();
        let imported = ExactShape::import_step(&step).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].solid_count, 0);
        let thick = imported[0]
            .offset(0.5, 1e-6, &[], OffsetMode::ThickenSurface)
            .unwrap();
        assert_eq!(thick.solid_count, 1);
        assert!(thick.face_count > surface.face_count);
    }
    #[test]
    fn exact_roundtrip_preserves_faces_and_step_surfaces() {
        let sphere = ExactShape::fixture(1);
        let restored = ExactShape::from_brep(&sphere.brep).unwrap();
        assert_eq!(sphere.face_count, restored.face_count);
        assert_eq!(sphere.triangle_faces, restored.triangle_faces);
        let step = restored.export_step().unwrap();
        assert!(step.contains("SPHERICAL_SURFACE"));
        let imported = ExactShape::import_step(&step).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].solid_count, 1);
    }
    #[test]
    fn exact_offset_and_hollowing_reject_invalid_requests() {
        let cube = ExactShape::fixture(0);
        let hollow = cube
            .offset(-2.0, 1e-6, &[0], OffsetMode::HollowSolid)
            .unwrap();
        assert_eq!(hollow.solid_count, 1);
        assert!(hollow.face_count > cube.face_count);
        assert!(cube
            .offset(-2.0, 1e-6, &[999], OffsetMode::HollowSolid)
            .is_err());
        assert!(cube
            .offset(f64::NAN, 1e-6, &[0], OffsetMode::HollowSolid)
            .is_err());
        assert!(cube
            .offset(2.0, 1e-6, &[], OffsetMode::ThickenSurface)
            .is_err());
        let sphere = ExactShape::fixture(1);
        assert!(sphere
            .offset(1.0, 1e-6, &[], OffsetMode::SurfaceOffset)
            .is_ok());
        assert!(ExactShape::from_brep("not geometry").is_err());
    }
}
