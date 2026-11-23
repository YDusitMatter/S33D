//! Independent geometric regressions for thread entry and screw engagement.
//!
//! These checks probe the final Boolean result, rather than trusting the
//! cutter's bounding box or reimplementing its tooth-profile formula.

use crate::threads::{self, Params, Target};
use glam::DVec3;
use s33d_kernel::{mesh_bool, MeshData, PlaneDef};
use std::f64::consts::{PI, TAU};

const ANGLES: usize = 96;
const SECTION_EPS: f64 = 0.002;
// The surfaces use 96 samples per turn. This allows their chord error while
// still rejecting the former approximately 0.059 mm M6 flank interference.
const GEOMETRY_TOLERANCE: f64 = 0.01;

struct Triangle {
    a: DVec3,
    ab: DVec3,
    ac: DVec3,
    z_min: f64,
    z_max: f64,
}

struct SectionProbe {
    origin: DVec3,
    axis: DVec3,
    u: DVec3,
    v: DVec3,
    triangles: Vec<Triangle>,
}

impl SectionProbe {
    fn new(mesh: &MeshData, target: &Target) -> Self {
        let origin = DVec3::from_array(target.origin);
        let axis = DVec3::from_array(target.axis);
        let u = PlaneDef::from_normal(target.axis).x_axis();
        let v = axis.cross(u);
        let triangles = mesh
            .indices
            .chunks_exact(3)
            .map(|tri| {
                let points = [tri[0], tri[1], tri[2]]
                    .map(|i| DVec3::from_array(mesh.positions[i as usize].map(f64::from)));
                assert!(points.iter().all(|p| p.is_finite()));
                let z = points.map(|p| (p - origin).dot(axis));
                Triangle {
                    a: points[0],
                    ab: points[1] - points[0],
                    ac: points[2] - points[0],
                    z_min: z.into_iter().fold(f64::INFINITY, f64::min),
                    z_max: z.into_iter().fold(f64::NEG_INFINITY, f64::max),
                }
            })
            .collect();
        Self {
            origin,
            axis,
            u,
            v,
            triangles,
        }
    }

    /// Cast a radial ray through the actual mesh section. A male thread's
    /// outside is the last crossing; a female thread's bore is the first.
    fn radius(&self, z: f64, angle: f64, internal: bool) -> f64 {
        let origin = self.origin + self.axis * z;
        let direction = self.u * angle.cos() + self.v * angle.sin();
        let mut minimum = f64::INFINITY;
        let mut maximum = f64::NEG_INFINITY;
        for tri in &self.triangles {
            if z < tri.z_min - 1e-7 || z > tri.z_max + 1e-7 {
                continue;
            }
            // Moller-Trumbore, with small barycentric slack at mesh seams.
            let p = direction.cross(tri.ac);
            let determinant = tri.ab.dot(p);
            if determinant.abs() < 1e-12 {
                continue;
            }
            let inverse = determinant.recip();
            let delta = origin - tri.a;
            let u = delta.dot(p) * inverse;
            if !(-1e-7..=1. + 1e-7).contains(&u) {
                continue;
            }
            let q = delta.cross(tri.ab);
            let v = direction.dot(q) * inverse;
            if v < -1e-7 || u + v > 1. + 1e-7 {
                continue;
            }
            let distance = tri.ac.dot(q) * inverse;
            if distance > 1e-5 {
                minimum = minimum.min(distance);
                maximum = maximum.max(distance);
            }
        }
        let result = if internal { minimum } else { maximum };
        assert!(
            result.is_finite(),
            "No radial mesh boundary at z={z:.6}, angle={angle:.6}, internal={internal}"
        );
        result
    }
}

fn angle(index: usize) -> f64 {
    // Avoid rays that coincide with tessellation edges or helix end seams.
    TAU * (index as f64 + 0.371) / ANGLES as f64
}

fn m6(internal: bool, height: f64) -> (Target, Params, MeshData) {
    m6_at(internal, height, [0., 0., 0.], [0., 1., 0.])
}

fn m6_at(
    internal: bool,
    height: f64,
    origin: [f64; 3],
    axis: [f64; 3],
) -> (Target, Params, MeshData) {
    let params = threads::preset_params("M6", internal).expect("M6 preset");
    let radius = 3. - if internal { params.depth } else { 0. };
    let target = Target {
        body_id: 1,
        face_id: 0,
        origin,
        axis,
        radius,
        length: height,
        internal,
    };
    let blank = blank_for_target(&target);
    (target, params, blank)
}

fn blank_for_target(target: &Target) -> MeshData {
    let radius = target.radius;
    let height = target.length;
    let profile = if target.internal {
        [[radius, 0.], [5., 0.], [5., height], [radius, height]]
    } else {
        // The revolver requires positive rho. This microscopic center hole is
        // outside the measurement: male sections use the last radial hit.
        [[0.001, 0.], [radius, 0.], [radius, height], [0.001, height]]
    };
    mesh_bool::revolve_profile(
        &profile,
        target.origin,
        target.axis,
        PlaneDef::from_normal(target.axis).x_axis().to_array(),
        96,
    )
    .expect("watertight thread blank")
}

fn assert_periodic_sections(probe: &SectionProbe, a: f64, b: f64, internal: bool) {
    for i in 0..ANGLES {
        let theta = angle(i);
        let ra = probe.radius(a, theta, internal);
        let rb = probe.radius(b, theta, internal);
        assert!(
            (ra - rb).abs() <= GEOMETRY_TOLERANCE,
            "Thread stops before its axial end: z={a:.6}/{b:.6}, theta={theta:.6}, radii={ra:.6}/{rb:.6}, internal={internal}"
        );
    }
}

fn assert_original_section(cut: &SectionProbe, original: &SectionProbe, z: f64, internal: bool) {
    for i in 0..ANGLES {
        let theta = angle(i);
        let actual = cut.radius(z, theta, internal);
        let expected = original.radius(z, theta, internal);
        assert!(
            (actual - expected).abs() < 0.002,
            "Thread escaped its requested range: z={z:.6}, theta={theta:.6}, actual={actual:.6}, original={expected:.6}, internal={internal}"
        );
    }
}

#[test]
fn m6_eight_mm_thread_reaches_the_end_without_an_entry_collar() {
    // Screenshot regression: r=3, h=30, offset=0, thread length=8. Check both
    // wall types and hands because a single angular or bounding-box test can
    // miss a partly closed entry left by the finite sweep's end cap.
    for internal in [false, true] {
        for left_hand in [false, true] {
            let (target, mut params, blank) = m6(internal, 30.);
            params.length = 8.;
            params.offset = 0.;
            params.left_hand = left_hand;
            let result = threads::cut(&blank, &target, &params).expect("M6 cut");
            let cut = SectionProbe::new(&result, &target);
            let original = SectionProbe::new(&blank, &target);

            assert_periodic_sections(&cut, SECTION_EPS, params.pitch + SECTION_EPS, internal);
            assert_periodic_sections(
                &cut,
                params.length - SECTION_EPS,
                params.length - params.pitch - SECTION_EPS,
                internal,
            );
            assert_original_section(&cut, &original, params.length + SECTION_EPS, internal);

            let largest_entry_change = (0..ANGLES)
                .map(|i| {
                    (cut.radius(SECTION_EPS, angle(i), internal)
                        - original.radius(SECTION_EPS, angle(i), internal))
                    .abs()
                })
                .fold(0., f64::max);
            assert!(
                largest_entry_change > params.depth * 0.75,
                "The physical entry still has a full uncut collar: internal={internal}, left_hand={left_hand}, change={largest_entry_change:.6}"
            );
        }
    }
}

#[test]
fn partial_m6_thread_preserves_both_requested_axial_limits() {
    for internal in [false, true] {
        let (target, mut params, blank) = m6(internal, 30.);
        params.offset = 2.25;
        params.length = 4.5;
        let end = params.offset + params.length;
        let result = threads::cut(&blank, &target, &params).expect("partial M6 cut");
        let cut = SectionProbe::new(&result, &target);
        let original = SectionProbe::new(&blank, &target);
        assert_original_section(&cut, &original, params.offset - SECTION_EPS, internal);
        assert_original_section(&cut, &original, end + SECTION_EPS, internal);
        assert_periodic_sections(
            &cut,
            params.offset + SECTION_EPS,
            params.offset + params.pitch + SECTION_EPS,
            internal,
        );
        assert_periodic_sections(
            &cut,
            end - SECTION_EPS,
            end - params.pitch - SECTION_EPS,
            internal,
        );
    }
}

#[test]
fn inclined_full_length_m6_keeps_both_physical_ends_open() {
    let axis = DVec3::new(1., 2., -3.).normalize().to_array();
    for internal in [false, true] {
        for left_hand in [false, true] {
            // A fractional turn count makes the opposite end's phase differ
            // from the entry. Mixed-sign world coordinates/axis also catch
            // clipping which accidentally uses a world-coordinate component.
            let (target, mut params, blank) = m6_at(internal, 5.25, [7.25, -11.5, 4.], axis);
            params.length = target.length;
            params.offset = 0.;
            params.clearance = 0.05;
            params.left_hand = left_hand;
            let result = threads::cut(&blank, &target, &params).expect("inclined full M6 cut");
            let cut = SectionProbe::new(&result, &target);
            assert_periodic_sections(&cut, SECTION_EPS, params.pitch + SECTION_EPS, internal);
            assert_periodic_sections(
                &cut,
                params.length - SECTION_EPS,
                params.length - params.pitch - SECTION_EPS,
                internal,
            );
            for z in [SECTION_EPS, params.length - SECTION_EPS] {
                let largest_cut = (0..ANGLES)
                    .map(|i| (cut.radius(z, angle(i), internal) - target.radius).abs())
                    .fold(0., f64::max);
                assert!(
                    largest_cut > params.depth * 0.75,
                    "An inclined physical end is blocked: z={z}, internal={internal}, left_hand={left_hand}"
                );
            }
        }
    }
}

#[test]
fn m6_male_and_female_allow_pitch_coupled_screw_motion() {
    let (male_target, mut male_params, male_blank) = m6(false, 30.);
    male_params.length = 8.;
    let male_mesh = threads::cut(&male_blank, &male_target, &male_params).expect("male M6");
    let male = SectionProbe::new(&male_mesh, &male_target);

    let (female_target, mut female_params, female_blank) = m6(true, 3.);
    female_params.length = 3.;
    let female_mesh =
        threads::cut(&female_blank, &female_target, &female_params).expect("female M6");
    let female = SectionProbe::new(&female_mesh, &female_target);

    // Relative half-pitch phase aligns female grooves with male crests. The
    // nut then rotates and advances together, rather than merely testing one
    // static placement. All its sections stay inside the male's 8 mm range.
    let initial_translation = 1.5 * male_params.pitch;
    let mut worst_interference: f64 = f64::NEG_INFINITY;
    let mut examined = 0;
    for rotation in [0., PI / 3., 4. * PI / 3.] {
        let translation = initial_translation + male_params.pitch * rotation / TAU;
        for local_z in [0.013, 0.31, 0.53, 1.33, 2.987] {
            for i in 0..ANGLES {
                let theta = angle(i);
                let male_radius = male.radius(translation + local_z, theta, false);
                let female_radius = female.radius(local_z, theta - rotation, true);
                let interference = male_radius - female_radius;
                worst_interference = worst_interference.max(interference);
                examined += 1;
                // These axial sections are radial intervals. Containment of
                // the male outer boundary within the female bore is therefore
                // an actual mesh collision check, including both tooth flanks.
                assert!(
                    interference <= GEOMETRY_TOLERANCE,
                    "M6 tooth interference during screw motion: rotation={rotation:.6}, translation={translation:.6}, z={local_z:.6}, theta={theta:.6}, male={male_radius:.6}, bore={female_radius:.6}, penetration={interference:.6} mm"
                );
            }
        }
    }
    assert_eq!(examined, 3 * 5 * ANGLES);
    assert!(worst_interference.is_finite());
}

#[test]
fn printed_pairs_have_positive_clearance_through_screw_motion() {
    use threads::config::{PrintProcess, PrintSettings};
    for settings in [
        PrintSettings::default(),
        PrintSettings {
            nozzle: 0.6,
            fdm_layer: 0.3,
            ..Default::default()
        },
        PrintSettings {
            process: PrintProcess::Resin,
            ..Default::default()
        },
        PrintSettings {
            process: PrintProcess::Resin,
            resin_layer: 0.1,
            ..Default::default()
        },
    ] {
        let process = settings.process;
        for left_hand in [false, true] {
            let axis = if process == PrintProcess::Resin && left_hand {
                DVec3::new(1., 2., 3.).normalize().to_array()
            } else {
                [0., 1., 0.]
            };
            let origin = [7., -4., 2.];
            let (male_target, original, male_blank) = m6_at(false, 16., origin, axis);
            let mut male_params = settings.suggest(&original, false).unwrap();
            male_params.length = 12.;
            male_params.left_hand = left_hand;
            let male_mesh = threads::cut(&male_blank, &male_target, &male_params).unwrap();
            let male = SectionProbe::new(&male_mesh, &male_target);
            let (mut female_target, original, _) = m6_at(true, 3., origin, axis);
            let mut female_params = settings.suggest(&original, true).unwrap();
            female_params.length = 3.;
            female_params.left_hand = left_hand;
            female_target.radius = threads::required_blank_radius(true, &female_params);
            let female_blank = blank_for_target(&female_target);
            assert_eq!(male_params.pitch, female_params.pitch);
            let female_mesh = threads::cut(&female_blank, &female_target, &female_params).unwrap();
            let female = SectionProbe::new(&female_mesh, &female_target);
            let expected_gap = 2. * male_params.radial_fit;
            for rotation in [0., PI / 3., 4. * PI / 3.] {
                let translation = 2.5 * male_params.pitch
                    + if left_hand { -1. } else { 1. } * male_params.pitch * rotation / TAU;
                for z in [0.013, 0.31, 0.53, 1.33, 2.987] {
                    for i in 0..ANGLES {
                        let theta = angle(i);
                        let gap = female.radius(z, theta - rotation, true)
                            - male.radius(z + translation, theta, false);
                        assert!(gap >= expected_gap - GEOMETRY_TOLERANCE,
                            "{process:?}, left={left_hand}, z={z}, rotation={rotation}, angle={theta}: gap {gap}, expected >= {expected_gap}");
                    }
                }
            }
            // Compensation must reach the physical entry while leaving the
            // unthreaded shoulder outside the requested range untouched.
            assert_periodic_sections(&male, 0.013, 0.013 + male_params.pitch, false);
            assert_periodic_sections(&female, 0.013, 0.013 + female_params.pitch, true);
            assert_original_section(
                &male,
                &SectionProbe::new(&male_blank, &male_target),
                12.1,
                false,
            );
        }
    }
}

/// Run only against an independent saved copy of the screenshot project:
/// S33D_THREAD_END_FIXTURE=<input.s33d>
/// S33D_THREAD_END_OUTPUT=<new-output.s33d>
/// cargo test -p s33d-app thread_fit_qa::repair_saved_m6_thread_end_fixture -- --ignored --exact
#[test]
#[ignore = "requires an explicit local fixture and a new independent output path"]
fn repair_saved_m6_thread_end_fixture() {
    use crate::app::S33DApp;
    use s33d_kernel::GeometryKernel;
    use s33d_model::Feature;
    use std::path::PathBuf;

    let input = PathBuf::from(
        std::env::var_os("S33D_THREAD_END_FIXTURE").expect("Set S33D_THREAD_END_FIXTURE"),
    );
    let output = PathBuf::from(
        std::env::var_os("S33D_THREAD_END_OUTPUT").expect("Set S33D_THREAD_END_OUTPUT"),
    );
    assert!(
        input.is_absolute() && output.is_absolute(),
        "Use absolute fixture/output paths"
    );
    let input = input.canonicalize().expect("Existing input fixture");
    // Refuse overwrites, including an input alias/hard link. A new output also
    // makes failed or repeated manual verification unable to replace a backup.
    assert!(!output.exists(), "Output must be a new independent file");
    let output_parent = output
        .parent()
        .unwrap()
        .canonicalize()
        .expect("Existing output directory");
    let output = output_parent.join(output.file_name().expect("Output filename"));
    assert_ne!(
        input.to_string_lossy().to_lowercase(),
        output.to_string_lossy().to_lowercase(),
        "Never overwrite the input fixture"
    );
    let input_bytes = std::fs::read(&input).expect("Read fixture without mutation");
    let creation = eframe::CreationContext::_new_kittest(egui::Context::default());
    let mut app = S33DApp::new(&creation);
    app.open_project_from_path(&input)
        .expect("Open independent saved fixture");
    assert_eq!(
        app.doc.bodies.len(),
        9,
        "Expected the nine-cylinder screenshot scene"
    );
    let original_bodies = app.doc.bodies.clone();
    let original_meshes: Vec<_> = original_bodies
        .iter()
        .map(|body| {
            let mesh = app
                .kernel
                .tessellate(body.shape.expect("Solid fixture shape"))
                .unwrap();
            (body.id, mesh)
        })
        .collect();
    let saved_camera = app.camera.clone();
    let saved_selection = app.selected.clone();
    let saved_view = format!("{saved_camera:?}");
    for body in &original_bodies {
        let saved = match &body.feature {
            Feature::Derived { source, .. } => source.strip_prefix("Thread v1: "),
            _ => None,
        };
        if body.id == 7 {
            let saved: threads::Saved =
                serde_json::from_str(saved.expect("Body 7 must be the old threaded body")).unwrap();
            assert!((saved.params.flat_fraction - 0.125).abs() < 1e-12);
            assert!((saved.params.length - 8.).abs() < 1e-12);
            assert!(saved.params.offset.abs() < 1e-12);
            assert!((saved.params.diameter - 6.).abs() < 1e-12);
            assert!((saved.params.pitch - 1.).abs() < 1e-12);
        } else {
            assert!(
                saved.is_none(),
                "Only Body 7 may contain a thread operation"
            );
        }
    }
    assert!(original_bodies.iter().any(|body| body.id == 7));

    // Exactly one Undo is permitted. Any unexpected later geometry operation
    // or change to a different body stops repair before an output is written.
    assert!(
        app.undo(),
        "The old thread must have a restorable Undo snapshot"
    );
    assert_eq!(app.doc.bodies.len(), 9);
    for before in original_bodies.iter().filter(|body| body.id != 7) {
        assert_eq!(
            app.doc.body(before.id),
            Some(before),
            "Undo changed another object"
        );
        let expected = &original_meshes
            .iter()
            .find(|(id, _)| *id == before.id)
            .unwrap()
            .1;
        let actual = app.kernel.tessellate(before.shape.unwrap()).unwrap();
        assert_eq!(&actual, expected, "Undo changed another object's geometry");
    }
    let restored_bodies = app.doc.bodies.clone();
    let original_body = original_bodies.iter().find(|body| body.id == 7).unwrap();
    let restored_body = app.doc.body(7).unwrap();
    assert_eq!(restored_body.name, original_body.name);
    assert_eq!(restored_body.kind, original_body.kind);
    assert_eq!(restored_body.material, original_body.material);
    assert_eq!(restored_body.visible, original_body.visible);
    let restored_shape = app.doc.body(7).unwrap().shape.unwrap();
    let restored_mesh = app.kernel.tessellate(restored_shape).unwrap();
    let mut targets = app
        .thread_targets(&[7])
        .expect("Recollect the restored cylinder");
    targets
        .retain(|t| !t.internal && (t.radius - 3.).abs() < 1e-5 && (t.length - 30.).abs() < 1e-5);
    assert_eq!(
        targets.len(),
        1,
        "Body 7 must restore to one r3 h30 outer wall"
    );
    let target = targets.remove(0);
    let original_probe = SectionProbe::new(&restored_mesh, &target);
    let old_mesh = &original_meshes.iter().find(|(id, _)| *id == 7).unwrap().1;
    assert_original_section(
        &SectionProbe::new(old_mesh, &target),
        &original_probe,
        SECTION_EPS,
        false,
    );

    let mut params = threads::preset_params("M6", false).unwrap();
    params.length = 8.;
    params.offset = 0.;
    assert_eq!(
        app.apply_threads(vec![target.clone()], params.clone())
            .unwrap(),
        vec![7]
    );
    assert_eq!(app.doc.bodies.len(), 9);
    for before in &restored_bodies {
        let after = app.doc.body(before.id).unwrap();
        if before.id != 7 {
            assert_eq!(
                after, before,
                "Repair changed another object's metadata/shape"
            );
            let expected = &original_meshes
                .iter()
                .find(|(id, _)| *id == before.id)
                .unwrap()
                .1;
            assert_eq!(
                app.kernel.tessellate(after.shape.unwrap()).unwrap(),
                *expected
            );
        } else {
            assert_eq!(after.id, before.id);
            assert_eq!(after.name, before.name);
            assert_eq!(after.kind, before.kind);
            assert_eq!(after.material, before.material);
            assert_eq!(after.visible, before.visible);
            assert_ne!(
                after.shape, before.shape,
                "Repair did not replace Body 7's shape"
            );
        }
    }
    let repaired_mesh = app
        .kernel
        .tessellate(app.doc.body(7).unwrap().shape.unwrap())
        .unwrap();
    assert_ne!(
        threads::mesh_signature(&repaired_mesh),
        threads::mesh_signature(old_mesh)
    );
    assert_ne!(
        threads::mesh_signature(&repaired_mesh),
        threads::mesh_signature(&restored_mesh)
    );
    let repaired = SectionProbe::new(&repaired_mesh, &target);
    assert_periodic_sections(&repaired, SECTION_EPS, params.pitch + SECTION_EPS, false);
    assert_periodic_sections(
        &repaired,
        params.length - SECTION_EPS,
        params.length - params.pitch - SECTION_EPS,
        false,
    );
    assert_original_section(
        &repaired,
        &original_probe,
        params.length + SECTION_EPS,
        false,
    );

    app.camera = saved_camera;
    app.selected = saved_selection.clone();
    assert_eq!(format!("{:?}", app.camera), saved_view);
    assert_eq!(app.selected, saved_selection);
    app.save_project_to_path(&output)
        .expect("Save independent repaired output");
    assert_eq!(
        std::fs::read(&input).unwrap(),
        input_bytes,
        "Input backup was modified"
    );
    let output_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    let input_json: serde_json::Value = serde_json::from_slice(&input_bytes).unwrap();
    assert_eq!(
        output_json["camera"], input_json["camera"],
        "Saved camera changed"
    );
    assert_eq!(
        output_json["selected"], input_json["selected"],
        "Saved object selection changed"
    );
    eprintln!(
        "Repaired only Body 7; saved independent project to {}",
        output.display()
    );
}
