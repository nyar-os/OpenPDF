//! Pure-Rust PDF engine for OpenPDF.
//!
//! The crate implements a classic PDF 1.7 reader: it parses the object table,
//! page tree, metadata and outlines. Malformed input becomes a [`ServiceError`];
//! no entry point panics.

#![deny(missing_docs)]

mod error;
mod object;
mod parse;

pub use crate::error::{Result, ServiceError};
