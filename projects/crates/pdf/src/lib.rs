//! OpenPDF product surface.
//!
//! The PDF object model, parser, filters, writer and page operations live in
//! the shared `document-pdf` crate. This crate owns only the product contract:
//! the public re-exports the CLI and Node-API layers call, plus the capability
//! matrix that describes this build.

#![deny(missing_docs)]

pub use document_pdf::{
    ArrangeReport, Bookmark, BookmarkReport, CreateReport, CreateRequest, DeletePagesReport, DocumentArrange, ExtractLoss,
    ExtractReport, ExtractedImage, FormFillReport, FormValue, InspectReport, MergeReport, MetadataPatch, PdfBookmark, PdfFormField,
    PdfMetadata, PdfPageSize, RotatePagesReport, ServiceError, SetMetadataReport, SetPageBoxesReport, SplitPart, SplitReport,
    StampReport, StampRequest, arrange, create, delete_pages, extract_images, extract_images_bytes, extract_text,
    extract_text_bytes, fill_form, inspect, inspect_bytes, merge, rotate_pages, set_bookmarks, set_metadata, set_page_boxes, split,
    split_every, stamp,
};

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::{Value, json};

static TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Extracts text from the selected one-based pages, in the requested order.
///
/// `ranges` accepts comma-separated pages and inclusive ranges such as
/// `1-3,5`. A selection that is not the whole document in order is projected
/// into a temporary classic PDF first, so filters still run through the same
/// decoder. The report carries the extracted Markdown, its losses, the source
/// page count and the pages that were actually extracted.
pub fn extract_text_pages(path: &Path, ranges: &str) -> Result<Value, ServiceError> {
    let report = inspect(path)?;
    let selected = parse_page_ranges(ranges, report.pages)?;
    let extracted = if selected.iter().copied().eq(1..=report.pages) {
        extract_text(path)?
    }
    else {
        let remove = (1..=report.pages).filter(|page| !selected.contains(page)).collect::<Vec<_>>();
        let temporary = temporary_path();
        let request = DocumentArrange { order: Some(selected.clone()), rotate: None, remove: Some(remove) };
        let outcome = arrange(path, &temporary, &request).and_then(|_| extract_text(&temporary));
        let _ = fs::remove_file(&temporary);
        outcome?
    };
    Ok(json!({
        "pages": extracted.pages,
        "sourcePages": report.pages,
        "selected": selected,
        "markdown": extracted.markdown,
        "losses": extracted.losses,
    }))
}

/// Searches extracted text and reports every case-sensitive line match.
///
/// Matching runs on the Markdown produced by [`extract_text`], so a PDF page
/// without extractable text cannot contribute hits; such pages are reported in
/// `losses` instead. `limit` defaults to 100 and is clamped to 1-500. Each hit
/// carries the one-based line, a one-based column, byte offsets within the line
/// and the matching fragment.
pub fn search(path: &Path, query: &str, limit: Option<u32>) -> Result<Value, ServiceError> {
    if query.trim().is_empty() {
        return Err(invalid_pages("`query` must be a non-empty string"));
    }
    let limit = limit.map_or(100usize, |value| value.clamp(1, 500) as usize);
    let report = extract_text(path)?;
    let mut hits = Vec::new();
    let mut truncated = false;
    'lines: for (index, line) in report.markdown.lines().enumerate() {
        let mut from = 0;
        while let Some(offset) = line[from..].find(query) {
            if hits.len() == limit {
                truncated = true;
                break 'lines;
            }
            let start = from + offset;
            let end = start + query.len();
            hits.push(json!({
                "line": index + 1,
                "column": start + 1,
                "start": start,
                "end": end,
                "match": &line[start..end],
                "text": line,
            }));
            from = end;
        }
    }
    Ok(json!({
        "path": path.display().to_string(),
        "query": query,
        "limit": limit,
        "pages": report.pages,
        "lines": report.markdown.lines().count(),
        "truncated": truncated,
        "hits": hits,
        "losses": report.losses,
    }))
}

/// Expands one-based ranges such as `1-3,5`, refusing pages the PDF cannot satisfy.
fn parse_page_ranges(ranges: &str, page_count: usize) -> Result<Vec<usize>, ServiceError> {
    let mut selected = Vec::new();
    for raw in ranges.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            return Err(invalid_pages("page ranges contain an empty entry"));
        }
        let (start, end) = match token.split_once('-') {
            Some((start, end)) => (start.trim(), end.trim()),
            None => (token, token),
        };
        let start = start.parse::<usize>().map_err(|_| invalid_pages(format!("invalid page range `{token}`")))?;
        let end = end.parse::<usize>().map_err(|_| invalid_pages(format!("invalid page range `{token}`")))?;
        if start == 0 || end == 0 {
            return Err(invalid_pages("page numbers start at 1"));
        }
        if end < start {
            return Err(invalid_pages(format!("page range `{token}` is reversed")));
        }
        if end > page_count {
            return Err(invalid_pages(format!(
                "page range `{token}` is outside the {page_count}-page document"
            )));
        }
        for page in start..=end {
            if selected.contains(&page) {
                return Err(invalid_pages(format!("page {page} appears twice in the selection")));
            }
            selected.push(page);
        }
    }
    Ok(selected)
}

fn invalid_pages(message: impl Into<String>) -> ServiceError {
    ServiceError::new("session/argument-invalid", message)
}

fn temporary_path() -> PathBuf {
    let unique = TEMPORARY.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("open-pdf-extract-{}-{unique}.pdf", std::process::id()))
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
                    "acroForm": "supported",
                    "encrypted": "unsupported"
                },
                "filters": {
                    "flateDecode": "supported",
                    "asciiHexDecode": "supported",
                    "ascii85Decode": "supported",
                    "runLengthDecode": "supported",
                    "lzwDecode": "supported",
                    "pngPredictor": "supported",
                    "tiffPredictor": "supported",
                    "other": "unsupported"
                },
                "image": {
                    "colorSpaces": {
                        "deviceGray": "supported",
                        "deviceRgb": "supported",
                        "deviceCmyk": "supported",
                        "indexed": "supported",
                        "other": "unsupported"
                    }
                }
            }
        },
        "operations": {
            "inspect": "supported",
            "create": "supported",
            "merge": "supported",
            "split": "supported",
            "arrange": "supported",
            "deletePages": "supported",
            "rotatePages": "supported",
            "setPageBoxes": "supported",
            "setMetadata": "supported",
            "setBookmarks": "supported",
            "fillForm": "supported",
            "stamp": "supported",
            "extractText": "partial",
            "extractTextPages": "supported",
            "search": "partial",
            "extractImages": "supported"
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
