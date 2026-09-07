//! Platform-provided artwork analyzer. One `analyze` pass over an image yields both
//! the barcode payloads and the recognized text lines, so extraction visits — and
//! decodes — each image exactly once.
//!
//! Core defines the trait; a platform that has an analyzer registers one at app
//! boot. A platform that has none registers nothing, and extraction then treats
//! artwork as no signal source at all — there is no stand-in that reports an
//! empty decode, because "decoded and found nothing" and "never decoded" are
//! different answers and the identify pipeline acts on the difference.
//!
//! The call is sync because Apple's Vision `VNImageRequestHandler.perform` is: its
//! completion handlers fire before `perform` returns, so a sync signature is the
//! honest one. The extraction service calls it from `spawn_blocking` to keep the
//! async runtime off the FFI thread.

use super::ImageRegion;
use std::path::Path;

/// One barcode the detector found, and where on the image it found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedBarcode {
    pub payload: String,
    /// Where the code sits on the image. `None` from a detector that reports
    /// payloads alone.
    pub region: Option<ImageRegion>,
}

/// One visual line the recognizer read, and where on the image it read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecognizedLine {
    pub text: String,
    /// Where the line sits on the image. `None` from a recognizer that reports
    /// text alone.
    pub region: Option<ImageRegion>,
}

/// What one pass over an image surfaces, from a single decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkAnalysis {
    pub barcodes: Vec<DetectedBarcode>,
    /// One per visual line, in whatever order the recognizer emits them.
    pub text_lines: Vec<RecognizedLine>,
}

impl ArtworkAnalysis {
    /// An image nothing was read off: a decode that failed, or one with
    /// nothing on it.
    pub fn empty() -> Self {
        Self {
            barcodes: Vec::new(),
            text_lines: Vec::new(),
        }
    }

    /// An analysis of text alone, each line without a place on the image —
    /// what a recognizer that reports no positions produces.
    pub fn of_text(lines: Vec<String>) -> Self {
        Self {
            barcodes: Vec::new(),
            text_lines: lines
                .into_iter()
                .map(|text| RecognizedLine { text, region: None })
                .collect(),
        }
    }
}

pub trait ArtworkAnalyzer: Send + Sync {
    /// Detect barcodes and recognize text in one decode of the image at `path`.
    /// Comes back empty on failure, as when there's nothing to find.
    fn analyze(&self, path: &Path) -> ArtworkAnalysis;
}
