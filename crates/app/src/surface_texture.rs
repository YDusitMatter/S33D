//! Procedural surface knurling for the built-in Surface Texture plug-in.
//!
//! The operation is intentionally non-destructive at document level: the
//! source body remains in the model tree (hidden), while this module cuts a
//! generated set of shallow grooves into a new result shape.  Planar patches
//! use clipped straight grooves. Cylinders use tangent cutters distributed
//! around the complete continuous side surface.

use glam::DVec3;
use s33d_kernel::{GeometryKernel, MeshData, ShapeId, TruckKernel};
use s33d_model::{SurfaceTextureParams, SurfaceTexturePattern, SurfaceTextureSurface};

pub fn validate(
    surface: &SurfaceTextureSurface,
    params: &SurfaceTextureParams,
) -> Result<(), String> {
    if !params.pitch.is_finite() || params.pitch <= 0.05 {
        return Err("Pitch must be greater than 0.05 mm".into());
    }
    if !params.groove_width.is_finite()
        || params.groove_width <= 0.01
        || params.groove_width >= params.pitch * 0.9
    {
        return Err("Groove width must be positive and smaller than 90% of pitch".into());
    }
    if !params.depth.is_finite() || params.depth <= 0.01 {
        return Err("Depth must be greater than 0.01 mm".into());
    }
    if !params.angle_deg.is_finite() || !(5.0..=85.0).contains(&params.angle_deg) {
        return Err("Knurl angle must be between 5° and 85°".into());
    }
    if !params.margin.is_finite() || params.margin < 0.0 {
        return Err("Margin cannot be negative".into());
    }
    match surface {
        SurfaceTextureSurface::Planar {
            normal,
            x_axis,
            width,
            height,
            ..
        } => {
            if *width <= params.margin * 2.0 + params.pitch
                || *height <= params.margin * 2.0 + params.pitch
            {
                return Err("Selected planar face is too small for this pitch and margin".into());
            }
            validate_basis(*normal, *x_axis)?;
        }
        SurfaceTextureSurface::Cylinder {
            axis,
            x_axis,
            radius,
            length,
            ..
        } => {
            if *radius <= params.depth || *length <= params.margin * 2.0 + params.pitch {
                return Err(
                    "Selected cylinder is too small for this depth, pitch and margin".into(),
                );
            }
            validate_basis(*axis, *x_axis)?;
        }
    }
    Ok(())
}

fn validate_basis(axis: [f64; 3], x_axis: [f64; 3]) -> Result<(), String> {
    let axis = DVec3::from_array(axis);
    let x_axis = DVec3::from_array(x_axis);
    if axis.length_squared() < 0.99
        || x_axis.length_squared() < 0.99
        || axis.dot(x_axis).abs() > 0.02
    {
        Err("Selected surface has an invalid local coordinate frame".into())
    } else {
        Ok(())
    }
}

pub fn build_surface_texture_shape(
    kernel: &mut TruckKernel,
    source: ShapeId,
    surface: &SurfaceTextureSurface,
    params: &SurfaceTextureParams,
) -> Result<ShapeId, String> {
    validate(surface, params)?;
    if matches!(surface, SurfaceTextureSurface::Cylinder { .. }) {
        return Ok(kernel.insert_mesh(textured_cylinder_mesh(surface, params)?));
    }
    let cutter_meshes = planar_cutters(surface, params)?;
    let mut result = source;
    for (index, cutters) in cutter_meshes.into_iter().enumerate() {
        if cutters.is_empty() {
            continue;
        }
        let cutter = kernel.insert_mesh(cutters);
        result = kernel.boolean_subtract(result, cutter).map_err(|error| {
            format!(
                "Could not cut surface-texture groove {}: {error}",
                index + 1
            )
        })?;
    }
    Ok(result)
}

fn pattern_angles(pattern: SurfaceTexturePattern, angle_deg: f64) -> Vec<f64> {
    let angle = angle_deg.to_radians();
    match pattern {
        SurfaceTexturePattern::Straight => vec![0.0],
        SurfaceTexturePattern::DiagonalLeft => vec![angle],
        SurfaceTexturePattern::DiagonalRight => vec![-angle],
        SurfaceTexturePattern::Diamond => vec![angle, -angle],
    }
}

fn planar_cutters(
    surface: &SurfaceTextureSurface,
    params: &SurfaceTextureParams,
) -> Result<Vec<MeshData>, String> {
    let SurfaceTextureSurface::Planar {
        origin,
        normal,
        x_axis,
        width,
        height,
    } = surface
    else {
        unreachable!()
    };
    let center = DVec3::from_array(*origin);
    let normal = DVec3::from_array(*normal).normalize();
    let x_axis = DVec3::from_array(*x_axis).normalize();
    let y_axis = normal.cross(x_axis).normalize();
    let usable_width = width - params.margin * 2.0;
    let usable_height = height - params.margin * 2.0;
    let mut groups = Vec::new();

    for angle in pattern_angles(params.pattern, params.angle_deg) {
        let direction = (x_axis * angle.cos() + y_axis * angle.sin()).normalize();
        let perpendicular = normal.cross(direction).normalize();
        let projected = usable_width * angle.sin().abs() + usable_height * angle.cos().abs();
        let count = (projected / params.pitch).ceil() as i32;
        let mut cutters = MeshData::default();
        for index in -count..=count {
            let offset = index as f64 * params.pitch;
            if offset.abs() > projected * 0.5 + params.pitch * 0.25 {
                continue;
            }
            let direction_2d = glam::DVec2::new(angle.cos(), angle.sin());
            let perpendicular_2d = glam::DVec2::new(-angle.sin(), angle.cos());
            let start_2d = perpendicular_2d * offset;
            let Some((line_start, line_end)) = line_rectangle_interval(
                start_2d,
                direction_2d,
                (usable_width - params.groove_width).max(0.0) * 0.5,
                (usable_height - params.groove_width).max(0.0) * 0.5,
            ) else {
                continue;
            };
            let groove_center =
                center + perpendicular * offset + direction * ((line_start + line_end) * 0.5)
                    - normal * params.depth * 0.42;
            cutters.merge(&oriented_box(
                groove_center,
                direction,
                perpendicular,
                normal,
                line_end - line_start,
                params.groove_width,
                params.depth * 1.35,
            ));
        }
        groups.push(cutters);
    }
    Ok(groups)
}

fn line_rectangle_interval(
    origin: glam::DVec2,
    direction: glam::DVec2,
    half_width: f64,
    half_height: f64,
) -> Option<(f64, f64)> {
    let mut start = f64::NEG_INFINITY;
    let mut end = f64::INFINITY;
    for (coordinate, component, half_extent) in [
        (origin.x, direction.x, half_width),
        (origin.y, direction.y, half_height),
    ] {
        if component.abs() < 1.0e-12 {
            if coordinate.abs() > half_extent {
                return None;
            }
            continue;
        }
        let a = (-half_extent - coordinate) / component;
        let b = (half_extent - coordinate) / component;
        start = start.max(a.min(b));
        end = end.min(a.max(b));
    }
    (end > start + 1.0e-6).then_some((start, end))
}

fn textured_cylinder_mesh(
    surface: &SurfaceTextureSurface,
    params: &SurfaceTextureParams,
) -> Result<MeshData, String> {
    let SurfaceTextureSurface::Cylinder {
        center,
        axis,
        x_axis,
        radius,
        length,
    } = surface
    else {
        unreachable!()
    };
    let center = DVec3::from_array(*center);
    let axis = DVec3::from_array(*axis).normalize();
    let x_axis = DVec3::from_array(*x_axis).normalize();
    let z_axis = axis.cross(x_axis).normalize();
    let usable_length = length - params.margin * 2.0;
    let circumference = std::f64::consts::TAU * radius;
    let around = ((circumference / params.pitch) * 12.0)
        .ceil()
        .clamp(96.0, 960.0) as usize;
    let along = ((length / params.pitch) * 10.0).ceil().clamp(12.0, 320.0) as usize;
    let mut positions = Vec::with_capacity((along + 1) * around + 2);
    for axial_index in 0..=along {
        let t = axial_index as f64 / along as f64;
        let axial_offset = (t - 0.5) * length;
        for around_index in 0..around {
            let theta = std::f64::consts::TAU * around_index as f64 / around as f64;
            let arc = theta * radius;
            let margin_gate = if axial_offset.abs() <= usable_length * 0.5 {
                1.0
            } else {
                0.0
            };
            let groove = pattern_angles(params.pattern, params.angle_deg)
                .into_iter()
                .map(|angle| {
                    groove_profile(
                        arc - axial_offset * angle.tan(),
                        params.pitch,
                        params.groove_width,
                    )
                })
                .fold(0.0_f64, f64::max)
                * margin_gate;
            let local_radius = radius - params.depth * groove;
            let radial = x_axis * theta.cos() + z_axis * theta.sin();
            positions.push(
                (center + axis * axial_offset + radial * local_radius)
                    .as_vec3()
                    .to_array(),
            );
        }
    }
    let bottom_center = positions.len() as u32;
    positions.push((center - axis * (length * 0.5)).as_vec3().to_array());
    let top_center = positions.len() as u32;
    positions.push((center + axis * (length * 0.5)).as_vec3().to_array());
    let mut indices = Vec::with_capacity(along * around * 6 + around * 6);
    for axial_index in 0..along {
        for around_index in 0..around {
            let next_around = (around_index + 1) % around;
            let a = (axial_index * around + around_index) as u32;
            let b = (axial_index * around + next_around) as u32;
            let c = ((axial_index + 1) * around + next_around) as u32;
            let d = ((axial_index + 1) * around + around_index) as u32;
            indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }
    let top_ring = (along * around) as u32;
    for around_index in 0..around {
        let next = ((around_index + 1) % around) as u32;
        let current = around_index as u32;
        indices.extend_from_slice(&[
            bottom_center,
            next,
            current,
            top_center,
            top_ring + current,
            top_ring + next,
        ]);
    }
    let mut mesh = MeshData::new(positions, indices);
    mesh.weld(1e-5);
    Ok(mesh)
}

fn groove_profile(coordinate: f64, pitch: f64, width: f64) -> f64 {
    let distance = ((coordinate + pitch * 0.5).rem_euclid(pitch) - pitch * 0.5).abs();
    let half_width = width * 0.5;
    if distance >= half_width {
        0.0
    } else {
        // Rounded V groove: smooth at the shoulders and full depth at center.
        let normalized = 1.0 - distance / half_width.max(1.0e-9);
        normalized * normalized * (3.0 - 2.0 * normalized)
    }
}

fn oriented_box(
    center: DVec3,
    x_axis: DVec3,
    y_axis: DVec3,
    z_axis: DVec3,
    x_size: f64,
    y_size: f64,
    z_size: f64,
) -> MeshData {
    let x = x_axis.normalize() * (x_size * 0.5);
    let y = y_axis.normalize() * (y_size * 0.5);
    let z = z_axis.normalize() * (z_size * 0.5);
    let positions = [
        center - x - y - z,
        center + x - y - z,
        center + x + y - z,
        center - x + y - z,
        center - x - y + z,
        center + x - y + z,
        center + x + y + z,
        center - x + y + z,
    ]
    .into_iter()
    .map(|point| point.as_vec3().to_array())
    .collect();
    MeshData::new(
        positions,
        vec![
            0, 2, 1, 0, 3, 2, // -Z
            4, 5, 6, 4, 6, 7, // +Z
            0, 1, 5, 0, 5, 4, // -Y
            1, 2, 6, 1, 6, 5, // +X
            2, 3, 7, 2, 7, 6, // +Y
            3, 0, 4, 3, 4, 7, // -X
        ],
    )
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
    fn generated_cutters_are_closed() {
        assert_watertight(&oriented_box(
            DVec3::ZERO,
            DVec3::X,
            DVec3::Y,
            DVec3::Z,
            4.0,
            2.0,
            1.0,
        ));
    }

    #[test]
    fn planar_and_cylindrical_targets_validate() {
        let params = SurfaceTextureParams::default();
        let planar = SurfaceTextureSurface::Planar {
            origin: [0.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            x_axis: [1.0, 0.0, 0.0],
            width: 40.0,
            height: 30.0,
        };
        let cylinder = SurfaceTextureSurface::Cylinder {
            center: [0.0, 0.0, 0.0],
            axis: [0.0, 1.0, 0.0],
            x_axis: [1.0, 0.0, 0.0],
            radius: 12.0,
            length: 30.0,
        };
        assert!(validate(&planar, &params).is_ok());
        assert!(validate(&cylinder, &params).is_ok());
        assert!(!planar_cutters(&planar, &params).unwrap()[0].is_empty());
        assert!(!textured_cylinder_mesh(&cylinder, &params)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn planar_knurl_cuts_a_closed_solid() {
        use s33d_kernel::{DatumPlane, Region2D};
        let mut kernel = TruckKernel::new();
        let plane = DatumPlane::Top.def();
        let source = kernel
            .extrude_region(
                &Region2D::new(vec![[-10.0, -8.0], [10.0, -8.0], [10.0, 8.0], [-10.0, 8.0]]),
                &plane,
                4.0,
            )
            .unwrap();
        let surface = SurfaceTextureSurface::Planar {
            origin: [0.0, 4.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            x_axis: [1.0, 0.0, 0.0],
            width: 20.0,
            height: 16.0,
        };
        let params = SurfaceTextureParams {
            pattern: SurfaceTexturePattern::Diamond,
            pitch: 4.0,
            groove_width: 1.0,
            depth: 0.6,
            angle_deg: 45.0,
            margin: 1.0,
        };
        let result = build_surface_texture_shape(&mut kernel, source, &surface, &params).unwrap();
        let mesh = kernel.tessellate(result).unwrap();
        assert_watertight(&mesh);
    }

    #[test]
    fn continuous_cylinder_knurl_cuts_a_closed_solid() {
        use s33d_kernel::{DatumPlane, Region2D};
        let mut kernel = TruckKernel::new();
        let plane = DatumPlane::Top.def();
        let profile = (0..64)
            .map(|index| {
                let angle = std::f64::consts::TAU * index as f64 / 64.0;
                [10.0 * angle.cos(), 10.0 * angle.sin()]
            })
            .collect();
        let source = kernel
            .extrude_region(&Region2D::new(profile), &plane, 20.0)
            .unwrap();
        let surface = SurfaceTextureSurface::Cylinder {
            center: [0.0, 10.0, 0.0],
            axis: [0.0, 1.0, 0.0],
            x_axis: [1.0, 0.0, 0.0],
            radius: 10.0,
            length: 20.0,
        };
        let params = SurfaceTextureParams {
            pattern: SurfaceTexturePattern::Diamond,
            pitch: 8.0,
            groove_width: 1.2,
            depth: 0.8,
            angle_deg: 45.0,
            margin: 1.0,
        };
        let result = build_surface_texture_shape(&mut kernel, source, &surface, &params).unwrap();
        assert_watertight(&kernel.tessellate(result).unwrap());
    }
}
