use crate::error::Result;
use crate::object::{Dict, PdfObject, PdfStream, encode_text};
use crate::writer::DocWriter;

const A4_WIDTH: f64 = 612.0;
const A4_HEIGHT: f64 = 792.0;
const MARGIN: f64 = 72.0;
const FONT_SIZE: f64 = 12.0;
const LEADING: f64 = 15.0;

/// Builds a fresh PDF with one Helvetica text page per input string.
pub(crate) fn build(title: Option<&str>, pages: &[String]) -> Result<Vec<u8>> {
    let mut writer = DocWriter::new();
    let font = writer.add(PdfObject::Dictionary(font_dictionary()));
    let pages_number = writer.reserve();
    let mut kids = Vec::with_capacity(pages.len());

    for text in pages {
        let content = writer.add(PdfObject::Stream(PdfStream { dict: Dict::new(), data: page_content(text) }));
        let mut page = Dict::new();
        page.insert("Type".to_string(), PdfObject::Name("Page".to_string()));
        page.insert("Parent".to_string(), PdfObject::Reference(pages_number, 0));
        page.insert(
            "MediaBox".to_string(),
            PdfObject::Array(vec![
                PdfObject::Integer(0),
                PdfObject::Integer(0),
                PdfObject::Integer(A4_WIDTH as i64),
                PdfObject::Integer(A4_HEIGHT as i64),
            ]),
        );
        page.insert("Contents".to_string(), PdfObject::Reference(content, 0));
        page.insert("Resources".to_string(), PdfObject::Dictionary(resources(font)));
        kids.push(PdfObject::Reference(writer.add(PdfObject::Dictionary(page)), 0));
    }

    let mut pages_tree = Dict::new();
    pages_tree.insert("Type".to_string(), PdfObject::Name("Pages".to_string()));
    pages_tree.insert("Kids".to_string(), PdfObject::Array(kids));
    pages_tree.insert("Count".to_string(), PdfObject::Integer(pages.len() as i64));
    writer.set(pages_number, PdfObject::Dictionary(pages_tree));

    let mut catalog = Dict::new();
    catalog.insert("Type".to_string(), PdfObject::Name("Catalog".to_string()));
    catalog.insert("Pages".to_string(), PdfObject::Reference(pages_number, 0));
    let root = writer.add(PdfObject::Dictionary(catalog));

    let info = match title {
        Some(title) => {
            let mut info = Dict::new();
            info.insert("Title".to_string(), PdfObject::String(encode_text(title)));
            info.insert("Producer".to_string(), PdfObject::String(b"OpenPDF".to_vec()));
            Some(writer.add(PdfObject::Dictionary(info)))
        }
        None => None,
    };

    writer.finish(root, info)
}

fn font_dictionary() -> Dict {
    let mut font = Dict::new();
    font.insert("Type".to_string(), PdfObject::Name("Font".to_string()));
    font.insert("Subtype".to_string(), PdfObject::Name("Type1".to_string()));
    font.insert("BaseFont".to_string(), PdfObject::Name("Helvetica".to_string()));
    font.insert("Encoding".to_string(), PdfObject::Name("WinAnsiEncoding".to_string()));
    font
}

fn resources(font: u32) -> Dict {
    let mut fonts = Dict::new();
    fonts.insert("F1".to_string(), PdfObject::Reference(font, 0));
    let mut resources = Dict::new();
    resources.insert("Font".to_string(), PdfObject::Dictionary(fonts));
    resources.insert(
        "ProcSet".to_string(),
        PdfObject::Array(vec![PdfObject::Name("PDF".to_string()), PdfObject::Name("Text".to_string())]),
    );
    resources
}

fn page_content(text: &str) -> Vec<u8> {
    let mut stream = String::from("BT\n");
    stream.push_str(&format!("/F1 {FONT_SIZE} Tf\n"));
    stream.push_str(&format!("{} TL\n", LEADING as i64));
    stream.push_str(&format!("{} {} Td\n", MARGIN as i64, (A4_HEIGHT - MARGIN) as i64));

    for (index, line) in text.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if index > 0 {
            stream.push_str("T*\n");
        }
        if !line.is_empty() {
            stream.push('(');
            push_literal(&mut stream, line);
            stream.push_str(") Tj\n");
        }
    }
    stream.push_str("ET\n");
    stream.into_bytes()
}

fn push_literal(output: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '(' => output.push_str("\\("),
            ')' => output.push_str("\\)"),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            other => {
                let code = other as u32;
                if (0x20..=0x7E).contains(&code) {
                    output.push(other);
                } else {
                    let fallback = if code <= 0xFF { code } else { u32::from(b'?') };
                    output.push_str(&format!("\\{fallback:03o}"));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_escapes_parentheses_and_backslashes() {
        let mut output = String::new();
        push_literal(&mut output, "a(b)c\\d");
        assert_eq!(output, "a\\(b\\)c\\\\d");
    }

    #[test]
    fn content_stream_moves_between_lines() {
        let content = String::from_utf8(page_content("first\nsecond")).expect("utf-8");
        assert!(content.contains("(first) Tj"));
        assert!(content.contains("T*"));
        assert!(content.contains("(second) Tj"));
        assert!(content.contains(&format!("{} TL", LEADING as i64)));
    }
}
