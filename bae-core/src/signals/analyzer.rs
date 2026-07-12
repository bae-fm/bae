//! Platform-provided artwork analyzer. One `analyze` pass over an image yields both
//! the barcode payloads and the recognized text lines, so extraction visits — and
//! decodes — each image exactly once.
//!
//! Core defines the trait; the bridge registers an implementation at app boot. The
//! call is sync because Apple's Vision `VNImageRequestHandler.perform` is: its
//! completion handlers fire before `perform` returns, so a sync signature is the
//! honest one. The extraction service calls it from `spawn_blocking` to keep the
//! async runtime off the FFI thread.

use std::path::Path;

/// What one pass over an image surfaces, from a single decode.
#[derive(Debug, Clone)]
pub struct ArtworkAnalysis {
    pub barcodes: Vec<String>,
    /// One per visual line, in whatever order the recognizer emits them.
    pub text_lines: Vec<String>,
}

pub trait ArtworkAnalyzer: Send + Sync {
    /// Detect barcodes and recognize text in one decode of the image at `path`.
    /// Comes back empty on failure, as when there's nothing to find.
    fn analyze(&self, path: &Path) -> ArtworkAnalysis;
}

/// Registered when no platform analyzer is available, so the extraction service gets
/// empty results instead of needing a null check at every call.
pub(crate) struct NoopAnalyzer;

impl ArtworkAnalyzer for NoopAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        ArtworkAnalysis {
            barcodes: Vec::new(),
            text_lines: Vec::new(),
        }
    }
}
