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

/// An image the platform could not read at all, as opposed to one it read and
/// found nothing printed on. The pass over the candidate's artwork stops at
/// it and says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtworkUnreadable {
    /// Why the image could not be read.
    pub detail: String,
}

impl coven::task::CarriesBug for ArtworkUnreadable {
    /// An unreadable image is the image's fault, not a bug.
    fn bug(&self) -> Option<&coven::task::Bug> {
        None
    }
}

/// Called on blocking threads, several images at once.
pub trait ArtworkAnalyzer: Send + Sync {
    /// Read the image at `path`. An image with nothing on it reads as
    /// [`ArtworkAnalysis::empty`]; one that can't be decoded or prepared is
    /// [`ArtworkUnreadable`].
    fn analyze(&self, path: &Path) -> Result<ArtworkAnalysis, ArtworkUnreadable>;
}

#[cfg(test)]
#[path = "analyzer_tests.rs"]
mod tests;
