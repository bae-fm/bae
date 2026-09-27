//! The platform's artwork analyzer: one sync decode per image yields its
//! barcodes and text. A platform without one registers nothing, and artwork is
//! then no signal source at all rather than an empty read.

use std::path::Path;

/// What one pass over an image surfaces, from a single decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkAnalysis {
    /// The payloads of the barcodes the detector found.
    pub barcodes: Vec<String>,
    /// One per visual line, in the recognizer's order.
    pub text_lines: Vec<String>,
}

impl ArtworkAnalysis {
    /// An image nothing was read off.
    pub fn empty() -> Self {
        Self {
            barcodes: Vec::new(),
            text_lines: Vec::new(),
        }
    }
}

pub trait ArtworkAnalyzer: Send + Sync {
    /// Read the image at `path`; empty on failure.
    fn analyze(&self, path: &Path) -> ArtworkAnalysis;
}
