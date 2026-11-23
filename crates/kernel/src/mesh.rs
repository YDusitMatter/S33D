//! 三角网格数据（内核与渲染层之间的唯一通道）。

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// 索引三角网格。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// 可选：每顶点法线（与 positions 对齐；weld_hard 输出，None 时渲染端按需计算）。
    pub vertex_normals: Option<Vec<[f32; 3]>>,
}

impl MeshData {
    pub fn new(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> Self {
        Self {
            positions,
            indices,
            vertex_normals: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// 逐三角形面法线（用于线框/着色调试）。
    pub fn face_normals(&self) -> Vec<[f32; 3]> {
        let mut out = Vec::with_capacity(self.triangle_count());
        for t in self.indices.chunks_exact(3) {
            let a = Vec3::from_array(self.positions[t[0] as usize]);
            let b = Vec3::from_array(self.positions[t[1] as usize]);
            let c = Vec3::from_array(self.positions[t[2] as usize]);
            let n = (b - a).cross(c - a);
            out.push(if n.length_squared() > 1e-12 {
                n.normalize().to_array()
            } else {
                [0.0, 1.0, 0.0]
            });
        }
        out
    }

    /// 按共享顶点的平均法线（平滑着色）。
    pub fn vertex_normals(&self) -> Vec<[f32; 3]> {
        if let Some(n) = &self.vertex_normals {
            return n.clone();
        }
        let mut acc: Vec<Vec3> = vec![Vec3::ZERO; self.positions.len()];
        for t in self.indices.chunks_exact(3) {
            let a = Vec3::from_array(self.positions[t[0] as usize]);
            let b = Vec3::from_array(self.positions[t[1] as usize]);
            let c = Vec3::from_array(self.positions[t[2] as usize]);
            let n = (b - a).cross(c - a);
            for &i in t {
                acc[i as usize] += n;
            }
        }
        acc.iter()
            .map(|n| {
                if n.length_squared() > 1e-12 {
                    n.normalize().to_array()
                } else {
                    [0.0, 1.0, 0.0]
                }
            })
            .collect()
    }

    /// 用 4x4 矩阵变换所有顶点。
    pub fn transform(&mut self, mat: &glam::Mat4) {
        for p in &mut self.positions {
            *p = (mat.transform_point3(Vec3::from_array(*p))).to_array();
        }
    }

    /// 世界空间包围盒 (min, max)。
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        if self.positions.is_empty() {
            return None;
        }
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for p in &self.positions {
            let v = Vec3::from_array(*p);
            min = min.min(v);
            max = max.max(v);
        }
        Some((min, max))
    }

    /// 平移。
    pub fn translate(&mut self, t: Vec3) {
        for p in &mut self.positions {
            let v = Vec3::from_array(*p) + t;
            *p = v.to_array();
        }
    }

    /// 合并位置相同的顶点（容差内），并过滤退化三角形。
    /// truck 的 tessellation 在面边界产生重复顶点；布尔运算需要闭合流形。
    pub fn weld(&mut self, tol: f32) {
        let mut map: std::collections::HashMap<(i64, i64, i64), u32> = Default::default();
        let mut new_positions: Vec<[f32; 3]> = Vec::with_capacity(self.positions.len());
        let mut remap = vec![0u32; self.positions.len()];
        for (i, p) in self.positions.iter().enumerate() {
            let key = (
                (p[0] / tol).round() as i64,
                (p[1] / tol).round() as i64,
                (p[2] / tol).round() as i64,
            );
            if let Some(&ni) = map.get(&key) {
                remap[i] = ni;
            } else {
                let ni = new_positions.len() as u32;
                map.insert(key, ni);
                new_positions.push(*p);
                remap[i] = ni;
            }
        }
        self.positions = new_positions;
        // 重映射索引并过滤退化三角形
        let mut new_indices = Vec::with_capacity(self.indices.len());
        for t in self.indices.chunks_exact(3) {
            let a = remap[t[0] as usize];
            let b = remap[t[1] as usize];
            let c = remap[t[2] as usize];
            if a != b && b != c && a != c {
                new_indices.extend_from_slice(&[a, b, c]);
            }
        }
        self.indices = new_indices;
    }

    /// 按面夹角分组平滑法线（CAD 硬边渲染）：
    /// 每个顶点按相邻三角形法线聚类（夹角 < hard_angle_deg 同组），
    /// 每组平均出一个法线；位置相同但法线不同的顶点分裂。
    /// 弧面条带间（约 5°）平滑；顶/底面与侧面（90°）硬边。
    /// 结果包含 positions + vertex_normals + indices。
    pub fn weld_hard(&self, tol: f32, hard_angle_deg: f32) -> MeshData {
        let mut m = self.clone();
        m.weld(tol);
        let n = m.positions.len();
        let mut face_normals: Vec<[f32; 3]> = Vec::new();
        for t in m.indices.chunks_exact(3) {
            let a = Vec3::from_array(m.positions[t[0] as usize]);
            let b = Vec3::from_array(m.positions[t[1] as usize]);
            let c = Vec3::from_array(m.positions[t[2] as usize]);
            let nrm = (b - a).cross(c - a);
            face_normals.push(nrm.to_array());
        }
        let mut vert_faces: Vec<Vec<(usize, [f32; 3])>> = vec![Vec::new(); n];
        for (fi, t) in m.indices.chunks_exact(3).enumerate() {
            for &vi in t {
                vert_faces[vi as usize].push((fi, face_normals[fi]));
            }
        }
        let cos_thresh = (hard_angle_deg * std::f32::consts::PI / 180.0).cos();
        let mut out_pos: Vec<[f32; 3]> = Vec::with_capacity(n);
        let mut out_nrm: Vec<[f32; 3]> = Vec::with_capacity(n);
        let mut dedup: std::collections::HashMap<(i64, i64, i64, i64, i64, i64), u32> =
            Default::default();
        let mut vert_group_idx: Vec<std::collections::HashMap<usize, u32>> =
            vec![Default::default(); n];
        let mut new_indices = Vec::with_capacity(m.indices.len());
        for (fi, t) in m.indices.chunks_exact(3).enumerate() {
            let mut tri = [0u32; 3];
            for (k, &vi) in t.iter().enumerate() {
                let vi = vi as usize;
                if let Some(&gi) = vert_group_idx[vi].get(&fi) {
                    tri[k] = gi;
                    continue;
                }
                let faces = &vert_faces[vi];
                let mut groups: Vec<Vec<[f32; 3]>> = Vec::new();
                for (_, nrm) in faces {
                    let nn = Vec3::from_array(*nrm);
                    let l = nn.length();
                    if l < 1e-12 {
                        continue;
                    }
                    let un = (nn / l).to_array();
                    let mut placed = false;
                    for g in groups.iter_mut() {
                        let g0 = Vec3::from_array(g[0]);
                        let g0u = (g0 / g0.length().max(1e-12)).to_array();
                        let dot = un[0] * g0u[0] + un[1] * g0u[1] + un[2] * g0u[2];
                        if dot >= cos_thresh {
                            g.push(*nrm);
                            placed = true;
                            break;
                        }
                    }
                    if !placed {
                        groups.push(vec![*nrm]);
                    }
                }
                let mut gi = u32::MAX;
                for g in groups {
                    let in_group = g.iter().any(|gn| {
                        let gg = Vec3::from_array(*gn);
                        let gl = gg.length();
                        let fnn = Vec3::from_array(face_normals[fi]);
                        let fl = fnn.length();
                        if gl < 1e-12 || fl < 1e-12 {
                            return false;
                        }
                        let d = (gg / gl).dot(fnn / fl);
                        d >= cos_thresh
                    });
                    if !in_group {
                        continue;
                    }
                    let mut sum = Vec3::ZERO;
                    for nrm in &g {
                        sum += Vec3::from_array(*nrm);
                    }
                    let l = sum.length();
                    let nrm = if l > 1e-12 {
                        (sum / l).to_array()
                    } else {
                        [0.0, 1.0, 0.0]
                    };
                    let p = m.positions[vi];
                    let key = (
                        (p[0] / tol).round() as i64,
                        (p[1] / tol).round() as i64,
                        (p[2] / tol).round() as i64,
                        (nrm[0] * 1000.0).round() as i64,
                        (nrm[1] * 1000.0).round() as i64,
                        (nrm[2] * 1000.0).round() as i64,
                    );
                    if let Some(&ni) = dedup.get(&key) {
                        gi = ni;
                    } else {
                        let ni = out_pos.len() as u32;
                        dedup.insert(key, ni);
                        out_pos.push(p);
                        out_nrm.push(nrm);
                        gi = ni;
                    }
                    break;
                }
                if gi == u32::MAX {
                    let p = m.positions[vi];
                    let ni = out_pos.len() as u32;
                    out_pos.push(p);
                    out_nrm.push([0.0, 1.0, 0.0]);
                    gi = ni;
                }
                vert_group_idx[vi].insert(fi, gi);
                tri[k] = gi;
            }
            if tri[0] != tri[1] && tri[1] != tri[2] && tri[0] != tri[2] {
                new_indices.extend_from_slice(&tri);
            }
        }
        MeshData {
            positions: out_pos,
            indices: new_indices,
            vertex_normals: Some(out_nrm),
        }
    }

    /// 网格锐边检测：共享两顶点的相邻三角形对，两面夹角 > min_angle_deg 的边。
    /// 返回（顶点索引 a, b）列表（去重）。
    pub fn sharp_edges(&self, min_angle_deg: f32) -> Vec<(usize, usize)> {
        let cos_thresh = (min_angle_deg * std::f32::consts::PI / 180.0).cos();
        // 边 → 相邻面法线列表
        let mut edge_faces: std::collections::HashMap<(u32, u32), Vec<glam::Vec3>> =
            Default::default();
        for t in self.indices.chunks_exact(3) {
            let a = Vec3::from_array(self.positions[t[0] as usize]);
            let b = Vec3::from_array(self.positions[t[1] as usize]);
            let c = Vec3::from_array(self.positions[t[2] as usize]);
            let n = (b - a).cross(c - a);
            if n.length() < 1e-12 {
                continue;
            }
            let n = n.normalize();
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let key = if t[i] < t[j] {
                    (t[i], t[j])
                } else {
                    (t[j], t[i])
                };
                edge_faces.entry(key).or_default().push(n);
            }
        }
        let mut out = Vec::new();
        for (key, normals) in edge_faces {
            if normals.len() < 2 {
                continue;
            }
            // 所有法线两两取最小点积（通常恰好两个面）
            let mut min_dot = 2.0f32;
            for i in 0..normals.len() {
                for j in (i + 1)..normals.len() {
                    min_dot = min_dot.min(normals[i].dot(normals[j]));
                }
            }
            if min_dot < cos_thresh {
                out.push((key.0 as usize, key.1 as usize));
            }
        }
        out
    }

    /// 把锐边合并成链：共享顶点且方向夹角 < merge_angle_deg 的连续边归为一条。
    /// 圆形棱（圆柱顶面圆边）被 tessellation 离散为多段，合并后可整体识别。
    pub fn sharp_edge_chains(
        &self,
        min_angle_deg: f32,
        merge_angle_deg: f32,
    ) -> Vec<Vec<(usize, usize)>> {
        let edges = self.sharp_edges(min_angle_deg);
        let cos_merge = (merge_angle_deg * std::f32::consts::PI / 180.0).cos();
        let n = edges.len();
        // union-find
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(parent: &mut Vec<usize>, x: usize) -> usize {
            if parent[x] != x {
                let p = find(parent, parent[x]);
                parent[x] = p;
            }
            parent[x]
        }
        // 顶点 → 邻接边索引
        let mut adj: std::collections::HashMap<u32, Vec<usize>> = std::collections::HashMap::new();
        for (i, (a, b)) in edges.iter().enumerate() {
            adj.entry(*a as u32).or_default().push(i);
            adj.entry(*b as u32).or_default().push(i);
        }
        let dir = |a: usize, b: usize| -> glam::Vec3 {
            let v = Vec3::from_array(self.positions[b]) - Vec3::from_array(self.positions[a]);
            let l = v.length();
            if l > 1e-9 {
                v / l
            } else {
                glam::Vec3::ZERO
            }
        };
        // 共享顶点且方向一致的边 → union
        for (i, (a, b)) in edges.iter().enumerate() {
            for tip in [*a as u32, *b as u32] {
                if let Some(nbrs) = adj.get(&tip) {
                    for &j in nbrs {
                        if j <= i {
                            continue;
                        }
                        let (na, nb) = edges[j];
                        let other_i = if a == &(tip as usize) {
                            *b as usize
                        } else {
                            *a as usize
                        };
                        let other_j = if na as u32 == tip {
                            nb as usize
                        } else {
                            na as usize
                        };
                        let d1 = dir(other_i, tip as usize);
                        let d2 = dir(tip as usize, other_j);
                        if d1.length() < 1e-9 || d2.length() < 1e-9 {
                            continue;
                        }
                        if d1.dot(d2) >= cos_merge {
                            let ri = find(&mut parent, i);
                            let rj = find(&mut parent, j);
                            if ri != rj {
                                parent[ri] = rj;
                            }
                        }
                    }
                }
            }
        }
        // 分组
        let mut groups: std::collections::HashMap<usize, Vec<(usize, usize)>> = Default::default();
        for (i, e) in edges.iter().enumerate() {
            let r = find(&mut parent, i);
            groups.entry(r).or_default().push(*e);
        }
        // 链内按连接顺序排序（贪心：从任意段出发沿共享顶点串联）
        let mut chains = Vec::new();
        for (_, mut segs) in groups {
            if segs.len() == 1 {
                chains.push(segs);
                continue;
            }
            // 建局部邻接
            let mut out: Vec<(usize, usize)> = Vec::new();
            let mut used = vec![false; segs.len()];
            out.push(segs[0]);
            used[0] = true;
            for _ in 1..segs.len() {
                let (ea, eb) = *out.last().unwrap();
                let mut found = false;
                for (k, (na, nb)) in segs.iter().enumerate() {
                    if used[k] {
                        continue;
                    }
                    if *na == ea || *na == eb || *nb == ea || *nb == eb {
                        out.push((*na, *nb));
                        used[k] = true;
                        found = true;
                        break;
                    }
                }
                if !found {
                    break;
                }
            }
            // 剩余未连上的（分叉罕见）：直接追加
            for (k, e) in segs.iter().enumerate() {
                if !used[k] {
                    out.push(*e);
                }
            }
            chains.push(out);
        }
        chains
    }

    /// 合并两个网格（顶点拼接、索引偏移）。
    pub fn merge(&mut self, other: &MeshData) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_offsets_indices() {
        let a = MeshData::new(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], vec![0, 1, 0]);
        let b = MeshData::new(vec![[2.0, 0.0, 0.0]], vec![0, 0, 0]);
        let mut m = a.clone();
        m.merge(&b);
        assert_eq!(m.positions.len(), 3);
        assert_eq!(m.indices, vec![0, 1, 0, 2, 2, 2]);
    }

    #[test]
    fn bounds_ok() {
        let m = MeshData::new(vec![[-1.0, 0.0, 0.0], [1.0, 2.0, 3.0]], vec![0, 1, 0]);
        let (min, max) = m.bounds().unwrap();
        assert_eq!(min, Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(max, Vec3::new(1.0, 2.0, 3.0));
    }
}
