//! Pure-Rust PDF engine for OpenPDF.
//!
//! The crate implements a PDF 1.7 reader and writer: it parses the object
//! table, page tree, metadata and outlines, decodes Flate-compressed streams,
//! and writes fresh files for creation, merging and splitting. Cross-reference
//! streams and object streams are supported. Encrypted files and stream filters
//! other than `/FlateDecode` are refused instead of being guessed.
//!
//! Every malformed input becomes a [`ServiceError`]; no entry point panics.

#![deny(missing_docs)]

mod create;
mod error;
mod extract;
mod filters;
mod object;
mod parse;
mod writer;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, argument, exists_error, read_error, write_error};
use crate::object::{Dict, PdfObject};
use crate::parse::{ParsedDocument, decode_text_string, parse_document, resolve_object};
use crate::writer::{DocWriter, copy_info, copy_page};

pub use crate::error::ServiceError;

/// Document metadata as reported by [`inspect`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfMetadata {
    /// `/Title`, or `null` when the document has none.
    pub title: Option<String>,
    /// `/Author`, or `null`.
    pub author: Option<String>,
    /// `/Subject`, or `null`.
    pub subject: Option<String>,
    /// `/Keywords`, or `null`.
    pub keywords: Option<String>,
    /// `/Creator`, or `null`.
    pub creator: Option<String>,
    /// `/Producer`, or `null`.
    pub producer: Option<String>,
    /// `/CreationDate` in its original PDF text form.
    pub creation_date: Option<String>,
    /// `/ModDate` in its original PDF text form.
    pub mod_date: Option<String>,
}

/// One page's geometry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfPageSize {
    /// Page width in points.
    pub width_pt: f64,
    /// Page height in points.
    pub height_pt: f64,
    /// Effective `/Rotate` value normalized to `0..360`.
    pub rotation: i64,
}

/// One outline entry.
#[derive(Debug, Clone, Serialize)]
pub struct PdfBookmark {
    /// Outline title.
    pub title: String,
    /// Nesting level, top-level entries are `1`.
    pub depth: usize,
}

/// Result of [`inspect`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectReport {
    /// File name of the inspected document.
    pub label: String,
    /// PDF header version such as `1.7`.
    pub version: String,
    /// Number of pages in reading order.
    pub pages: usize,
    /// Always `false`: encrypted documents are rejected before inspection.
    pub encrypted: bool,
    /// Document information dictionary.
    pub metadata: PdfMetadata,
    /// Effective geometry of every page.
    pub page_sizes: Vec<PdfPageSize>,
    /// Flat outline list with nesting depth.
    pub bookmarks: Vec<PdfBookmark>,
}

/// Result of [`merge`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeReport {
    /// Output path.
    pub path: String,
    /// Total pages written.
    pub pages: usize,
    /// Output size in bytes.
    pub bytes: usize,
}

/// One file produced by [`split`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPart {
    /// Output path.
    pub path: String,
    /// Pages contained in this file.
    pub pages: usize,
}

/// Result of [`split`].
#[derive(Debug, Clone, Serialize)]
pub struct SplitReport {
    /// One entry per requested range, in input order.
    pub documents: Vec<SplitPart>,
}

/// Request accepted by [`create`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    /// Output file path.
    pub out: PathBuf,
    /// Optional `/Title` metadata.
    #[serde(default)]
    pub title: Option<String>,
    /// One string per page; `\n` starts a new line inside the page.
    #[serde(default)]
    pub pages: Vec<String>,
    /// Replace the output when it already exists.
    #[serde(default)]
    pub overwrite: bool,
}

/// Result of [`create`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReport {
    /// Output path.
    pub path: String,
    /// Pages written.
    pub pages: usize,
    /// Output size in bytes.
    pub bytes: usize,
}

/// One text fragment that could not be extracted.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractLoss {
    /// Stable loss code.
    pub code: String,
    /// One-based page number.
    pub page: usize,
    /// Human-readable explanation.
    pub message: String,
}

/// Result of [`extract_text`].
#[derive(Debug, Clone, Serialize)]
pub struct ExtractReport {
    /// Page count of the source document.
    pub pages: usize,
    /// Extracted text with pages separated by blank lines.
    pub markdown: String,
    /// Everything the extractor skipped, never silent.
    pub losses: Vec<ExtractLoss>,
}

/// Reads a PDF and reports its version, page geometry, metadata and outlines.
pub fn inspect(path: &Path) -> Result<InspectReport> {
    let document = load_document(path)?;
    let metadata = read_metadata(&document)?;
    let page_sizes = document
        .pages
        .iter()
        .map(|page| PdfPageSize {
            width_pt: page.media_box[2] - page.media_box[0],
            height_pt: page.media_box[3] - page.media_box[1],
            rotation: page.rotation,
        })
        .collect();
    let bookmarks = document
        .bookmarks
        .iter()
        .map(|bookmark| PdfBookmark { title: bookmark.title.clone(), depth: bookmark.depth })
        .collect();
    Ok(InspectReport {
        label: label_of(path),
        version: document.version.clone(),
        pages: document.pages.len(),
        encrypted: false,
        metadata,
        page_sizes,
        bookmarks,
    })
}

/// Concatenates the pages of every input, in order.
///
/// The `/Info` dictionary comes from the first input; `title` replaces its
/// `/Title` when provided.
pub fn merge(inputs: &[PathBuf], out: &Path, overwrite: bool, title: Option<&str>) -> Result<MergeReport> {
    if inputs.is_empty() {
        return Err(argument("merge needs at least one input file"));
    }
    if out.exists() && !overwrite {
        return Err(exists_error(format!("{} already exists", out.display())));
    }

    let mut documents = Vec::with_capacity(inputs.len());
    for path in inputs {
        documents.push(load_document(path)?);
    }

    let mut writer = DocWriter::new();
    let mut pages = Vec::new();
    let mut info = None;
    for (index, document) in documents.iter().enumerate() {
        let mut map = BTreeMap::new();
        for page in &document.pages {
            pages.push(copy_page(&mut writer, document, &mut map, page)?);
        }
        if index == 0 {
            info = copy_info(&mut writer, document, &mut map, title)?;
        }
    }

    let bytes = finish_document(&mut writer, &pages, info)?;
    write_file(out, &bytes, overwrite)?;
    Ok(MergeReport { path: out.display().to_string(), pages: pages.len(), bytes: bytes.len() })
}

/// Splits one PDF into files selected by a 1-based range list such as
/// `1-5,10,15-20`.
pub fn split(path: &Path, ranges: &str, out_dir: &Path) -> Result<SplitReport> {
    let document = load_document(path)?;
    let ranges = parse_ranges(ranges, document.pages.len())?;
    prepare_out_dir(out_dir)?;
    let stem = document_stem(path);
    let mut documents = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        documents.push(write_part(&document, out_dir, &stem, start, end)?);
    }
    Ok(SplitReport { documents })
}

fn prepare_out_dir(out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir).map_err(|error| write_error(format!("cannot create {}: {error}", out_dir.display())))
}

fn document_stem(path: &Path) -> String {
    path.file_stem().and_then(|value| value.to_str()).unwrap_or("document").to_string()
}

fn write_part(document: &ParsedDocument, out_dir: &Path, stem: &str, start: usize, end: usize) -> Result<SplitPart> {
    let mut writer = DocWriter::new();
    let mut map = BTreeMap::new();
    let mut pages = Vec::new();
    for page in document.pages.iter().take(end).skip(start - 1) {
        pages.push(copy_page(&mut writer, document, &mut map, page)?);
    }
    let info = copy_info(&mut writer, document, &mut map, None)?;
    let bytes = finish_document(&mut writer, &pages, info)?;

    let name = if start == end { format!("{stem}-p{start}.pdf") } else { format!("{stem}-p{start}-{end}.pdf") };
    let target = out_dir.join(name);
    write_file(&target, &bytes, true)?;
    Ok(SplitPart { path: target.display().to_string(), pages: pages.len() })
}

/// Creates a new PDF with one Helvetica text page per request entry.
pub fn create(request: &CreateRequest) -> Result<CreateReport> {
    if request.pages.is_empty() {
        return Err(argument("create needs at least one page"));
    }
    if request.out.exists() && !request.overwrite {
        return Err(exists_error(format!("{} already exists", request.out.display())));
    }
    let bytes = create::build(request.title.as_deref(), &request.pages)?;
    write_file(&request.out, &bytes, request.overwrite)?;
    Ok(CreateReport { path: request.out.display().to_string(), pages: request.pages.len(), bytes: bytes.len() })
}

/// Extracts text from content streams decoded through their filters.
///
/// `/FlateDecode` is decoded. Unsupported filters are reported in
/// [`ExtractReport::losses`] instead of being approximated; see
/// [`capabilities`] for the disclosed support level.
pub fn extract_text(path: &Path) -> Result<ExtractReport> {
    let document = load_document(path)?;
    let extraction = extract::extract(&document)?;
    Ok(ExtractReport {
        pages: document.pages.len(),
        markdown: extraction.markdown,
        losses: extraction
            .losses
            .into_iter()
            .map(|loss| ExtractLoss { code: loss.code, page: loss.page, message: loss.message })
            .collect(),
    })
}

/// Reports what this build actually supports.
pub fn capabilities() -> serde_json::Value {
    serde_json::json!({
        "engine": {
            "package": "open-pdf-core",
            "version": env!("CARGO_PKG_VERSION")
        },
        "formats": {
            "pdf": {
                "import": "partial",
                "extract": "partial",
                "export": "partial",
                "structures": {
                    "classicXref": "supported",
                    "xrefStream": "supported",
                    "objectStream": "supported",
                    "encrypted": "unsupported"
                },
                "filters": {
                    "flateDecode": "supported",
                    "pngPredictor": "supported",
                    "tiffPredictor": "supported",
                    "other": "unsupported"
                }
            }
        },
        "operations": {
            "inspect": "supported",
            "create": "supported",
            "merge": "supported",
            "split": "supported",
            "extractText": "partial"
        },
        "checks": {
            "metadata": "supported",
            "outline": "supported",
            "pageTree": "supported"
        },
        "renderers": {
            "pdf": "unsupported",
            "raster": "unsupported",
            "svg": "unsupported"
        }
    })
}

fn load_document(path: &Path) -> Result<ParsedDocument> {
    let bytes = fs::read(path).map_err(|error| read_error(format!("cannot read {}: {error}", path.display())))?;
    parse_document(&bytes)
}

fn label_of(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

fn read_metadata(document: &ParsedDocument) -> Result<PdfMetadata> {
    let dict = match document.info {
        Some(number) => document.objects.get(&number).and_then(PdfObject::as_dict),
        None => None,
    };
    Ok(PdfMetadata {
        title: information_field(document, dict, "Title")?,
        author: information_field(document, dict, "Author")?,
        subject: information_field(document, dict, "Subject")?,
        keywords: information_field(document, dict, "Keywords")?,
        creator: information_field(document, dict, "Creator")?,
        producer: information_field(document, dict, "Producer")?,
        creation_date: information_field(document, dict, "CreationDate")?,
        mod_date: information_field(document, dict, "ModDate")?,
    })
}

fn information_field(document: &ParsedDocument, dict: Option<&Dict>, key: &str) -> Result<Option<String>> {
    let Some(value) = dict.and_then(|dict| dict.get(key)) else {
        return Ok(None);
    };
    match resolve_object(&document.objects, value)? {
        PdfObject::String(bytes) => Ok(Some(decode_text_string(bytes))),
        PdfObject::Name(name) => Ok(Some(name.clone())),
        _ => Ok(None),
    }
}

fn finish_document(writer: &mut DocWriter, pages: &[u32], info: Option<u32>) -> Result<Vec<u8>> {
    let pages_number = writer.reserve();
    let mut pages_tree = Dict::new();
    pages_tree.insert("Type".to_string(), PdfObject::Name("Pages".to_string()));
    pages_tree
        .insert("Kids".to_string(), PdfObject::Array(pages.iter().map(|number| PdfObject::Reference(*number, 0)).collect()));
    pages_tree.insert("Count".to_string(), PdfObject::Integer(pages.len() as i64));
    writer.set(pages_number, PdfObject::Dictionary(pages_tree));

    let mut catalog = Dict::new();
    catalog.insert("Type".to_string(), PdfObject::Name("Catalog".to_string()));
    catalog.insert("Pages".to_string(), PdfObject::Reference(pages_number, 0));
    let root = writer.add(PdfObject::Dictionary(catalog));
    writer.finish(root, info)
}

fn parse_ranges(ranges: &str, page_count: usize) -> Result<Vec<(usize, usize)>> {
    let mut parsed = Vec::new();
    for raw in ranges.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            return Err(argument("page ranges contain an empty entry"));
        }
        let (start, end) = match token.split_once('-') {
            Some((start, end)) => (start.trim(), end.trim()),
            None => (token, token),
        };
        let start: usize = start.parse().map_err(|_| argument(format!("invalid page range `{token}`")))?;
        let end: usize = end.parse().map_err(|_| argument(format!("invalid page range `{token}`")))?;
        if start == 0 || end == 0 {
            return Err(argument("page numbers start at 1"));
        }
        if end < start {
            return Err(argument(format!("page range `{token}` is reversed")));
        }
        if end > page_count {
            return Err(argument(format!("page range `{token}` exceeds the {page_count} available pages")));
        }
        parsed.push((start, end));
    }
    if parsed.is_empty() {
        return Err(argument("at least one page range is required"));
    }
    Ok(parsed)
}

fn write_file(path: &Path, bytes: &[u8], overwrite: bool) -> Result<()> {
    if path.exists() && !overwrite {
        return Err(exists_error(format!("{} already exists", path.display())));
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| write_error(format!("cannot create {}: {error}", parent.display())))?;
    }
    fs::write(path, bytes).map_err(|error| write_error(format!("cannot write {}: {error}", path.display())))
}
