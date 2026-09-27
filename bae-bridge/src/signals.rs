//! The core `ArtworkAnalyzer` on top of the platform's analyzer callback, which
//! lives here because the callback trait is a uniffi export.

use bae_core::signals::{ArtworkAnalysis, ArtworkAnalyzer};
use std::path::Path;

use crate::types::ArtworkAnalyzerCallback;

/// A platform analyzer callback as the core trait. Sync on both sides; core
/// calls it on a blocking task.
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

mirror_struct! {
    crate::types::BridgeArtworkAnalysis = ArtworkAnalysis,
    into_core: fn,
    fields: { barcodes, text_lines },
}
