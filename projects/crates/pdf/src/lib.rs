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
    let (source_pages, selected, extracted) = extract_selected(path, ranges)?;
    Ok(json!({
        "pages": extracted.pages,
        "sourcePages": source_pages,
        "selected": selected,
        "markdown": extracted.markdown,
        "losses": extracted.losses,
    }))
}

/// Extracts text from `ranges`, projecting the selection when it is partial.
///
/// Returns the source page count, the one-based pages that were extracted in
/// order, and the extraction report produced from the projection.
fn extract_selected(
    path: &Path,
    ranges: &str,
) -> Result<(usize, Vec<usize>, ExtractReport), ServiceError> {
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
    Ok((report.pages, selected, extracted))
}

/// Searches extracted text and reports every line match.
///
/// Matching runs on the Markdown produced by [`extract_text`], so a PDF page
/// without extractable text cannot contribute hits; such pages are reported in
/// `losses` instead. `limit` defaults to 100 and is clamped to 1-500. With
/// `match_case` off, ASCII letters compare case-insensitively while byte
/// offsets still name the authored line. `whole_word` accepts only matches
/// whose neighbouring characters are not word characters. Each hit carries the
/// one-based line, a one-based column, byte offsets within the line and the
/// matching fragment.
pub fn search(
    path: &Path,
    query: &str,
    limit: Option<u32>,
    match_case: bool,
    whole_word: bool,
) -> Result<Value, ServiceError> {
    let report = extract_text(path)?;
    search_report(path, query, limit, match_case, whole_word, &report, None)
}

/// Searches only the selected one-based pages, in the requested order.
///
/// `ranges` accepts comma-separated pages and inclusive ranges such as
/// `1-3,5`. The selection is projected the same way as
/// [`extract_text_pages`], so hits report lines and columns within that
/// projection while `sourcePages` and `selected` name the source pages.
pub fn search_pages(
    path: &Path,
    query: &str,
    limit: Option<u32>,
    ranges: &str,
    match_case: bool,
    whole_word: bool,
) -> Result<Value, ServiceError> {
    let (source_pages, selected, report) = extract_selected(path, ranges)?;
    search_report(path, query, limit, match_case, whole_word, &report, Some((source_pages, &selected)))
}

fn search_report(
    path: &Path,
    query: &str,
    limit: Option<u32>,
    match_case: bool,
    whole_word: bool,
    report: &ExtractReport,
    selection: Option<(usize, &[usize])>,
) -> Result<Value, ServiceError> {
    if query.trim().is_empty() {
        return Err(invalid_pages("`query` must be a non-empty string"));
    }
    let limit = limit.map_or(100usize, |value| value.clamp(1, 500) as usize);
    let needle = if match_case { query.to_owned() } else { query.to_ascii_lowercase() };
    let mut hits = Vec::new();
    let mut truncated = false;
    'lines: for (index, line) in report.markdown.lines().enumerate() {
        let haystack = if match_case { line.to_owned() } else { line.to_ascii_lowercase() };
        let mut from = 0;
        while let Some(offset) = haystack[from..].find(&needle) {
            let start = from + offset;
            let end = start + needle.len();
            if whole_word && !is_whole_word(line, start, end) {
                from = start + line[start..].chars().next().map_or(1, char::len_utf8);
                continue;
            }
            if hits.len() == limit {
                truncated = true;
                break 'lines;
            }
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
    let mut outcome = json!({
        "path": path.display().to_string(),
        "query": query,
        "matchCase": match_case,
        "wholeWord": whole_word,
        "limit": limit,
        "pages": report.pages,
        "lines": report.markdown.lines().count(),
        "truncated": truncated,
        "hits": hits,
        "losses": report.losses,
    });
    if let Some((source_pages, selected)) = selection {
        outcome["sourcePages"] = json!(source_pages);
        outcome["selected"] = json!(selected);
    }
    Ok(outcome)
}

/// Reports whether the byte span sits on word boundaries in `line`.
///
/// A boundary means the neighbouring character is absent or is not a letter,
/// digit or underscore, so `cat` matches in `a cat.` but not in `concat`.
fn is_whole_word(line: &str, start: usize, end: usize) -> bool {
    let before = line[..start].chars().next_back();
    let after = line[end..].chars().next();
    !before.is_some_and(is_word_character) && !after.is_some_and(is_word_character)
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Reads the outline tree of one PDF without rendering or rewriting it.
///
/// Entries keep document order and report a one-based `depth`, so top-level
/// bookmarks are `1`. `count` is the number of entries and `maxDepth` the
/// deepest level, both zero when the document has no outline.
pub fn outline(path: &Path) -> Result<Value, ServiceError> {
    let report = inspect(path)?;
    let max_depth = report.bookmarks.iter().map(|bookmark| bookmark.depth).max().unwrap_or(0);
    let count = report.bookmarks.len();
    Ok(json!({
        "path": path.display().to_string(),
        "pages": report.pages,
        "count": count,
        "maxDepth": max_depth,
        "bookmarks": report.bookmarks,
    }))
}

/// Extracts images from the selected one-based pages into `out_dir`.
///
/// `ranges` accepts comma-separated pages and inclusive ranges such as
/// `1-3,5`. A selection that is not the whole document in order is projected
/// into a temporary classic PDF first, so images keep the same decoder path.
/// Every reported page is remapped to its source page and written files are
/// renamed to carry that page number.
pub fn extract_images_pages(path: &Path, out_dir: &Path, ranges: &str) -> Result<Value, ServiceError> {
    let report = inspect(path)?;
    let selected = parse_page_ranges(ranges, report.pages)?;
    let mut images = if selected.iter().copied().eq(1..=report.pages) {
        extract_images(path, out_dir)?
    }
    else {
        let remove = (1..=report.pages).filter(|page| !selected.contains(page)).collect::<Vec<_>>();
        let temporary = temporary_path();
        let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("pdf");
        let request = DocumentArrange { order: Some(selected.clone()), rotate: None, remove: Some(remove) };
        let outcome = (|| {
            arrange(path, &temporary, &request)?;
            let bytes = fs::read(&temporary).map_err(|error| {
                ServiceError::new("io/read", format!("failed to read {}: {error}", temporary.display()))
            })?;
            extract_images_bytes(&bytes, out_dir, stem)
        })();
        let _ = fs::remove_file(&temporary);
        outcome?
    };
    for image in &mut images {
        let Some(source) = image.page.checked_sub(1).and_then(|index| selected.get(index)).copied() else {
            continue;
        };
        if source == image.page {
            continue;
        }
        if !image.path.is_empty()
            && let Some(name) = Path::new(&image.path).file_name().and_then(|name| name.to_str())
        {
            let from = format!("-p{}-img{}", image.page, image.index);
            let to = format!("-p{source}-img{}", image.index);
            let renamed = Path::new(&image.path).with_file_name(name.replacen(&from, &to, 1));
            if fs::rename(&image.path, &renamed).is_ok() {
                image.path = renamed.display().to_string();
            }
            else {
                continue;
            }
        }
        image.page = source;
    }
    serde_json::to_value(&images)
        .map_err(|error| ServiceError::new("format/invalid", format!("the image report cannot be serialized: {error}")))
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
            "outline": "supported",
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
            "extractImages": "supported",
            "extractImagesPages": "supported"
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
