use super::*;
use std::io::Write;

fn zip(parts: &[(&str, &[u8])], compressed: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut central = Vec::new();
    for &(name, data) in parts {
        let data_out = if compressed {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(data).unwrap();
            encoder.finish().unwrap()
        } else {
            data.to_vec()
        };
        let method = if compressed { 8u16 } else { 0 };
        let offset = bytes.len() as u32;
        bytes.extend(0x04034b50u32.to_le_bytes());
        bytes.extend(20u16.to_le_bytes());
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(method.to_le_bytes());
        bytes.extend([0; 4]);
        bytes.extend(crc32fast::hash(data).to_le_bytes());
        bytes.extend((data_out.len() as u32).to_le_bytes());
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend((name.len() as u16).to_le_bytes());
        bytes.extend([0; 2]);
        bytes.extend(name.as_bytes());
        bytes.extend(data_out);
        central.extend(0x02014b50u32.to_le_bytes());
        central.extend([20, 0, 20, 0, 0, 0]);
        central.extend(method.to_le_bytes());
        central.extend([0; 4]);
        central.extend(crc32fast::hash(data).to_le_bytes());
        central.extend(((bytes.len() - offset as usize - 30 - name.len()) as u32).to_le_bytes());
        central.extend((data.len() as u32).to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes());
        central.extend([0; 12]);
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let offset = bytes.len() as u32;
    let size = central.len() as u32;
    bytes.extend(central);
    bytes.extend(0x06054b50u32.to_le_bytes());
    bytes.extend([0; 4]);
    bytes.extend((parts.len() as u16).to_le_bytes());
    bytes.extend((parts.len() as u16).to_le_bytes());
    bytes.extend(size.to_le_bytes());
    bytes.extend(offset.to_le_bytes());
    bytes.extend([0; 2]);
    bytes
}
const REL: &[u8] = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="r1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel" Target="/3D/main.model"/></Relationships>"#;
const TETRA: &str = r#"<object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/><vertex x="0" y="0" z="1"/></vertices><triangles><triangle v1="0" v2="2" v3="1"/><triangle v1="0" v2="1" v3="3"/><triangle v1="0" v2="3" v3="2"/><triangle v1="1" v2="2" v3="3"/></triangles></mesh></object>"#;
fn model(unit: &str, resources: &str, build: &str) -> String {
    format!(
        r#"<model xmlns="{}" unit="{unit}"><resources>{resources}</resources><build>{build}</build></model>"#,
        std::str::from_utf8(CORE).unwrap()
    )
}
fn package(xml: &str, compressed: bool) -> Vec<u8> {
    zip(
        &[("_rels/.rels", REL), ("3D/main.model", xml.as_bytes())],
        compressed,
    )
}
fn signed_volume(mesh: &MeshData) -> f64 {
    mesh.indices
        .chunks_exact(3)
        .map(|t| {
            let p = [t[0], t[1], t[2]]
                .map(|i| DVec3::from_array(mesh.positions[i as usize].map(f64::from)));
            p[0].dot(p[1].cross(p[2])) / 6.
        })
        .sum()
}

#[test]
fn three_mf_build_components_units_transforms_and_mirror() {
    let resources = format!(
        r#"{TETRA}<object id="2"><components><component objectid="1" transform="1 0 0 0 1 0 0 0 1 2 3 4"/></components></object>"#
    );
    let xml = model(
        "inch",
        &resources,
        r#"<item objectid="2" transform="0 1 0 -1 0 0 0 0 1 10 0 0"/><item objectid="1" transform="-1 0 0 0 1 0 0 0 1 20 0 0"/>"#,
    );
    for compressed in [false, true] {
        let meshes = decode(package(&xml, compressed)).unwrap();
        assert_eq!(
            meshes.len(),
            2,
            "unbuilt resources must not become extra objects"
        );
        let p = meshes[0].positions[0];
        for (actual, expected) in p.into_iter().zip([7. * 25.4, 4. * 25.4, -2. * 25.4]) {
            assert!((actual - expected).abs() < 1e-4);
        }
        assert!(signed_volume(&meshes[0]) > 0.);
        assert!(signed_volume(&meshes[1]) > 0.);
    }
    for (unit, scale) in [
        ("micron", 0.001),
        ("millimeter", 1.),
        ("centimeter", 10.),
        ("meter", 1000.),
        ("foot", 304.8),
    ] {
        let m = decode(package(
            &model(unit, TETRA, r#"<item objectid="1"/>"#),
            true,
        ))
        .unwrap();
        assert!((m[0].positions[1][0] - scale).abs() < 1e-6);
    }
}

#[test]
fn three_mf_production_cross_part_and_namespaced_core() {
    let xml = format!(
        r#"<m:model xmlns:m="{}" xmlns:p="{PRODUCTION}" requiredextensions="p"><m:resources><m:object id="5"><m:components><m:component p:path="/3D/parts/body.model" objectid="1"/></m:components></m:object></m:resources><m:build><m:item objectid="5"/></m:build></m:model>"#,
        std::str::from_utf8(CORE).unwrap()
    );
    let child = model("millimeter", TETRA, "");
    let parts = [
        ("_rels/.rels", REL),
        ("3D/main.model", xml.as_bytes()),
        ("3D/parts/body.model", child.as_bytes()),
    ];
    let meshes = decode(zip(&parts, true)).unwrap();
    assert_eq!(meshes.len(), 1);
    assert_eq!(meshes[0].triangle_count(), 4);
}

#[test]
fn three_mf_rejects_invalid_geometry_cycles_and_required_extensions() {
    for xml in [
        model("millimeter", TETRA, r#"<item objectid="999"/>"#),
        model(
            "millimeter",
            &TETRA.replace("v3=\"3\"", "v3=\"9\""),
            r#"<item objectid="1"/>"#,
        ),
        model(
            "millimeter",
            &TETRA.replace("x=\"1\"", "x=\"NaN\""),
            r#"<item objectid="1"/>"#,
        ),
        model(
            "millimeter",
            TETRA,
            r#"<item objectid="1" transform="0 0 0 0 0 0 0 0 0 0 0 0"/>"#,
        ),
        model(
            "millimeter",
            r#"<object id="1"><components><component objectid="2"/></components></object><object id="2"><components><component objectid="1"/></components></object>"#,
            r#"<item objectid="1"/>"#,
        ),
        model("millimeter", TETRA, "").replace("<model ", "<model requiredextensions=\"unknown\" "),
        model("millimeter", TETRA, "")
            .replace("<model ", "<!DOCTYPE model [<!ENTITY a 'b'>]><model "),
        model("kilometer", TETRA, r#"<item objectid="1"/>"#),
    ] {
        assert!(decode(package(&xml, true)).is_err(), "{xml}");
    }
}

#[test]
fn three_mf_rejects_corrupt_zip_duplicate_parts_paths_and_expansion() {
    let xml = model("millimeter", TETRA, r#"<item objectid="1"/>"#);
    let mut corrupt = package(&xml, false);
    corrupt[45] ^= 1;
    assert!(decode(corrupt).is_err());
    let mut truncated = package(&xml, true);
    truncated.truncate(truncated.len() - 8);
    assert!(decode(truncated).is_err());
    assert!(Package::new(zip(&[("a", b"1"), ("a", b"2")], false)).is_err());
    assert!(Package::new(zip(&[("../../bad", b"1")], false)).is_err());
    let mut oversized = package(&xml, true);
    let central = oversized
        .windows(4)
        .position(|p| p == b"PK\x01\x02")
        .unwrap();
    oversized[central + 24..central + 28].copy_from_slice(&((MAX_XML + 1) as u32).to_le_bytes());
    assert!(decode(oversized).is_err());
    let resources = format!(
        r#"{TETRA}<object id="2"><components><component objectid="1"/><component objectid="1"/></components></object>"#
    );
    let m = decode(package(
        &model("millimeter", &resources, r#"<item objectid="2"/>"#),
        true,
    ))
    .unwrap();
    assert_eq!(
        m[0].triangle_count(),
        8,
        "DAG reuse is allowed; only recursion cycles fail"
    );
}
