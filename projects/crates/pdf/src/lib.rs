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
/// whose neighbouring characters are not word characters. `skip` discards that
/// many accepted matches before collecting, so callers can page through hits.
/// Each hit carries the one-based line, a one-based column, byte offsets within
/// the line and the matching fragment.
pub fn search(
    path: &Path,
    query: &str,
    limit: Option<u32>,
    match_case: bool,
    whole_word: bool,
    skip: u32,
) -> Result<Value, ServiceError> {
    let report = extract_text(path)?;
    search_report(path, query, limit, match_case, whole_word, skip, &report, None)
}

/// Searches only the selected one-based pages, in the requested order.
///
/// `ranges` accepts comma-separated pages and inclusive ranges such as
/// `1-3,5`. The selection is projected the same way as
/// [`extract_text_pages`], so hits report lines and columns within that
/// projection while `sourcePages` and `selected` name the source pages.
#[allow(clippy::too_many_arguments)]
pub fn search_pages(
    path: &Path,
    query: &str,
    limit: Option<u32>,
    ranges: &str,
    match_case: bool,
    whole_word: bool,
    skip: u32,
) -> Result<Value, ServiceError> {
    let (source_pages, selected, report) = extract_selected(path, ranges)?;
    search_report(path, query, limit, match_case, whole_word, skip, &report, Some((source_pages, &selected)))
}

#[allow(clippy::too_many_arguments)]
fn search_report(
    path: &Path,
    query: &str,
    limit: Option<u32>,
    match_case: bool,
    whole_word: bool,
    skip: u32,
    report: &ExtractReport,
    selection: Option<(usize, &[usize])>,
) -> Result<Value, ServiceError> {
    if query.trim().is_empty() {
        return Err(invalid_pages("`query` must be a non-empty string"));
    }
    let limit = limit.map_or(100usize, |value| value.clamp(1, 500) as usize);
    let skip = skip as usize;
    let needle = if match_case { query.to_owned() } else { query.to_ascii_lowercase() };
    let mut hits = Vec::new();
    let mut skipped = 0usize;
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
            if skipped < skip {
                skipped += 1;
                from = end;
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
        "skip": skip,
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

/// Reads the document information dictionary of one PDF without rewriting it.
///
/// The report carries the page count and the metadata fields; each field is a
/// string or `null` when the document omits it.
pub fn metadata(path: &Path) -> Result<Value, ServiceError> {
    let report = inspect(path)?;
    Ok(json!({
        "path": path.display().to_string(),
        "pages": report.pages,
        "metadata": report.metadata,
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

/// Repeats the selected pages directly after themselves.
///
/// The document keeps its order; each selected page is written `times` times,
/// so `times: 2` doubles the selection in place. The output is replaced when it
/// already exists only if `overwrite` is true.
pub fn duplicate_pages(
    path: &Path,
    pages: &str,
    times: u32,
    out: &Path,
    overwrite: bool,
) -> Result<serde_json::Value, ServiceError> {
    if !(2..=100).contains(&times) {
        return Err(invalid_pages("`times` must be between 2 and 100"));
    }
    let total = document_pdf::inspect(path)?.pages;
    let selected = parse_page_ranges(pages, total)?;
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        ));
    }
    // `arrange` refuses duplicate pages, so split the source into one-page
    // files and merge the repeated sequence back in order.
    let scratch = temporary_path().with_extension("dup");
    let split = document_pdf::split_every(path, 1, &scratch)?;
    let mut inputs = Vec::with_capacity(total + selected.len() * (times as usize - 1));
    for part in &split.documents {
        inputs.push(PathBuf::from(&part.path));
        let page = part.path.rsplit_once("-p").and_then(|(_, tail)| tail.split('.').next()).and_then(|number| number.parse::<usize>().ok());
        let Some(page) = page else {
            let _ = fs::remove_dir_all(&scratch);
            return Err(ServiceError::new("format/invalid", "split pages are not named by page number"));
        };
        if selected.contains(&page) {
            for _ in 1..times {
                inputs.push(PathBuf::from(&part.path));
            }
        }
    }
    let merged = document_pdf::merge(&inputs, out, true, None);
    let _ = fs::remove_dir_all(&scratch);
    let merged = merged?;
    Ok(json!({
        "path": merged.path,
        "pages": merged.pages,
        "sourcePages": total,
        "duplicated": selected,
        "times": times,
    }))
}

/// Inserts every page of another PDF after the selected page of one PDF.
///
/// `after` is the one-based page of `path` after which the pages of `insert`
/// are placed; `0` puts them before the first page and `pages` appends them at
/// the end. The output is replaced when it already exists only if `overwrite`
/// is true.
pub fn insert_pages(
    path: &Path,
    insert: &Path,
    after: u32,
    out: &Path,
    overwrite: bool,
) -> Result<serde_json::Value, ServiceError> {
    let total = document_pdf::inspect(path)?.pages;
    let inserted = document_pdf::inspect(insert)?.pages;
    if after as usize > total {
        return Err(invalid_pages(format!("page {after} is outside the {total}-page document")));
    }
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        ));
    }
    // `arrange` cannot reach across documents, so split both files into
    // one-page parts and merge the interleaved sequence back in order.
    let target_scratch = temporary_path().with_extension("ins-a");
    let insert_scratch = temporary_path().with_extension("ins-b");
    let outcome = (|| {
        let target = document_pdf::split_every(path, 1, &target_scratch)?;
        let parts = document_pdf::split_every(insert, 1, &insert_scratch)?;
        if target.documents.len() != total || parts.documents.len() != inserted {
            return Err(ServiceError::new("format/invalid", "split did not produce one file per page"));
        }
        let mut inputs = Vec::with_capacity(total + inserted);
        inputs.extend(target.documents.iter().take(after as usize).map(|part| PathBuf::from(&part.path)));
        inputs.extend(parts.documents.iter().map(|part| PathBuf::from(&part.path)));
        inputs.extend(target.documents.iter().skip(after as usize).map(|part| PathBuf::from(&part.path)));
        document_pdf::merge(&inputs, out, true, None)
    })();
    let _ = fs::remove_dir_all(&target_scratch);
    let _ = fs::remove_dir_all(&insert_scratch);
    let merged = outcome?;
    Ok(json!({
        "path": merged.path,
        "pages": merged.pages,
        "sourcePages": total,
        "insertedPages": inserted,
        "insert": insert.display().to_string(),
        "after": after,
    }))
}

/// Splits one PDF into `parts` consecutive files with counts as even as possible.
///
/// Earlier files take the remainder, so seven pages in three parts become
/// `3,2,2`. `parts` must be between `1` and the page count. Files are named by
/// the split route and written into `out_dir`.
pub fn split_parts(path: &Path, parts: usize, out_dir: &Path) -> Result<serde_json::Value, ServiceError> {
    if parts == 0 {
        return Err(invalid_pages("`parts` must be at least 1"));
    }
    let total = document_pdf::inspect(path)?.pages;
    if parts > total {
        return Err(invalid_pages(format!("`parts` {parts} is larger than the {total}-page document")));
    }
    let base = total / parts;
    let extra = total % parts;
    let mut ranges = Vec::with_capacity(parts);
    let mut start = 1usize;
    for index in 0..parts {
        let count = base + usize::from(index < extra);
        let end = start + count - 1;
        ranges.push(if start == end { start.to_string() } else { format!("{start}-{end}") });
        start = end + 1;
    }
    let report = document_pdf::split(path, &ranges.join(","), out_dir)?;
    Ok(json!({
        "outDir": out_dir.display().to_string(),
        "parts": parts,
        "pages": total,
        "documents": report.documents,
    }))
}

/// Inserts `count` blank pages after a page of one PDF.
///
/// `after` is the one-based page the blanks follow; `0` puts them before the
/// first page and `pages` appends them at the end. When `size` is absent the
/// blank pages take the geometry of the source's first page, so an insertion
/// keeps the document's paper. The output is replaced when it already exists
/// only if `overwrite` is true.
pub fn insert_blank_pages(
    path: &Path,
    after: u32,
    count: u32,
    size: Option<&str>,
    out: &Path,
    overwrite: bool,
) -> Result<serde_json::Value, ServiceError> {
    let report = document_pdf::inspect(path)?;
    let total = report.pages;
    if after as usize > total {
        return Err(invalid_pages(format!("page {after} is outside the {total}-page document")));
    }
    if !(1..=100).contains(&count) {
        return Err(invalid_pages("`count` must be between 1 and 100"));
    }
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        ));
    }
    // A blank page is a page whose text is empty; `create` still writes the
    // MediaBox, so the inserted pages keep a real size.
    let page_size = size.map(str::to_owned).or_else(|| {
        report.page_sizes.first().map(|page| format!("{}x{}", page.width_pt, page.height_pt))
    });
    let scratch = temporary_path().with_extension("blank.pdf");
    let created = document_pdf::create(&CreateRequest {
        out: scratch.clone(),
        title: None,
        pages: vec![String::new(); count as usize],
        overwrite: true,
        page_size,
        font: None,
        font_size: None,
        margin_pt: None,
        align: None,
        author: None,
        subject: None,
        keywords: None,
        creator: None,
        page_numbers: None,
    });
    let outcome = created.and_then(|_| insert_pages(path, &scratch, after, out, true));
    let _ = fs::remove_file(&scratch);
    let merged = outcome?;
    Ok(json!({
        "path": merged["path"],
        "pages": merged["pages"],
        "sourcePages": total,
        "insertedPages": count,
        "after": after,
    }))
}

/// Replaces the selected pages of one PDF with every page of another PDF.
///
/// The selected pages are removed and the pages of `insert` take the position
/// of the first removed page; the remaining pages keep their order. The output
/// is replaced when it already exists only if `overwrite` is true.
pub fn replace_pages(
    path: &Path,
    pages: &str,
    insert: &Path,
    out: &Path,
    overwrite: bool,
) -> Result<serde_json::Value, ServiceError> {
    let total = document_pdf::inspect(path)?.pages;
    let selected = parse_page_ranges(pages, total)?;
    let replacement = document_pdf::inspect(insert)?.pages;
    if replacement == 0 {
        return Err(invalid_pages("the replacement PDF has no pages"));
    }
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        ));
    }
    // `arrange` cannot reach across documents, so split both files into
    // one-page parts and merge the spliced sequence back in order.
    let target_scratch = temporary_path().with_extension("rep-a");
    let insert_scratch = temporary_path().with_extension("rep-b");
    let outcome = (|| {
        let target = document_pdf::split_every(path, 1, &target_scratch)?;
        let parts = document_pdf::split_every(insert, 1, &insert_scratch)?;
        if target.documents.len() != total {
            return Err(ServiceError::new("format/invalid", "split did not produce one file per page"));
        }
        let first = selected.first().copied().unwrap_or_default();
        let mut inputs = Vec::with_capacity(total + parts.documents.len());
        for (index, part) in target.documents.iter().enumerate() {
            let page = index + 1;
            if page == first {
                inputs.extend(parts.documents.iter().map(|replacement| PathBuf::from(&replacement.path)));
            }
            if !selected.contains(&page) {
                inputs.push(PathBuf::from(&part.path));
            }
        }
        document_pdf::merge(&inputs, out, true, None)
    })();
    let _ = fs::remove_dir_all(&target_scratch);
    let _ = fs::remove_dir_all(&insert_scratch);
    let merged = outcome?;
    Ok(json!({
        "path": merged.path,
        "pages": merged.pages,
        "sourcePages": total,
        "replaced": selected,
        "replacementPages": replacement,
        "insert": insert.display().to_string(),
    }))
}

/// Reverses the page order of one PDF into a new document.
///
/// The last page becomes the first; page content is copied, not rewritten.
/// The output is replaced when it already exists only if `overwrite` is true.
pub fn reverse_pages(path: &Path, out: &Path, overwrite: bool) -> Result<serde_json::Value, ServiceError> {
    let total = document_pdf::inspect(path)?.pages;
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        ));
    }
    let order: Vec<usize> = (1..=total).rev().collect();
    let request = DocumentArrange { order: Some(order), rotate: None, remove: None };
    let arranged = document_pdf::arrange(path, out, &request)?;
    Ok(json!({
        "path": arranged.path,
        "pages": arranged.pages,
        "sourcePages": total,
    }))
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
            "splitParts": "supported",
            "arrange": "supported",
            "deletePages": "supported",
            "duplicatePages": "supported",
            "insertPages": "supported",
            "insertBlankPages": "supported",
            "replacePages": "supported",
            "reversePages": "supported",
            "rotatePages": "supported",
            "setPageBoxes": "supported",
            "metadata": "supported",
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
