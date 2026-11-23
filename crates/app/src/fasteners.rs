//! Printable fasteners with real helical geometry, not cosmetic thread lines.
use crate::{app::S33DApp, mode::Mode, threads};
use s33d_kernel::{mesh_bool, MeshData};
use s33d_model::{Body, BodyId, BodyKind, FastenerKind as K, FastenerParams, Feature, Material};
use std::f64::consts::{PI, TAU};

pub fn error_text(error: &str, language: crate::app::UiLanguage) -> &str {
    error
        .split_once(" / ")
        .map_or(error, |(en, zh)| match language {
            crate::app::UiLanguage::English => en,
            crate::app::UiLanguage::ChineseSimplified => zh,
        })
}

pub fn validate(p: &FastenerParams) -> Result<(), String> {
    let values = [
        p.diameter,
        p.pitch,
        p.length,
        p.head_size,
        p.head_height,
        p.socket_size,
        p.thread_length,
        p.clearance,
        p.lead_in,
        p.position[0],
        p.position[1],
        p.position[2],
    ];
    if values.iter().chain(p.basis.iter()).any(|v| !v.is_finite())
        || glam::DMat3::from_cols_array(&p.basis).determinant().abs() < 1e-12
    {
        return Err("All dimensions and placement must be finite and nondegenerate / 尺寸和位置必须有限且不可退化".into());
    }
    if !(1.5..=100.).contains(&p.diameter)
        || p.pitch < 0.2
        || p.pitch > p.diameter * 0.35
        || p.length <= p.pitch
        || p.length > 500.
        || p.length / p.pitch > 125.
    {
        return Err("Invalid diameter, pitch or length (maximum 125 turns) / 直径、螺距或长度无效（最多125圈）".into());
    }
    if p.clearance < 0.
        || p.clearance > p.pitch * 0.2
        || p.lead_in < 0.
        || p.lead_in > p.pitch * 0.8
    {
        return Err("Clearance or lead-in is too large / 间隙或入口倒角过大".into());
    }
    let shaft = p.shaft_length();
    if !p.kind.is_nut()
        && (shaft <= p.pitch || p.thread_length <= p.pitch || p.thread_length > shaft + 1e-8)
    {
        return Err("Thread length must fit the shank and exceed one pitch / 螺纹长度须大于一圈且不超过杆长".into());
    }
    if p.kind != K::Stud {
        if p.head_size < p.diameter + p.pitch
            || p.head_size > 250.
            || p.head_height <= 0.
            || p.head_height > 150.
        {
            return Err("Head/body size must leave enough wall around the thread / 头部或帽体尺寸须保留足够壁厚".into());
        }
        if p.kind.is_nut() && p.head_size * 0.5 <= p.diameter * 0.5 + p.clearance + 0.3 {
            return Err("Nut wall is too thin / 螺帽壁厚过小".into());
        }
        if matches!(p.kind, K::SocketScrew | K::CountersunkScrew)
            && (p.socket_size <= 0. || p.socket_size / (PI / 6.).cos() >= p.head_size * 0.85)
        {
            return Err("Socket must fit inside the head / 内六角孔须位于头部以内".into());
        }
    }
    Ok(())
}

/// Axisymmetric filled profile, including points on the axis. Y-up winding.
fn lathe(profile: &[[f64; 2]], n: usize) -> MeshData {
    let mut positions = Vec::new();
    let mut rings = Vec::new();
    for &[r, y] in profile {
        let mut ring = Vec::new();
        if r == 0. {
            let index = positions.len() as u32;
            positions.push([0., y as f32, 0.]);
            ring.resize(n, index);
        } else {
            for i in 0..n {
                let a = TAU * i as f64 / n as f64;
                ring.push(positions.len() as u32);
                positions.push([(r * a.cos()) as f32, y as f32, (-r * a.sin()) as f32]);
            }
        }
        rings.push(ring);
    }
    let mut indices = Vec::new();
    for pair in rings.windows(2) {
        for i in 0..n {
            let j = (i + 1) % n;
            let [a, b, c, d] = [pair[0][i], pair[0][j], pair[1][j], pair[1][i]];
            if a != b && b != d && d != a {
                indices.extend([a, b, d]);
            }
            if b != c && c != d && d != b {
                indices.extend([b, c, d]);
            }
        }
    }
    MeshData::new(positions, indices)
}

fn prism(flats: f64, sides: usize, bottom: f64, top: f64) -> MeshData {
    let r = flats * 0.5 / (PI / sides as f64).cos();
    lathe(&[[0., bottom], [r, bottom], [r, top], [0., top]], sides)
}
fn cylinder(r: f64, bottom: f64, top: f64) -> MeshData {
    lathe(&[[0., bottom], [r, bottom], [r, top], [0., top]], 96)
}
fn boolean(a: &MeshData, b: &MeshData, op: mesh_bool::BoolOp) -> Result<MeshData, String> {
    mesh_bool::boolean(a, b, op).map_err(|e| e.to_string())
}

pub fn build_mesh(p: &FastenerParams) -> Result<MeshData, String> {
    validate(p)?;
    let nut = p.kind.is_nut();
    let height = if nut { p.length } else { p.shaft_length() };
    let thread = threads::Params {
        // Shift the whole mating profile radially. Increasing cutter depth
        // instead would change flank width and can overlap adjacent turns.
        diameter: p.diameter
            + if nut {
                2. * p.clearance
            } else {
                -2. * p.clearance
            },
        pitch: p.pitch,
        length: if nut { height } else { p.thread_length },
        offset: 0.,
        depth: p.pitch * if nut { 0.541266 } else { 0.61343 },
        clearance: 0.,
        left_hand: p.left_hand,
        flat_fraction: if nut { 1. / 8. } else { 1. / 6. },
        ..Default::default()
    };
    let radius = p.diameter * 0.5
        - if nut {
            thread.depth - p.clearance
        } else {
            p.clearance
        };
    let target = threads::Target {
        body_id: 0,
        face_id: 0,
        origin: [0.; 3],
        axis: [0., 1., 0.],
        radius,
        length: height,
        internal: nut,
    };
    let mut mesh = if nut {
        let body = match p.kind {
            K::SquareNut => prism(p.head_size, 4, 0., height),
            K::RoundNut => cylinder(p.head_size * 0.5, 0., height),
            _ => prism(p.head_size, 6, 0., height),
        };
        let body = if p.kind == K::FlangeNut {
            boolean(
                &body,
                &cylinder(p.head_size * 0.65, 0., (height * 0.22).max(0.5)),
                mesh_bool::BoolOp::Union,
            )?
        } else {
            body
        };
        // Bore/chamfers run through the entire body, so there is no bottom cap
        // left behind to obstruct a mating screw.
        let lead = p.lead_in.min(height * 0.25);
        let mouth = (p.diameter * 0.5 + p.clearance).max(radius);
        let bore = lathe(
            &[
                [0., -0.1],
                [mouth, -0.1],
                [mouth, 0.],
                [radius, lead],
                [radius, height - lead],
                [mouth, height],
                [mouth, height + 0.1],
                [0., height + 0.1],
            ],
            96,
        );
        let blank = boolean(&body, &bore, mesh_bool::BoolOp::Subtract)?;
        if p.modeled_thread {
            let mut tool = threads::cutter(&target, &thread)?;
            // The female groove accepts the male crest, halfway between its
            // grooves. Rotate the cutter without rotating the nut body.
            tool.transform(&glam::Mat4::from_rotation_y(PI as f32));
            boolean(&blank, &tool, mesh_bool::BoolOp::Subtract)?
        } else {
            blank
        }
    } else {
        let lead = p.lead_in.min(radius * 0.6);
        let blank = lathe(
            &[
                [0., 0.],
                [(radius - lead).max(0.1), 0.],
                [radius, lead],
                [radius, height],
                [0., height],
            ],
            96,
        );
        let shaft = if p.modeled_thread {
            threads::cut(&blank, &target, &thread)?
        } else {
            blank
        };
        if p.kind == K::Stud {
            shaft
        } else {
            let bottom = height - 0.02;
            let top = height + p.head_height;
            let head = match p.kind {
                K::HexScrew => prism(p.head_size, 6, bottom, top),
                K::CountersunkScrew => lathe(
                    &[
                        [0., bottom],
                        [radius, bottom],
                        [p.head_size * 0.5, top],
                        [0., top],
                    ],
                    96,
                ),
                _ => cylinder(p.head_size * 0.5, bottom, top),
            };
            let head = if matches!(p.kind, K::SocketScrew | K::CountersunkScrew) {
                let socket = prism(p.socket_size, 6, top - p.head_height * 0.55, top + 0.1);
                boolean(&head, &socket, mesh_bool::BoolOp::Subtract)?
            } else {
                head
            };
            boolean(&shaft, &head, mesh_bool::BoolOp::Union)?
        }
    };
    let offset = glam::Vec3::from_array(p.position.map(|v| v as f32));
    mesh.transform(
        &(glam::Mat4::from_translation(offset)
            * glam::Mat4::from_mat3(glam::DMat3::from_cols_array(&p.basis).as_mat3())),
    );
    Ok(mesh)
}

pub fn transformed_params(mut p: FastenerParams, matrix: &glam::Mat4) -> FastenerParams {
    p.position = matrix
        .as_dmat4()
        .transform_point3(glam::DVec3::from_array(p.position))
        .to_array();
    p.basis = (glam::DMat3::from_mat4(matrix.as_dmat4()) * glam::DMat3::from_cols_array(&p.basis))
        .to_cols_array();
    p
}

pub struct Job {
    started: std::time::Instant,
    receiver: std::sync::mpsc::Receiver<Result<MeshData, String>>,
    params: FastenerParams,
    replace: Option<BodyId>,
    bodies: Vec<Body>,
}
impl S33DApp {
    pub(crate) fn fastener_progress_modal(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.fastener_job else {
            return;
        };
        if crate::app::model_progress::progress_modal(
            ctx,
            self.settings.language,
            ("Generate fastener", "生成紧固件"),
            ("Computing geometry…", "正在计算几何…"),
            None,
            job.started.elapsed().as_secs_f64(),
        ) {
            self.fastener_job = None;
            self.set_hint("Ready");
            self.toast(
                self.settings
                    .language
                    .text("Cancelled; document unchanged", "已取消，模型未改变"),
            );
        }
    }

    pub fn generate_fastener(&mut self, params: FastenerParams, replace: Option<BodyId>) {
        if self.mode != Mode::Idle || self.progress_busy() {
            self.toast(
                self.settings
                    .language
                    .text("Finish the current operation first", "请先完成当前操作"),
            );
            return;
        }
        if let Err(e) = validate(&params) {
            self.toast(error_text(&e, self.settings.language));
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| build_mesh(&params))
                .unwrap_or_else(|_| Err("Fastener geometry failed / 紧固件几何生成失败".into()));
            let _ = tx.send(result);
        });
        self.fastener_job = Some(Job {
            started: std::time::Instant::now(),
            receiver: rx,
            params,
            replace,
            bodies: self.doc.bodies.clone(),
        });
        self.set_hint(
            self.settings
                .language
                .text("Generating fastener…", "正在生成紧固件…"),
        );
        self.toast(
            self.settings
                .language
                .text("Generating fastener…", "正在生成紧固件…"),
        );
    }

    pub fn poll_fastener_job(&mut self) {
        let Some(job) = &self.fastener_job else {
            return;
        };
        let result = match job.receiver.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => Err("Fastener worker disconnected / 紧固件生成进程中断".into()),
        };
        let job = self.fastener_job.take().unwrap();
        self.set_hint(self.settings.language.text("Ready", "就绪"));
        if job.bodies != self.doc.bodies || self.mode != Mode::Idle {
            self.toast(
                self.settings
                    .language
                    .text("Document changed; generate again", "项目已改变，请重新生成"),
            );
            return;
        }
        let mesh = match result {
            Ok(m) => m,
            Err(e) => {
                self.toast(error_text(&e, self.settings.language));
                return;
            }
        };
        if let Some(id) = job.replace {
            if !self.body(id).is_some_and(|b| matches!(b.feature,Feature::Fastener{params} if params.kind.is_nut()==job.params.kind.is_nut())) {
                self.toast(self.settings.language.text("Fastener to update is no longer available", "原紧固件已不存在")); return;
            }
        }
        let had_solids = self
            .doc
            .bodies
            .iter()
            .any(|b| b.visible && b.kind == BodyKind::Solid);
        let shape = self.kernel.insert_mesh(mesh);
        self.begin_document_change();
        let id = if let Some(id) = job.replace {
            let b = self.doc.body_mut(id).unwrap();
            b.shape = Some(shape);
            b.feature = Feature::Fastener { params: job.params };
            b.visible = true;
            id
        } else {
            let id = self.doc.alloc_id();
            self.doc.add_body(Body {
                id,
                name: format!("{} {}", job.params.kind.name(), id),
                kind: BodyKind::Solid,
                shape: Some(shape),
                feature: Feature::Fastener { params: job.params },
                visible: true,
                material: Material::default(),
            });
            id
        };
        self.selected = vec![id];
        self.face_pick = None;
        self.selected_edge = None;
        self.clear_region_selection();
        self.mesh_cache.clear();
        self.section_cache.clear();
        self.record_history(format!("Generate {} {id}", job.params.kind.name()));
        self.bump_scene();
        if !had_solids {
            self.fit_view();
        }
        self.set_hint(
            self.settings
                .language
                .text("Fastener ready", "紧固件已生成"),
        );
        self.toast(
            self.settings
                .language
                .text("Fastener ready", "紧固件已生成"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn signed_volume(mesh: &MeshData) -> f64 {
        mesh.indices
            .chunks_exact(3)
            .map(|t| {
                let a = glam::DVec3::from_array(mesh.positions[t[0] as usize].map(f64::from));
                let b = glam::DVec3::from_array(mesh.positions[t[1] as usize].map(f64::from));
                let c = glam::DVec3::from_array(mesh.positions[t[2] as usize].map(f64::from));
                a.dot(b.cross(c)) / 6.
            })
            .sum()
    }
    #[test]
    fn every_fastener_style_is_closed_and_has_positive_volume() {
        for kind in K::SCREWS.into_iter().chain(K::NUTS) {
            let mut p = FastenerParams::defaults(kind);
            p.length = if kind.is_nut() { 5.2 } else { 8. };
            p.thread_length = p.shaft_length();
            let mesh = build_mesh(&p).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            assert!(signed_volume(&mesh) > 1., "{kind:?}");
            let copy = boolean(&mesh, &mesh, mesh_bool::BoolOp::Union).expect("manifold");
            assert!((signed_volume(&copy) - signed_volume(&mesh)).abs() < 0.1);
        }
    }
    #[test]
    fn print_pair_has_clearance_and_through_bore() {
        let mut male = FastenerParams::print_coarse(K::Stud);
        male.length = 8.;
        male.thread_length = 8.;
        let mut female = FastenerParams::print_coarse(K::HexNut);
        female.length = 8.;
        female.thread_length = 8.;
        let a = build_mesh(&male).unwrap();
        let b = build_mesh(&female).unwrap();
        // Mesh boolean rejects empty intersections. A nonempty result would
        // indicate a blocking entry or interfering thread flanks.
        let error = boolean(&a, &b, mesh_bool::BoolOp::Intersect).unwrap_err();
        assert!(error.to_lowercase().contains("empty"), "{error}");
        let bore = cylinder(male.diameter * 0.5 - male.pitch * 0.7, 0., female.length);
        let error = boolean(&b, &bore, mesh_bool::BoolOp::Intersect).unwrap_err();
        assert!(error.to_lowercase().contains("empty"), "{error}");
    }

    #[test]
    fn fastener_placement_survives_transform_and_recipe_roundtrip() {
        let mut p = FastenerParams::defaults(K::Stud);
        p.modeled_thread = false;
        p.position = [4., 8., 11.];
        let original = build_mesh(&p).unwrap();
        let matrix = glam::Mat4::from_translation(glam::vec3(20., -4., 3.))
            * glam::Mat4::from_rotation_z(0.8);
        let transformed = transformed_params(p, &matrix);
        let feature = Feature::Fastener {
            params: transformed,
        };
        let restored: Feature =
            serde_json::from_str(&serde_json::to_string(&feature).unwrap()).unwrap();
        assert_eq!(restored, feature);
        let rebuilt = build_mesh(&transformed).unwrap();
        for (before, after) in original.positions.iter().zip(&rebuilt.positions) {
            assert!(matrix
                .transform_point3(glam::Vec3::from_array(*before))
                .abs_diff_eq(glam::Vec3::from_array(*after), 1e-4));
        }
    }

    #[test]
    fn fastener_worker_commit_updates_one_id_and_is_undoable() {
        let mut app = S33DApp::new(&eframe::CreationContext::_new_kittest(
            egui::Context::default(),
        ));
        let mut p = FastenerParams::defaults(K::Stud);
        p.modeled_thread = false;
        let ready = |app: &mut S33DApp, p, replace| {
            let (tx, rx) = std::sync::mpsc::channel();
            tx.send(build_mesh(&p)).unwrap();
            app.fastener_job = Some(Job {
                started: std::time::Instant::now(),
                receiver: rx,
                params: p,
                replace,
                bodies: app.doc.bodies.clone(),
            });
            app.poll_fastener_job();
        };
        ready(&mut app, p, None);
        assert_eq!(app.doc.bodies.len(), 1);
        let id = app.selected[0];
        let original = app.doc.bodies.clone();
        p.length = 24.;
        p.thread_length = 24.;
        ready(&mut app, p, Some(id));
        assert_eq!(app.selected, vec![id]);
        assert_eq!(app.doc.bodies.len(), 1);
        assert!(app.doc.undo());
        assert_eq!(app.doc.bodies, original);
        app.mode = Mode::PickPlane;
        ready(&mut app, p, None);
        assert_eq!(
            app.doc.bodies, original,
            "late result must not change another tool's document"
        );
    }
}
