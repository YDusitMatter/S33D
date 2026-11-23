//! 网格文件 IO：STL（二进制+ASCII）、OBJ、PLY（ASCII+二进制）、GLB/glTF。
//!
//! STL/OBJ/PLY 为自实现（格式简单、零依赖）；
//! GLB/glTF 读取用 `gltf` crate，写出为手写最小 GLB 容器。

use crate::mesh::MeshData;
use crate::{KernelError, Result};
use std::collections::HashMap;
use std::io::Write;

// ==================== 工具 ====================

/// 顶点焊接（按容差合并重复顶点），返回 (positions, indices)。
fn weld(positions_in: &[[f32; 3]], indices_in: &[u32], tol: f32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut map: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices = Vec::with_capacity(indices_in.len());
    for &i in indices_in {
        let p = positions_in[i as usize];
        let key = (
            (p[0] / tol).round() as i64,
            (p[1] / tol).round() as i64,
            (p[2] / tol).round() as i64,
        );
        let id = if let Some(&id) = map.get(&key) {
            id
        } else {
            let id = positions.len() as u32;
            map.insert(key, id);
            positions.push(p);
            id
        };
        indices.push(id);
    }
    (positions, indices)
}

// ==================== STL ====================

fn read_stl_binary(bytes: &[u8]) -> Result<MeshData> {
    let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    if bytes.len() < 84 + n * 50 {
        return Err(KernelError::Geometry("truncated binary STL".into()));
    }
    let mut positions = Vec::with_capacity(n * 3);
    let mut indices = Vec::with_capacity(n * 3);
    for i in 0..n {
        let base = 84 + i * 50;
        for v in 0..3 {
            let off = base + 12 + v * 12;
            let x = f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap());
            let y = f32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap());
            let z = f32::from_le_bytes(bytes[off + 8..off + 12].try_into().unwrap());
            positions.push([x, y, z]);
            indices.push((i * 3 + v) as u32);
        }
    }
    let (positions, indices) = weld(&positions, &indices, 1e-5);
    Ok(MeshData::new(positions, indices))
}

fn read_stl_ascii(bytes: &[u8]) -> Result<MeshData> {
    let text = String::from_utf8_lossy(bytes);
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut tri: Vec<[f32; 3]> = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("vertex") => {
                let coords: Vec<f32> = it.filter_map(|s| s.parse().ok()).collect();
                if coords.len() == 3 {
                    tri.push([coords[0], coords[1], coords[2]]);
                    if tri.len() == 3 {
                        positions.extend_from_slice(&tri);
                        tri.clear();
                    }
                }
            }
            Some("endsolid") | Some("end") => break,
            _ => {}
        }
    }
    let indices: Vec<u32> = (0..positions.len() as u32).collect();
    let (positions, indices) = weld(&positions, &indices, 1e-5);
    Ok(MeshData::new(positions, indices))
}

pub fn read_stl(path: &str) -> Result<Vec<MeshData>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() >= 84 {
        let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        if bytes.len() == 84 + n * 50 {
            return Ok(vec![read_stl_binary(&bytes)?]);
        }
    }
    Ok(vec![read_stl_ascii(&bytes)?])
}

pub fn write_stl(mesh: &MeshData, path: &str) -> Result<()> {
    crate::faceted::validate_mesh(mesh)?;
    let stage = crate::export_file::StagedExport::new(std::path::Path::new(path))?;
    write_stl_staged(
        mesh,
        stage
            .path()
            .to_str()
            .ok_or_else(|| crate::KernelError::Geometry("Invalid export path".into()))?,
    )?;
    stage.commit()?;
    Ok(())
}

fn write_stl_staged(mesh: &MeshData, path: &str) -> Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut header = [0u8; 80];
    header[..11].copy_from_slice(b"S33D export");
    f.write_all(&header)?;
    let tri_count = mesh.triangle_count() as u32;
    f.write_all(&tri_count.to_le_bytes())?;
    for t in mesh.indices.chunks_exact(3) {
        let a = mesh.positions[t[0] as usize];
        let b = mesh.positions[t[1] as usize];
        let c = mesh.positions[t[2] as usize];
        let n = face_normal(a, b, c);
        for v in [n, a, b, c] {
            for x in v {
                f.write_all(&x.to_le_bytes())?;
            }
        }
        f.write_all(&0u16.to_le_bytes())?;
    }
    f.flush()?;
    Ok(())
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
    [n[0] / l, n[1] / l, n[2] / l]
}

// ==================== OBJ ====================

pub fn read_obj(path: &str) -> Result<Vec<MeshData>> {
    let text = std::fs::read_to_string(path)?;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let coords: Vec<f32> = it.filter_map(|s| s.parse().ok()).collect();
                if coords.len() >= 3 {
                    positions.push([coords[0], coords[1], coords[2]]);
                }
            }
            Some("f") => {
                // 面顶点：i[/vt/vn]（1-based，可为负 = 相对末尾）
                let face: Vec<u32> = it
                    .map(|tok| {
                        let idx = tok.split('/').next().unwrap_or("0");
                        let i: i64 = idx.parse().unwrap_or(0);
                        if i > 0 {
                            (i - 1) as u32
                        } else if i < 0 {
                            (positions.len() as i64 + i) as u32
                        } else {
                            0
                        }
                    })
                    .collect();
                // 扇形三角化
                for k in 1..face.len().saturating_sub(1) {
                    indices.push(face[0]);
                    indices.push(face[k]);
                    indices.push(face[k + 1]);
                }
            }
            _ => {}
        }
    }
    Ok(vec![MeshData::new(positions, indices)])
}

pub fn write_obj(mesh: &MeshData, path: &str) -> Result<()> {
    crate::faceted::validate_mesh(mesh)?;
    let stage = crate::export_file::StagedExport::new(std::path::Path::new(path))?;
    write_obj_staged(
        mesh,
        stage
            .path()
            .to_str()
            .ok_or_else(|| crate::KernelError::Geometry("Invalid export path".into()))?,
    )?;
    stage.commit()?;
    Ok(())
}

fn write_obj_staged(mesh: &MeshData, path: &str) -> Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(f, "# S33D export")?;
    for p in &mesh.positions {
        writeln!(f, "v {} {} {}", p[0], p[1], p[2])?;
    }
    for t in mesh.indices.chunks_exact(3) {
        writeln!(f, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1)?;
    }
    f.flush()?;
    Ok(())
}

// ==================== PLY ====================

pub fn read_ply(path: &str) -> Result<Vec<MeshData>> {
    let bytes = std::fs::read(path)?;
    // 解析 header
    let header_end = find_header_end(&bytes)?;
    let header = String::from_utf8_lossy(&bytes[..header_end]);
    let mut n_vertices = 0usize;
    let mut n_faces = 0usize;
    let mut binary = false;
    let mut big_endian = false;
    let mut vertex_props: Vec<String> = Vec::new();
    let mut in_vertex = false;
    let mut in_face = false;
    for line in header.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("format") => {
                binary = it.next().unwrap_or("ascii") != "ascii";
                big_endian = it.next().unwrap_or("little") == "big_endian";
            }
            Some("element") => {
                let name = it.next().unwrap_or("");
                let count: usize = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                in_vertex = name == "vertex";
                in_face = name == "face";
                if in_vertex {
                    n_vertices = count;
                } else if in_face {
                    n_faces = count;
                }
            }
            Some("property") => {
                if in_vertex {
                    // 记录顺序：x y z 之外跳过
                    if let Some(name) = it.last() {
                        vertex_props.push(name.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    if n_vertices == 0 {
        return Ok(vec![MeshData::default()]);
    }
    let data = &bytes[header_end..];
    if binary {
        read_ply_binary(data, n_vertices, n_faces, big_endian, &vertex_props)
    } else {
        read_ply_ascii(data, n_vertices)
    }
}

fn find_header_end(bytes: &[u8]) -> Result<usize> {
    let mut i = 0;
    while i + 11 <= bytes.len() {
        if &bytes[i..i + 11] == b"end_header\n" || &bytes[i..i + 10] == b"end_header" {
            return Ok(if bytes[i + 10] == b'\n' {
                i + 11
            } else {
                i + 10
            });
        }
        i += 1;
    }
    Err(KernelError::Geometry("invalid PLY: no end_header".into()))
}

fn read_ply_ascii(data: &[u8], n_vertices: usize) -> Result<Vec<MeshData>> {
    let text = String::from_utf8_lossy(data);
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut lines = text.lines();
    // vertices
    for _ in 0..n_vertices {
        let Some(line) = lines.next() else { break };
        let coords: Vec<f32> = line
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if coords.len() >= 3 {
            positions.push([coords[0], coords[1], coords[2]]);
        }
    }
    // faces: "count i j k ..."
    for line in lines {
        let nums: Vec<i64> = line
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if nums.is_empty() {
            continue;
        }
        let count = nums[0] as usize;
        let face: Vec<u32> = nums[1..]
            .iter()
            .take(count)
            .map(|&i| i.max(0) as u32)
            .collect();
        for k in 1..face.len().saturating_sub(1) {
            indices.push(face[0]);
            indices.push(face[k]);
            indices.push(face[k + 1]);
        }
    }
    Ok(vec![MeshData::new(positions, indices)])
}

fn read_ply_binary(
    data: &[u8],
    n_vertices: usize,
    n_faces: usize,
    big_endian: bool,
    vertex_props: &[String],
) -> Result<Vec<MeshData>> {
    // 属性布局：默认 x y z 为 float32（也兼容 double）。仅取前 3 个 float 属性。
    let read_f = |off: usize| -> f32 {
        if big_endian {
            f32::from_be_bytes(data[off..off + 4].try_into().unwrap())
        } else {
            f32::from_le_bytes(data[off..off + 4].try_into().unwrap())
        }
    };
    let stride = vertex_props.len().max(3) * 4;
    let mut positions = Vec::with_capacity(n_vertices);
    for i in 0..n_vertices {
        let base = i * stride;
        positions.push([read_f(base), read_f(base + 4), read_f(base + 8)]);
    }
    // faces：uchar count + int list（简化：假定 u8 计数 + i32 索引）
    let mut indices: Vec<u32> = Vec::new();
    let mut off = n_vertices * stride;
    for _ in 0..n_faces {
        if off >= data.len() {
            break;
        }
        let count = data[off] as usize;
        off += 1;
        let mut face = Vec::with_capacity(count);
        for _ in 0..count {
            if off + 4 > data.len() {
                break;
            }
            let idx = if big_endian {
                i32::from_be_bytes(data[off..off + 4].try_into().unwrap())
            } else {
                i32::from_le_bytes(data[off..off + 4].try_into().unwrap())
            };
            face.push(idx.max(0) as u32);
            off += 4;
        }
        for k in 1..face.len().saturating_sub(1) {
            indices.push(face[0]);
            indices.push(face[k]);
            indices.push(face[k + 1]);
        }
    }
    Ok(vec![MeshData::new(positions, indices)])
}

pub fn write_ply(mesh: &MeshData, path: &str) -> Result<()> {
    crate::faceted::validate_mesh(mesh)?;
    let stage = crate::export_file::StagedExport::new(std::path::Path::new(path))?;
    write_ply_staged(
        mesh,
        stage
            .path()
            .to_str()
            .ok_or_else(|| crate::KernelError::Geometry("Invalid export path".into()))?,
    )?;
    stage.commit()?;
    Ok(())
}

fn write_ply_staged(mesh: &MeshData, path: &str) -> Result<()> {
    // 二进制 little endian
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(f, "ply")?;
    writeln!(f, "format binary_little_endian 1.0")?;
    writeln!(f, "comment S33D export")?;
    writeln!(f, "element vertex {}", mesh.positions.len())?;
    writeln!(f, "property float x")?;
    writeln!(f, "property float y")?;
    writeln!(f, "property float z")?;
    writeln!(f, "element face {}", mesh.triangle_count())?;
    writeln!(f, "property list uchar int vertex_indices")?;
    writeln!(f, "end_header")?;
    for p in &mesh.positions {
        for x in p {
            f.write_all(&x.to_le_bytes())?;
        }
    }
    for t in mesh.indices.chunks_exact(3) {
        f.write_all(&[3u8])?;
        for &i in t {
            f.write_all(&(i as i32).to_le_bytes())?;
        }
    }
    f.flush()?;
    Ok(())
}

// ==================== GLB / glTF ====================

pub fn read_glb(path: &str) -> Result<Vec<MeshData>> {
    let bytes = std::fs::read(path)?;
    let (document, buffers, _) =
        gltf::import_slice(&bytes).map_err(|e| KernelError::Step(format!("gltf import: {e}")))?;
    let mut out = Vec::new();
    for mesh in document.meshes() {
        for prim in mesh.primitives() {
            let reader = prim.reader(|buffer| Some(&buffers[buffer.index()]));
            let Some(positions) = reader.read_positions() else {
                continue;
            };
            let positions: Vec<[f32; 3]> = positions.collect();
            let indices: Vec<u32> = reader
                .read_indices()
                .map(|i| i.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());
            out.push(MeshData::new(positions, indices));
        }
    }
    Ok(out)
}

pub fn write_glb(mesh: &MeshData, path: &str) -> Result<()> {
    crate::faceted::validate_mesh(mesh)?;
    let stage = crate::export_file::StagedExport::new(std::path::Path::new(path))?;
    write_glb_staged(
        mesh,
        stage
            .path()
            .to_str()
            .ok_or_else(|| crate::KernelError::Geometry("Invalid export path".into()))?,
    )?;
    stage.commit()?;
    Ok(())
}

fn write_glb_staged(mesh: &MeshData, path: &str) -> Result<()> {
    let n_vert = mesh.positions.len();
    let n_idx = mesh.indices.len();
    // BIN：positions（f32×3）+ indices（u32），均为 4 字节对齐
    let pos_len = n_vert * 12;
    let idx_len = n_idx * 4;
    let bin_len = pos_len + idx_len;
    let (min, max) = mesh.bounds().unwrap_or((glam::Vec3::ZERO, glam::Vec3::ONE));

    let mut json = format!(
        r#"{{
  "asset": {{ "version": "2.0", "generator": "S33D" }},
  "scene": 0,
  "scenes": [ {{ "nodes": [0] }} ],
  "nodes": [ {{ "mesh": 0 }} ],
  "meshes": [ {{ "primitives": [ {{ "attributes": {{ "POSITION": 0 }}, "indices": 1 }} ] }} ],
  "buffers": [ {{ "byteLength": {bin_len} }} ],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": 0, "byteLength": {pos_len}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {pos_len}, "byteLength": {idx_len}, "target": 34963 }}
  ],
  "accessors": [
    {{ "bufferView": 0, "componentType": 5126, "count": {n_vert}, "type": "VEC3",
       "min": [{minx}, {miny}, {minz}], "max": [{maxx}, {maxy}, {maxz}] }},
    {{ "bufferView": 1, "componentType": 5125, "count": {n_idx}, "type": "SCALAR" }}
  ]
}}"#,
        minx = min.x,
        miny = min.y,
        minz = min.z,
        maxx = max.x,
        maxy = max.y,
        maxz = max.z,
    );
    // JSON chunk 4 字节对齐（补空格）
    while json.len() % 4 != 0 {
        json.push(' ');
    }
    let total = 12 + 8 + json.len() + 8 + bin_len;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(&0x4654_6C67u32.to_le_bytes())?; // "glTF"
    f.write_all(&2u32.to_le_bytes())?;
    f.write_all(&(total as u32).to_le_bytes())?;
    f.write_all(&(json.len() as u32).to_le_bytes())?;
    f.write_all(&0x4E4F_534Au32.to_le_bytes())?; // "JSON"
    f.write_all(json.as_bytes())?;
    f.write_all(&(bin_len as u32).to_le_bytes())?;
    f.write_all(&0x004E_4942u32.to_le_bytes())?; // "BIN\0"
    for p in &mesh.positions {
        for x in p {
            f.write_all(&x.to_le_bytes())?;
        }
    }
    for &i in &mesh.indices {
        f.write_all(&i.to_le_bytes())?;
    }
    f.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_mesh() -> MeshData {
        let v: Vec<[f32; 3]> = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        let i: Vec<u32> = vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7,
            6, 3, 0, 4, 3, 4, 7,
        ];
        MeshData::new(v, i)
    }

    fn tmp(name: &str) -> String {
        std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn stl_binary_roundtrip() {
        let m = cube_mesh();
        let p = tmp("s33d-test.stl");
        write_stl(&m, &p).unwrap();
        let read = read_stl(&p).unwrap();
        assert_eq!(read.len(), 1);
        // 二进制 STL 焊接后：8 顶点 12 三角形
        assert_eq!(
            read[0].positions.len(),
            8,
            "verts={}",
            read[0].positions.len()
        );
        assert_eq!(read[0].triangle_count(), 12);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn obj_roundtrip() {
        let m = cube_mesh();
        let p = tmp("s33d-test.obj");
        write_obj(&m, &p).unwrap();
        let read = read_obj(&p).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].positions.len(), 8);
        assert_eq!(read[0].triangle_count(), 12);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn ply_binary_roundtrip() {
        let m = cube_mesh();
        let p = tmp("s33d-test.ply");
        write_ply(&m, &p).unwrap();
        let read = read_ply(&p).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].positions.len(), 8);
        assert_eq!(read[0].triangle_count(), 12);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn glb_roundtrip() {
        let m = cube_mesh();
        let p = tmp("s33d-test.glb");
        write_glb(&m, &p).unwrap();
        let read = read_glb(&p).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].positions.len(), 8);
        assert_eq!(read[0].triangle_count(), 12);
        std::fs::remove_file(&p).ok();
    }
}
