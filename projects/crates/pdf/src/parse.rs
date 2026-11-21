use std::collections::{BTreeMap, HashSet};

use crate::error::{Result, invalid, unsupported};
use crate::object::{Dict, PdfObject, PdfStream};

/// A parsed PDF document: every body object plus the resolved page tree.
pub(crate) struct ParsedDocument {
    pub version: String,
    pub objects: BTreeMap<u32, PdfObject>,
    pub info: Option<u32>,
    pub pages: Vec<PageNode>,
    pub bookmarks: Vec<Bookmark>,
}

/// One page in reading order. `own` is the page dictionary, `inherited` holds
/// the effective inheritable attributes resolved from its ancestors.
pub(crate) struct PageNode {
    pub own: Dict,
    pub inherited: Dict,
    pub media_box: [f64; 4],
    pub rotation: i64,
}

/// One outline entry.
pub(crate) struct Bookmark {
    pub title: String,
    pub depth: usize,
}

const DEFAULT_MEDIA_BOX: [f64; 4] = [0.0, 0.0, 612.0, 792.0];

pub(crate) fn is_whitespace(byte: u8) -> bool {
    matches!(byte, 0 | 9 | 10 | 12 | 13 | 32)
}

pub(crate) fn is_delimiter(byte: u8) -> bool {
    matches!(byte, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

pub(crate) fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn rfind_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).rposition(|window| window == needle)
}

/// Resolves an indirect reference chain to the concrete object.
pub(crate) fn resolve_object<'a>(objects: &'a BTreeMap<u32, PdfObject>, object: &'a PdfObject) -> Result<&'a PdfObject> {
    let mut current = object;
    for _ in 0..16 {
        match current {
            PdfObject::Reference(number, _) => {
                current = objects
                    .get(number)
                    .ok_or_else(|| invalid(format!("object {number} referenced from the page tree is missing")))?;
            }
            _ => return Ok(current),
        }
    }
    Err(invalid("reference chain is too deep"))
}

/// Loads and validates a PDF byte buffer.
pub(crate) fn parse_document(data: &[u8]) -> Result<ParsedDocument> {
    let (version, scan_from) = read_header(data)?;
    let objects = scan_objects(data, scan_from)?;
    let startxref = read_startxref(data)?;
    let trailer = read_trailer(data, startxref)?;

    if trailer.contains_key("Encrypt") {
        return Err(unsupported("xref streams / encrypted PDF not supported: encrypted PDF"));
    }

    let root_reference = trailer.get("Root").ok_or_else(|| invalid("trailer has no /Root"))?;
    let root = match root_reference {
        PdfObject::Reference(number, _) => *number,
        _ => return Err(invalid("trailer /Root must be an indirect reference")),
    };
    let info = match trailer.get("Info") {
        Some(PdfObject::Reference(number, _)) => Some(*number),
        _ => None,
    };

    let root_object = objects.get(&root).ok_or_else(|| invalid("/Root object is missing"))?;
    let catalog = root_object.as_dict().ok_or_else(|| invalid("/Root is not a dictionary"))?;
    if catalog.get("Type").and_then(PdfObject::as_name) != Some("Catalog") {
        return Err(invalid("/Root is not a catalog"));
    }
    let pages_object = catalog.get("Pages").ok_or_else(|| invalid("/Catalog has no /Pages"))?;

    let mut pages = Vec::new();
    let mut visited = HashSet::new();
    walk_page_tree(&objects, pages_object, &Dict::new(), &mut visited, 0, &mut pages)?;
    let bookmarks = collect_bookmarks(&objects, catalog)?;

    Ok(ParsedDocument { version, objects, info, pages, bookmarks })
}

fn read_header(data: &[u8]) -> Result<(String, usize)> {
    let window = data.get(..data.len().min(4096)).unwrap_or(data);
    let index = find_sub(window, b"%PDF-").ok_or_else(|| invalid("missing %PDF- header"))?;
    let mut position = index + 5;
    let mut version = String::new();
    while let Some(byte) = data.get(position).copied() {
        if is_whitespace(byte) || byte == b'%' || version.len() >= 8 {
            break;
        }
        version.push(char::from(byte));
        position += 1;
    }
    if version.is_empty() {
        return Err(invalid("missing PDF version after %PDF-"));
    }
    Ok((version, position))
}

fn scan_objects(data: &[u8], from: usize) -> Result<BTreeMap<u32, PdfObject>> {
    let mut parser = Parser::new(data);
    parser.pos = from.min(data.len());
    let mut objects = BTreeMap::new();
    while parser.pos < data.len() {
        parser.skip_ws();
        if parser.pos >= data.len() {
            break;
        }
        let start = parser.pos;
        match parser.try_indirect_header()? {
            Some((number, _generation)) => {
                let object = parser.parse_object(0)?;
                objects.insert(number, object);
                parser.skip_to_endobj();
                if objects.len() > 1_000_000 {
                    return Err(invalid("the file declares too many objects"));
                }
            }
            None => {
                parser.pos = start.saturating_add(1).min(data.len());
            }
        }
    }
    Ok(objects)
}

fn read_startxref(data: &[u8]) -> Result<usize> {
    let index = rfind_sub(data, b"startxref").ok_or_else(|| invalid("missing startxref"))?;
    let mut parser = Parser::new(data);
    parser.pos = index + b"startxref".len();
    parser.skip_ws();
    let offset = parser.take_unsigned()?.ok_or_else(|| invalid("startxref has no offset"))?;
    usize::try_from(offset).map_err(|_| invalid("startxref offset does not fit in memory"))
}

fn read_trailer(data: &[u8], offset: usize) -> Result<Dict> {
    if offset >= data.len() {
        return Err(invalid("startxref points past the end of the file"));
    }
    let mut parser = Parser::new(data);
    parser.pos = offset;
    parser.skip_ws();
    if parser.take_keyword(b"xref") {
        return read_classic_trailer(&mut parser);
    }

    // A startxref target that is an object rather than the `xref` keyword is a
    // cross-reference stream, which this engine deliberately does not decode.
    if parser.try_indirect_header()?.is_some() {
        let object = parser.parse_object(0)?;
        let object_type = match &object {
            PdfObject::Dictionary(dict) => dict.get("Type").and_then(PdfObject::as_name),
            PdfObject::Stream(stream) => stream.dict.get("Type").and_then(PdfObject::as_name),
            _ => None,
        };
        if object_type == Some("XRef") {
            return Err(unsupported("xref streams / encrypted PDF not supported: cross-reference stream"));
        }
        return Err(invalid("startxref does not point to a cross-reference table"));
    }
    Err(invalid("startxref does not point to a cross-reference table"))
}

fn read_classic_trailer(parser: &mut Parser<'_>) -> Result<Dict> {
    loop {
        parser.skip_ws();
        if parser.pos >= parser.data.len() {
            return Err(invalid("cross-reference table has no trailer"));
        }
        if parser.take_keyword(b"trailer") {
            let trailer = parser.parse_object(0)?;
            return trailer.as_dict().cloned().ok_or_else(|| invalid("trailer is not a dictionary"));
        }
        let start = parser.take_unsigned()?.ok_or_else(|| invalid("invalid cross-reference subsection"))?;
        let _ = start;
        parser.skip_ws();
        let count = parser.take_unsigned()?.ok_or_else(|| invalid("invalid cross-reference subsection"))?;
        if count > 10_000_000 {
            return Err(invalid("cross-reference subsection is too large"));
        }
        for _ in 0..count {
            parser.skip_ws();
            parser.take_unsigned()?.ok_or_else(|| invalid("invalid cross-reference entry"))?;
            parser.skip_ws();
            parser.take_unsigned()?.ok_or_else(|| invalid("invalid cross-reference entry"))?;
            parser.skip_ws();
            let flag = parser.peek().ok_or_else(|| invalid("truncated cross-reference entry"))?;
            if flag != b'n' && flag != b'f' {
                return Err(invalid("invalid cross-reference entry flag"));
            }
            parser.bump();
        }
    }
}

fn walk_page_tree(
    objects: &BTreeMap<u32, PdfObject>,
    node: &PdfObject,
    inherited: &Dict,
    visited: &mut HashSet<u32>,
    depth: usize,
    pages: &mut Vec<PageNode>,
) -> Result<()> {
    if depth > 256 {
        return Err(invalid("page tree is too deep"));
    }
    if let PdfObject::Reference(number, _) = node
        && !visited.insert(*number)
    {
        return Err(invalid("page tree contains a cycle"));
    }
    let dictionary = resolve_object(objects, node)?.as_dict().ok_or_else(|| invalid("page tree node is not a dictionary"))?;

    let mut effective = inherited.clone();
    for key in ["MediaBox", "CropBox", "Resources", "Rotate", "UserUnit"] {
        if let Some(value) = dictionary.get(key) {
            effective.insert(key.to_string(), value.clone());
        }
    }

    let kids = match dictionary.get("Kids") {
        Some(value) => {
            Some(resolve_object(objects, value)?.as_array().ok_or_else(|| invalid("/Kids is not an array"))?.to_vec())
        }
        None => None,
    };

    if let Some(kids) = kids {
        for kid in &kids {
            walk_page_tree(objects, kid, &effective, visited, depth + 1, pages)?;
        }
        return Ok(());
    }

    let media_box = match effective.get("MediaBox") {
        Some(value) => parse_media_box(objects, value)?,
        None => DEFAULT_MEDIA_BOX,
    };
    let rotation = match effective.get("Rotate") {
        Some(value) => parse_number(objects, value)?.round() as i64,
        None => 0,
    };
    pages.push(PageNode { own: dictionary.clone(), inherited: effective, media_box, rotation: normalize_rotation(rotation) });
    Ok(())
}

fn normalize_rotation(value: i64) -> i64 {
    let wrapped = value % 360;
    if wrapped < 0 { wrapped + 360 } else { wrapped }
}

fn parse_media_box(objects: &BTreeMap<u32, PdfObject>, value: &PdfObject) -> Result<[f64; 4]> {
    let resolved = resolve_object(objects, value)?;
    let items = resolved.as_array().ok_or_else(|| invalid("/MediaBox is not an array"))?;
    if items.len() < 4 {
        return Err(invalid("/MediaBox must contain four numbers"));
    }
    let mut rectangle = [0.0; 4];
    for (index, slot) in rectangle.iter_mut().enumerate() {
        let item = items.get(index).ok_or_else(|| invalid("/MediaBox must contain four numbers"))?;
        *slot = parse_number(objects, item)?;
    }
    Ok(rectangle)
}

fn parse_number(objects: &BTreeMap<u32, PdfObject>, value: &PdfObject) -> Result<f64> {
    match resolve_object(objects, value)? {
        PdfObject::Integer(number) => Ok(*number as f64),
        PdfObject::Real(number) => Ok(*number),
        _ => Err(invalid("expected a number")),
    }
}

fn collect_bookmarks(objects: &BTreeMap<u32, PdfObject>, catalog: &Dict) -> Result<Vec<Bookmark>> {
    let outlines = match catalog.get("Outlines") {
        Some(value) => resolve_object(objects, value)?.as_dict().cloned(),
        None => None,
    };
    let Some(outlines) = outlines else {
        return Ok(Vec::new());
    };
    let Some(first) = outlines.get("First") else {
        return Ok(Vec::new());
    };
    let mut bookmarks = Vec::new();
    let mut visited = HashSet::new();
    let mut total = 0usize;
    walk_outline(objects, first, 1, &mut visited, &mut total, &mut bookmarks)?;
    Ok(bookmarks)
}

fn walk_outline(
    objects: &BTreeMap<u32, PdfObject>,
    first: &PdfObject,
    depth: usize,
    visited: &mut HashSet<u32>,
    total: &mut usize,
    bookmarks: &mut Vec<Bookmark>,
) -> Result<()> {
    if depth > 64 {
        return Ok(());
    }
    let mut current = first.clone();
    loop {
        if *total >= 10_000 {
            break;
        }
        *total += 1;
        if let PdfObject::Reference(number, _) = current
            && !visited.insert(number)
        {
            break;
        }
        let dictionary = match resolve_object(objects, &current) {
            Ok(PdfObject::Dictionary(dictionary)) => dictionary,
            _ => break,
        };
        let title = match dictionary.get("Title") {
            Some(value) => string_value(objects, value)?,
            None => String::new(),
        };
        bookmarks.push(Bookmark { title, depth });
        if let Some(child) = dictionary.get("First")
            && resolve_object(objects, child).map(PdfObject::as_dict).unwrap_or(None).is_some()
        {
            walk_outline(objects, child, depth + 1, visited, total, bookmarks)?;
        }
        match dictionary.get("Next") {
            Some(next) => current = next.clone(),
            None => break,
        }
    }
    Ok(())
}

fn string_value(objects: &BTreeMap<u32, PdfObject>, value: &PdfObject) -> Result<String> {
    match resolve_object(objects, value)? {
        PdfObject::String(bytes) => Ok(decode_text_string(bytes)),
        PdfObject::Name(name) => Ok(name.clone()),
        _ => Err(invalid("expected a text string")),
    }
}

/// Decodes a PDF text string: UTF-16BE/LE with a byte-order mark, otherwise
/// PDFDocEncoding (approximated by Latin-1 plus the Windows-1252 punctuation).
pub(crate) fn decode_text_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        return decode_utf16(&bytes[2..], true);
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        return decode_utf16(&bytes[2..], false);
    }
    bytes.iter().map(|byte| pdf_doc_char(*byte)).collect()
}

fn decode_utf16(bytes: &[u8], big_endian: bool) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| if big_endian { u16::from_be_bytes(*pair) } else { u16::from_le_bytes(*pair) })
        .collect();
    char::decode_utf16(units).map(|unit| unit.unwrap_or('\u{FFFD}')).collect()
}

fn pdf_doc_char(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{02C6}', '\u{2030}',
        '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}', '\u{017D}', '\u{FFFD}', '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}',
        '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{FFFD}',
        '\u{017E}', '\u{0178}',
    ];
    match byte {
        0x80..=0x9F => HIGH.get((byte - 0x80) as usize).copied().unwrap_or('\u{FFFD}'),
        other => char::from(other),
    }
}

pub(crate) struct Parser<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    pub(crate) fn bump(&mut self) {
        self.pos = self.pos.saturating_add(1).min(self.data.len());
    }

    pub(crate) fn starts_with(&self, needle: &[u8]) -> bool {
        self.data.get(self.pos..).is_some_and(|rest| rest.starts_with(needle))
    }

    pub(crate) fn skip_ws(&mut self) {
        loop {
            while let Some(byte) = self.peek() {
                if is_whitespace(byte) {
                    self.bump();
                } else {
                    break;
                }
            }
            if self.peek() == Some(b'%') {
                while let Some(byte) = self.peek() {
                    self.bump();
                    if byte == b'\n' || byte == b'\r' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn skip_to_endobj(&mut self) {
        if let Some(rest) = self.data.get(self.pos..)
            && let Some(index) = find_sub(rest, b"endobj")
        {
            self.pos = self.pos.saturating_add(index).saturating_add(b"endobj".len()).min(self.data.len());
        } else {
            self.pos = self.data.len();
        }
    }

    fn take_keyword(&mut self, keyword: &[u8]) -> bool {
        if !self.starts_with(keyword) {
            return false;
        }
        let after = self.pos.saturating_add(keyword.len());
        let boundary = self.data.get(after).copied().map(|byte| is_whitespace(byte) || is_delimiter(byte)).unwrap_or(true);
        if boundary {
            self.pos = after.min(self.data.len());
            true
        } else {
            false
        }
    }

    fn try_indirect_header(&mut self) -> Result<Option<(u32, u16)>> {
        let start = self.pos;
        let Some(number) = self.take_unsigned()? else {
            self.pos = start;
            return Ok(None);
        };
        self.skip_ws();
        let Some(generation) = self.take_unsigned()? else {
            self.pos = start;
            return Ok(None);
        };
        self.skip_ws();
        if self.take_keyword(b"obj") {
            let number = u32::try_from(number).map_err(|_| invalid("object number is too large"))?;
            let generation = u16::try_from(generation).map_err(|_| invalid("object generation is too large"))?;
            Ok(Some((number, generation)))
        } else {
            self.pos = start;
            Ok(None)
        }
    }

    fn take_unsigned(&mut self) -> Result<Option<u64>> {
        let start = self.pos;
        let mut value: u64 = 0;
        while let Some(byte) = self.peek() {
            let Some(digit) = char::from(byte).to_digit(10) else {
                break;
            };
            value = value
                .checked_mul(10)
                .and_then(|current| current.checked_add(u64::from(digit)))
                .ok_or_else(|| invalid("integer is too large"))?;
            self.bump();
        }
        if self.pos == start { Ok(None) } else { Ok(Some(value)) }
    }

    /// Parses one object. `depth` is bounded so malformed nesting cannot
    /// exhaust the stack.
    pub(crate) fn parse_object(&mut self, depth: usize) -> Result<PdfObject> {
        if depth > 64 {
            return Err(invalid("object nesting is too deep"));
        }
        self.skip_ws();
        match self.peek() {
            None => Err(invalid("unexpected end of input")),
            Some(b'/') => Ok(PdfObject::Name(self.parse_name()?)),
            Some(b'(') => Ok(PdfObject::String(self.parse_literal_string()?)),
            Some(b'<') => {
                if self.starts_with(b"<<") {
                    self.parse_dictionary(depth)
                } else {
                    Ok(PdfObject::String(self.parse_hex_string()?))
                }
            }
            Some(b'[') => Ok(PdfObject::Array(self.parse_array(depth)?)),
            Some(byte) if byte == b'+' || byte == b'-' || byte == b'.' || byte.is_ascii_digit() => {
                self.parse_number_or_reference()
            }
            Some(b']' | b')' | b'>' | b'}' | b'{') => Err(invalid("unexpected delimiter")),
            Some(_) => self.parse_keyword(),
        }
    }

    fn parse_keyword(&mut self) -> Result<PdfObject> {
        let start = self.pos;
        while let Some(byte) = self.peek() {
            if is_whitespace(byte) || is_delimiter(byte) {
                break;
            }
            self.bump();
        }
        let token = self.data.get(start..self.pos).ok_or_else(|| invalid("invalid token"))?;
        match token {
            b"true" => Ok(PdfObject::Boolean(true)),
            b"false" => Ok(PdfObject::Boolean(false)),
            b"null" => Ok(PdfObject::Null),
            _ => Err(invalid("unexpected keyword")),
        }
    }

    fn parse_name(&mut self) -> Result<String> {
        self.bump();
        let mut decoded = Vec::new();
        while let Some(byte) = self.peek() {
            if is_whitespace(byte) || is_delimiter(byte) {
                break;
            }
            if byte == b'#' {
                self.bump();
                let high = self.take_hex_digit()?;
                let low = self.take_hex_digit()?;
                decoded.push(high.saturating_mul(16).saturating_add(low));
            } else {
                decoded.push(byte);
                self.bump();
            }
        }
        Ok(String::from_utf8_lossy(&decoded).into_owned())
    }

    fn take_hex_digit(&mut self) -> Result<u8> {
        let byte = self.peek().ok_or_else(|| invalid("truncated name escape"))?;
        let digit = char::from(byte).to_digit(16).ok_or_else(|| invalid("invalid name escape"))?;
        self.bump();
        Ok(digit as u8)
    }

    fn parse_literal_string(&mut self) -> Result<Vec<u8>> {
        self.bump();
        let mut bytes = Vec::new();
        let mut depth = 1usize;
        loop {
            let byte = self.peek().ok_or_else(|| invalid("unterminated literal string"))?;
            match byte {
                b'\\' => {
                    self.bump();
                    let escaped = self.peek().ok_or_else(|| invalid("truncated string escape"))?;
                    match escaped {
                        b'n' => {
                            bytes.push(b'\n');
                            self.bump();
                        }
                        b'r' => {
                            bytes.push(b'\r');
                            self.bump();
                        }
                        b't' => {
                            bytes.push(b'\t');
                            self.bump();
                        }
                        b'b' => {
                            bytes.push(0x08);
                            self.bump();
                        }
                        b'f' => {
                            bytes.push(0x0C);
                            self.bump();
                        }
                        b'(' | b')' | b'\\' => {
                            bytes.push(escaped);
                            self.bump();
                        }
                        b'\r' => {
                            self.bump();
                            if self.peek() == Some(b'\n') {
                                self.bump();
                            }
                        }
                        b'\n' => {
                            self.bump();
                        }
                        b'0'..=b'7' => {
                            let mut value: u16 = 0;
                            for _ in 0..3 {
                                let Some(digit) = self.peek().and_then(|byte| char::from(byte).to_digit(8)) else {
                                    break;
                                };
                                value = value.saturating_mul(8).saturating_add(digit as u16);
                                self.bump();
                            }
                            bytes.push((value & 0xFF) as u8);
                        }
                        other => {
                            bytes.push(other);
                            self.bump();
                        }
                    }
                }
                b'(' => {
                    depth = depth.saturating_add(1);
                    bytes.push(byte);
                    self.bump();
                }
                b')' => {
                    depth -= 1;
                    self.bump();
                    if depth == 0 {
                        break;
                    }
                    bytes.push(byte);
                }
                b'\r' => {
                    self.bump();
                    if self.peek() == Some(b'\n') {
                        self.bump();
                    }
                    bytes.push(b'\n');
                }
                other => {
                    bytes.push(other);
                    self.bump();
                }
            }
        }
        Ok(bytes)
    }

    fn parse_hex_string(&mut self) -> Result<Vec<u8>> {
        self.bump();
        let mut bytes = Vec::new();
        let mut high: Option<u8> = None;
        loop {
            let byte = self.peek().ok_or_else(|| invalid("unterminated hex string"))?;
            if is_whitespace(byte) {
                self.bump();
                continue;
            }
            if byte == b'>' {
                self.bump();
                if let Some(high) = high {
                    bytes.push(high.saturating_mul(16));
                }
                return Ok(bytes);
            }
            let digit = char::from(byte).to_digit(16).ok_or_else(|| invalid("invalid hex string"))? as u8;
            self.bump();
            match high {
                None => high = Some(digit),
                Some(previous) => {
                    bytes.push(previous.saturating_mul(16).saturating_add(digit));
                    high = None;
                }
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<Vec<PdfObject>> {
        self.bump();
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => return Err(invalid("unterminated array")),
                Some(b']') => {
                    self.bump();
                    return Ok(items);
                }
                Some(_) => items.push(self.parse_object(depth + 1)?),
            }
            if items.len() > 1_000_000 {
                return Err(invalid("array is too large"));
            }
        }
    }

    fn parse_dictionary(&mut self, depth: usize) -> Result<PdfObject> {
        self.pos = self.pos.saturating_add(2).min(self.data.len());
        let mut dict = Dict::new();
        loop {
            self.skip_ws();
            if self.starts_with(b">>") {
                self.pos = self.pos.saturating_add(2).min(self.data.len());
                break;
            }
            if self.peek().is_none() {
                return Err(invalid("unterminated dictionary"));
            }
            if self.peek() != Some(b'/') {
                return Err(invalid("dictionary key must be a name"));
            }
            let key = self.parse_name()?;
            let value = self.parse_object(depth + 1)?;
            dict.insert(key, value);
        }

        let after_dict = self.pos;
        self.skip_ws();
        if self.take_keyword(b"stream") {
            return self.parse_stream(dict);
        }
        self.pos = after_dict;
        Ok(PdfObject::Dictionary(dict))
    }

    fn parse_stream(&mut self, dict: Dict) -> Result<PdfObject> {
        if self.peek() == Some(b'\r') {
            self.bump();
            if self.peek() == Some(b'\n') {
                self.bump();
            }
        } else if self.peek() == Some(b'\n') {
            self.bump();
        }
        let stream_start = self.pos;

        let declared = dict.get("Length").and_then(PdfObject::as_i64).and_then(|length| usize::try_from(length).ok());
        if let Some(length) = declared
            && let Some(end) = stream_start.checked_add(length)
            && end <= self.data.len()
        {
            let before_probe = self.pos;
            self.pos = end;
            self.skip_ws();
            if self.take_keyword(b"endstream") {
                let data = self.data.get(stream_start..end).unwrap_or(&[]).to_vec();
                return Ok(PdfObject::Stream(PdfStream { dict, data }));
            }
            self.pos = before_probe;
        }

        let rest = self.data.get(stream_start..).ok_or_else(|| invalid("truncated stream"))?;
        let index = find_sub(rest, b"endstream").ok_or_else(|| invalid("unterminated stream"))?;
        let mut end = stream_start.saturating_add(index);
        if end > stream_start && self.data.get(end - 1) == Some(&b'\n') {
            end -= 1;
            if end > stream_start && self.data.get(end - 1) == Some(&b'\r') {
                end -= 1;
            }
        } else if end > stream_start && self.data.get(end - 1) == Some(&b'\r') {
            end -= 1;
        }
        let data = self.data.get(stream_start..end).unwrap_or(&[]).to_vec();
        self.pos = stream_start.saturating_add(index).saturating_add(b"endstream".len()).min(self.data.len());
        Ok(PdfObject::Stream(PdfStream { dict, data }))
    }

    fn parse_number_or_reference(&mut self) -> Result<PdfObject> {
        let start = self.pos;
        while let Some(byte) = self.peek() {
            if byte.is_ascii_digit() || byte == b'.' || byte == b'+' || byte == b'-' {
                self.bump();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(invalid("invalid number"));
        }
        let token = self.data.get(start..self.pos).ok_or_else(|| invalid("invalid number"))?;
        let token = std::str::from_utf8(token).map_err(|_| invalid("invalid number"))?;

        if token.contains('.') {
            let value = parse_real(token).ok_or_else(|| invalid("invalid real number"))?;
            if !value.is_finite() {
                return Err(invalid("real number is not finite"));
            }
            return Ok(PdfObject::Real(value));
        }

        let first: i64 = token.parse().map_err(|_| invalid("invalid integer"))?;
        if first >= 0
            && let Ok(number) = u32::try_from(first)
        {
            let saved = self.pos;
            self.skip_ws();
            if let Some(second) = self.take_unsigned()? {
                let had_space = self.pos > saved;
                self.skip_ws();
                if had_space
                    && self.peek() == Some(b'R')
                    && self
                        .data
                        .get(self.pos + 1)
                        .copied()
                        .map(|byte| is_whitespace(byte) || is_delimiter(byte))
                        .unwrap_or(true)
                {
                    self.bump();
                    if let Ok(generation) = u16::try_from(second) {
                        return Ok(PdfObject::Reference(number, generation));
                    }
                }
            }
            self.pos = saved;
        }
        Ok(PdfObject::Integer(first))
    }
}

fn parse_real(token: &str) -> Option<f64> {
    let mut owned = token.to_string();
    if owned.starts_with('.') {
        owned.insert(0, '0');
    } else if owned.starts_with("-.") || owned.starts_with("+.") {
        owned.insert(1, '0');
    }
    owned.parse().ok()
}
