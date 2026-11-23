//! Parametric gear geometry for the built-in Gear Generator plug-in.
//!
//! External teeth use a sampled involute flank. Spur gears are a zero-twist
//! loft, helical gears twist the same section through the face width, bevel
//! gears taper it, and internal gears subtract an involute-shaped cutter from
//! a cylindrical rim.

use s33d_kernel::{DatumPlane, GeometryKernel, MeshData, Region2D, ShapeId, TruckKernel};
use s33d_model::{triangulate_region, GearKind, GearParams};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GearMetrics {
    pub pitch_diameter: f64,
    pub outside_diameter: f64,
    pub root_diameter: f64,
}

pub fn metrics(params: &GearParams) -> GearMetrics {
    let pitch_radius = params.module * f64::from(params.teeth) * 0.5;
    match params.kind {
        GearKind::Internal => GearMetrics {
            pitch_diameter: pitch_radius * 2.0,
            outside_diameter: (pitch_radius + params.module * 1.25 + params.ring_wall_thickness)
                * 2.0,
            root_diameter: (pitch_radius - params.module).max(params.module) * 2.0,
        },
        _ => GearMetrics {
            pitch_diameter: pitch_radius * 2.0,
            outside_diameter: (pitch_radius + params.module) * 2.0,
            root_diameter: (pitch_radius - params.module * 1.25).max(params.module * 0.5) * 2.0,
        },
    }
}

pub fn validate(params: &GearParams) -> Result<(), String> {
    if !params.module.is_finite() || !(0.1..=50.0).contains(&params.module) {
        return Err("Module must be between 0.1 and 50 mm".into());
    }
    let minimum_teeth = if params.kind == GearKind::Internal {
        12
    } else {
        8
    };
    if !(minimum_teeth..=200).contains(&params.teeth) {
        return Err(format!(
            "Tooth count must be between {minimum_teeth} and 200"
        ));
    }
    if !params.pressure_angle_deg.is_finite() || !(14.0..=30.0).contains(&params.pressure_angle_deg)
    {
        return Err("Pressure angle must be between 14° and 30°".into());
    }
    if !params.face_width.is_finite() || params.face_width <= 0.05 {
        return Err("Face width must be greater than zero".into());
    }
    if !params.backlash.is_finite()
        || params.backlash < 0.0
        || params.backlash >= std::f64::consts::PI * params.module * 0.45
    {
        return Err("Backlash is outside the supported range".into());
    }
    if params.position.iter().any(|value| !value.is_finite()) {
        return Err("Position must be finite".into());
    }
    if params.kind != GearKind::Internal {
        let root_radius = metrics(params).root_diameter * 0.5;
        if !params.bore_diameter.is_finite()
            || params.bore_diameter < 0.0
            || params.bore_diameter * 0.5 >= root_radius - params.module * 0.15
        {
            return Err("Bore diameter must remain inside the root diameter".into());
        }
    }
    if params.kind == GearKind::Helical
        && (!params.helix_angle_deg.is_finite() || params.helix_angle_deg.abs() > 45.0)
    {
        return Err("Helix angle must be between -45° and 45°".into());
    }
    if params.kind == GearKind::Bevel {
        if !params.bevel_angle_deg.is_finite() || !(5.0..=60.0).contains(&params.bevel_angle_deg) {
            return Err("Bevel angle must be between 5° and 60°".into());
        }
        let outer_radius = metrics(params).outside_diameter * 0.5;
        let top_scale =
            1.0 - params.face_width * params.bevel_angle_deg.to_radians().tan() / outer_radius;
        if top_scale < 0.18 {
            return Err("Face width and bevel angle collapse the small end".into());
        }
    }
    if params.kind == GearKind::Internal
        && (!params.ring_wall_thickness.is_finite()
            || params.ring_wall_thickness < params.module * 0.5)
    {
        return Err("Ring wall must be at least half a module".into());
    }
    Ok(())
}

pub fn build_gear_shape(kernel: &mut TruckKernel, params: &GearParams) -> Result<ShapeId, String> {
    validate(params)?;
    if params.kind == GearKind::Internal {
        return build_internal_gear(kernel, params);
    }

    let pitch_radius = params.module * f64::from(params.teeth) * 0.5;
    let root_radius = (pitch_radius - params.module * 1.25).max(params.module * 0.5);
    let outside_radius = pitch_radius + params.module;
    let profile = involute_profile(
        params.module,
        params.teeth,
        params.pressure_angle_deg,
        params.backlash,
        root_radius,
        outside_radius,
    )?;
    let (twist, top_scale, layers) = match params.kind {
        GearKind::Spur => (0.0, 1.0, 1),
        GearKind::Helical => {
            let twist = params.face_width * params.helix_angle_deg.to_radians().tan()
                / pitch_radius.max(1e-6);
            let layers =
                ((twist.abs() * f64::from(params.teeth) * 2.0).ceil() as usize).clamp(8, 48);
            (twist, 1.0, layers)
        }
        GearKind::Bevel => {
            let scale = 1.0
                - params.face_width * params.bevel_angle_deg.to_radians().tan() / outside_radius;
            (0.0, scale, 10)
        }
        GearKind::Internal => unreachable!(),
    };
    let mesh = loft_profile(
        &profile,
        params.face_width,
        params.position,
        twist,
        top_scale,
        layers,
    )?;
    let body = kernel.insert_mesh(mesh);
    subtract_bore(kernel, body, params)
}

fn build_internal_gear(kernel: &mut TruckKernel, params: &GearParams) -> Result<ShapeId, String> {
    let pitch_radius = params.module * f64::from(params.teeth) * 0.5;
    let inner_tip_radius = (pitch_radius - params.module).max(params.module);
    let cavity_root_radius = pitch_radius + params.module * 1.25;
    let outside_radius = cavity_root_radius + params.ring_wall_thickness;

    let mut plane = DatumPlane::Top.def();
    plane.origin = params.position;

    let cutter_profile = involute_profile(
        params.module,
        params.teeth,
        params.pressure_angle_deg,
        params.backlash,
        inner_tip_radius,
        cavity_root_radius,
    )?;
    let margin = (params.module * 0.2).max(0.1);
    let cutter_mesh = loft_profile(
        &cutter_profile,
        params.face_width + margin * 2.0,
        [
            params.position[0],
            params.position[1] - margin,
            params.position[2],
        ],
        0.0,
        1.0,
        1,
    )?;
    let rim = kernel
        .extrude_region(
            &Region2D::new(circle_points(outside_radius, 160)),
            &plane,
            params.face_width,
        )
        .map_err(|error| error.to_string())?;

    let cutter = kernel.insert_mesh(cutter_mesh);
    let result = kernel
        .boolean_subtract(rim, cutter)
        .map_err(|error| format!("Internal gear subtraction failed: {error}"));
    kernel.delete(rim);
    kernel.delete(cutter);
    result
}

fn subtract_bore(
    kernel: &mut TruckKernel,
    body: ShapeId,
    params: &GearParams,
) -> Result<ShapeId, String> {
    if params.bore_diameter <= 1e-6 {
        return Ok(body);
    }
    let margin = (params.module * 0.2).max(0.1);
    let mut plane = DatumPlane::Top.def();
    plane.origin = [
        params.position[0],
        params.position[1] - margin,
        params.position[2],
    ];
    let bore = match kernel.extrude_region(
        &Region2D::new(circle_points(params.bore_diameter * 0.5, 96)),
        &plane,
        params.face_width + margin * 2.0,
    ) {
        Ok(bore) => bore,
        Err(error) => {
            kernel.delete(body);
            return Err(error.to_string());
        }
    };
    let result = kernel
        .boolean_subtract(body, bore)
        .map_err(|error| format!("Bore subtraction failed: {error}"));
    kernel.delete(body);
    kernel.delete(bore);
    result
}

fn involute_profile(
    module: f64,
    teeth: u32,
    pressure_angle_deg: f64,
    backlash: f64,
    root_radius: f64,
    outside_radius: f64,
) -> Result<Vec<[f64; 2]>, String> {
    let tooth_count = f64::from(teeth);
    let pitch_radius = module * tooth_count * 0.5;
    let pressure = pressure_angle_deg.to_radians();
    let base_radius = pitch_radius * pressure.cos();
    if root_radius <= 0.0 || outside_radius <= base_radius {
        return Err("Gear radii do not admit an involute tooth".into());
    }
    let pitch = std::f64::consts::TAU / tooth_count;
    let involute_pitch = pressure.tan() - pressure;
    let half_pitch_tooth = (std::f64::consts::PI / (2.0 * tooth_count)
        - backlash / (2.0 * pitch_radius))
        .clamp(pitch * 0.04, pitch * 0.22);
    let involute_angle = |radius: f64| {
        let radius = radius.max(base_radius);
        let t = ((radius / base_radius).powi(2) - 1.0).max(0.0).sqrt();
        t - t.atan()
    };
    let half_angle = |radius: f64| half_pitch_tooth + involute_pitch - involute_angle(radius);
    let flank_start = root_radius.max(base_radius);
    let flank_steps = 5usize;
    let outer_steps = 3usize;
    let valley_steps = 2usize;
    let mut points = Vec::with_capacity(teeth as usize * 18);

    for tooth in 0..teeth {
        let center = f64::from(tooth) * pitch;
        let root_half = half_angle(flank_start);
        if root_radius < flank_start - 1e-9 {
            points.push(polar(root_radius, center - root_half));
        }
        for step in 0..=flank_steps {
            let fraction = step as f64 / flank_steps as f64;
            let radius = flank_start + (outside_radius - flank_start) * fraction;
            points.push(polar(radius, center - half_angle(radius)));
        }
        let outer_half = half_angle(outside_radius);
        for step in 1..=outer_steps {
            let fraction = step as f64 / outer_steps as f64;
            points.push(polar(
                outside_radius,
                center - outer_half + outer_half * 2.0 * fraction,
            ));
        }
        for step in 1..=flank_steps {
            let fraction = step as f64 / flank_steps as f64;
            let radius = outside_radius - (outside_radius - flank_start) * fraction;
            points.push(polar(radius, center + half_angle(radius)));
        }
        if root_radius < flank_start - 1e-9 {
            points.push(polar(root_radius, center + root_half));
        }
        let valley_end = center + pitch - root_half;
        for step in 1..=valley_steps {
            let fraction = step as f64 / (valley_steps + 1) as f64;
            points.push(polar(
                root_radius,
                center + root_half + (valley_end - center - root_half) * fraction,
            ));
        }
    }
    if s33d_kernel::region::signed_area(&points) < 0.0 {
        points.reverse();
    }
    Ok(points)
}

fn loft_profile(
    profile: &[[f64; 2]],
    width: f64,
    position: [f64; 3],
    twist: f64,
    top_scale: f64,
    layers: usize,
) -> Result<MeshData, String> {
    if profile.len() < 3 {
        return Err("Gear profile is empty".into());
    }
    let layers = layers.max(1);
    let ring = profile.len();
    let mut positions = Vec::with_capacity((layers + 1) * ring);
    for layer in 0..=layers {
        let fraction = layer as f64 / layers as f64;
        let angle = twist * fraction;
        let scale = 1.0 + (top_scale - 1.0) * fraction;
        let (sin, cos) = angle.sin_cos();
        for point in profile {
            let x = point[0] * scale;
            let z = point[1] * scale;
            positions.push([
                (position[0] + x * cos - z * sin) as f32,
                (position[1] + width * fraction) as f32,
                (position[2] + x * sin + z * cos) as f32,
            ]);
        }
    }

    let mut indices = Vec::with_capacity(layers * ring * 6 + (ring - 2) * 6);
    for layer in 0..layers {
        let bottom = layer * ring;
        let top = (layer + 1) * ring;
        for index in 0..ring {
            let next = (index + 1) % ring;
            let a = (bottom + index) as u32;
            let b = (bottom + next) as u32;
            let c = (top + next) as u32;
            let d = (top + index) as u32;
            indices.extend_from_slice(&[a, c, b, a, d, c]);
        }
    }
    let cap_triangles = triangulate_region(profile, &[])
        .map_err(|error| format!("Gear face triangulation failed: {error}"))?
        .triangles;
    if cap_triangles.is_empty() {
        return Err("Gear face triangulation failed".into());
    }
    let top_offset = layers * ring;
    for triangle in cap_triangles {
        let mut local = [0usize; 3];
        for (slot, point) in triangle.iter().enumerate() {
            local[slot] = profile
                .iter()
                .position(|candidate| candidate == point)
                .ok_or_else(|| "Gear cap mapping failed".to_string())?;
        }
        indices.extend_from_slice(&[local[0] as u32, local[1] as u32, local[2] as u32]);
        indices.extend_from_slice(&[
            (top_offset + local[0]) as u32,
            (top_offset + local[2]) as u32,
            (top_offset + local[1]) as u32,
        ]);
    }
    let mut mesh = MeshData::new(positions, indices);
    mesh.weld(1e-5);
    Ok(mesh)
}

fn circle_points(radius: f64, segments: usize) -> Vec<[f64; 2]> {
    (0..segments)
        .map(|index| {
            polar(
                radius,
                std::f64::consts::TAU * index as f64 / segments as f64,
            )
        })
        .collect()
}

fn polar(radius: f64, angle: f64) -> [f64; 2] {
    [radius * angle.cos(), radius * angle.sin()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn assert_watertight(mesh: &MeshData) {
        let mut mesh = mesh.clone();
        mesh.weld(1e-5);
        let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
        for triangle in mesh.indices.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(edges.values().all(|count| *count == 2));
    }

    #[test]
    fn involute_profile_is_ccw_and_reaches_standard_diameters() {
        let params = GearParams::default();
        let dimensions = metrics(&params);
        let profile = involute_profile(
            params.module,
            params.teeth,
            params.pressure_angle_deg,
            params.backlash,
            dimensions.root_diameter * 0.5,
            dimensions.outside_diameter * 0.5,
        )
        .unwrap();
        assert!(s33d_kernel::region::signed_area(&profile) > 0.0);
        let maximum = profile
            .iter()
            .map(|point| point[0].hypot(point[1]))
            .fold(0.0_f64, f64::max);
        assert!((maximum * 2.0 - dimensions.outside_diameter).abs() < 1e-6);
    }

    #[test]
    fn spur_helical_and_bevel_lofts_are_closed_solids() {
        for kind in [GearKind::Spur, GearKind::Helical, GearKind::Bevel] {
            let params = GearParams {
                kind,
                bore_diameter: 0.0,
                ..GearParams::default()
            };
            let dimensions = metrics(&params);
            let profile = involute_profile(
                params.module,
                params.teeth,
                params.pressure_angle_deg,
                params.backlash,
                dimensions.root_diameter * 0.5,
                dimensions.outside_diameter * 0.5,
            )
            .unwrap();
            let (twist, scale, layers) = match kind {
                GearKind::Spur => (0.0, 1.0, 1),
                GearKind::Helical => (0.35, 1.0, 12),
                GearKind::Bevel => (0.0, 0.7, 8),
                GearKind::Internal => unreachable!(),
            };
            let mesh = loft_profile(&profile, 10.0, [0.0; 3], twist, scale, layers).unwrap();
            assert_watertight(&mesh);
        }
    }

    #[test]
    fn all_gear_kinds_build_and_tessellate() {
        for kind in GearKind::ALL {
            let mut kernel = TruckKernel::new();
            let params = GearParams {
                kind,
                teeth: if kind == GearKind::Internal { 30 } else { 18 },
                ..GearParams::default()
            };
            let shape = build_gear_shape(&mut kernel, &params)
                .unwrap_or_else(|error| panic!("{kind:?}: {error}"));
            assert_eq!(
                kernel.shape_count(),
                1,
                "{kind:?}: construction temporaries must be released"
            );
            let mesh = kernel.tessellate(shape).unwrap();
            assert!(!mesh.is_empty(), "{kind:?}");
            assert_watertight(&mesh);
        }
    }
}
