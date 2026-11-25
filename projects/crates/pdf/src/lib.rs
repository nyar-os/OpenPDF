//! OpenPDF product surface.
//!
//! The PDF object model, parser, filters, writer and page operations live in
//! the shared `document-pdf` crate. This crate owns only the product contract:
//! the public re-exports the CLI and Node-API layers call, plus the capability
//! matrix that describes this build.

#![deny(missing_docs)]

pub use document_pdf::{
    ArrangeReport, Bookmark, BookmarkReport, CreateReport, CreateRequest, DeletePagesReport, DocumentArrange, ExtractLoss,
    ExtractReport, ExtractedImage, FidelityCode, FidelityLoss, FormFillReport, FormValue, InspectReport, MergeReport,
    MetadataPatch, PdfBookmark, PdfFormField, PdfMetadata, PdfPageSize, RotatePagesReport, SetMetadataReport, SetPageBoxesReport,
    SplitPart, SplitReport, StampReport, StampRequest, arrange, create, delete_pages, extract_images, extract_images_bytes,
    extract_text, extract_text_bytes, fill_form, inspect, inspect_bytes, merge, rotate_pages, set_bookmarks, set_metadata,
    set_page_boxes, split, split_every, stamp,
};

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::{Value, json};

/// A failure reported by the product surface.
///
/// The code is part of the public contract and stays stable across message
/// changes, so callers compare codes instead of parsing text. `params` carries
/// the machine-readable values behind the message (for example the blocked
/// `path` of an `io/exists` failure), so the SDK can render localized copy
/// without ever reading prose.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceError {
    code: String,
    message: String,
    params: Value,
}

impl ServiceError {
    /// Builds an error from a stable code and a human-readable message.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into(), params: Value::Null }
    }

    /// Attaches the machine-readable parameters of the code.
    #[must_use]
    pub fn with_params(mut self, params: Value) -> Self {
        self.params = params;
        self
    }

    /// Returns the stable error code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the human-readable message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the machine-readable parameters, or `Value::Null` when absent.
    pub fn params(&self) -> &Value {
        &self.params
    }
}

impl From<document_pdf::ServiceError> for ServiceError {
    fn from(error: document_pdf::ServiceError) -> Self {
        Self::new(error.code(), error.message())
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ServiceError {}

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
        return Err(invalid_pages_with("`query` must be a non-empty string", json!({ "arg": "query" })));
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
        let outcome = (|| -> Result<Vec<ExtractedImage>, ServiceError> {
            arrange(path, &temporary, &request)?;
            let bytes = fs::read(&temporary).map_err(|error| {
                ServiceError::new("io/read", format!("failed to read {}: {error}", temporary.display()))
            })?;
            Ok(extract_images_bytes(&bytes, out_dir, stem)?)
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
            return Err(invalid_pages_with("page ranges contain an empty entry", json!({ "pages": ranges })));
        }
        let (start, end) = match token.split_once('-') {
            Some((start, end)) => (start.trim(), end.trim()),
            None => (token, token),
        };
        let start = start
            .parse::<usize>()
            .map_err(|_| invalid_pages_with(format!("invalid page range `{token}`"), json!({ "pages": ranges })))?;
        let end = end
            .parse::<usize>()
            .map_err(|_| invalid_pages_with(format!("invalid page range `{token}`"), json!({ "pages": ranges })))?;
        if start == 0 || end == 0 {
            return Err(invalid_pages_with("page numbers start at 1", json!({ "pages": ranges })));
        }
        if end < start {
            return Err(invalid_pages_with(format!("page range `{token}` is reversed"), json!({ "pages": ranges })));
        }
        if end > page_count {
            return Err(invalid_pages_with(
                format!("page range `{token}` is outside the {page_count}-page document"),
                json!({ "pages": ranges }),
            ));
        }
        for page in start..=end {
            if selected.contains(&page) {
                return Err(invalid_pages_with(
                    format!("page {page} appears twice in the selection"),
                    json!({ "pages": ranges }),
                ));
            }
            selected.push(page);
        }
    }
    Ok(selected)
}

/// Copies a PDF while emptying its information dictionary.
///
/// Every standard field (`Title`, `Author`, `Subject`, `Keywords`, `Creator`,
/// `Producer`) is cleared, while pages, outlines and other catalog entries are
/// kept. The output is always replaced when it already exists, matching
/// `set-metadata`.
pub fn clear_metadata(path: &Path, out: &Path) -> Result<SetMetadataReport, ServiceError> {
    let patch = MetadataPatch {
        title: Some(String::new()),
        author: Some(String::new()),
        subject: Some(String::new()),
        keywords: Some(String::new()),
        creator: Some(String::new()),
        producer: Some(String::new()),
    };
    Ok(document_pdf::set_metadata(path, out, &patch)?)
}

/// Merges inputs after removing pages outside each input's selection.
///
/// `selections` is positional: entry `i` is the one-based page ranges kept from
/// input `i`, and inputs past the end of the list keep every page. Every input
/// contributes one page plan, so the output order follows the ranges (`3,1`
/// writes page 3 before page 1) in a single compose pass. `title` replaces the
/// `/Title` of the first input's information dictionary when given. Ranges that
/// are malformed, empty or outside their document fail with
/// `session/argument-invalid` before the output is written, and an existing
/// output is replaced only with `overwrite`.
pub fn merge_selected(
    inputs: &[PathBuf],
    selections: Option<&[String]>,
    out: &Path,
    overwrite: bool,
    title: Option<&str>,
) -> Result<Value, ServiceError> {
    if inputs.is_empty() {
        return Err(invalid_pages_with("at least one input PDF is required", json!({ "arg": "inputs" })));
    }
    if let Some(selections) = selections
        && selections.len() > inputs.len()
    {
        return Err(invalid_pages_with(
            format!("{} page selections were given for {} input(s)", selections.len(), inputs.len()),
            json!({ "arg": "selections" }),
        ));
    }
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        )
        .with_params(json!({ "path": out.display().to_string() })));
    }
    let mut sources = Vec::with_capacity(inputs.len());
    let mut placements = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        let total = document_pdf::inspect(input)?.pages;
        let ranges = selections.and_then(|selections| selections.get(index)).map(|ranges| ranges.trim());
        let selected = match ranges {
            Some(ranges) => parse_page_ranges(ranges, total)?,
            None => (1..=total).collect(),
        };
        placements.extend(selected.iter().map(|page| document_pdf::PagePlacement::Source { source: index, page: *page }));
        sources.push(json!({
            "path": input.display().to_string(),
            "pages": total,
            "selected": selected.len(),
            "ranges": ranges,
        }));
    }
    let composed = document_pdf::compose_with_title(
        &document_pdf::PagePlan { sources: inputs.to_vec(), placements },
        out,
        overwrite,
        title,
    )?;
    Ok(json!({
        "path": composed.path,
        "pages": composed.pages,
        "bytes": composed.bytes,
        "sources": sources,
        "fidelity": composed.fidelity,
    }))
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
        return Err(invalid_pages_with("`times` must be between 2 and 100", json!({ "arg": "times" })));
    }
    let total = document_pdf::inspect(path)?.pages;
    let selected = parse_page_ranges(pages, total)?;
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        )
        .with_params(json!({ "path": out.display().to_string() })));
    }
    // Upstream compose builds the repeated sequence in one pass, so no
    // temporary split-and-merge round trip is needed here.
    let composed = document_pdf::duplicate_pages(path, &selected, times, out, overwrite)?;
    Ok(json!({
        "path": composed.path,
        "pages": composed.pages,
        "sourcePages": total,
        "duplicated": selected,
        "times": times,
        "fidelity": composed.fidelity,
    }))
}

/// Inserts pages of another PDF after the selected page of one PDF.
///
/// `after` is the one-based page of `path` after which the pages of `insert`
/// are placed; `0` puts them before the first page and `pages` appends them at
/// the end. `selected` is an optional one-based range list such as `3,1`: the
/// source pages are inserted in that order, and every page is used when it is
/// omitted. The output is replaced when it already exists only if `overwrite`
/// is true.
pub fn insert_pages(
    path: &Path,
    insert: &Path,
    selected: Option<&str>,
    after: u32,
    out: &Path,
    overwrite: bool,
) -> Result<serde_json::Value, ServiceError> {
    let total = document_pdf::inspect(path)?.pages;
    let inserted = document_pdf::inspect(insert)?.pages;
    let chosen = match selected {
        Some(ranges) => parse_page_ranges(ranges, inserted)?,
        None => (1..=inserted).collect(),
    };
    if after as usize > total {
        return Err(invalid_pages_with(format!("page {after} is outside the {total}-page document"), json!({ "arg": "after" })));
    }
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        )
        .with_params(json!({ "path": out.display().to_string() })));
    }
    // Upstream compose splices both documents through one page plan, so no
    // temporary split-and-merge round trip is needed here.
    let composed = document_pdf::insert_pages(path, insert, after as usize, Some(&chosen), out, overwrite)?;
    Ok(json!({
        "path": composed.path,
        "pages": composed.pages,
        "sourcePages": total,
        "insertedPages": chosen.len(),
        "sourceInsertPages": inserted,
        "selectedPages": chosen,
        "insert": insert.display().to_string(),
        "after": after,
        "fidelity": composed.fidelity,
    }))
}

/// Splits one PDF into `parts` consecutive files with counts as even as possible.
///
/// Earlier files take the remainder, so seven pages in three parts become
/// `3,2,2`. `parts` must be between `1` and the page count. Files are named by
/// the split route and written into `out_dir`.
pub fn split_parts(path: &Path, parts: usize, out_dir: &Path) -> Result<serde_json::Value, ServiceError> {
    if parts == 0 {
        return Err(invalid_pages_with("`parts` must be at least 1", json!({ "arg": "parts" })));
    }
    let total = document_pdf::inspect(path)?.pages;
    if parts > total {
        return Err(invalid_pages_with(
            format!("`parts` {parts} is larger than the {total}-page document"),
            json!({ "arg": "parts" }),
        ));
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
        return Err(invalid_pages_with(format!("page {after} is outside the {total}-page document"), json!({ "arg": "after" })));
    }
    if !(1..=100).contains(&count) {
        return Err(invalid_pages_with("`count` must be between 1 and 100", json!({ "arg": "count" })));
    }
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        )
        .with_params(json!({ "path": out.display().to_string() })));
    }
    // Upstream compose writes real blank pages inside the plan, so no
    // temporary created PDF or split-and-merge round trip is needed here.
    let size = match size {
        Some(raw) => Some(parse_blank_size(raw)?),
        None => None,
    };
    let composed = document_pdf::insert_blank_pages(path, after as usize, count as usize, size, out, overwrite)?;
    Ok(json!({
        "path": composed.path,
        "pages": composed.pages,
        "sourcePages": total,
        "insertedPages": count,
        "after": after,
        "fidelity": composed.fidelity,
    }))
}

/// Resolves a blank-page size spelling into points.
///
/// Accepts the same named papers as `create` (`a3`, `a4`, `a5`, `letter`,
/// `legal`, `tabloid`), an explicit `WIDTHxHEIGHT` extent in points, and an
/// empty spelling as the default Letter geometry. Anything else fails with
/// `session/argument-invalid` naming the `size` argument.
fn parse_blank_size(size: &str) -> Result<(f64, f64), ServiceError> {
    let raw = size.trim();
    if raw.is_empty() {
        return Ok((612.0, 792.0));
    }
    let (width, height) = match raw.to_ascii_lowercase().as_str() {
        "a3" => (841.89, 1190.55),
        "a4" => (595.28, 841.89),
        "a5" => (419.53, 595.28),
        "letter" | "us-letter" => (612.0, 792.0),
        "legal" | "us-legal" => (612.0, 1008.0),
        "tabloid" => (792.0, 1224.0),
        other => {
            let Some((width, height)) = other.split_once('x') else {
                return Err(invalid_pages_with(
                    format!("page size `{raw}` is not a named size or a WIDTHxHEIGHT extent such as 400x600"),
                    json!({ "arg": "size" }),
                ));
            };
            let width = width
                .trim()
                .parse::<f64>()
                .map_err(|_| invalid_pages_with(format!("page width in `{raw}` is not a number"), json!({ "arg": "size" })))?;
            let height = height
                .trim()
                .parse::<f64>()
                .map_err(|_| invalid_pages_with(format!("page height in `{raw}` is not a number"), json!({ "arg": "size" })))?;
            (width, height)
        }
    };
    if !width.is_finite() || !height.is_finite() || !(100.0..=14400.0).contains(&width) || !(100.0..=14400.0).contains(&height) {
        return Err(invalid_pages_with("page width and height must be between 100 and 14400 points", json!({ "arg": "size" })));
    }
    Ok((width, height))
}

/// Replaces the selected pages of one PDF with pages of another PDF.
///
/// The selected pages are removed and the chosen replacement pages take the
/// position of the lowest removed page; the remaining pages keep their order.
/// `selected` is an optional one-based source range list such as `3,1`, so the
/// replacement pages follow that order instead of the whole input; every page
/// is used when it is omitted. The output is replaced when it already exists
/// only if `overwrite` is true.
pub fn replace_pages(
    path: &Path,
    pages: &str,
    insert: &Path,
    selected: Option<&str>,
    out: &Path,
    overwrite: bool,
) -> Result<serde_json::Value, ServiceError> {
    let total = document_pdf::inspect(path)?.pages;
    let selection = parse_page_ranges(pages, total)?;
    let available = document_pdf::inspect(insert)?.pages;
    if available == 0 {
        return Err(invalid_pages_with("the replacement PDF has no pages", json!({ "arg": "insert" })));
    }
    let chosen = match selected {
        Some(ranges) => parse_page_ranges(ranges, available)?,
        None => (1..=available).collect(),
    };
    let replacement = chosen.len();
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        )
        .with_params(json!({ "path": out.display().to_string() })));
    }
    // Upstream compose splices the replacement pages at the lowest selected
    // page in one pass, so no temporary split-and-merge round trip is needed.
    let composed = document_pdf::replace_pages(path, &selection, insert, Some(&chosen), out, overwrite)?;
    Ok(json!({
        "path": composed.path,
        "pages": composed.pages,
        "sourcePages": total,
        "replaced": selection,
        "replacementPages": replacement,
        "sourceInsertPages": available,
        "selectedPages": chosen,
        "insert": insert.display().to_string(),
        "fidelity": composed.fidelity,
    }))
}

/// Reverses the page order, or only the selected pages, into a new PDF.
///
/// `selected` is an optional one-based range list; those pages trade places in
/// place while every other page keeps its position. The output is replaced when
/// it already exists only if `overwrite` is true.
pub fn reverse_pages(
    path: &Path,
    out: &Path,
    overwrite: bool,
    selected: Option<&str>,
) -> Result<serde_json::Value, ServiceError> {
    let total = document_pdf::inspect(path)?.pages;
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        )
        .with_params(json!({ "path": out.display().to_string() })));
    }
    let reversed = match selected {
        Some(ranges) => parse_page_ranges(ranges, total)?,
        None => (1..=total).collect(),
    };
    // Upstream owns the flip permutation, so no local order vector and
    // arrange round trip is needed here.
    let arranged = document_pdf::reverse_pages(path, Some(&reversed), out, overwrite)?;
    Ok(json!({
        "path": arranged.path,
        "pages": arranged.pages,
        "sourcePages": total,
        "reversed": reversed,
        "fidelity": arranged.fidelity,
    }))
}

/// Moves the selected pages so they follow one anchor page.
///
/// The pages keep their relative order and every other page keeps its original
/// order. `after` is a one-based page that must stay put, or `0` to move the
/// selection to the front; an anchor inside the selection is refused because
/// its position is being moved. The output is replaced when it already exists
/// only if `overwrite` is true.
pub fn move_pages(path: &Path, pages: &str, after: u32, out: &Path, overwrite: bool) -> Result<serde_json::Value, ServiceError> {
    let total = document_pdf::inspect(path)?.pages;
    let selected = parse_page_ranges(pages, total)?;
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        )
        .with_params(json!({ "path": out.display().to_string() })));
    }
    // Upstream owns the anchor rules and the relocation permutation; the
    // product only re-attaches the rejected argument to the typed failure.
    let arranged = document_pdf::move_pages(path, &selected, after as usize, out, overwrite)
        .map_err(|error| annotate_argument(error, json!({ "arg": "after", "pages": pages })))?;
    Ok(json!({
        "path": arranged.path,
        "pages": arranged.pages,
        "sourcePages": total,
        "moved": selected,
        "after": after,
        "fidelity": arranged.fidelity,
    }))
}

/// Exchanges two equal-length page selections while every other page keeps its
/// position.
///
/// Both selections expand the same way as [`reverse_pages`], must be the same
/// length and must not share a page; the first page of `pages` trades places
/// with the first of `with`, and so on. The output is replaced when it already
/// exists only if `overwrite` is true.
pub fn swap_pages(path: &Path, pages: &str, with: &str, out: &Path, overwrite: bool) -> Result<serde_json::Value, ServiceError> {
    let total = document_pdf::inspect(path)?.pages;
    let left = parse_page_ranges(pages, total)?;
    let right = parse_page_ranges(with, total)?;
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        )
        .with_params(json!({ "path": out.display().to_string() })));
    }
    // Upstream owns the pairing rules and the swap permutation; the product
    // only re-attaches the rejected selections to the typed failure.
    let arranged = document_pdf::swap_pages(path, &left, &right, out, overwrite)
        .map_err(|error| annotate_argument(error, json!({ "pages": pages, "with": with })))?;
    Ok(json!({
        "path": arranged.path,
        "pages": arranged.pages,
        "sourcePages": total,
        "swapped": left,
        "with": right,
        "fidelity": arranged.fidelity,
    }))
}

/// Stamps sequential page numbers on the selected pages of one PDF.
///
/// `pages` expands the same way as [`reverse_pages`]; the first selected page
/// is numbered `start` and every following page counts up, so the last number
/// is `start + selected - 1`. `format` accepts `{page}` and `{pages}` and
/// defaults to `{page} of {pages}`; the numbers are substituted here, so the
/// stamped text never follows the source page count. `position` is `center`,
/// `top` or `bottom` and defaults to `bottom`. The output is replaced when it
/// already exists only if `overwrite` is true.
pub fn stamp_page_numbers(
    path: &Path,
    pages: &str,
    start: u32,
    format: Option<&str>,
    position: Option<&str>,
    out: &Path,
    overwrite: bool,
) -> Result<Value, ServiceError> {
    if start < 1 {
        return Err(invalid_pages_with("`start` must be at least 1", json!({ "arg": "start" })));
    }
    let format = format.unwrap_or("{page} of {pages}");
    if format.trim().is_empty() {
        return Err(invalid_pages_with("`format` must not be empty", json!({ "arg": "format" })));
    }
    let position = position.unwrap_or("bottom");
    if !matches!(position, "center" | "top" | "bottom") {
        return Err(invalid_pages_with("`position` must be `center`, `top` or `bottom`", json!({ "arg": "position" })));
    }
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        )
        .with_params(json!({ "path": out.display().to_string() })));
    }
    let total = document_pdf::inspect(path)?.pages;
    let selected = parse_page_ranges(pages, total)?;
    let last = start + selected.len() as u32 - 1;
    // `stamp` substitutes `{page}` with the source page number, so every
    // number is resolved here and the pages are chained one stamp at a time.
    let mut temporaries = Vec::new();
    let outcome = (|| -> Result<(), ServiceError> {
        let mut current = path.to_path_buf();
        for (ordinal, page) in selected.iter().enumerate() {
            let number = start + ordinal as u32;
            let request = StampRequest {
                text: format.replace("{page}", &number.to_string()).replace("{pages}", &last.to_string()),
                position: Some(position.to_string()),
                pages: Some(page.to_string()),
                ..StampRequest::default()
            };
            let next = if ordinal + 1 == selected.len() { out.to_path_buf() } else { temporary_path() };
            document_pdf::stamp(&current, &next, &request)?;
            if next.as_path() != out {
                temporaries.push(next.clone());
            }
            current = next;
        }
        Ok(())
    })();
    for temporary in &temporaries {
        let _ = fs::remove_file(temporary);
    }
    outcome?;
    Ok(json!({
        "path": out.display().to_string(),
        "pages": total,
        "numbered": selected,
        "start": start,
        "last": last,
        "format": format,
        "position": position,
    }))
}

/// Splits one PDF into consecutive files at its top-level bookmarks.
///
/// The outline tree is read through `document.pdf`'s resolved outline report:
/// every level-one entry names the page where a new file starts, and the
/// ranges between consecutive bookmark pages are written by `split`. Pages
/// before the first bookmark form their own file when there are any. A
/// document without top-level bookmarks fails with `session/argument-invalid`,
/// as does a bookmark whose destination cannot be resolved to a page of the
/// document; nothing is written in either case.
pub fn split_bookmarks(path: &Path, out_dir: &Path) -> Result<Value, ServiceError> {
    let entries: Vec<(String, usize)> = document_pdf::list_outline(path)?
        .into_iter()
        .filter(|entry| entry.depth == 1)
        .map(|entry| match entry.page {
            Some(page) => Ok((entry.title, page)),
            None => Err(invalid_pages(format!("bookmark `{}` has no internal destination", entry.title))),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if entries.is_empty() {
        return Err(invalid_pages("the document has no bookmarks"));
    }
    let total = document_pdf::inspect(path)?.pages;
    let mut boundaries: Vec<usize> = entries.iter().map(|(_, page)| *page).collect();
    boundaries.sort_unstable();
    boundaries.dedup();
    if boundaries[0] > 1 {
        boundaries.insert(0, 1);
    }
    let mut ranges = Vec::with_capacity(boundaries.len());
    for (index, start) in boundaries.iter().enumerate() {
        let end = boundaries.get(index + 1).map_or(total, |next| next - 1);
        ranges.push(if start == &end { end.to_string() } else { format!("{start}-{end}") });
    }
    let report = document_pdf::split(path, &ranges.join(","), out_dir)?;
    Ok(json!({
        "outDir": out_dir.display().to_string(),
        "pages": total,
        "parts": ranges.len(),
        "boundaries": entries
            .iter()
            .map(|(title, page)| json!({ "title": title, "page": page }))
            .collect::<Vec<_>>(),
        "documents": report.documents,
    }))
}

/// Lists every link annotation a PDF carries.
///
/// The `/Annots` arrays of all pages are read directly and every `/Link`
/// annotation is reported once, in page order and then annotation order. An
/// entry carries the one-based `page`, the annotation `rect` when it is four
/// numbers, the external `uri` for URI actions and the one-based
/// `destination` page for internal `GoTo` targets, whether the target sits in
/// an `/A` action or a direct `/Dest`, including named destinations and
/// `GoToE` targets resolved inside an embedded document. `uri` and
/// `destination` are `null` when the annotation has no resolvable target.
pub fn extract_links(path: &Path) -> Result<Value, ServiceError> {
    let report = document_pdf::inspect(path)?;
    let records = document_pdf::extract_links(path)?;
    let links = records
        .iter()
        .map(|record| {
            json!({
                "page": record.page,
                "rect": record.rect,
                "uri": matches!(record.kind, document_pdf::LinkKind::Uri).then_some(record.target.clone()).flatten(),
                "destination": matches!(record.kind, document_pdf::LinkKind::GoTo | document_pdf::LinkKind::GoToEmbedded)
                    .then_some(record.destination)
                    .flatten(),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "path": path.display().to_string(),
        "pages": report.pages,
        "count": links.len(),
        "links": links,
    }))
}

/// Lists the named destinations a PDF resolves to pages of itself.
///
/// `/Root/Dests` is read first and the classic `/Root/Names/Dests` tree is
/// used as the fallback. Each entry carries the destination `name` as spelled
/// in the name tree and the one-based `page` it points at; destinations that
/// do not resolve to a page of the document are skipped.
pub fn list_destinations(path: &Path) -> Result<Value, ServiceError> {
    let destinations = document_pdf::list_destinations(path)?;
    serde_json::to_value(destinations)
        .map_err(|error| ServiceError::new("format/invalid", format!("the destination report cannot be serialized: {error}")))
}

/// Interleaves the pages of two PDFs into one document.
///
/// The output starts with the first page of `first`, then the first page of
/// `second`, and so on; when one document runs out, the remaining pages of
/// the longer one follow in order. Both documents must carry at least one
/// page, and the output is replaced only when `overwrite` is true.
pub fn interleave_pages(first: &Path, second: &Path, out: &Path, overwrite: bool) -> Result<Value, ServiceError> {
    if out.exists() && !overwrite {
        return Err(ServiceError::new(
            "io/exists",
            format!("{} already exists; pass overwrite to replace it", out.display()),
        )
        .with_params(json!({ "path": out.display().to_string() })));
    }
    let first_pages = document_pdf::inspect(first)?.pages;
    let second_pages = document_pdf::inspect(second)?.pages;
    if first_pages == 0 || second_pages == 0 {
        return Err(invalid_pages("both documents must carry at least one page"));
    }
    // Upstream compose alternates the two documents through one page plan,
    // so no temporary split-and-merge round trip is needed here.
    let composed = document_pdf::interleave_pages(first, second, out, overwrite)?;
    Ok(json!({
        "path": composed.path,
        "pages": composed.pages,
        "first": first.display().to_string(),
        "firstPages": first_pages,
        "second": second.display().to_string(),
        "secondPages": second_pages,
        "fidelity": composed.fidelity,
    }))
}

/// Builds a `session/argument-invalid` failure whose `reason` param carries the message.
fn invalid_pages(message: impl Into<String>) -> ServiceError {
    invalid_pages_with(message, Value::Null)
}

/// Builds a `session/argument-invalid` failure merging extra typed params over the `reason`.
fn invalid_pages_with(message: impl Into<String>, extra: Value) -> ServiceError {
    let message = message.into();
    let mut params = serde_json::Map::new();
    params.insert("reason".to_string(), json!(message.clone()));
    if let Value::Object(extra) = extra {
        params.extend(extra);
    }
    ServiceError::new("session/argument-invalid", message).with_params(Value::Object(params))
}

/// Re-attaches typed product params to an upstream argument failure, keeping
/// every other upstream code on its plain product mapping.
fn annotate_argument(error: document_pdf::ServiceError, extra: Value) -> ServiceError {
    if error.code() == "session/argument-invalid" {
        return invalid_pages_with(error.message(), extra);
    }
    error.into()
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
            "splitBookmarks": "supported",
            "arrange": "supported",
            "movePages": "supported",
            "deletePages": "supported",
            "duplicatePages": "supported",
            "insertPages": "supported",
            "insertBlankPages": "supported",
            "replacePages": "supported",
            "interleavePages": "supported",
            "reversePages": "supported",
            "rotatePages": "supported",
            "setPageBoxes": "supported",
            "metadata": "supported",
            "setMetadata": "supported",
            "clearMetadata": "supported",
            "setBookmarks": "supported",
            "fillForm": "supported",
            "stamp": "supported",
            "stampPageNumbers": "supported",
            "extractText": "partial",
            "extractTextPages": "supported",
            "search": "partial",
            "extractImages": "supported",
            "extractImagesPages": "supported",
            "extractLinks": "supported",
            "listDestinations": "supported"
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
