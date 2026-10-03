use crate::domain::{
    models::{Envelope, Error, FieldKind},
    ports::Documents,
};
use lopdf::{
    Document, Object, Stream,
    content::{Content, Operation},
    dictionary,
};

#[cfg(test)]
mod test;

/// PDF processor supporting ordinary, unencrypted, unrotated documents.
#[derive(Clone, Copy)]
pub struct Pdf;
fn pdf_error(error: impl std::fmt::Display) -> Error {
    Error::Pdf(format!("Could not process PDF: {error}"))
}
fn page_box(doc: &Document, id: lopdf::ObjectId) -> Result<[f64; 4], Error> {
    let mut node = id;
    loop {
        let dictionary = doc.get_dictionary(node).map_err(pdf_error)?;
        if let Ok(object) = dictionary.get(b"MediaBox") {
            let array = doc
                .dereference(object)
                .map_err(pdf_error)?
                .1
                .as_array()
                .map_err(pdf_error)?;
            if array.len() != 4 {
                return Err(pdf_error("Invalid page dimensions"));
            }
            let mut values = [0.0; 4];
            for (i, value) in array.iter().enumerate() {
                values[i] = value.as_float().map_err(pdf_error)? as f64;
            }
            return Ok(values);
        }
        node = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .map_err(pdf_error)?;
    }
}
fn page_rotation(doc: &Document, mut id: lopdf::ObjectId) -> i64 {
    loop {
        let Ok(dict) = doc.get_dictionary(id) else {
            return 0;
        };
        if let Ok(rotation) = dict.get(b"Rotate").and_then(Object::as_i64) {
            return rotation.rem_euclid(360);
        }
        match dict.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent) => id = parent,
            Err(_) => return 0,
        }
    }
}
const REGULAR_FONT: &[u8] = include_bytes!("../../assets/DejaVuSans.ttf");
const SIGNATURE_FONT: &[u8] = include_bytes!("../../assets/DejaVuSerif-Italic.ttf");
fn face(font: &str) -> ttf_parser::Face<'static> {
    ttf_parser::Face::parse(
        if font == "MacroLegalSignature" {
            SIGNATURE_FONT
        } else {
            REGULAR_FONT
        },
        0,
    )
    .expect("bundled fonts are valid")
}
fn text(operations: &mut Vec<Operation>, value: &str, x: f64, y: f64, size: f64, font: &str) {
    let face = face(font);
    let glyphs: Vec<u8> = value
        .chars()
        .flat_map(|c| {
            face.glyph_index(c)
                .unwrap_or(ttf_parser::GlyphId(0))
                .0
                .to_be_bytes()
        })
        .collect();
    operations.extend([
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![Object::Name(font.as_bytes().to_vec()), size.into()],
        ),
        Operation::new("Td", vec![x.into(), y.into()]),
        Operation::new(
            "Tj",
            vec![Object::String(glyphs, lopdf::StringFormat::Hexadecimal)],
        ),
        Operation::new("ET", vec![]),
    ]);
}
fn embed_font(
    doc: &mut Document,
    bytes: &'static [u8],
    name: &str,
    values: &str,
) -> lopdf::ObjectId {
    use std::collections::BTreeMap;
    let face = ttf_parser::Face::parse(bytes, 0).expect("bundled font is valid");
    let units = f64::from(face.units_per_em());
    let font_file = doc.add_object(Stream::new(
        dictionary! { "Length1" => bytes.len() as i64 },
        bytes.to_vec(),
    ));
    let bounds = face.global_bounding_box();
    let descriptor = doc.add_object(dictionary!{
        "Type" => "FontDescriptor", "FontName" => name, "Flags" => 32,
        "FontBBox" => vec![(f64::from(bounds.x_min) / units * 1000.0).into(), (f64::from(bounds.y_min) / units * 1000.0).into(), (f64::from(bounds.x_max) / units * 1000.0).into(), (f64::from(bounds.y_max) / units * 1000.0).into()],
        "ItalicAngle" => if name.contains("Italic") { -12 } else { 0 },
        "Ascent" => f64::from(face.ascender()) / units * 1000.0,
        "Descent" => f64::from(face.descender()) / units * 1000.0,
        "CapHeight" => f64::from(face.capital_height().unwrap_or(face.ascender())) / units * 1000.0,
        "StemV" => 80, "FontFile2" => font_file,
    });
    let glyphs: BTreeMap<u16, char> = values
        .chars()
        .filter_map(|c| face.glyph_index(c).map(|g| (g.0, c)))
        .collect();
    let mut widths = vec![];
    for glyph in glyphs.keys() {
        widths.push(Object::Integer(i64::from(*glyph)));
        widths.push(Object::Array(vec![
            (f64::from(
                face.glyph_hor_advance(ttf_parser::GlyphId(*glyph))
                    .unwrap_or(600),
            ) / units
                * 1000.0)
                .into(),
        ]));
    }
    let cid = doc.add_object(dictionary!{ "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => name,
        "CIDSystemInfo" => dictionary!{ "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        "FontDescriptor" => descriptor, "CIDToGIDMap" => "Identity", "DW" => 600, "W" => Object::Array(widths),
    });
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /MacroLegalUnicode def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    for chunk in glyphs.into_iter().collect::<Vec<_>>().chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (glyph, c) in chunk {
            let mut utf16 = [0u16; 2];
            let unicode = c
                .encode_utf16(&mut utf16)
                .iter()
                .map(|v| format!("{v:04X}"))
                .collect::<String>();
            cmap.push_str(&format!("<{glyph:04X}> <{unicode}>\n"));
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend");
    let unicode = doc.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
    doc.add_object(dictionary!{ "Type" => "Font", "Subtype" => "Type0", "BaseFont" => name, "Encoding" => "Identity-H", "DescendantFonts" => vec![cid.into()], "ToUnicode" => unicode })
}
fn add_fonts(
    doc: &mut Document,
    page: lopdf::ObjectId,
    regular: lopdf::ObjectId,
    signature: lopdf::ObjectId,
) -> Result<(), Error> {
    // Clone inherited resources to avoid mutating resources used by other pages.
    let (resources, inherited) = doc.get_page_resources(page).map_err(pdf_error)?;
    let mut resources = resources
        .cloned()
        .or_else(|| {
            inherited
                .first()
                .and_then(|id| doc.get_dictionary(*id).ok())
                .cloned()
        })
        .unwrap_or_default();
    let mut fonts = resources
        .get(b"Font")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    fonts.set("MacroLegalRegular", regular);
    fonts.set("MacroLegalSignature", signature);
    resources.set("Font", fonts);
    doc.get_dictionary_mut(page)
        .map_err(pdf_error)?
        .set("Resources", resources);
    Ok(())
}
impl Documents for Pdf {
    fn validate_text(&self, text: &str) -> Result<(), Error> {
        let regular = face("MacroLegalRegular");
        let signature = face("MacroLegalSignature");
        if text.chars().any(|c| {
            !c.is_whitespace()
                && (regular.glyph_index(c).is_none() || signature.glyph_index(c).is_none())
        }) {
            return Err(Error::Invalid("A character cannot be rendered by the signature font. Use supported Latin, Greek, or Cyrillic characters.".into()));
        }
        Ok(())
    }

    fn inspect(&self, bytes: &[u8]) -> Result<u32, Error> {
        if !bytes.starts_with(b"%PDF-") {
            return Err(pdf_error("Upload a PDF document"));
        }
        let doc = Document::load_mem(bytes).map_err(pdf_error)?;
        if doc.is_encrypted() {
            return Err(pdf_error("Password-protected PDFs are not supported"));
        }
        let pages = doc.get_pages();
        if pages.is_empty() || pages.len() > 100 {
            return Err(pdf_error("Use a PDF with 1–100 pages"));
        }
        // Active PDF actions and existing signatures cannot safely survive flattening.
        if doc
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .any(|d| {
                d.has(b"JavaScript")
                    || d.has(b"JS")
                    || d.has(b"AA")
                    || d.has(b"OpenAction")
                    || d.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"Sig")
                    || d.has(b"XFA")
            })
        {
            return Err(pdf_error(
                "Use a static PDF without scripts, existing digital signatures, or XFA forms",
            ));
        }
        for id in pages.values() {
            let bounds = page_box(&doc, *id)?;
            if bounds[2] <= bounds[0]
                || bounds[3] <= bounds[1]
                || !bounds.iter().all(|v| v.is_finite())
            {
                return Err(pdf_error("Invalid page dimensions"));
            }
            if page_rotation(&doc, *id) != 0 {
                return Err(pdf_error(
                    "Rotate pages to portrait/landscape in your PDF editor before uploading",
                ));
            }
            let page = doc.get_dictionary(*id).map_err(pdf_error)?;
            if page.has(b"CropBox") || page.has(b"UserUnit") {
                return Err(pdf_error(
                    "Export an uncropped PDF with standard page units before uploading",
                ));
            }
        }
        Ok(pages.len() as u32)
    }
    fn complete(&self, bytes: &[u8], envelope: &Envelope) -> Result<Vec<u8>, Error> {
        self.inspect(bytes)?;
        let mut doc = Document::load_mem(bytes).map_err(pdf_error)?;
        let values = format!(
            "{}{}",
            (32u8..=126).map(char::from).collect::<String>(),
            serde_json::to_string(envelope).map_err(pdf_error)?
        );
        let regular = embed_font(&mut doc, REGULAR_FONT, "DejaVuSans", &values);
        let signature = embed_font(&mut doc, SIGNATURE_FONT, "DejaVuSerif-Italic", &values);
        for (number, id) in doc.get_pages() {
            add_fonts(&mut doc, id, regular, signature)?;
            let bounds = page_box(&doc, id)?;
            let width = bounds[2] - bounds[0];
            let height = bounds[3] - bounds[1];
            let mut operations = vec![Operation::new("q", vec![])];
            for field in envelope.fields.iter().filter(|f| f.page == number) {
                let Some(value) = &field.value else {
                    continue;
                };
                let font = if matches!(field.kind, FieldKind::Signature | FieldKind::Initials) {
                    "MacroLegalSignature"
                } else {
                    "MacroLegalRegular"
                };
                let size = (field.height * height * 0.55)
                    .clamp(7.0, 24.0)
                    .min(field.width * width / (value.len().max(1) as f64 * 0.6));
                text(
                    &mut operations,
                    value,
                    bounds[0] + field.x * width + 3.0,
                    bounds[3] - field.y * height - field.height * height * 0.72,
                    size,
                    font,
                );
            }
            operations.push(Operation::new("Q", vec![]));
            let original = doc.get_page_content(id).map_err(pdf_error)?;
            let mut content = b"q\n".to_vec();
            content.extend(original);
            content.extend(b"\nQ\n");
            content.extend(Content { operations }.encode().map_err(pdf_error)?);
            let content_id = doc.add_object(Stream::new(dictionary! {}, content));
            doc.get_dictionary_mut(id)
                .map_err(pdf_error)?
                .set("Contents", content_id);
        }
        let mut lines = vec![
            "CERTIFICATE OF COMPLETION".into(),
            format!("Envelope: {}", envelope.id),
            format!("Subject: {}", envelope.title),
            format!("Source SHA-256: {}", envelope.source_sha256),
            "Electronic records and signature consent recorded for every signer.".into(),
            "All timestamps are UTC. Identity verified by possession of emailed signing link."
                .into(),
            String::new(),
        ];
        for recipient in &envelope.recipients {
            lines.push(format!(
                "{} <{}> | signed {}",
                recipient.name,
                recipient.email,
                recipient
                    .signed_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default()
            ));
        }
        lines.push(String::new());
        for event in &envelope.audit {
            lines.push(format!(
                "{} | {:?} | {}",
                event.at.to_rfc3339(),
                event.action,
                event.actor
            ));
            for chunk in event.detail.as_bytes().chunks(95) {
                lines.push(String::from_utf8_lossy(chunk).into());
            }
        }
        let root = doc
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .map_err(pdf_error)?;
        let pages_id = doc
            .get_dictionary(root)
            .and_then(|d| d.get(b"Pages"))
            .and_then(Object::as_reference)
            .map_err(pdf_error)?;
        for chunk in lines.chunks(42) {
            let mut operations = vec![];
            for (i, line) in chunk.iter().enumerate() {
                text(
                    &mut operations,
                    line,
                    42.0,
                    748.0 - i as f64 * 16.0,
                    if i == 0 { 13.0 } else { 9.0 },
                    "MacroLegalRegular",
                );
            }
            let content = doc.add_object(Stream::new(
                dictionary! {},
                Content { operations }.encode().map_err(pdf_error)?,
            ));
            let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()], "Resources" => dictionary!{ "Font" => dictionary!{ "MacroLegalRegular" => regular } }, "Contents" => content });
            let pages = doc.get_dictionary_mut(pages_id).map_err(pdf_error)?;
            pages
                .get_mut(b"Kids")
                .and_then(Object::as_array_mut)
                .map_err(pdf_error)?
                .push(page.into());
            let count = pages
                .get(b"Count")
                .and_then(Object::as_i64)
                .map_err(pdf_error)?;
            pages.set("Count", count + 1);
        }
        let mut output = Vec::new();
        doc.save_to(&mut output).map_err(pdf_error)?;
        Ok(output)
    }
}
