// SPDX-License-Identifier: MPL-2.0
//! EPSF-3.0 with vector strokes and editable text (embedded Type 42 TrueType).
//! Adobe Technical Note 5012 governs sfnts padding and glyph-name mapping.
use super::*;
use std::collections::BTreeSet;

fn u16_at(data: &[u8], n: usize) -> u16 {
    u16::from_be_bytes(data[n..n + 2].try_into().unwrap())
}
fn u32_at(data: &[u8], n: usize) -> u32 {
    u32::from_be_bytes(data[n..n + 4].try_into().unwrap())
}
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

// Keep original glyph IDs and metrics, but omit unused outlines. Composite
// dependencies are retained recursively. This avoids embedding the entire CJK
// font in every sheet and adds no runtime font or converter dependency.
fn subset(face: &ttf_parser::Face<'_>, used: impl Iterator<Item = u16>) -> Vec<u8> {
    let raw = face.raw_face();
    let table = |name| raw.table(ttf_parser::Tag::from_bytes(name)).unwrap();
    let mut head = table(b"head").to_vec();
    let loca = table(b"loca");
    let glyf = table(b"glyf");
    let long = u16_at(&head, 50) == 1;
    let offset = |gid: usize| {
        if long {
            u32_at(loca, gid * 4) as usize
        } else {
            u16_at(loca, gid * 2) as usize * 2
        }
    };
    let mut keep = BTreeSet::new();
    let mut pending: Vec<_> = used.chain(std::iter::once(0)).collect();
    while let Some(gid) = pending.pop() {
        if !keep.insert(gid) {
            continue;
        }
        let glyph = &glyf[offset(gid as usize)..offset(gid as usize + 1)];
        if glyph.len() < 10 || (u16_at(glyph, 0) as i16) >= 0 {
            continue;
        }
        let mut p = 10;
        loop {
            let flags = u16_at(glyph, p);
            pending.push(u16_at(glyph, p + 2));
            p += 4 + if flags & 1 != 0 { 4 } else { 2 };
            p += if flags & 8 != 0 {
                2
            } else if flags & 64 != 0 {
                4
            } else if flags & 128 != 0 {
                8
            } else {
                0
            };
            if flags & 32 == 0 {
                break;
            }
        }
    }
    let mut outlines = Vec::new();
    let mut locations = Vec::new();
    for gid in 0..face.number_of_glyphs() {
        locations.extend_from_slice(&(outlines.len() as u32).to_be_bytes());
        if keep.contains(&gid) {
            outlines.extend_from_slice(&glyf[offset(gid as usize)..offset(gid as usize + 1)]);
            while outlines.len() % 4 != 0 {
                outlines.push(0);
            }
        }
    }
    locations.extend_from_slice(&(outlines.len() as u32).to_be_bytes());
    head[8..12].fill(0); // checkSumAdjustment is calculated after the directory.
    head[50..52].copy_from_slice(&1u16.to_be_bytes());
    let mut tables = BTreeMap::from([
        (*b"head", head),
        (*b"loca", locations),
        (*b"glyf", outlines),
    ]);
    for name in [b"hhea", b"hmtx", b"maxp", b"cvt ", b"fpgm", b"prep"] {
        if let Some(data) = raw.table(ttf_parser::Tag::from_bytes(name)) {
            tables.insert(*name, data.to_vec());
        }
    }
    let count = tables.len() as u16;
    let power = 1u16 << count.ilog2();
    let mut out = vec![0; 12 + count as usize * 16];
    out[..4].copy_from_slice(&0x10000u32.to_be_bytes());
    for (pos, value) in [
        (4, count),
        (6, power * 16),
        (8, count.ilog2() as u16),
        (10, (count - power) * 16),
    ] {
        out[pos..pos + 2].copy_from_slice(&value.to_be_bytes());
    }
    let mut head_offset = 0;
    for (i, (tag, data)) in tables.into_iter().enumerate() {
        let entry = 12 + i * 16;
        let start = out.len();
        out[entry..entry + 4].copy_from_slice(&tag);
        out[entry + 4..entry + 8].copy_from_slice(&checksum(&data).to_be_bytes());
        out[entry + 8..entry + 12].copy_from_slice(&(start as u32).to_be_bytes());
        out[entry + 12..entry + 16].copy_from_slice(&(data.len() as u32).to_be_bytes());
        if tag == *b"head" {
            head_offset = start;
        }
        out.extend(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    let adjustment = 0xB1B0AFBAu32.wrapping_sub(checksum(&out));
    out[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    out
}

pub(super) fn page(s: &Sheet) -> std::io::Result<String> {
    let face = ttf_parser::Face::parse(FONT, 0).map_err(std::io::Error::other)?;
    let mut chars = BTreeMap::new();
    // Keep all basic Latin characters/digits available for numeric edits in
    // receiving editors, even when a particular digit was absent on this sheet.
    for c in s
        .texts
        .iter()
        .flat_map(|(_, text, _)| text.chars())
        .chain(' '..='~')
        .chain("Ø±°×".chars())
    {
        let gid = face.glyph_index(c).ok_or_else(|| {
            std::io::Error::other(format!("EPS font does not support U+{:04X}", c as u32))
        })?;
        chars.insert(c, gid);
    }
    let sfnt = subset(&face, chars.values().map(|g| g.0));
    let mut out = String::from("%!PS-Adobe-3.0 EPSF-3.0\n%%Creator: S33D\n%%BoundingBox: 0 0 842 596\n%%HiResBoundingBox: 0 0 841.889764 595.275591\n%%LanguageLevel: 3\n%%Pages: 1\n%%DocumentData: Clean7Bit\n%%DocumentSuppliedResources: font S33D-Drawing-Subset\n%%EndComments\n%%BeginProlog\n/S33DSave save def\n20 dict begin\n%%BeginResource: font S33D-Drawing-Subset\n11 dict begin\n/FontName /S33D-Drawing-Subset def\n/FontType 42 def\n/PaintType 0 def\n/FontMatrix [1 0 0 1 0 0] def\n/Encoding StandardEncoding def\n");
    let bbox = face.global_bounding_box();
    writeln!(
        out,
        "/FontBBox [{} {} {} {}] def",
        bbox.x_min, bbox.y_min, bbox.x_max, bbox.y_max
    )
    .unwrap();
    out.push_str("/sfnts [\n");
    // Level 3 supports tables across strings. Each chunk is even-sized plus
    // the ignored Type 42 padding byte, and below the 65535-byte PS limit.
    for chunk in sfnt.chunks(65532) {
        out.push_str("<\n");
        for line in chunk.chunks(32) {
            for byte in line {
                write!(out, "{byte:02X}").unwrap();
            }
            out.push('\n');
        }
        out.push_str("00>\n");
    }
    out.push_str("] def\n/CharStrings ");
    writeln!(out, "{} dict dup begin\n/.notdef 0 def", chars.len() + 1).unwrap();
    let name = |c: char| {
        if c as u32 <= 0xFFFF {
            format!("uni{:04X}", c as u32)
        } else {
            format!("u{:X}", c as u32)
        }
    };
    for (&c, gid) in &chars {
        writeln!(out, "/{} {} def", name(c), gid.0).unwrap();
    }
    out.push_str(
        "end readonly def\n/Encoding 256 array def\n0 1 255 { Encoding exch /.notdef put } for\n",
    );
    for &c in chars.keys().filter(|c| c.is_ascii()) {
        writeln!(out, "Encoding {} /{} put", c as u32, name(c)).unwrap();
    }
    out.push_str("FontName currentdict end definefont pop\n%%EndResource\n%%EndProlog\n%%Page: 1 1\ngsave\n2.8346456693 dup scale\n1 setlinecap 1 setlinejoin\n0 setgray 0.3 setlinewidth\nnewpath 7 7 moveto 290 7 lineto 290 203 lineto 7 203 lineto closepath stroke\n0.4 setgray 0.18 setlinewidth [1.8 1] 0 setdash\n");
    let line = |out: &mut String, a: Vec2, b: Vec2| {
        writeln!(
            out,
            "newpath {} {} moveto {} {} lineto stroke",
            a.x,
            210. - a.y,
            b.x,
            210. - b.y
        )
        .unwrap()
    };
    for &[a, b] in &s.dashed {
        line(&mut out, a, b);
    }
    out.push_str("[] 0 setdash 0 setgray\n");
    for &([a, b], thin) in &s.lines {
        writeln!(out, "{} setlinewidth", if thin { 0.18 } else { 0.3 }).unwrap();
        line(&mut out, a, b);
    }
    out.push_str("/S33D-Drawing-Subset findfont 3 scalefont setfont\n");
    let em = face.units_per_em() as f32;
    for (p, text, angle) in &s.texts {
        let width = text
            .chars()
            .map(|c| face.glyph_hor_advance(chars[&c]).unwrap_or(0) as f32)
            .sum::<f32>()
            * 3.
            / em;
        let baseline = -(face.ascender() + face.descender()) as f32 * 1.5 / em;
        writeln!(
            out,
            "gsave {} {} translate {} rotate {} {} moveto",
            p.x,
            210. - p.y,
            -angle.to_degrees(),
            -width / 2.,
            baseline
        )
        .unwrap();
        // Glyph names preserve Unicode/AGL semantics without lossy 8-bit text
        // conversion. glyphshow remains a text operator, never outlined paths.
        // ASCII runs stay ordinary editable strings (including dimensions).
        let mut run = String::new();
        let flush = |out: &mut String, run: &mut String| {
            if !run.is_empty() {
                writeln!(out, "({run}) show").unwrap();
                run.clear();
            }
        };
        for c in text.chars() {
            if c.is_ascii() && !c.is_control() {
                if matches!(c, '(' | ')' | '\\') {
                    run.push('\\');
                }
                run.push(c);
            } else {
                flush(&mut out, &mut run);
                writeln!(out, "/{} glyphshow", name(c)).unwrap();
            }
        }
        flush(&mut out, &mut run);
        out.push_str("grestore\n");
    }
    out.push_str("grestore\nend\nS33DSave restore\nshowpage\n%%Trailer\n%%EOF\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eps_is_vector_with_embedded_subset_unicode_and_rotated_editable_text() {
        let data = page(&super::super::tests::fixture()).unwrap();
        assert!(data.starts_with("%!PS-Adobe-3.0 EPSF-3.0"));
        assert!(data.contains("/FontType 42"));
        assert!(data.contains("[1.8 1] 0 setdash"));
        assert!(data.contains("-90 rotate"));
        assert!(data.contains("glyphshow"));
        assert!(data.contains("Encoding 57 /uni0039 put"));
        assert!(!data.contains("charpath"));
        assert!(
            data.len() < 1_000_000,
            "subset should stay compact: {}",
            data.len()
        );
        let font = subset(&ttf_parser::Face::parse(FONT, 0).unwrap(), [0].into_iter());
        assert_eq!(checksum(&font), 0xB1B0AFBA);
        assert!(data.ends_with("%%EOF\n"));
    }
}
