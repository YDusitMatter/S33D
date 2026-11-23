//! Parametric wheel-rim geometry for the built-in Wheel Rim Generator.
//!
//! A closed annular barrel and hub are joined by style-specific tapered spoke
//! prisms. The center bore is intrinsic to the hub; bolt holes are applied as
//! one multi-component cutter so the final result remains a single solid.

use s33d_kernel::{DatumPlane, GeometryKernel, MeshData, Region2D, ShapeId, TruckKernel};
use s33d_model::{WheelRimParams, WheelRimStyle};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelRimMetrics {
    pub outside_radius: f64,
    pub barrel_inner_radius: f64,
    pub hub_radius: f64,
    pub spoke_count: usize,
}

pub fn metrics(params: &WheelRimParams) -> WheelRimMetrics {
    WheelRimMetrics {
        outside_radius: params.diameter * 0.5,
        barrel_inner_radius: params.diameter * 0.5 - params.rim_thickness,
        hub_radius: params.hub_diameter * 0.5,
        spoke_count: style_spokes(params.style).len(),
    }
}

pub fn validate(params: &WheelRimParams) -> Result<(), String> {
    if params.position.iter().any(|value| !value.is_finite()) {
        return Err("Position must be finite".into());
    }
    if !params.diameter.is_finite() || params.diameter <= 30.0 {
        return Err("Rim diameter must be greater than 30 mm".into());
    }
    if !params.width.is_finite() || params.width <= 4.0 {
        return Err("Rim width must be greater than 4 mm".into());
    }
    let outer = params.diameter * 0.5;
    if !params.rim_thickness.is_finite()
        || params.rim_thickness <= 1.0
        || params.rim_thickness >= outer * 0.35
    {
        return Err("Rim thickness must be between 1 mm and 35% of radius".into());
    }
    let hub = params.hub_diameter * 0.5;
    let bore = params.center_bore_diameter * 0.5;
    if !hub.is_finite() || !bore.is_finite() || bore <= 1.0 || hub <= bore + 3.0 {
        return Err("Hub diameter must leave material around the center bore".into());
    }
    if hub >= outer - params.rim_thickness - 3.0 {
        return Err("Hub must remain inside the rim barrel".into());
    }
    if !params.spoke_width.is_finite()
        || params.spoke_width <= 2.0
        || params.spoke_width >= params.diameter * 0.3
    {
        return Err("Spoke width is outside the supported range".into());
    }
    if !(3..=10).contains(&params.bolt_count) {
        return Err("Bolt count must be between 3 and 10".into());
    }
    if !params.bolt_circle_diameter.is_finite()
        || !params.bolt_hole_diameter.is_finite()
        || params.bolt_hole_diameter <= 1.0
        || params.bolt_circle_diameter <= params.center_bore_diameter + params.bolt_hole_diameter
        || params.bolt_circle_diameter + params.bolt_hole_diameter >= params.hub_diameter
    {
        return Err("Bolt circle and holes must fit inside the hub".into());
    }
    Ok(())
}

pub fn build_wheel_rim_shape(
    kernel: &mut TruckKernel,
    params: &WheelRimParams,
) -> Result<ShapeId, String> {
    validate(params)?;
    let center = params.position;
    let y0 = center[1] - params.width * 0.5;
    let y1 = center[1] + params.width * 0.5;
    let outer = params.diameter * 0.5;
    let barrel_inner = outer - params.rim_thickness;
    let barrel = kernel.insert_mesh(annular_cylinder_mesh(
        barrel_inner,
        outer,
        y0,
        y1,
        [center[0], center[2]],
        160,
    ));

    let hub_half_width = (params.width * 0.22).max(params.rim_thickness * 0.8);
    let hub = kernel.insert_mesh(annular_cylinder_mesh(
        params.center_bore_diameter * 0.5,
        params.hub_diameter * 0.5,
        center[1] - hub_half_width,
        center[1] + hub_half_width,
        [center[0], center[2]],
        128,
    ));
    let mut result = kernel
        .boolean_union(barrel, hub)
        .map_err(|error| format!("Could not join rim barrel and hub: {error}"))?;

    let spoke_half_width = (params.width * 0.12).max(params.rim_thickness * 0.55);
    let inner_radius = params.center_bore_diameter * 0.5
        + (params.hub_diameter - params.center_bore_diameter) * 0.30;
    let outer_radius = barrel_inner + params.rim_thickness * 0.55;
    let spoke_specs = style_spokes(params.style);
    for (inner_angle, outer_angle, width_scale) in spoke_specs {
        let spoke = kernel.insert_mesh(spoke_prism_mesh(
            center,
            inner_radius,
            outer_radius,
            inner_angle,
            outer_angle,
            params.spoke_width * width_scale,
            (params.spoke_width * 0.66 * width_scale).max(3.0),
            center[1] - spoke_half_width,
            center[1] + spoke_half_width,
        ));
        result = kernel
            .boolean_union(result, spoke)
            .map_err(|error| format!("Could not join a rim spoke: {error}"))?;
    }

    let bolt_radius = params.bolt_circle_diameter * 0.5;
    let hole_radius = params.bolt_hole_diameter * 0.5;
    let cutter_margin = 2.0_f64.max(params.width * 0.02);
    for index in 0..params.bolt_count {
        let angle = std::f64::consts::TAU * index as f64 / params.bolt_count as f64;
        let hole_center = [
            center[0] + bolt_radius * angle.cos(),
            center[2] + bolt_radius * angle.sin(),
        ];
        let cutter_y0 = center[1] - hub_half_width - cutter_margin;
        let cutter_y1 = center[1] + hub_half_width + cutter_margin;
        let mut plane = DatumPlane::Top.def();
        plane.origin = [hole_center[0], cutter_y0, hole_center[1]];
        let profile = (0..48)
            .map(|segment| {
                let angle = std::f64::consts::TAU * segment as f64 / 48.0;
                [hole_radius * angle.cos(), hole_radius * angle.sin()]
            })
            .collect();
        let cutter = kernel
            .extrude_region(&Region2D::new(profile), &plane, cutter_y1 - cutter_y0)
            .map_err(|error| format!("Could not build rim bolt hole {}: {error}", index + 1))?;
        result = kernel
            .boolean_subtract(result, cutter)
            .map_err(|error| format!("Could not cut rim bolt hole {}: {error}", index + 1))?;
    }
    Ok(result)
}

/// Returns `(inner angle, outer angle, width scale)` for each spoke. Split and
/// multi-spoke styles fan the outer endpoint to create a visibly different,
/// still deterministic wheel family.
fn style_spokes(style: WheelRimStyle) -> Vec<(f64, f64, f64)> {
    let (groups, split, width_scale) = match style {
        WheelRimStyle::FiveSpoke => (5usize, false, 1.30),
        WheelRimStyle::SixSpoke => (6usize, false, 1.05),
        WheelRimStyle::SplitFive => (5usize, true, 0.62),
        WheelRimStyle::MultiSpoke => (12usize, false, 0.52),
    };
    let mut specs = Vec::with_capacity(if split { groups * 2 } else { groups });
    for group in 0..groups {
        let center = std::f64::consts::TAU * group as f64 / groups as f64;
        if split {
            // Keep paired roots topologically separate inside the hub. Very
            // small root offsets make the two tapered prisms overlap almost
            // coplanarly, which is visually redundant and fragile in CSG.
            let inner_offset = 0.20;
            let outer_offset = 0.10;
            specs.push((center - inner_offset, center - outer_offset, width_scale));
            specs.push((center + inner_offset, center + outer_offset, width_scale));
        } else {
            specs.push((center, center, width_scale));
        }
    }
    specs
}

fn annular_cylinder_mesh(
    inner_radius: f64,
    outer_radius: f64,
    y0: f64,
    y1: f64,
    center: [f64; 2],
    segments: usize,
) -> MeshData {
    let mut positions = Vec::with_capacity(segments * 4);
    for index in 0..segments {
        let angle = std::f64::consts::TAU * index as f64 / segments as f64;
        let (sin, cos) = angle.sin_cos();
        positions.extend_from_slice(&[
            [
                (center[0] + outer_radius * cos) as f32,
                y0 as f32,
                (center[1] + outer_radius * sin) as f32,
            ],
            [
                (center[0] + outer_radius * cos) as f32,
                y1 as f32,
                (center[1] + outer_radius * sin) as f32,
            ],
            [
                (center[0] + inner_radius * cos) as f32,
                y0 as f32,
                (center[1] + inner_radius * sin) as f32,
            ],
            [
                (center[0] + inner_radius * cos) as f32,
                y1 as f32,
                (center[1] + inner_radius * sin) as f32,
            ],
        ]);
    }
    let mut indices = Vec::with_capacity(segments * 24);
    for index in 0..segments {
        let next = (index + 1) % segments;
        let ob = (index * 4) as u32;
        let ot = ob + 1;
        let ib = ob + 2;
        let it = ob + 3;
        let nob = (next * 4) as u32;
        let not = nob + 1;
        let nib = nob + 2;
        let nit = nob + 3;
        indices.extend_from_slice(&[
            ob, ot, not, ob, not, nob, // outer wall
            ib, nib, nit, ib, nit, it, // inner wall
            ob, nob, nib, ob, nib, ib, // back annulus
            ot, it, nit, ot, nit, not, // front annulus
        ]);
    }
    MeshData::new(positions, indices)
}

#[allow(clippy::too_many_arguments)]
fn spoke_prism_mesh(
    center: [f64; 3],
    inner_radius: f64,
    outer_radius: f64,
    inner_angle: f64,
    outer_angle: f64,
    inner_width: f64,
    outer_width: f64,
    y0: f64,
    y1: f64,
) -> MeshData {
    let corner = |radius: f64, angle: f64, half_width: f64, side: f64, y: f64| {
        let radial = glam::DVec2::new(angle.cos(), angle.sin());
        let tangent = glam::DVec2::new(-angle.sin(), angle.cos());
        let point =
            glam::DVec2::new(center[0], center[2]) + radial * radius + tangent * half_width * side;
        [point.x as f32, y as f32, point.y as f32]
    };
    let profile = [
        (inner_radius, inner_angle, inner_width * 0.5, -1.0),
        (outer_radius, outer_angle, outer_width * 0.5, -1.0),
        (outer_radius, outer_angle, outer_width * 0.5, 1.0),
        (inner_radius, inner_angle, inner_width * 0.5, 1.0),
    ];
    let mut positions = Vec::with_capacity(8);
    for &(radius, angle, half_width, side) in &profile {
        positions.push(corner(radius, angle, half_width, side, y0));
    }
    for &(radius, angle, half_width, side) in &profile {
        positions.push(corner(radius, angle, half_width, side, y1));
    }
    let mut indices = vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7];
    for index in 0..4u32 {
        let next = (index + 1) % 4;
        indices.extend_from_slice(&[index, next, 4 + next, index, 4 + next, 4 + index]);
    }
    MeshData::new(positions, indices)
}

fn cylinder_y_mesh(radius: f64, y0: f64, y1: f64, center: [f64; 2], segments: usize) -> MeshData {
    let mut positions = Vec::with_capacity(segments * 2 + 2);
    for index in 0..segments {
        let angle = std::f64::consts::TAU * index as f64 / segments as f64;
        positions.push([
            (center[0] + radius * angle.cos()) as f32,
            y0 as f32,
            (center[1] + radius * angle.sin()) as f32,
        ]);
        positions.push([
            (center[0] + radius * angle.cos()) as f32,
            y1 as f32,
            (center[1] + radius * angle.sin()) as f32,
        ]);
    }
    let bottom_center = positions.len() as u32;
    positions.push([center[0] as f32, y0 as f32, center[1] as f32]);
    let top_center = positions.len() as u32;
    positions.push([center[0] as f32, y1 as f32, center[1] as f32]);
    let mut indices = Vec::with_capacity(segments * 12);
    for index in 0..segments {
        let next = (index + 1) % segments;
        let bottom = (index * 2) as u32;
        let top = bottom + 1;
        let next_bottom = (next * 2) as u32;
        let next_top = next_bottom + 1;
        indices.extend_from_slice(&[
            bottom,
            top,
            next_top,
            bottom,
            next_top,
            next_bottom,
            bottom_center,
            next_bottom,
            bottom,
            top_center,
            top,
            next_top,
        ]);
    }
    MeshData::new(positions, indices)
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
    fn component_meshes_are_closed() {
        assert_watertight(&annular_cylinder_mesh(8.0, 10.0, -2.0, 2.0, [0.0, 0.0], 32));
        assert_watertight(&spoke_prism_mesh(
            [0.0, 0.0, 0.0],
            4.0,
            9.0,
            0.0,
            0.1,
            3.0,
            2.0,
            -1.0,
            1.0,
        ));
        assert_watertight(&cylinder_y_mesh(1.0, -2.0, 2.0, [3.0, 0.0], 24));
    }

    #[test]
    fn every_wheel_style_builds_as_one_closed_result() {
        for style in WheelRimStyle::ALL {
            let params = WheelRimParams {
                diameter: 120.0,
                width: 40.0,
                rim_thickness: 8.0,
                hub_diameter: 44.0,
                center_bore_diameter: 16.0,
                spoke_width: 9.0,
                style,
                bolt_count: 5,
                bolt_circle_diameter: 30.0,
                bolt_hole_diameter: 4.0,
                position: [0.0, 0.0, 0.0],
            };
            let mut kernel = TruckKernel::new();
            let shape = build_wheel_rim_shape(&mut kernel, &params)
                .unwrap_or_else(|error| panic!("{} failed: {error}", style.name()));
            let mesh = kernel.tessellate(shape).unwrap();
            assert_watertight(&mesh);
            let (min, max) = mesh.bounds().unwrap();
            assert!((max.y - min.y - params.width as f32).abs() < 0.1);
        }
    }
}
