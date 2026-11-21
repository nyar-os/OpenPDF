use std::fmt;

use serde::Serialize;

/// A failure reported by the PDF engine.
///
/// The code is part of the public contract and stays stable across message
/// changes, so callers compare codes instead of parsing text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceError {
    code: String,
    message: String,
}

impl ServiceError {
    /// Builds an error from a stable code and a human-readable message.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }

    /// Returns the stable error code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the human-readable message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ServiceError {}

/// Result alias used by every fallible engine entry point.
pub type Result<T> = std::result::Result<T, ServiceError>;

pub(crate) fn argument(message: impl Into<String>) -> ServiceError {
    ServiceError::new("session/argument-invalid", message)
}

pub(crate) fn invalid(message: impl Into<String>) -> ServiceError {
    ServiceError::new("format/invalid", message)
}

pub(crate) fn unsupported(message: impl Into<String>) -> ServiceError {
    ServiceError::new("format/unsupported", message)
}

pub(crate) fn read_error(message: impl Into<String>) -> ServiceError {
    ServiceError::new("io/read", message)
}

pub(crate) fn write_error(message: impl Into<String>) -> ServiceError {
    ServiceError::new("io/write", message)
}

pub(crate) fn exists_error(message: impl Into<String>) -> ServiceError {
    ServiceError::new("io/exists", message)
}
