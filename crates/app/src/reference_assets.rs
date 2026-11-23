//! Portable reference images: bounded decode, orientation normalization, no external links.
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path};

pub const MAX_FILE_BYTES: u64 = 24 * 1024 * 1024;
pub const MAX_LIBRARY_BYTES: usize = 32 * 1024 * 1024;
const MAX_PIXELS: u64 = 32_000_000;
const MAX_ASSETS: usize = 16;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceAsset {
    pub id: String,
    pub name: String,
    pub source_sha256: String,
    pub png_sha256: String,
    pub width: u32,
    pub height: u32,
    pub original_width: u32,
    pub original_height: u32,
    pub orientation_applied: bool,
    pub png_data: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceLibrary {
    pub schema_version: u32,
    pub assets: Vec<ReferenceAsset>,
}
impl Default for ReferenceLibrary {
    fn default() -> Self {
        Self {
            schema_version: 1,
            assets: vec![],
        }
    }
}

fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn png(image: &DynamicImage) -> Result<Vec<u8>, String> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

fn decode(bytes: &[u8]) -> Result<(DynamicImage, bool), String> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("Reference image exceeds 24 MiB".into());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if !matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)) {
        return Err("Reference photos support PNG and JPEG only".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (w, h) = decoder.dimensions();
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err("Reference image exceeds 32 megapixels".into());
    }
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let rotated = orientation != image::metadata::Orientation::NoTransforms;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    Ok((image, rotated))
}

impl ReferenceAsset {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Select a regular image file".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let (image, rotated) = decode(&bytes)?;
        let (original_width, original_height) = (image.width(), image.height());
        let image = if image.width() > 2048 || image.height() > 2048 {
            image.thumbnail(2048, 2048)
        } else {
            image
        };
        let data = png(&image)?;
        let source_sha256 = hash(&bytes);
        Ok(Self {
            id: format!("ref-{source_sha256}"),
            source_sha256,
            png_sha256: hash(&data),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .take(180)
                .collect(),
            width: image.width(),
            height: image.height(),
            original_width,
            original_height,
            orientation_applied: rotated,
            png_data: STANDARD.encode(data),
        })
    }
    pub fn metadata(&self) -> Value {
        json!({"asset_id":self.id,"name":self.name,"sha256":self.source_sha256,"image_sha256":self.png_sha256,
            "mime_type":"image/png","width":self.width,"height":self.height,
            "original_width":self.original_width,"original_height":self.original_height,
            "orientation_applied":self.orientation_applied,"portable":true,"embedded":true})
    }
    pub fn image(&self, max_edge: u32) -> Result<Value, String> {
        if !(128..=2048).contains(&max_edge) {
            return Err("max_edge must be 128..2048".into());
        }
        let bytes = STANDARD.decode(&self.png_data).map_err(|e| e.to_string())?;
        let (decoded, _) = decode(&bytes)?;
        let scaled = if decoded.width() > max_edge || decoded.height() > max_edge {
            decoded.thumbnail(max_edge, max_edge)
        } else {
            decoded
        };
        Ok(
            json!({"metadata":self.metadata(), "width":scaled.width(), "height":scaled.height(),
            "image":{"mime_type":"image/png","data":STANDARD.encode(png(&scaled)?)}}),
        )
    }
}

impl ReferenceLibrary {
    pub fn list(&self) -> Value {
        json!({"assets":self.assets.iter().map(ReferenceAsset::metadata).collect::<Vec<_>>()})
    }
    pub fn byte_size(&self) -> usize {
        self.assets.iter().map(|a| a.png_data.len()).sum()
    }
    pub fn import(&mut self, paths: &[String]) -> Result<Value, String> {
        if paths.is_empty() || paths.len() > MAX_ASSETS {
            return Err("Import 1..16 reference photos".into());
        }
        let mut candidate = self.clone();
        let mut imported = Vec::new();
        for path in paths {
            let asset = ReferenceAsset::from_path(Path::new(path))?;
            imported.push(asset.metadata());
            if !candidate.assets.iter().any(|a| a.id == asset.id) {
                candidate.assets.push(asset);
            }
            if candidate.assets.len() > MAX_ASSETS || candidate.byte_size() > MAX_LIBRARY_BYTES {
                return Err("Reference library limit: 16 images / 32 MiB encoded PNG; use fewer or smaller photos".into());
            }
        }
        *self = candidate;
        Ok(json!({"imported":imported, "embedded":true, "save_required":true}))
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("Unsupported reference asset schema; project was not opened".into());
        }
        if self.assets.len() > MAX_ASSETS || self.byte_size() > MAX_LIBRARY_BYTES {
            return Err("Reference library exceeds limits".into());
        }
        let mut ids = std::collections::HashSet::new();
        for asset in &self.assets {
            if !ids.insert(&asset.id) || asset.id != format!("ref-{}", asset.source_sha256) {
                return Err("Invalid/duplicate reference identity".into());
            }
            let bytes = STANDARD
                .decode(&asset.png_data)
                .map_err(|e| e.to_string())?;
            if hash(&bytes) != asset.png_sha256 {
                return Err("Reference image checksum mismatch".into());
            }
            let (image, _) = decode(&bytes)?;
            if image.width() != asset.width
                || image.height() != asset.height
                || asset.width > 2048
                || asset.height > 2048
            {
                return Err("Invalid stored reference dimensions".into());
            }
        }
        Ok(())
    }
}
