//! The platform's artwork analyzer: one sync decode per image yields its
//! barcodes and text. A platform without one registers nothing, and artwork is
//! then no signal source at all rather than an empty read.

use bae_types::barcode::Barcode;
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
        Self { barcodes: Vec::new(), text_lines: Vec::new() }
    }

    /// The codes on the image: detector payloads first, then lines that are
    /// the digits printed under bars — which catch codes whose bars did not
    /// decode. What counts as a code is [`bae_types::barcode`]'s to say.
    pub(crate) fn codes(&self) -> impl Iterator<Item = Barcode> + '_ {
        let detected = self.barcodes.iter().filter_map(|payload| Barcode::stated(payload));
        let printed = self.text_lines.iter().filter_map(|line| Barcode::printed(line));
        detected.chain(printed)
    }
}

/// Called on blocking threads, several images at once.
pub trait ArtworkAnalyzer: Send + Sync {
    /// Read the image at `path`; empty on failure.
    fn analyze(&self, path: &Path) -> ArtworkAnalysis;
}

#[cfg(test)]
#[path = "analyzer_tests.rs"]
mod tests;
