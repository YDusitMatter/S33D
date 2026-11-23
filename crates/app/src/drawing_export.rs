// SPDX-License-Identifier: MPL-2.0
//! Paper-space vector exports. The edited Sheet is the single source for every format.
use super::*;
use std::{collections::BTreeMap, fmt::Write as _, io::Write as _, path::Path};
#[path = "drawing_eps.rs"]
mod eps;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Svg,
    Dxf,
    Pdf,
    Eps,
}
impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Dxf => "dxf",
            Self::Pdf => "pdf",
            Self::Eps => "eps",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Svg => "SVG",
            Self::Dxf => "DXF",
            Self::Pdf => "PDF",
            Self::Eps => "EPS",
        }
    }
}

pub(super) fn files(
    d: &Drawing,
    unit: UiUnit,
    zh: bool,
    path: &Path,
    format: ExportFormat,
) -> std::io::Result<usize> {
    // Never silently put PDF/SVG bytes in a .dwg or mismatched file.
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(format.extension()))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Expected .{} file extension", format.extension()),
        ));
    }
    let pages = features::pages(d, unit, zh);
    if format == ExportFormat::Pdf {
        let sheets: Vec<_> = (0..pages).map(|p| sheet_page(d, unit, zh, p)).collect();
        std::fs::write(path, pdf(&sheets)?)?;
    } else {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        for page in 0..pages {
            let output = if page == 0 {
                path.to_owned()
            } else {
                path.with_file_name(format!("{stem}-dimensions-{page}.{}", format.extension()))
            };
            let data = match format {
                ExportFormat::Svg => svg_page(d, unit, zh, page),
                ExportFormat::Dxf => dxf(&sheet_page(d, unit, zh, page)),
                ExportFormat::Eps => eps::page(&sheet_page(d, unit, zh, page))?,
                ExportFormat::Pdf => unreachable!(),
            };
            std::fs::write(output, data)?;
        }
    }
    Ok(pages)
}

pub fn menu(app: &mut S33DApp, ui: &mut egui::Ui, label: &str) {
    ui.add_enabled_ui(app.drawing.views.len() == 3, |ui| {
        ui.menu_button(label, |ui| {
            for format in [ExportFormat::Svg, ExportFormat::Dxf, ExportFormat::Pdf, ExportFormat::Eps] {
                if ui.button(format!("{}…", format.label())).clicked() {
                    ui.close_menu();
                    dialog(app, format);
                }
            }
            ui.separator();
            ui.add_enabled(false, egui::Button::new("DWG…")).on_disabled_hover_text(
                app.settings.language.text(
                    "Native DWG is not included: it needs a separately licensed writer. Export DXF and convert in your CAD application.",
                    "尚未内置 DWG：需要另行许可的写入器。请先导出 DXF，再用 CAD 软件转换。"));
        });
    });
}

fn dialog(app: &mut S33DApp, format: ExportFormat) {
    let ext = format.extension();
    if let Some(path) = crate::ui::file_dialog(app, true)
        .add_filter(format.label(), &[ext])
        .set_file_name(format!("drawing.{ext}"))
        .save_file()
    {
        app.remember_directory(&path, true);
        let zh = app.settings.language == crate::app::UiLanguage::ChineseSimplified;
        app.drawing.message = match files(&app.drawing, app.settings.unit, zh, &path, format) {
            Ok(n) => {
                if zh {
                    format!(
                        "已导出 {n} 张图纸（{}）：{}",
                        format.label(),
                        path.display()
                    )
                } else {
                    format!("Exported {n} {} sheets: {}", format.label(), path.display())
                }
            }
            Err(e) => {
                if zh {
                    format!("导出未全部完成：{e}")
                } else {
                    format!("Export did not complete: {e}")
                }
            }
        };
    }
}

pub(crate) fn export_to_path(app: &mut S33DApp, path: &Path, extension: &str) {
    let format = match extension {
        "svg" => ExportFormat::Svg,
        "dxf" => ExportFormat::Dxf,
        "pdf" => ExportFormat::Pdf,
        "eps" => ExportFormat::Eps,
        _ => return,
    };
    let zh = app.settings.language == crate::app::UiLanguage::ChineseSimplified;
    let message = match files(&app.drawing, app.settings.unit, zh, path, format) {
        Ok(count) => {
            if zh {
                format!("已导出 {count} 张图纸：{}", path.display())
            } else {
                format!("Exported {count} sheets: {}", path.display())
            }
        }
        Err(error) => {
            if zh {
                format!("导出未全部完成：{error}")
            } else {
                format!("Export did not complete: {error}")
            }
        }
    };
    app.drawing.message = message.clone();
    app.toast(message);
}

// DXF uses Y-up coordinates. All entities are sheet-space mm, regardless of dimension label units.
// R2000 supports Unicode escapes, line weights and true aligned TEXT, without a DWG dependency.
fn dxf_text(value: &str) -> String {
    let mut result = String::new();
    for c in value.chars() {
        if c.is_ascii() && !c.is_control() && c != '\\' {
            result.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                write!(result, "\\U+{unit:04X}").unwrap();
            }
        }
    }
    result
}
fn dxf(s: &Sheet) -> String {
    let mut out = String::from("999\nS33D A4 sheet; paper-space mm; dimensions show model values at the stated scale\n0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$INSUNITS\n70\n4\n9\n$MEASUREMENT\n70\n1\n9\n$LUNITS\n70\n2\n9\n$EXTMIN\n10\n0\n20\n0\n30\n0\n9\n$EXTMAX\n10\n297\n20\n210\n30\n0\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n");
    // Stable unique handles and ownership allow ordinary CAD editors to reopen and modify the drawing.
    out.push_str("0\nTABLE\n2\nLTYPE\n5\n1\n100\nAcDbSymbolTable\n70\n2\n0\nLTYPE\n5\n2\n330\n1\n100\nAcDbSymbolTableRecord\n100\nAcDbLinetypeTableRecord\n2\nCONTINUOUS\n70\n0\n3\nSolid line\n72\n65\n73\n0\n40\n0\n0\nLTYPE\n5\n3\n330\n1\n100\nAcDbSymbolTableRecord\n100\nAcDbLinetypeTableRecord\n2\nHIDDEN\n70\n0\n3\nHidden line\n72\n65\n73\n2\n40\n2.8\n49\n1.8\n74\n0\n49\n-1.0\n74\n0\n0\nENDTAB\n");
    out.push_str("0\nTABLE\n2\nLAYER\n5\n4\n100\nAcDbSymbolTable\n70\n4\n");
    for (handle, name, lt, color, weight) in [
        (5, "0", "CONTINUOUS", 7, 30),
        (6, "OUTLINE", "CONTINUOUS", 7, 30),
        (7, "ANNOTATION", "CONTINUOUS", 7, 18),
        (8, "HIDDEN", "HIDDEN", 8, 18),
    ] {
        writeln!(out,"0\nLAYER\n5\n{handle:X}\n330\n4\n100\nAcDbSymbolTableRecord\n100\nAcDbLayerTableRecord\n2\n{name}\n70\n0\n62\n{color}\n6\n{lt}\n370\n{weight}").unwrap();
    }
    out.push_str("0\nENDTAB\n0\nTABLE\n2\nSTYLE\n5\n9\n100\nAcDbSymbolTable\n70\n1\n0\nSTYLE\n5\nA\n330\n9\n100\nAcDbSymbolTableRecord\n100\nAcDbTextStyleTableRecord\n2\nSTANDARD\n70\n0\n40\n0\n41\n1\n50\n0\n71\n0\n42\n3\n3\nNotoSansSC.ttf\n4\n\n0\nENDTAB\n0\nTABLE\n2\nBLOCK_RECORD\n5\nB\n100\nAcDbSymbolTable\n70\n2\n");
    for (handle, name) in [(12, "*Model_Space"), (13, "*Paper_Space")] {
        writeln!(out,"0\nBLOCK_RECORD\n5\n{handle:X}\n330\nB\n100\nAcDbSymbolTableRecord\n100\nAcDbBlockTableRecord\n2\n{name}").unwrap();
    }
    out.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n");
    for (handle, owner, name) in [(14, 12, "*Model_Space"), (16, 13, "*Paper_Space")] {
        writeln!(out,"0\nBLOCK\n5\n{handle:X}\n330\n{owner:X}\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockBegin\n2\n{name}\n70\n0\n10\n0\n20\n0\n30\n0\n3\n{name}\n1\n\n0\nENDBLK\n5\n{:X}\n330\n{owner:X}\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockEnd",handle+1).unwrap();
    }
    out.push_str("0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    let mut handle = 256u32;
    let border = [
        (Vec2::new(7., 7.), Vec2::new(290., 7.)),
        (Vec2::new(290., 7.), Vec2::new(290., 203.)),
        (Vec2::new(290., 203.), Vec2::new(7., 203.)),
        (Vec2::new(7., 203.), Vec2::new(7., 7.)),
    ];
    for (a, b, layer) in border
        .into_iter()
        .map(|(a, b)| (a, b, "OUTLINE"))
        .chain(s.dashed.iter().map(|[a, b]| (*a, *b, "HIDDEN")))
        .chain(
            s.lines
                .iter()
                .map(|([a, b], thin)| (*a, *b, if *thin { "ANNOTATION" } else { "OUTLINE" })),
        )
    {
        writeln!(out,"0\nLINE\n5\n{handle:X}\n330\nC\n100\nAcDbEntity\n8\n{layer}\n100\nAcDbLine\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0",a.x,210.-a.y,b.x,210.-b.y).unwrap();
        handle += 1;
    }
    for (p, text, angle) in &s.texts {
        writeln!(out,"0\nTEXT\n5\n{handle:X}\n330\nC\n100\nAcDbEntity\n8\nANNOTATION\n100\nAcDbText\n10\n{}\n20\n{}\n30\n0\n40\n3\n1\n{}\n50\n{}\n7\nSTANDARD\n72\n1\n11\n{}\n21\n{}\n31\n0\n100\nAcDbText\n73\n2",p.x,210.-p.y,dxf_text(text),-angle.to_degrees(),p.x,210.-p.y).unwrap();
        handle += 1;
    }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    out
}

const FONT: &[u8] = include_bytes!("../../../assets/fonts/NotoSansSC.ttf");

/// Minimal PDF 1.7 document writer. Font and vector content are losslessly compressed;
/// embedded CID TrueType + ToUnicode keeps Chinese text portable, searchable and selectable.
#[derive(Default)]
struct Pdf {
    objects: Vec<Vec<u8>>,
}
impl Pdf {
    fn add(&mut self, bytes: impl Into<Vec<u8>>) -> usize {
        self.objects.push(bytes.into());
        self.objects.len()
    }
    fn stream(&mut self, dict: &str, bytes: &[u8]) -> std::io::Result<usize> {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(bytes)?;
        let data = encoder.finish()?;
        let mut stream = format!(
            "<< {dict} /Length {} /Filter /FlateDecode >>\nstream\n",
            data.len()
        )
        .into_bytes();
        stream.extend(data);
        stream.extend_from_slice(b"\nendstream");
        Ok(self.add(stream))
    }
    fn finish(self) -> Vec<u8> {
        let mut out = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let mut offsets = vec![0];
        for (i, object) in self.objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend(object);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes(),
        );
        for offset in offsets.iter().skip(1) {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                offsets.len()
            )
            .as_bytes(),
        );
        out
    }
}
fn pdf(sheets: &[Sheet]) -> std::io::Result<Vec<u8>> {
    let face = ttf_parser::Face::parse(FONT, 0).map_err(std::io::Error::other)?;
    let em = face.units_per_em() as f32;
    let mut chars = BTreeMap::new();
    for c in sheets
        .iter()
        .flat_map(|s| s.texts.iter())
        .flat_map(|(_, t, _)| t.chars())
    {
        let gid = face.glyph_index(c).ok_or_else(|| {
            std::io::Error::other(format!("PDF font does not support U+{:04X}", c as u32))
        })?;
        chars.entry(gid.0).or_insert(c);
    }
    let mut pdf = Pdf::default();
    pdf.add(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    pdf.add(Vec::new()); // Page tree filled after stream/page allocation.
    let font_file = pdf.stream(&format!("/Length1 {}", FONT.len()), FONT)?;
    let bbox = face.global_bounding_box();
    let scaled = |n: i16| n as f32 * 1000. / em;
    let descriptor = pdf.add(format!("<< /Type /FontDescriptor /FontName /NotoSansSC-Thin /Flags 4 /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile2 {font_file} 0 R >>",scaled(bbox.x_min),scaled(bbox.y_min),scaled(bbox.x_max),scaled(bbox.y_max),scaled(face.ascender()),scaled(face.descender()),scaled(face.capital_height().unwrap_or(face.ascender()))).into_bytes());
    let widths = chars
        .keys()
        .map(|&gid| {
            format!(
                "{gid} [{}]",
                face.glyph_hor_advance(ttf_parser::GlyphId(gid))
                    .unwrap_or(1000) as f32
                    * 1000.
                    / em
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let cid = pdf.add(format!("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /NotoSansSC-Thin /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /CIDToGIDMap /Identity /DW 1000 /W [{widths}] >>").into_bytes());
    let mut cmap = String::from("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /S33DUnicode def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
    let chars: Vec<_> = chars.into_iter().collect();
    for chunk in chars.chunks(100) {
        writeln!(cmap, "{} beginbfchar", chunk.len()).unwrap();
        for (gid, c) in chunk {
            let mut buffer = [0u16; 2];
            let utf16 = c
                .encode_utf16(&mut buffer)
                .iter()
                .map(|u| format!("{u:04X}"))
                .collect::<String>();
            writeln!(cmap, "<{gid:04X}> <{utf16}>").unwrap();
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let unicode = pdf.stream("", cmap.as_bytes())?;
    let font = pdf.add(format!("<< /Type /Font /Subtype /Type0 /BaseFont /NotoSansSC-Thin /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {unicode} 0 R >>").into_bytes());
    let mut kids = Vec::new();
    for s in sheets {
        let mut content = String::from("q\n2.8346456693 0 0 2.8346456693 0 0 cm\n1 J 1 j\n0 G 0 g\n0.3 w\n7 7 283 196 re S\n0.4 G 0.18 w [1.8 1] 0 d\n");
        for [a, b] in &s.dashed {
            writeln!(
                content,
                "{} {} m {} {} l S",
                a.x,
                210. - a.y,
                b.x,
                210. - b.y
            )
            .unwrap();
        }
        content.push_str("[] 0 d 0 G\n");
        for ([a, b], thin) in &s.lines {
            writeln!(
                content,
                "{} w {} {} m {} {} l S",
                if *thin { 0.18 } else { 0.3 },
                a.x,
                210. - a.y,
                b.x,
                210. - b.y
            )
            .unwrap();
        }
        for (p, text, angle) in &s.texts {
            let gids: Vec<_> = text.chars().map(|c| face.glyph_index(c).unwrap()).collect();
            let width = gids
                .iter()
                .map(|&g| face.glyph_hor_advance(g).unwrap_or(0) as f32)
                .sum::<f32>()
                * 3.
                / em;
            let baseline = -(face.ascender() + face.descender()) as f32 * 1.5 / em;
            let (sin, cos) = (-angle).sin_cos();
            let x = p.x - cos * width / 2. - sin * baseline;
            let y = 210. - p.y - sin * width / 2. + cos * baseline;
            let encoded = gids
                .iter()
                .map(|g| format!("{:04X}", g.0))
                .collect::<String>();
            writeln!(
                content,
                "BT /F1 3 Tf {cos} {sin} {} {cos} {x} {y} Tm <{encoded}> Tj ET",
                -sin
            )
            .unwrap();
        }
        content.push_str("Q\n");
        let stream = pdf.stream("", content.as_bytes())?;
        let page=pdf.add(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 841.889764 595.275591] /Resources << /Font << /F1 {font} 0 R >> >> /Contents {stream} 0 R >>").into_bytes());
        kids.push(format!("{page} 0 R"));
    }
    pdf.objects[1] = format!(
        "<< /Type /Pages /Count {} /Kids [{}] >>",
        kids.len(),
        kids.join(" ")
    )
    .into_bytes();
    Ok(pdf.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn fixture() -> Sheet {
        Sheet {
            lines: vec![([Vec2::new(10., 20.), Vec2::new(40., 50.)], false)],
            dashed: vec![[Vec2::new(20., 30.), Vec2::new(30., 40.)]],
            texts: vec![(
                Vec2::new(50., 60.),
                "中文 Ø6.000 mm <修改> \\".into(),
                std::f32::consts::FRAC_PI_2,
            )],
            ..Default::default()
        }
    }
    #[test]
    fn dxf_uses_mm_y_up_rotated_unicode_text_and_hidden_layer() {
        let data = dxf(&fixture());
        assert!(data.contains("$ACADVER\n1\nAC1015\n"));
        assert!(data.contains("$INSUNITS\n70\n4\n"));
        assert!(data.contains("10\n10\n20\n190\n30\n0\n11\n40\n21\n160\n"));
        assert!(data.contains("8\nHIDDEN\n100\nAcDbLine"));
        assert!(data.contains("50\n-90\n7\nSTANDARD\n72\n1\n11\n50\n21\n150\n"));
        assert!(data.contains("\\U+4E2D\\U+6587 \\U+00D86.000 mm <\\U+4FEE\\U+6539> \\U+005C"));
        assert_eq!(dxf_text("😀"), "\\U+D83D\\U+DE00");
        assert!(data.ends_with("0\nEOF\n"));
    }
    #[test]
    fn pdf_embeds_unicode_font_page_tree_and_correct_xref_offsets() {
        let data = pdf(&[fixture(), fixture()]).unwrap();
        assert!(data.starts_with(b"%PDF-1.7"));
        let text = String::from_utf8_lossy(&data);
        for token in [
            "/Count 2",
            "/MediaBox [0 0 841.889764 595.275591]",
            "/FontFile2",
            "/ToUnicode",
            "/CIDToGIDMap /Identity",
        ] {
            assert!(text.contains(token), "missing {token}");
        }
        let xref = data.windows(5).rposition(|w| w == b"xref\n").unwrap();
        // Last occurrence is startxref; parse its offset and independently locate the actual table.
        let offset = std::str::from_utf8(&data[xref + 5..])
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert_eq!(&data[offset..offset + 5], b"xref\n");
        let table = std::str::from_utf8(&data[offset..]).unwrap();
        for (i, line) in table
            .lines()
            .skip(3)
            .take_while(|l| l.ends_with("n "))
            .enumerate()
        {
            let pos = line[..10].parse::<usize>().unwrap();
            assert!(data[pos..].starts_with(format!("{} 0 obj", i + 1).as_bytes()));
        }
    }
    #[test]
    fn rejects_wrong_extension_before_touching_existing_file() {
        let path = std::env::temp_dir().join(format!("s33d-export-{}.dwg", std::process::id()));
        std::fs::write(&path, b"keep existing drawing").unwrap();
        let err = files(
            &Drawing::default(),
            UiUnit::Millimeter,
            false,
            &path,
            ExportFormat::Dxf,
        )
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read(&path).unwrap(), b"keep existing drawing");
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn every_format_uses_edited_sheet_text_including_schedule_pages() {
        let mut d = Drawing {
            views: vec![
                Projection {
                    min: Vec2::ZERO,
                    max: Vec2::splat(30.),
                    ..Default::default()
                };
                3
            ],
            title: "修改后的图纸 A & B".into(),
            drawing_number: "S33D-001".into(),
            ..Default::default()
        };
        for id in 1..=9 {
            d.components.push(Component {
                id,
                name: format!("圆柱 {id}"),
                min: Vec3::ZERO,
                max: Vec3::splat(30.),
                ..Default::default()
            });
        }
        let pages = features::pages(&d, UiUnit::Inch, true);
        assert!(pages > 1);
        let raw = sheet_page(&d, UiUnit::Inch, true, 1);
        let key = edit::TextKey::new(&raw, 1, 0);
        d.text_edits.insert(key, "明细修改 Ø1.1811 in".into());
        let output =
            std::env::temp_dir().join(format!("s33d-vector-export-{}", std::process::id()));
        std::fs::create_dir_all(&output).unwrap();
        for format in [
            ExportFormat::Svg,
            ExportFormat::Dxf,
            ExportFormat::Pdf,
            ExportFormat::Eps,
        ] {
            let path = output.join(format!("edited.{}", format.extension()));
            assert_eq!(files(&d, UiUnit::Inch, true, &path, format).unwrap(), pages);
            if format != ExportFormat::Pdf {
                let companion = std::fs::read_to_string(
                    output.join(format!("edited-dimensions-1.{}", format.extension())),
                )
                .unwrap();
                let expected = if format == ExportFormat::Eps {
                    "(1.1811 in) show".into()
                } else if format == ExportFormat::Svg {
                    "明细修改 Ø1.1811 in".into()
                } else {
                    dxf_text("明细修改 Ø1.1811 in")
                };
                assert!(companion.contains(&expected));
            }
        }
        if let Some(root) = std::env::var_os("S33D_DRAWING_EXPORT_QA") {
            let root = Path::new(&root);
            std::fs::create_dir_all(root).unwrap();
            for entry in std::fs::read_dir(&output).unwrap() {
                let entry = entry.unwrap();
                std::fs::copy(entry.path(), root.join(entry.file_name())).unwrap();
            }
        }
        for entry in std::fs::read_dir(&output).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(output).unwrap();
    }
}
