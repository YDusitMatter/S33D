//! Procedural surface knurling for the built-in Surface Texture plug-in.
//!
//! The operation is intentionally non-destructive at document level: the
//! source body remains in the model tree (hidden), while this module cuts a
//! generated set of shallow grooves into a new result shape.  Planar patches
//! use clipped straight grooves. Cylindrical cuts have a periodic angular
//! pitch and preserve the source beyond the selected band.

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
            if !radius.is_finite()
                || !length.is_finite()
                || *radius <= params.depth
                || *length <= params.margin * 2.0 + params.pitch
            {
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
    if !axis.is_finite()
        || !x_axis.is_finite()
        || axis.length_squared() < 0.99
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
    let mut cutter_meshes = match surface {
        SurfaceTextureSurface::Planar { .. } => planar_cutters(surface, params)?,
        SurfaceTextureSurface::Cylinder { .. } => cylindrical_cutters(surface, params)?,
    };
    if matches!(surface, SurfaceTextureSurface::Cylinder { .. }) && cutter_meshes.len() == 2 {
        // Merge the two hands before cutting the wall. Cutting them in
        // sequence can leave degenerate triangles at their intersections.
        let right = kernel.insert_mesh(cutter_meshes.pop().unwrap());
        let left = kernel.insert_mesh(cutter_meshes.pop().unwrap());
        let combined = kernel.boolean_union(left, right);
        kernel.delete(left);
        kernel.delete(right);
        let combined =
            combined.map_err(|error| format!("Could not join diamond grooves: {error}"))?;
        let result = kernel.boolean_subtract(source, combined);
        kernel.delete(combined);
        return result.map_err(|error| format!("Could not cut diamond grooves: {error}"));
    }
    let mut result = source;
    for (index, cutters) in cutter_meshes.into_iter().enumerate() {
        if cutters.is_empty() {
            continue;
        }
        let cutter = kernel.insert_mesh(cutters);
        let next = kernel.boolean_subtract(result, cutter);
        kernel.delete(cutter);
        if result != source {
            kernel.delete(result);
        }
        result = next.map_err(|error| {
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

fn cylindrical_cutters(
    surface: &SurfaceTextureSurface,
    params: &SurfaceTextureParams,
) -> Result<Vec<MeshData>, String> {
    let SurfaceTextureSurface::Cylinder {
        center,
        axis,
        x_axis,
        radius,
        length,
        internal,
    } = *surface
    else {
        unreachable!()
    };
    let center = DVec3::from_array(center);
    let axis = DVec3::from_array(axis).normalize();
    let x = (DVec3::from_array(x_axis) - axis * axis.dot(DVec3::from_array(x_axis))).normalize();
    let y = axis.cross(x);
    let half_length = length * 0.5 - params.margin;
    let circumference = std::f64::consts::TAU * radius;
    // Fit an integral number of starts to the circumference. Each groove is
    // swept through its unwrapped angle, including crossings of zero radians.
    let starts = (circumference / params.pitch).round().max(1.0) as usize;
    if params.groove_width >= 0.9 * circumference / starts as f64 {
        return Err("Groove width is too large for the fitted cylindrical pitch".into());
    }
    let angular_step = (0.4 * params.depth / radius).sqrt().clamp(0.004, 0.08);
    let across = ((params.groove_width / (radius * angular_step)).ceil() as usize)
        .max(10)
        .next_multiple_of(2);
    let clearance = (radius * (1.0 - (angular_step * 0.5).cos()) + 0.002).min(radius * 0.25);
    let bore_overlap = (2.0 * clearance).max(radius * 0.02);
    let quad = |indices: &mut Vec<u32>, a, b, c, d| indices.extend_from_slice(&[a, b, c, a, c, d]);
    let mut groups = Vec::new();
    for angle in pattern_angles(params.pattern, params.angle_deg) {
        let twist = angle.tan() / radius;
        let along = ((2.0 * half_length * twist.abs() / angular_step).ceil() as usize).max(1);
        if starts
            .saturating_mul(along.saturating_add(1))
            .saturating_mul(across + 1)
            > 45_000
        {
            return Err(
                "Cylindrical texture is too dense; increase the pitch or narrow the textured band"
                    .into(),
            );
        }
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for groove in 0..starts {
            let phase = std::f64::consts::TAU * groove as f64 / starts as f64;
            let base = positions.len() as u32;
            let row = ((across + 1) * 2) as u32;
            for j in 0..=along {
                let z = (2.0 * j as f64 / along as f64 - 1.0) * half_length;
                for i in 0..=across {
                    let offset = (i as f64 / across as f64 - 0.5) * params.groove_width;
                    let theta = phase + z * twist + offset / radius;
                    let direction = x * theta.cos() + y * theta.sin();
                    let relief = groove_profile(offset, params.pitch, params.groove_width);
                    let penetration = params.depth * relief - clearance * (1.0 - relief);
                    let radii = if internal {
                        [radius - bore_overlap, radius + penetration]
                    } else {
                        [radius - penetration, radius + 2.0 * clearance]
                    };
                    positions.extend(
                        radii.map(|r| (center + axis * z + direction * r).as_vec3().to_array()),
                    );
                }
            }
            for j in 0..along {
                for i in 0..across {
                    let a = base + j as u32 * row + i as u32 * 2;
                    let b = a + 2;
                    let c = b + row;
                    let d = a + row;
                    quad(&mut indices, a + 1, b + 1, c + 1, d + 1);
                    quad(&mut indices, a, d, c, b);
                    if j == 0 {
                        quad(&mut indices, a, b, b + 1, a + 1);
                    }
                    if j + 1 == along {
                        quad(&mut indices, d, d + 1, c + 1, c);
                    }
                    if i == 0 {
                        quad(&mut indices, a, a + 1, d + 1, d);
                    }
                    if i + 1 == across {
                        quad(&mut indices, b, c, c + 1, b + 1);
                    }
                }
            }
        }
        groups.push(MeshData::new(positions, indices));
    }
    Ok(groups)
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
            internal: false,
        };
        assert!(validate(&planar, &params).is_ok());
        assert!(validate(&cylinder, &params).is_ok());
        assert!(!planar_cutters(&planar, &params).unwrap()[0].is_empty());
        assert!(cylindrical_cutters(&cylinder, &params)
            .unwrap()
            .iter()
            .all(|mesh| !mesh.is_empty()));
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
            internal: false,
        };
        let params = SurfaceTextureParams {
            pattern: SurfaceTexturePattern::Diamond,
            pitch: 8.0,
            groove_width: 1.2,
            depth: 0.8,
            angle_deg: 45.0,
            margin: 1.0,
        };
        for cutter in cylindrical_cutters(&surface, &params).unwrap() {
            assert_watertight(&cutter);
        }
        let original = kernel.tessellate(source).unwrap();
        let result = build_surface_texture_shape(&mut kernel, source, &surface, &params).unwrap();
        let mesh = kernel.tessellate(result).unwrap();
        assert_watertight(&mesh);
        assert_eq!(original.bounds(), mesh.bounds());
        assert!(
            crate::ui::mesh_surface_area_and_volume(&mesh).1
                < crate::ui::mesh_surface_area_and_volume(&original).1
        );
    }

    #[test]
    fn cylindrical_bore_texture_cuts_the_inner_wall_and_retains_the_sleeve() {
        use s33d_kernel::{DatumPlane, Region2D};
        let mut kernel = TruckKernel::new();
        let circle = |radius| {
            (0..64).map(move |index| {
                let angle = std::f64::consts::TAU * index as f64 / 64.0;
                [radius * angle.cos(), radius * angle.sin()]
            })
        };
        let sleeve = Region2D {
            outer: circle(15.0).collect(),
            holes: vec![circle(8.0).rev().collect()],
        };
        let source = kernel
            .extrude_region(&sleeve, &DatumPlane::Top.def(), 14.0)
            .unwrap();
        let surface = SurfaceTextureSurface::Cylinder {
            center: [0.0, 7.0, 0.0],
            axis: [0.0, 1.0, 0.0],
            x_axis: [1.0, 0.0, 0.0],
            radius: 8.0,
            length: 14.0,
            internal: true,
        };
        let params = SurfaceTextureParams {
            pattern: SurfaceTexturePattern::Straight,
            pitch: 6.0,
            groove_width: 1.2,
            depth: 0.8,
            angle_deg: 45.0,
            margin: 1.0,
        };
        let result = build_surface_texture_shape(&mut kernel, source, &surface, &params).unwrap();
        let original = kernel.tessellate(source).unwrap();
        let mesh = kernel.tessellate(result).unwrap();
        assert_watertight(&mesh);
        assert_eq!(original.bounds(), mesh.bounds());
        assert!(
            crate::ui::mesh_surface_area_and_volume(&original).1
                - crate::ui::mesh_surface_area_and_volume(&mesh).1
                > 10.0
        );
    }
}
