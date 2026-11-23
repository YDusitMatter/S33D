//! Faceted B-rep STEP 写入器（ISO 10303-21 / AP214）。
//!
//! 移植自 Super3D 原型 `js/step.js` 的 `meshesToStepText`：每个三角形
//! 输出一个平面 ADVANCED_FACE，顶点焊接。坐标转换 Y-up → Z-up：
//! `(x, y, z) -> (x, -z, y)`（一次旋转，绕序保持）。

use crate::mesh::MeshData;
use std::collections::HashMap;

fn esc_step(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "''")
}

fn fmt_num(x: f64) -> String {
    if !x.is_finite() {
        return "0".to_string();
    }
    format!("{}", (x * 1e7).round() / 1e7)
}

/// 三角形帧：法线与参考方向（与 step.js `triFrame` 一致）。
fn tri_frame(pos: &[[f32; 3]], i0: usize, i1: usize, i2: usize) -> Option<([f64; 3], [f64; 3])> {
    let a = pos[i0];
    let b = pos[i1];
    let c = pos[i2];
    let u = [
        b[0] as f64 - a[0] as f64,
        b[1] as f64 - a[1] as f64,
        b[2] as f64 - a[2] as f64,
    ];
    let w = [
        c[0] as f64 - a[0] as f64,
        c[1] as f64 - a[1] as f64,
        c[2] as f64 - a[2] as f64,
    ];
    let mut n = [
        u[1] * w[2] - u[2] * w[1],
        u[2] * w[0] - u[0] * w[2],
        u[0] * w[1] - u[1] * w[0],
    ];
    let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if nl < 1e-12 {
        return None;
    }
    for v in &mut n {
        *v /= nl;
    }
    let ul = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt().max(1.0);
    let r = [u[0] / ul, u[1] / ul, u[2] / ul];
    Some((n, r))
}

struct StepWriter {
    out: Vec<String>,
    next_id: usize,
}

impl StepWriter {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            next_id: 1,
        }
    }

    fn r(&self, id: usize) -> String {
        format!("#{id}")
    }

    fn emit(&mut self, text: String) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.out.push(format!("#{id}={text};"));
        id
    }

    /// 写一个 body 的完整产品链（含 SHAPE_DEFINITION_REPRESENTATION）。
    fn write_body(&mut self, name: &str, mesh: &MeshData, app_ctx: usize, geo_ctx: usize) {
        let name = esc_step(name);
        let pos = &mesh.positions;
        let tri_count = mesh.indices.len() / 3;
        if tri_count == 0 {
            return;
        }

        // 1. 焊接顶点（Y-up → Z-up）
        let mut point_ids: HashMap<(String, String, String), usize> = HashMap::new();
        let mut point_id = |w: &mut StepWriter, i: usize| -> usize {
            let p = pos[i];
            let x = fmt_num(p[0] as f64);
            let y = fmt_num(-(p[2] as f64));
            let z = fmt_num(p[1] as f64);
            let key = (x.clone(), y.clone(), z.clone());
            if let Some(&id) = point_ids.get(&key) {
                return id;
            }
            let id = w.emit(format!("CARTESIAN_POINT('',({x},{y},{z}))"));
            point_ids.insert(key, id);
            id
        };

        // 2. 每个三角形 → 一个平面 face
        let mut face_ids: Vec<usize> = Vec::new();
        for t in 0..tri_count {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;
            let a = point_id(self, i0);
            let b = point_id(self, i1);
            let c = point_id(self, i2);
            if a == b || b == c || a == c {
                continue;
            }
            let Some((n, rr)) = tri_frame(pos, i0, i1, i2) else {
                continue;
            };
            let p0 = pos[i0];
            let px = fmt_num(p0[0] as f64);
            let py = fmt_num(-(p0[2] as f64));
            let pz = fmt_num(p0[1] as f64);
            let p = self.emit(format!("CARTESIAN_POINT('',({px},{py},{pz}))"));
            let zup_n = [n[0], -n[2], n[1]];
            let zup_r = [rr[0], -rr[2], rr[1]];
            let dn = self.emit(format!(
                "DIRECTION('',({},{},{}))",
                fmt_num(zup_n[0]),
                fmt_num(zup_n[1]),
                fmt_num(zup_n[2])
            ));
            let dr = self.emit(format!(
                "DIRECTION('',({},{},{}))",
                fmt_num(zup_r[0]),
                fmt_num(zup_r[1]),
                fmt_num(zup_r[2])
            ));
            let axis = self.emit(format!(
                "AXIS2_PLACEMENT_3D('',{},{},{})",
                self.r(p),
                self.r(dn),
                self.r(dr)
            ));
            let plane = self.emit(format!("PLANE('',{})", self.r(axis)));
            let loop_ = self.emit(format!(
                "POLY_LOOP('',({},{},{},{}))",
                self.r(a),
                self.r(b),
                self.r(c),
                self.r(a)
            ));
            let bound = self.emit(format!("FACE_OUTER_BOUND('',{},.T.)", self.r(loop_)));
            let f = self.emit(format!(
                "ADVANCED_FACE('',({}),{},.T.)",
                self.r(bound),
                self.r(plane)
            ));
            face_ids.push(f);
        }
        if face_ids.is_empty() {
            return;
        }

        // 3. B-rep + 产品定义链（与 step.js 一致）
        let shell = self.emit(format!(
            "CLOSED_SHELL('',({}))",
            face_ids
                .iter()
                .map(|&i| self.r(i))
                .collect::<Vec<_>>()
                .join(",")
        ));
        let brep = self.emit(format!("MANIFOLD_SOLID_BREP('{name}',{})", self.r(shell)));
        let prod_ctx = self.emit(format!(
            "PRODUCT_CONTEXT('',{},'mechanical')",
            self.r(app_ctx)
        ));
        let prod = self.emit(format!(
            "PRODUCT('{name}','{name}','',({}))",
            self.r(prod_ctx)
        ));
        let pdf = self.emit(format!(
            "PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE('1','',{},.NOT_KNOWN.)",
            self.r(prod)
        ));
        let pd_ctx = self.emit(format!(
            "PRODUCT_DEFINITION_CONTEXT('part definition',{},'design')",
            self.r(app_ctx)
        ));
        let pd = self.emit(format!(
            "PRODUCT_DEFINITION('design','',{},{})",
            self.r(pdf),
            self.r(pd_ctx)
        ));
        let pds = self.emit(format!("PRODUCT_DEFINITION_SHAPE('','',{})", self.r(pd)));
        let sr = self.emit(format!(
            "ADVANCED_BREP_SHAPE_REPRESENTATION('{name}',({}),{})",
            self.r(brep),
            self.r(geo_ctx)
        ));
        self.emit(format!(
            "SHAPE_DEFINITION_REPRESENTATION({},{})",
            self.r(pds),
            self.r(sr)
        ));
    }
}

/// 把一个或多个 body 写成 STEP 文本。`items` 为 (名称, 网格)。
pub fn meshes_to_step_text(items: &[(String, &MeshData)], model_name: &str) -> String {
    let mut w = StepWriter::new();

    // 上下文
    let app_ctx = w.emit("APPLICATION_CONTEXT('automotive design')".into());
    w.emit(format!(
        "APPLICATION_PROTOCOL_DEFINITION('INTERNATIONAL STANDARD','automotive_design',2002,{})",
        w.r(app_ctx)
    ));
    let len_unit = w.emit("( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) )".into());
    let ang_unit = w.emit("( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) )".into());
    let sol_unit = w.emit("( NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT() )".into());
    let unc = w.emit(format!(
        "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-05),{},'distance_accuracy_value','confusion accuracy')",
        w.r(len_unit)
    ));
    let geo_ctx = w.emit(format!(
        "( GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT(({})) GLOBAL_UNIT_ASSIGNED_CONTEXT(({},{},{})) REPRESENTATION_CONTEXT('S33D','3D Context') )",
        w.r(unc),
        w.r(len_unit),
        w.r(ang_unit),
        w.r(sol_unit)
    ));

    // bodies
    for (name, mesh) in items {
        w.write_body(name, mesh, app_ctx, geo_ctx);
    }

    let mut text = String::new();
    text.push_str("ISO-10303-21;\n");
    text.push_str("HEADER;\n");
    text.push_str("FILE_DESCRIPTION(('S33D faceted B-rep'),'2;1');\n");
    text.push_str(&format!(
        "FILE_NAME('{}','',('S33D'),(''),'S33D','S33D','');\n",
        esc_step(model_name)
    ));
    text.push_str("FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 3 1 1 }'));\n");
    text.push_str("ENDSEC;\n");
    text.push_str("DATA;\n");
    for line in &w.out {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("ENDSEC;\n");
    text.push_str("END-ISO-10303-21;\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetra_mesh() -> MeshData {
        // 一个四面体（4 面）
        let v: [[f32; 3]; 4] = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let t: [[u32; 3]; 4] = [[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]];
        MeshData::new(v.to_vec(), t.iter().flatten().copied().collect())
    }

    #[test]
    fn writes_valid_step_header() {
        let m = tetra_mesh();
        let text = meshes_to_step_text(&[("T".to_string(), &m)], "test");
        assert!(text.starts_with("ISO-10303-21;"));
        assert!(text.contains("FILE_SCHEMA(('AUTOMOTIVE_DESIGN"));
        assert!(text.contains("MANIFOLD_SOLID_BREP('T'"));
        assert!(text.contains("END-ISO-10303-21;"));
        // 4 个三角形 → 4 个 ADVANCED_FACE
        assert_eq!(text.matches("ADVANCED_FACE").count(), 4);
    }
}
