//! The core `ArtworkAnalyzer` on top of the platform's analyzer callback, which
//! lives here because the callback trait is a uniffi export.

use bae_import::extraction::{ArtworkAnalysis, ArtworkAnalyzer, ArtworkUnreadable};
use std::path::Path;

use crate::types::{ArtworkAnalyzerCallback, BridgeArtworkUnreadable};

/// A platform analyzer callback as the core trait. Sync on both sides; core
/// calls it on blocking tasks, several images at once.
pub(crate) struct ArtworkAnalyzerAdapter {
    callback: Box<dyn ArtworkAnalyzerCallback>,
}

impl ArtworkAnalyzerAdapter {
    pub(crate) fn new(callback: Box<dyn ArtworkAnalyzerCallback>) -> Self {
        Self { callback }
    }
}

impl ArtworkAnalyzer for ArtworkAnalyzerAdapter {
    fn analyze(&self, path: &Path) -> Result<ArtworkAnalysis, ArtworkUnreadable> {
        match self.callback.analyze(coven::path_bytes(path)) {
            Ok(analysis) => Ok(analysis.into_core()),
            Err(BridgeArtworkUnreadable::Unreadable { detail }) => {
                Err(ArtworkUnreadable { detail })
            }
        }
    }
}

mirror_struct! {
    crate::types::BridgeArtworkAnalysis = ArtworkAnalysis,
    into_core: fn,
    fields: { barcodes, text_lines },
}
