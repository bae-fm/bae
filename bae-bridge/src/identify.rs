//! Bridge glue for the core identify pipeline: `ArtworkAnalyzerAdapter`
//! implements the core `ArtworkAnalyzer` trait on top of an
//! `ArtworkAnalyzerCallback` instance supplied by Swift (Vision-based). It
//! lives in the bridge because the callback trait is a uniffi export.

use bae_core::identify::{ArtworkAnalysis, ArtworkAnalyzer};
use std::path::Path;

use crate::types::ArtworkAnalyzerCallback;

/// Bridges a Swift-supplied analyzer callback into the core trait. The
/// callback is sync on both sides — the core service calls this on a
/// blocking task so the tokio runtime isn't parked.
pub(crate) struct ArtworkAnalyzerAdapter {
    callback: Box<dyn ArtworkAnalyzerCallback>,
}

impl ArtworkAnalyzerAdapter {
    pub(crate) fn new(callback: Box<dyn ArtworkAnalyzerCallback>) -> Self {
        Self { callback }
    }
}

impl ArtworkAnalyzer for ArtworkAnalyzerAdapter {
    fn analyze(&self, path: &Path) -> ArtworkAnalysis {
        self.callback
            .analyze(path.to_string_lossy().into_owned())
            .into_core()
    }
}

impl crate::types::BridgeArtworkAnalysis {
    fn into_core(self) -> ArtworkAnalysis {
        let crate::types::BridgeArtworkAnalysis {
            barcodes,
            text_lines,
        } = self;
        ArtworkAnalysis {
            barcodes,
            text_lines,
        }
    }
}
