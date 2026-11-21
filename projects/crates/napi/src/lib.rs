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
    let report = open_pdf_core::merge(&inputs, std::path::Path::new(&out), overwrite, title.as_deref()).map_err(failure)?;
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

/// Extracts images from one PDF file into a directory.
#[function]
fn pdf_extract_images(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let out_dir = text(&ctx, 1)?;
    respond(&ctx, &open_pdf_core::extract_images(std::path::Path::new(&path), std::path::Path::new(&out_dir)).map_err(failure)?)
}

register_module![
    version,
    capabilities,
    pdf_inspect,
    pdf_merge,
    pdf_split,
    pdf_split_every,
    pdf_arrange,
    pdf_set_metadata,
    pdf_create,
    pdf_extract_text,
    pdf_extract_images
];
