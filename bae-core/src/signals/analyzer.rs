//! Platform-provided artwork analyzer. A single `analyze` pass over one image
//! yields both the barcode payloads and the recognized text lines, so the
//! signal-extraction pass visits — and decodes — each image exactly once.
//!
//! Core defines the trait; the bridge registers a concrete implementation at
//! app boot. Analyzer calls are sync by design — Apple's Vision
//! `VNImageRequestHandler.perform` is synchronous and the completion handlers
//! fire before `perform` returns, so the honest signature is sync. The
//! extraction service calls this from `tokio::task::spawn_blocking` to keep the
//! async runtime off the FFI thread.

use std::path::Path;

/// Everything one Vision pass over an image surfaces: barcode payloads and
/// recognized text lines. Both come from a single image decode.
#[derive(Debug, Clone)]
pub struct ArtworkAnalysis {
    /// Barcode payloads visible in the image. Empty when none are present.
    pub barcodes: Vec<String>,
    /// Recognized text lines, one per visual line in whatever order the
    /// recognizer emits. Empty when no text is present.
    pub text_lines: Vec<String>,
}

/// Analyzes an artwork image in one pass. Implementations live in platform
/// wrappers (macOS Vision, etc.).
pub trait ArtworkAnalyzer: Send + Sync {
    /// Detect barcodes and recognize text in the image at `path` in a single
    /// decode. Returns empty payloads/lines on failure or when absent.
    fn analyze(&self, path: &Path) -> ArtworkAnalysis;
}

/// No-op fallback. Registered when no platform analyzer is available so the
/// extraction service can return empty results without a null check
/// everywhere.
pub struct NoopAnalyzer;

impl ArtworkAnalyzer for NoopAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        ArtworkAnalysis {
            barcodes: Vec::new(),
            text_lines: Vec::new(),
        }
    }
}
