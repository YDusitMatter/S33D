//! Parametric tire geometry for the built-in Tire Generator plug-in.
//!
//! The carcass is a closed elliptical torus around world Y. Tread is part of
//! the same watertight surface rather than a collection of floating blocks, so
//! the result remains robust for export and later Boolean operations.

use s33d_kernel::{MeshData, ShapeId, TruckKernel};
use s33d_model::{TireParams, TireTreadPattern};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TireMetrics {
    pub sidewall_height: f64,
    pub outer_radius: f64,
    pub rim_radius: f64,
}

pub fn metrics(params: &TireParams) -> TireMetrics {
    TireMetrics {
        sidewall_height: (params.outer_diameter - params.rim_diameter) * 0.5,
        outer_radius: params.outer_diameter * 0.5,
        rim_radius: params.rim_diameter * 0.5,
    }
}

pub fn validate(params: &TireParams) -> Result<(), String> {
    if params.position.iter().any(|value| !value.is_finite()) {
        return Err("Position must be finite".into());
    }
    if !params.outer_diameter.is_finite() || params.outer_diameter <= 20.0 {
        return Err("Outer diameter must be greater than 20 mm".into());
    }
    if !params.rim_diameter.is_finite()
        || params.rim_diameter <= 5.0
        || params.rim_diameter >= params.outer_diameter - 4.0
    {
        return Err("Rim diameter must remain below the tire outer diameter".into());
    }
    if !params.section_width.is_finite() || params.section_width <= 4.0 {
        return Err("Section width must be greater than 4 mm".into());
    }
    let sidewall = metrics(params).sidewall_height;
    if params.section_width > params.outer_diameter * 0.8 {
        return Err("Section width is too large for the selected diameter".into());
    }
    if !params.tread_depth.is_finite()
        || params.tread_depth < 0.0
        || params.tread_depth >= sidewall * 0.35
    {
        return Err("Tread depth must be less than 35% of sidewall height".into());
    }
    if !(8..=120).contains(&params.tread_repeats) {
        return Err("Tread repeats must be between 8 and 120".into());
    }
    Ok(())
}

pub fn build_tire_shape(kernel: &mut TruckKernel, params: &TireParams) -> Result<ShapeId, String> {
    validate(params)?;
    Ok(kernel.insert_mesh(tire_mesh(params)?))
}

pub fn tire_mesh(params: &TireParams) -> Result<MeshData, String> {
    validate(params)?;
    let outer_radius = params.outer_diameter * 0.5;
    let inner_radius = params.rim_diameter * 0.5;
    // `outer_diameter` is the finished tire envelope, including tread.
    let carcass_outer_radius = outer_radius - params.tread_depth;
    let radial_minor = (carcass_outer_radius - inner_radius) * 0.5;
    let major_radius = inner_radius + radial_minor;
    let axial_minor = params.section_width * 0.5;
    let theta_segments = (params.tread_repeats as usize * 8).clamp(128, 640);
    let phi_segments = 64usize;
    let mut positions = Vec::with_capacity(theta_segments * phi_segments);

    for theta_index in 0..theta_segments {
        let theta = std::f64::consts::TAU * theta_index as f64 / theta_segments as f64;
        let (theta_sin, theta_cos) = theta.sin_cos();
        for phi_index in 0..phi_segments {
            let phi = std::f64::consts::TAU * phi_index as f64 / phi_segments as f64;
            let (phi_sin, phi_cos) = phi.sin_cos();
            let tread = tread_height(
                params.pattern,
                theta,
                phi_sin,
                phi_cos,
                params.tread_repeats,
            );
            let shoulder_gate = ((phi_cos - 0.02) / 0.98).clamp(0.0, 1.0).powi(2);
            let radial =
                major_radius + radial_minor * phi_cos + params.tread_depth * shoulder_gate * tread;
            positions.push([
                (params.position[0] + radial * theta_cos) as f32,
                (params.position[1] + axial_minor * phi_sin) as f32,
                (params.position[2] + radial * theta_sin) as f32,
            ]);
        }
    }

    let mut indices = Vec::with_capacity(theta_segments * phi_segments * 6);
    for theta_index in 0..theta_segments {
        let next_theta = (theta_index + 1) % theta_segments;
        for phi_index in 0..phi_segments {
            let next_phi = (phi_index + 1) % phi_segments;
            let a = (theta_index * phi_segments + phi_index) as u32;
            let b = (theta_index * phi_segments + next_phi) as u32;
            let c = (next_theta * phi_segments + next_phi) as u32;
            let d = (next_theta * phi_segments + phi_index) as u32;
            indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }
    let mut mesh = MeshData::new(positions, indices);
    mesh.weld(1e-5);
    Ok(mesh)
}

fn tread_height(
    pattern: TireTreadPattern,
    theta: f64,
    lateral: f64,
    outward: f64,
    repeats: u32,
) -> f64 {
    if outward <= 0.02 {
        return 0.0;
    }
    let cycle = theta * repeats as f64;
    let circumferential = 0.5 + 0.5 * cycle.cos();
    let staggered = 0.5 + 0.5 * (cycle + lateral * std::f64::consts::PI * 2.5).cos();
    let ribs = 0.5 + 0.5 * (lateral * std::f64::consts::PI * 6.0).cos();
    let chevron =
        0.5 + 0.5 * (cycle + lateral.signum() * lateral.abs() * std::f64::consts::PI * 5.0).cos();
    match pattern {
        TireTreadPattern::HighwayRib => 0.38 + 0.62 * smooth_block(ribs, 0.42),
        TireTreadPattern::Block => 0.12 + 0.88 * smooth_block(circumferential.min(ribs), 0.34),
        TireTreadPattern::Chevron => 0.18 + 0.82 * smooth_block(chevron, 0.38),
        TireTreadPattern::AllTerrain => {
            let shoulder = lateral.abs().powf(1.6);
            let blocks = smooth_block(staggered.min(ribs), 0.28);
            (0.22 + 0.78 * blocks + 0.16 * shoulder * smooth_block(circumferential, 0.3)).min(1.0)
        }
    }
}

fn smooth_block(value: f64, edge: f64) -> f64 {
    ((value - edge) / (1.0 - edge)).clamp(0.0, 1.0)
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
    fn every_tread_pattern_is_watertight_and_keeps_requested_envelope() {
        for pattern in TireTreadPattern::ALL {
            let params = TireParams {
                pattern,
                ..TireParams::default()
            };
            let mesh = tire_mesh(&params).unwrap();
            assert_watertight(&mesh);
            let (min, max) = mesh.bounds().unwrap();
            assert!((max.y - min.y - params.section_width as f32).abs() < 0.01);
            let radial_max = mesh
                .positions
                .iter()
                .map(|point| (point[0] * point[0] + point[2] * point[2]).sqrt())
                .fold(0.0_f32, f32::max);
            assert!(radial_max >= params.outer_diameter as f32 * 0.5);
            assert!(radial_max <= (params.outer_diameter * 0.5 + 0.01) as f32);
        }
    }

    #[test]
    fn invalid_rim_diameter_is_rejected() {
        let params = TireParams {
            rim_diameter: 700.0,
            ..TireParams::default()
        };
        assert!(validate(&params).is_err());
    }
}
