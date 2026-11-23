//! Thin Node-API surface for OpenPDF.
//!
//! Every export performs one orthogonal operation and returns a JSON string, so
//! the TypeScript layer owns argument parsing, help text and exit codes. PDF
//! semantics stay in `open-pdf-core`; this layer only forwards the declared
//! contracts.

#![deny(missing_docs)]

use std::path::PathBuf;

use wae::{CallContext, Error, JsValue, Result, function, register_module};

fn argument(ctx: &CallContext, index: usize) -> Result<JsValue> {
    ctx.args().get(index).copied().ok_or_else(|| Error::new("session/argument-invalid: a required argument is missing"))
}

fn text(ctx: &CallContext, index: usize) -> Result<String> {
    argument(ctx, index)?.as_string()
}

fn optional_text(ctx: &CallContext, index: usize) -> Result<Option<String>> {
    match ctx.args().get(index) {
        Some(value) if !value.is_undefined()? => {
            let text = value.as_string()?;
            if text.is_empty() { Ok(None) } else { Ok(Some(text)) }
        }
        _ => Ok(None),
    }
}

/// Reads a `Buffer` or `Uint8Array` argument as raw bytes.
fn bytes(ctx: &CallContext, index: usize) -> Result<Vec<u8>> {
    let value = argument(ctx, index)?;
    let invalid = || Error::new("session/argument-invalid: expected a Buffer or Uint8Array of PDF bytes");
    let length = value.get_named("length").map_err(|_| invalid())?.as_i64().map_err(|_| invalid())?;
    let length = usize::try_from(length).map_err(|_| invalid())?;
    let mut output = Vec::with_capacity(length);
    for position in 0..length {
        let position = u32::try_from(position).map_err(|_| invalid())?;
        let byte = value.element(position)?.as_i64().map_err(|_| invalid())?;
        output.push(u8::try_from(byte).map_err(|_| invalid())?);
    }
    Ok(output)
}

fn respond(ctx: &CallContext, value: &impl serde::Serialize) -> Result<JsValue> {
    let text = serde_json::to_string(value).map_err(|error| Error::new(format!("capability/not-implemented: {error}")))?;
    ctx.env().string(&text)
}

fn failure(error: open_pdf_core::ServiceError) -> Error {
    Error::new(format!("{}: {}", error.code(), error.message()))
}

/// Library version of the native binding.
#[function]
fn version(ctx: CallContext) -> Result<JsValue> {
    ctx.env().string(env!("CARGO_PKG_VERSION"))
}

/// Reports the declared format, operation, check and renderer matrix.
#[function]
fn capabilities(ctx: CallContext) -> Result<JsValue> {
    let matrix = open_pdf_core::capabilities().to_string();
    ctx.env().string(&matrix)
}

/// Inspects one PDF file.
#[function]
fn pdf_inspect(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    respond(&ctx, &open_pdf_core::inspect(std::path::Path::new(&path)).map_err(failure)?)
}

/// Reads the outline tree of one PDF file.
#[function]
fn pdf_outline(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    respond(&ctx, &open_pdf_core::outline(std::path::Path::new(&path)).map_err(failure)?)
}

/// Reads the document information dictionary of one PDF.
#[function]
fn pdf_metadata(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let report = open_pdf_core::metadata(std::path::Path::new(&path)).map_err(failure)?;
    respond(&ctx, &report)
}

/// Merges PDF files in the given order.
#[function]
fn pdf_merge(ctx: CallContext) -> Result<JsValue> {
    let inputs_json = text(&ctx, 0)?;
    let out = text(&ctx, 1)?;
    let overwrite = ctx.args().get(2).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let title = optional_text(&ctx, 3)?;
    let inputs: Vec<String> =
        serde_json::from_str(&inputs_json).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let inputs: Vec<PathBuf> = inputs.into_iter().map(PathBuf::from).collect();
    let selections_json = optional_text(&ctx, 4)?;
    let selections: Option<Vec<String>> = match selections_json.as_deref() {
        None | Some("") => None,
        Some(json) => Some(
            serde_json::from_str(json).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?,
        ),
    };
    let report = open_pdf_core::merge_selected(
        &inputs,
        selections.as_deref(),
        std::path::Path::new(&out),
        overwrite,
        title.as_deref(),
    )
    .map_err(failure)?;
    respond(&ctx, &report)
}

/// Splits one PDF into page-range files.
#[function]
fn pdf_split(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let ranges = text(&ctx, 1)?;
    let out_dir = text(&ctx, 2)?;
    let report = open_pdf_core::split(std::path::Path::new(&path), &ranges, std::path::Path::new(&out_dir)).map_err(failure)?;
    respond(&ctx, &report)
}

/// Splits one PDF into consecutive files of at most `every` pages.
#[function]
fn pdf_split_every(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let every = argument(&ctx, 1)?.as_i64()?;
    let every = usize::try_from(every).map_err(|_| Error::new("session/argument-invalid: --every must be positive"))?;
    let out_dir = text(&ctx, 2)?;
    let report =
        open_pdf_core::split_every(std::path::Path::new(&path), every, std::path::Path::new(&out_dir)).map_err(failure)?;
    respond(&ctx, &report)
}

/// Splits one PDF into a fixed number of consecutive files.
#[function]
fn pdf_split_parts(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let parts = argument(&ctx, 1)?.as_i64()?;
    let parts = usize::try_from(parts).map_err(|_| Error::new("session/argument-invalid: `parts` must be positive"))?;
    let out_dir = text(&ctx, 2)?;
    let report =
        open_pdf_core::split_parts(std::path::Path::new(&path), parts, std::path::Path::new(&out_dir)).map_err(failure)?;
    respond(&ctx, &report)
}

/// Reorders, removes and rotates pages into a new PDF.
#[function]
fn pdf_arrange(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let out = text(&ctx, 1)?;
    let raw = text(&ctx, 2)?;
    let ops: open_pdf_core::DocumentArrange =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let report = open_pdf_core::arrange(std::path::Path::new(&path), std::path::Path::new(&out), &ops).map_err(failure)?;
    respond(&ctx, &report)
}

/// Removes the selected pages from one PDF file.
#[function]
fn pdf_delete_pages(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let pages = text(&ctx, 1)?;
    let out = text(&ctx, 2)?;
    let overwrite = ctx.args().get(3).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let report =
        open_pdf_core::delete_pages(std::path::Path::new(&path), &pages, std::path::Path::new(&out), overwrite)
            .map_err(failure)?;
    respond(&ctx, &report)
}

/// Repeats the selected pages of one PDF file directly after themselves.
#[function]
fn pdf_duplicate_pages(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let pages = text(&ctx, 1)?;
    let times = argument(&ctx, 2)?.as_i64()?;
    let times =
        u32::try_from(times).map_err(|_| Error::new("session/argument-invalid: `times` must be an integer"))?;
    let out = text(&ctx, 3)?;
    let overwrite = ctx.args().get(4).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let report =
        open_pdf_core::duplicate_pages(std::path::Path::new(&path), &pages, times, std::path::Path::new(&out), overwrite)
            .map_err(failure)?;
    respond(&ctx, &report)
}

/// Inserts every page of one PDF after the selected page of another PDF.
#[function]
fn pdf_insert_pages(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let insert = text(&ctx, 1)?;
    let after = argument(&ctx, 2)?.as_i64()?;
    let after =
        u32::try_from(after).map_err(|_| Error::new("session/argument-invalid: `after` must be a non-negative integer"))?;
    let out = text(&ctx, 3)?;
    let overwrite = ctx.args().get(4).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let selected = optional_text(&ctx, 5)?;
    let report = open_pdf_core::insert_pages(
        std::path::Path::new(&path),
        std::path::Path::new(&insert),
        selected.as_deref().filter(|ranges| !ranges.is_empty()),
        after,
        std::path::Path::new(&out),
        overwrite,
    )
    .map_err(failure)?;
    respond(&ctx, &report)
}

/// Inserts blank pages after the selected page of one PDF.
#[function]
fn pdf_insert_blank_pages(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let after = argument(&ctx, 1)?.as_i64()?;
    let after =
        u32::try_from(after).map_err(|_| Error::new("session/argument-invalid: `after` must be a non-negative integer"))?;
    let count = argument(&ctx, 2)?.as_i64()?;
    let count =
        u32::try_from(count).map_err(|_| Error::new("session/argument-invalid: `count` must be a non-negative integer"))?;
    let size = optional_text(&ctx, 3)?;
    let out = text(&ctx, 4)?;
    let overwrite = ctx.args().get(5).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let report = open_pdf_core::insert_blank_pages(
        std::path::Path::new(&path),
        after,
        count,
        size.as_deref(),
        std::path::Path::new(&out),
        overwrite,
    )
    .map_err(failure)?;
    respond(&ctx, &report)
}

/// Replaces the selected pages of one PDF with pages of another PDF.
#[function]
fn pdf_replace_pages(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let pages = text(&ctx, 1)?;
    let insert = text(&ctx, 2)?;
    let out = text(&ctx, 3)?;
    let overwrite = ctx.args().get(4).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let selected = optional_text(&ctx, 5)?;
    let report = open_pdf_core::replace_pages(
        std::path::Path::new(&path),
        &pages,
        std::path::Path::new(&insert),
        selected.as_deref(),
        std::path::Path::new(&out),
        overwrite,
    )
    .map_err(failure)?;
    respond(&ctx, &report)
}

/// Reverses the page order of one PDF into a new document.
#[function]
fn pdf_reverse_pages(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let out = text(&ctx, 1)?;
    let overwrite = ctx.args().get(2).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let report =
        open_pdf_core::reverse_pages(std::path::Path::new(&path), std::path::Path::new(&out), overwrite).map_err(failure)?;
    respond(&ctx, &report)
}

/// Adds a clockwise rotation to the selected pages of one PDF file.
#[function]
fn pdf_rotate_pages(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let pages = text(&ctx, 1)?;
    let degrees = argument(&ctx, 2)?.as_i64()?;
    let degrees =
        i32::try_from(degrees).map_err(|_| Error::new("session/argument-invalid: degrees must be 90, 180 or 270"))?;
    let out = text(&ctx, 3)?;
    let overwrite = ctx.args().get(4).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let report = open_pdf_core::rotate_pages(std::path::Path::new(&path), &pages, degrees, std::path::Path::new(&out), overwrite)
        .map_err(failure)?;
    respond(&ctx, &report)
}

/// Writes a `/CropBox` on the selected pages of one PDF file.
#[function]
fn pdf_set_page_boxes(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let pages = text(&ctx, 1)?;
    let raw = text(&ctx, 2)?;
    let crop: [f64; 4] =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let out = text(&ctx, 3)?;
    let overwrite = ctx.args().get(4).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let report =
        open_pdf_core::set_page_boxes(std::path::Path::new(&path), &pages, crop, std::path::Path::new(&out), overwrite)
            .map_err(failure)?;
    respond(&ctx, &report)
}

/// Copies every page while rewriting the information dictionary.
#[function]
fn pdf_set_metadata(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let out = text(&ctx, 1)?;
    let raw = text(&ctx, 2)?;
    let patch: open_pdf_core::MetadataPatch =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let report =
        open_pdf_core::set_metadata(std::path::Path::new(&path), std::path::Path::new(&out), &patch).map_err(failure)?;
    respond(&ctx, &report)
}

/// Copies every page while clearing the information dictionary.
#[function]
fn pdf_clear_metadata(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let out = text(&ctx, 1)?;
    let report = open_pdf_core::clear_metadata(std::path::Path::new(&path), std::path::Path::new(&out)).map_err(failure)?;
    respond(&ctx, &report)
}

/// Bookmark request payload accepted by `pdf_set_bookmarks`.
#[derive(serde::Deserialize)]
struct BookmarkRequest {
    bookmarks: Vec<open_pdf_core::Bookmark>,
}

/// Copies every page while replacing the outline tree.
#[function]
fn pdf_set_bookmarks(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let out = text(&ctx, 1)?;
    let raw = text(&ctx, 2)?;
    let request: BookmarkRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let report =
        open_pdf_core::set_bookmarks(std::path::Path::new(&path), std::path::Path::new(&out), &request.bookmarks)
            .map_err(failure)?;
    respond(&ctx, &report)
}

/// Field-value payload accepted by `pdf_fill_form`.
#[derive(serde::Deserialize)]
struct FillFormRequest {
    values: std::collections::BTreeMap<String, open_pdf_core::FormValue>,
}

/// Copies every page while filling AcroForm field values.
#[function]
fn pdf_fill_form(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let out = text(&ctx, 1)?;
    let raw = text(&ctx, 2)?;
    let request: FillFormRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let report = open_pdf_core::fill_form(std::path::Path::new(&path), std::path::Path::new(&out), &request.values)
        .map_err(failure)?;
    respond(&ctx, &report)
}

/// Creates a PDF from a JSON request.
#[function]
fn pdf_create(ctx: CallContext) -> Result<JsValue> {
    let raw = text(&ctx, 0)?;
    let request: open_pdf_core::CreateRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    respond(&ctx, &open_pdf_core::create(&request).map_err(failure)?)
}

/// Extracts text from one PDF file.
#[function]
fn pdf_extract_text(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    respond(&ctx, &open_pdf_core::extract_text(std::path::Path::new(&path)).map_err(failure)?)
}

/// Extracts text from selected one-based pages of one PDF file.
#[function]
fn pdf_extract_text_pages(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let ranges = text(&ctx, 1)?;
    respond(&ctx, &open_pdf_core::extract_text_pages(std::path::Path::new(&path), &ranges).map_err(failure)?)
}

/// Searches the extracted text of one PDF file.
#[function]
fn pdf_search(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let query = text(&ctx, 1)?;
    let limit = match ctx.args().get(2) {
        Some(value) if !value.is_undefined()? => {
            let value = value.as_i64()?;
            let value = u32::try_from(value)
                .map_err(|_| Error::new("session/argument-invalid: limit must be a positive integer"))?;
            Some(value)
        }
        _ => None,
    };
    let match_case = ctx.args().get(3).map(|value| value.as_bool()).transpose()?.unwrap_or(true);
    let whole_word = ctx.args().get(4).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let skip = match ctx.args().get(5) {
        Some(value) if !value.is_undefined()? => {
            let value = value.as_i64()?;
            u32::try_from(value).map_err(|_| Error::new("session/argument-invalid: skip must be a non-negative integer"))?
        }
        _ => 0,
    };
    respond(
        &ctx,
        &open_pdf_core::search(std::path::Path::new(&path), &query, limit, match_case, whole_word, skip)
            .map_err(failure)?,
    )
}

/// Searches the extracted text of the selected one-based pages.
#[function]
fn pdf_search_pages(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let query = text(&ctx, 1)?;
    let ranges = text(&ctx, 2)?;
    let limit = match ctx.args().get(3) {
        Some(value) if !value.is_undefined()? => {
            let value = value.as_i64()?;
            let value = u32::try_from(value)
                .map_err(|_| Error::new("session/argument-invalid: limit must be a positive integer"))?;
            Some(value)
        }
        _ => None,
    };
    let match_case = ctx.args().get(4).map(|value| value.as_bool()).transpose()?.unwrap_or(true);
    let whole_word = ctx.args().get(5).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let skip = match ctx.args().get(6) {
        Some(value) if !value.is_undefined()? => {
            let value = value.as_i64()?;
            u32::try_from(value).map_err(|_| Error::new("session/argument-invalid: skip must be a non-negative integer"))?
        }
        _ => 0,
    };
    respond(
        &ctx,
        &open_pdf_core::search_pages(
            std::path::Path::new(&path),
            &query,
            limit,
            &ranges,
            match_case,
            whole_word,
            skip,
        )
        .map_err(failure)?,
    )
}

/// Extracts images from one PDF file into a directory.
#[function]
fn pdf_extract_images(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let out_dir = text(&ctx, 1)?;
    respond(&ctx, &open_pdf_core::extract_images(std::path::Path::new(&path), std::path::Path::new(&out_dir)).map_err(failure)?)
}

/// Extracts images from selected one-based pages into a directory.
#[function]
fn pdf_extract_images_pages(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let out_dir = text(&ctx, 1)?;
    let ranges = text(&ctx, 2)?;
    respond(
        &ctx,
        &open_pdf_core::extract_images_pages(std::path::Path::new(&path), std::path::Path::new(&out_dir), &ranges)
            .map_err(failure)?,
    )
}

/// Inspects one PDF from bytes the caller already owns.
#[function]
fn pdf_inspect_bytes(ctx: CallContext) -> Result<JsValue> {
    let document = bytes(&ctx, 0)?;
    respond(&ctx, &open_pdf_core::inspect_bytes(&document).map_err(failure)?)
}

/// Extracts text from PDF bytes the caller already owns.
#[function]
fn pdf_extract_text_bytes(ctx: CallContext) -> Result<JsValue> {
    let document = bytes(&ctx, 0)?;
    respond(&ctx, &open_pdf_core::extract_text_bytes(&document).map_err(failure)?)
}

/// Extracts images from PDF bytes into a directory, naming files after `stem`.
#[function]
fn pdf_extract_images_bytes(ctx: CallContext) -> Result<JsValue> {
    let document = bytes(&ctx, 0)?;
    let out_dir = text(&ctx, 1)?;
    let stem = text(&ctx, 2)?;
    respond(
        &ctx,
        &open_pdf_core::extract_images_bytes(&document, std::path::Path::new(&out_dir), &stem).map_err(failure)?,
    )
}

/// Paints a text stamp on the selected pages of one PDF file.
#[function]
fn pdf_stamp(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let out = text(&ctx, 1)?;
    let raw = text(&ctx, 2)?;
    let request: open_pdf_core::StampRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let report =
        open_pdf_core::stamp(std::path::Path::new(&path), std::path::Path::new(&out), &request).map_err(failure)?;
    respond(&ctx, &report)
}

register_module![
    version,
    capabilities,
    pdf_inspect,
    pdf_outline,
    pdf_metadata,
    pdf_merge,
    pdf_split,
    pdf_split_every,
    pdf_split_parts,
    pdf_arrange,
    pdf_delete_pages,
    pdf_duplicate_pages,
    pdf_insert_pages,
    pdf_insert_blank_pages,
    pdf_replace_pages,
    pdf_reverse_pages,
    pdf_rotate_pages,
    pdf_set_page_boxes,
    pdf_set_metadata,
    pdf_clear_metadata,
    pdf_set_bookmarks,
    pdf_fill_form,
    pdf_create,
    pdf_extract_text,
    pdf_extract_text_pages,
    pdf_search,
    pdf_search_pages,
    pdf_extract_images,
    pdf_extract_images_pages,
    pdf_inspect_bytes,
    pdf_extract_text_bytes,
    pdf_extract_images_bytes,
    pdf_stamp
];
