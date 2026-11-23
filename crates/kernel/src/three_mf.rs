//! Bounded, in-memory OPC/3MF geometry reader. Printer settings are not geometry.
use crate::{KernelError, MeshData, Result};
use glam::{DMat4, DVec3, DVec4};
use quick_xml::{
    events::{BytesStart, Event},
    name::ResolveResult,
    reader::NsReader,
};
use std::{
    collections::{HashMap, HashSet},
    io::Read,
};

const CORE: &[u8] = b"http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const PRODUCTION: &str = "http://schemas.microsoft.com/3dmanufacturing/production/2015/06";
const RELS: &[u8] = b"http://schemas.openxmlformats.org/package/2006/relationships";
const MAX_XML: usize = 64 * 1024 * 1024;
const MAX_VERTICES: usize = 2_000_000;
const MAX_TRIANGLES: usize = 4_000_000;
type Fallible<T> = std::result::Result<T, String>;

fn failure(s: impl ToString) -> String {
    format!("3MF: {}", s.to_string())
}
fn u16_at(b: &[u8], at: usize) -> Fallible<u16> {
    Ok(u16::from_le_bytes(
        b.get(at..at + 2)
            .ok_or_else(|| failure("truncated ZIP"))?
            .try_into()
            .unwrap(),
    ))
}
fn u32_at(b: &[u8], at: usize) -> Fallible<u32> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4)
            .ok_or_else(|| failure("truncated ZIP"))?
            .try_into()
            .unwrap(),
    ))
}
fn part_path(base: &str, path: &str) -> Fallible<String> {
    if path.contains(['\\', ':', '\0', '?', '#', '%']) {
        return Err(failure("invalid or external part path"));
    }
    let full = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!(
            "{}/{}",
            base.rsplit_once('/').map(|x| x.0).unwrap_or(""),
            path
        )
    };
    let mut parts = Vec::new();
    for p in full.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(failure("part path escapes package"));
                }
            }
            _ => parts.push(p),
        }
    }
    if parts.is_empty() {
        return Err(failure("empty part path"));
    }
    Ok(parts.join("/"))
}

struct Entry {
    offset: usize,
    compressed: usize,
    size: usize,
    method: u16,
    crc: u32,
}
struct Package {
    bytes: Vec<u8>,
    entries: HashMap<String, Entry>,
}
impl Package {
    fn new(bytes: Vec<u8>) -> Fallible<Self> {
        if bytes.len() > 256 * 1024 * 1024 {
            return Err(failure("package exceeds 256 MiB"));
        }
        let end = (bytes.len().saturating_sub(65557)..bytes.len().saturating_sub(21))
            .rev()
            .find(|&i| {
                bytes.get(i..i + 4) == Some(b"PK\x05\x06")
                    && u16_at(&bytes, i + 20).is_ok_and(|n| i + 22 + n as usize == bytes.len())
            })
            .ok_or_else(|| failure("missing ZIP central directory"))?;
        if u16_at(&bytes, end + 4)? != 0 || u16_at(&bytes, end + 6)? != 0 {
            return Err(failure("multi-disk ZIP is unsupported"));
        }
        let count = u16_at(&bytes, end + 10)? as usize;
        if count == 0 || count > 4096 || u16_at(&bytes, end + 8)? as usize != count {
            return Err(failure("unsupported ZIP64 or entry count"));
        }
        let mut at = u32_at(&bytes, end + 16)? as usize;
        let limit = at
            .checked_add(u32_at(&bytes, end + 12)? as usize)
            .filter(|&v| v == end)
            .ok_or_else(|| failure("invalid ZIP directory bounds"))?;
        let mut entries = HashMap::new();
        for _ in 0..count {
            if u32_at(&bytes, at)? != 0x02014b50 {
                return Err(failure("invalid ZIP directory entry"));
            }
            let flags = u16_at(&bytes, at + 8)?;
            if flags & 0x41 != 0 || u16_at(&bytes, at + 34)? != 0 {
                return Err(failure("encrypted or multi-disk ZIP"));
            }
            let method = u16_at(&bytes, at + 10)?;
            let compressed = u32_at(&bytes, at + 20)? as usize;
            let size = u32_at(&bytes, at + 24)? as usize;
            let len = u16_at(&bytes, at + 28)? as usize;
            let next = at
                + 46
                + len
                + u16_at(&bytes, at + 30)? as usize
                + u16_at(&bytes, at + 32)? as usize;
            if next > limit {
                return Err(failure("truncated ZIP directory entry"));
            }
            let name = std::str::from_utf8(
                bytes
                    .get(at + 46..at + 46 + len)
                    .ok_or_else(|| failure("missing ZIP name"))?,
            )
            .map_err(failure)?;
            if !name.ends_with('/') {
                let name = part_path("", name)?;
                let entry = Entry {
                    offset: u32_at(&bytes, at + 42)? as usize,
                    compressed,
                    size,
                    method,
                    crc: u32_at(&bytes, at + 16)?,
                };
                if entries.insert(name, entry).is_some() {
                    return Err(failure("duplicate ZIP part"));
                }
            }
            at = next;
        }
        if at != limit {
            return Err(failure("ZIP directory size mismatch"));
        }
        Ok(Self { bytes, entries })
    }
    fn xml(&self, name: &str) -> Fallible<Vec<u8>> {
        let e = self
            .entries
            .get(name)
            .ok_or_else(|| failure(format!("missing part {name}")))?;
        if e.size > MAX_XML {
            return Err(failure("XML part exceeds 64 MiB"));
        }
        let b = &self.bytes;
        if u32_at(b, e.offset)? != 0x04034b50 || u16_at(b, e.offset + 8)? != e.method {
            return Err(failure("invalid ZIP local header"));
        }
        let name_len = u16_at(b, e.offset + 26)? as usize;
        let local_name = std::str::from_utf8(
            b.get(e.offset + 30..e.offset + 30 + name_len)
                .ok_or_else(|| failure("truncated local name"))?,
        )
        .map_err(failure)?;
        if part_path("", local_name)? != name {
            return Err(failure("local and central ZIP names differ"));
        }
        let start = e.offset + 30 + name_len + u16_at(b, e.offset + 28)? as usize;
        let data = b
            .get(
                start
                    ..start
                        .checked_add(e.compressed)
                        .ok_or_else(|| failure("ZIP overflow"))?,
            )
            .ok_or_else(|| failure("truncated ZIP data"))?;
        let out = match e.method {
            0 => {
                if data.len() != e.size {
                    return Err(failure("stored ZIP size mismatch"));
                }
                data.to_vec()
            }
            8 => {
                let mut out = Vec::new();
                flate2::read::DeflateDecoder::new(data)
                    .take((MAX_XML + 1) as u64)
                    .read_to_end(&mut out)
                    .map_err(failure)?;
                out
            }
            _ => return Err(failure("unsupported ZIP compression")),
        };
        if out.len() != e.size || crc32fast::hash(&out) != e.crc {
            return Err(failure("ZIP size or CRC mismatch"));
        }
        Ok(out)
    }
    fn root(&self) -> Fallible<String> {
        let xml = self.xml("_rels/.rels")?;
        let mut reader = NsReader::from_reader(xml.as_slice());
        let mut root = None;
        loop {
            let (namespace, event) = reader.read_resolved_event().map_err(failure)?;
            let rel = matches!(namespace, ResolveResult::Bound(n) if n.as_ref() == RELS);
            match event {
                Event::Start(e) | Event::Empty(e)
                    if rel && e.local_name().as_ref() == b"Relationship" =>
                {
                    let a = attrs(&e, reader.decoder())?;
                    if a.get("Type").is_some_and(|s| {
                        s == "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"
                    }) {
                        if root.is_some() || a.get("TargetMode").is_some_and(|s| s != "Internal") {
                            return Err(failure("ambiguous or external model relationship"));
                        }
                        root = Some(part_path("", required(&a, "Target")?)?);
                    }
                }
                Event::DocType(_) => return Err(failure("DTD is unsupported")),
                Event::Eof => break,
                _ => {}
            }
        }
        root.ok_or_else(|| failure("missing 3D model relationship"))
    }
}

fn attrs(
    e: &BytesStart<'_>,
    decoder: quick_xml::encoding::Decoder,
) -> Fallible<HashMap<String, String>> {
    e.attributes()
        .map(|a| {
            let a = a.map_err(failure)?;
            Ok((
                std::str::from_utf8(a.key.as_ref())
                    .map_err(failure)?
                    .to_owned(),
                a.decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, decoder)
                    .map_err(failure)?
                    .into_owned(),
            ))
        })
        .collect()
}
fn required<'a>(a: &'a HashMap<String, String>, key: &str) -> Fallible<&'a str> {
    a.get(key)
        .map(String::as_str)
        .ok_or_else(|| failure(format!("missing {key}")))
}
fn number(a: &HashMap<String, String>, key: &str) -> Fallible<f64> {
    let value: f64 = required(a, key)?.parse().map_err(failure)?;
    if !value.is_finite() {
        return Err(failure("non-finite coordinate"));
    }
    Ok(value)
}
fn id(a: &HashMap<String, String>, key: &str) -> Fallible<u32> {
    required(a, key)?.parse::<u32>().map_err(failure)
}
fn transform(a: &HashMap<String, String>) -> Fallible<DMat4> {
    let Some(s) = a.get("transform") else {
        return Ok(DMat4::IDENTITY);
    };
    let v: Vec<f64> = s
        .split_whitespace()
        .map(|s| s.parse::<f64>().map_err(failure))
        .collect::<Fallible<_>>()?;
    if v.len() != 12 || v.iter().any(|v| !v.is_finite()) {
        return Err(failure("invalid transform"));
    }
    let matrix = DMat4::from_cols(
        DVec4::new(v[0], v[1], v[2], 0.),
        DVec4::new(v[3], v[4], v[5], 0.),
        DVec4::new(v[6], v[7], v[8], 0.),
        DVec4::new(v[9], v[10], v[11], 1.),
    );
    if matrix.determinant().abs() < 1e-15 {
        return Err(failure("singular transform"));
    }
    Ok(matrix)
}
#[derive(Clone)]
struct Reference {
    part: String,
    object: u32,
    transform: DMat4,
}
#[derive(Default)]
struct Object {
    positions: Vec<DVec3>,
    indices: Vec<u32>,
    components: Vec<Reference>,
    mesh: bool,
    model: bool,
}
struct Model {
    objects: HashMap<u32, Object>,
    build: Vec<Reference>,
    unit: f64,
}
fn parse_model(xml: &[u8], part: &str) -> Fallible<Model> {
    let mut r = NsReader::from_reader(xml);
    let mut model = Model {
        objects: HashMap::new(),
        build: Vec::new(),
        unit: 1.,
    };
    let mut stack: Vec<String> = Vec::new();
    let mut current = None;
    let mut root = false;
    let mut production_prefixes = HashSet::new();
    loop {
        let (ns, event) = r.read_resolved_event().map_err(failure)?;
        let core = matches!(ns, ResolveResult::Bound(n) if n.as_ref()==CORE);
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                if stack.len() > 64 {
                    return Err(failure("XML nesting limit"));
                }
                let empty = matches!(event, Event::Empty(_));
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let a = attrs(e, r.decoder())?;
                let parent = stack.last().map(String::as_str);
                if stack.is_empty() {
                    if root || !core || name != "model" {
                        return Err(failure("invalid model root/namespace"));
                    }
                    root = true;
                    for (key, value) in &a {
                        if value == PRODUCTION {
                            if let Some(prefix) = key.strip_prefix("xmlns:") {
                                production_prefixes.insert(prefix.to_owned());
                            }
                        }
                    }
                    for prefix in a
                        .get("requiredextensions")
                        .map(String::as_str)
                        .unwrap_or("")
                        .split_whitespace()
                    {
                        if !production_prefixes.contains(prefix) {
                            return Err(failure(format!(
                                "unsupported required extension {prefix}"
                            )));
                        }
                    }
                    model.unit = match a.get("unit").map(String::as_str).unwrap_or("millimeter") {
                        "micron" => 0.001,
                        "millimeter" => 1.,
                        "centimeter" => 10.,
                        "inch" => 25.4,
                        "foot" => 304.8,
                        "meter" => 1000.,
                        _ => return Err(failure("unsupported unit")),
                    };
                }
                if core {
                    match (name.as_str(), parent) {
                        ("object", Some("resources")) => {
                            let object_id = id(&a, "id")?;
                            if object_id == 0
                                || model.objects.len() >= 4096
                                || model
                                    .objects
                                    .insert(
                                        object_id,
                                        Object {
                                            model: a.get("type").is_none_or(|s| s == "model"),
                                            ..Default::default()
                                        },
                                    )
                                    .is_some()
                            {
                                return Err(failure("invalid/duplicate object ID"));
                            }
                            current = Some(object_id);
                        }
                        ("mesh", Some("object")) => {
                            let object = model
                                .objects
                                .get_mut(&current.ok_or_else(|| failure("mesh outside object"))?)
                                .unwrap();
                            if object.mesh || !object.components.is_empty() {
                                return Err(failure("object has multiple geometry definitions"));
                            }
                            object.mesh = true;
                        }
                        ("vertex", Some("vertices")) => {
                            let object = model
                                .objects
                                .get_mut(&current.ok_or_else(|| failure("vertex outside object"))?)
                                .unwrap();
                            if object.positions.len() >= MAX_VERTICES {
                                return Err(failure("vertex limit"));
                            }
                            object.positions.push(DVec3::new(
                                number(&a, "x")?,
                                number(&a, "y")?,
                                number(&a, "z")?,
                            ));
                        }
                        ("triangle", Some("triangles")) => {
                            let object = model
                                .objects
                                .get_mut(
                                    &current.ok_or_else(|| failure("triangle outside object"))?,
                                )
                                .unwrap();
                            if object.indices.len() / 3 >= MAX_TRIANGLES {
                                return Err(failure("triangle limit"));
                            }
                            object
                                .indices
                                .extend([id(&a, "v1")?, id(&a, "v2")?, id(&a, "v3")?]);
                        }
                        ("component", Some("components")) | ("item", Some("build")) => {
                            let mut target = part.to_owned();
                            for prefix in &production_prefixes {
                                if let Some(path) = a.get(&format!("{prefix}:path")) {
                                    target = part_path(part, path)?;
                                }
                            }
                            let reference = Reference {
                                part: target,
                                object: id(&a, "objectid")?,
                                transform: transform(&a)?,
                            };
                            if name == "item" {
                                // Non-printable build objects still belong to the model.
                                if model.build.len() >= 4096 {
                                    return Err(failure("build item limit"));
                                }
                                model.build.push(reference);
                            } else {
                                let object = model
                                    .objects
                                    .get_mut(
                                        &current
                                            .ok_or_else(|| failure("component outside object"))?,
                                    )
                                    .unwrap();
                                if object.mesh || object.components.len() >= 4096 {
                                    return Err(failure("invalid component object"));
                                }
                                object.components.push(reference);
                            }
                        }
                        _ => {}
                    }
                }
                if !empty {
                    stack.push(if core { name } else { String::new() });
                }
            }
            Event::End(e) => {
                if e.local_name().as_ref() == b"object" && core {
                    current = None;
                }
                stack.pop();
            }
            Event::DocType(_) => return Err(failure("DTD is unsupported")),
            Event::GeneralRef(e) => {
                let name = std::str::from_utf8(e.as_ref()).map_err(failure)?;
                if !matches!(name, "amp" | "lt" | "gt" | "quot" | "apos") && !name.starts_with('#')
                {
                    return Err(failure("unknown XML entity"));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !root || !stack.is_empty() {
        return Err(failure("incomplete model XML"));
    }
    for object in model.objects.values() {
        if object.indices.chunks_exact(3).any(|t| {
            t.iter().any(|&i| i as usize >= object.positions.len())
                || t[0] == t[1]
                || t[1] == t[2]
                || t[0] == t[2]
        }) {
            return Err(failure("invalid triangle indices"));
        }
    }
    Ok(model)
}

struct Expansion<'a> {
    package: &'a Package,
    models: HashMap<String, Model>,
    visiting: HashSet<(String, u32)>,
    unit: f64,
    vertices: usize,
    triangles: usize,
    nodes: usize,
    xml_bytes: usize,
}
impl Expansion<'_> {
    fn append(
        &mut self,
        reference: &Reference,
        parent: DMat4,
        out: &mut MeshData,
        depth: usize,
    ) -> Fallible<()> {
        self.nodes += 1;
        if depth > 64 || self.nodes > 100_000 {
            return Err(failure("component expansion limit"));
        }
        let key = (reference.part.clone(), reference.object);
        if !self.visiting.insert(key.clone()) {
            return Err(failure("cyclic components"));
        }
        if !self.models.contains_key(&reference.part) {
            let xml = self.package.xml(&reference.part)?;
            self.xml_bytes += xml.len();
            if self.models.len() >= 128 || self.xml_bytes > 128 * 1024 * 1024 {
                return Err(failure("model part expansion limit"));
            }
            let model = parse_model(&xml, &reference.part)?;
            if model.unit != self.unit {
                return Err(failure("component model uses inconsistent units"));
            }
            self.models.insert(reference.part.clone(), model);
        }
        let object = self.models[&reference.part]
            .objects
            .get(&reference.object)
            .ok_or_else(|| failure("missing referenced object"))?;
        if !object.model {
            return Err(failure("build references a non-model object"));
        }
        let matrix = parent * reference.transform;
        if object.mesh {
            if object.indices.is_empty() {
                return Err(failure("empty build mesh"));
            }
            self.vertices += object.positions.len();
            self.triangles += object.indices.len() / 3;
            if self.vertices > MAX_VERTICES || self.triangles > MAX_TRIANGLES {
                return Err(failure("expanded mesh limit"));
            }
            let offset = out.positions.len() as u32;
            for &point in &object.positions {
                let p = matrix.transform_point3(point) * self.unit;
                // 3MF is Z-up; S33D is Y-up. A rotation preserves winding.
                let p = glam::Vec3::new(p.x as f32, p.z as f32, -p.y as f32);
                if !p.is_finite() {
                    return Err(failure("transformed coordinate overflow"));
                }
                out.positions.push(p.to_array());
            }
            let reverse = matrix.determinant() < 0.;
            for t in object.indices.chunks_exact(3) {
                let t = if reverse {
                    [t[0], t[2], t[1]]
                } else {
                    [t[0], t[1], t[2]]
                };
                out.indices.extend(t.map(|i| i + offset));
            }
        } else {
            let components = object.components.clone();
            if components.is_empty() {
                return Err(failure("build object has no geometry"));
            }
            for component in &components {
                self.append(component, matrix, out, depth + 1)?;
            }
        }
        self.visiting.remove(&key);
        Ok(())
    }
}
fn decode(bytes: Vec<u8>) -> Fallible<Vec<MeshData>> {
    let package = Package::new(bytes)?;
    let root = package.root()?;
    let xml = package.xml(&root)?;
    let model = parse_model(&xml, &root)?;
    if model.build.is_empty() {
        return Err(failure("model has no build items"));
    }
    let build = model.build.clone();
    let unit = model.unit;
    let mut expansion = Expansion {
        package: &package,
        models: HashMap::from([(root, model)]),
        visiting: HashSet::new(),
        unit,
        vertices: 0,
        triangles: 0,
        nodes: 0,
        xml_bytes: xml.len(),
    };
    let mut meshes = Vec::new();
    for reference in &build {
        let mut mesh = MeshData::default();
        expansion.append(reference, DMat4::IDENTITY, &mut mesh, 0)?;
        meshes.push(mesh);
    }
    Ok(meshes)
}
pub fn read(path: &str) -> Result<Vec<MeshData>> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > 256 * 1024 * 1024 {
        return Err(KernelError::Geometry(failure("package exceeds 256 MiB")));
    }
    let mut bytes = Vec::new();
    file.take(256 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    decode(bytes).map_err(KernelError::Geometry)
}

#[cfg(test)]
#[path = "three_mf_qa.rs"]
mod tests;
