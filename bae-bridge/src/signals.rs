//! Bridge glue for the core signal-extraction pipeline: `ArtworkAnalyzerAdapter`
//! implements the core `ArtworkAnalyzer` trait on top of an
//! `ArtworkAnalyzerCallback` instance supplied by Swift (Vision-based). It
//! lives in the bridge because the callback trait is a uniffi export.

use bae_core::signals::{ArtworkAnalysis, ArtworkAnalyzer, DetectedBarcode, RecognizedLine};
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

mirror_struct! {
    crate::types::BridgeArtworkAnalysis = ArtworkAnalysis,
    into_core: fn,
    fields: {
        barcodes: (each crate::types::BridgeDetectedBarcode),
        text_lines: (each crate::types::BridgeRecognizedLine),
    },
}

/// Not a `mirror_struct`: the region is admitted, not copied — core keeps only
/// a box inside the image.
impl crate::types::BridgeDetectedBarcode {
    fn into_core(self) -> DetectedBarcode {
        let crate::types::BridgeDetectedBarcode { payload, region } = self;
        DetectedBarcode {
            payload,
            region: region.and_then(image_region_into_core),
        }
    }
}

/// Not a `mirror_struct`, for the same reason.
impl crate::types::BridgeRecognizedLine {
    fn into_core(self) -> RecognizedLine {
        let crate::types::BridgeRecognizedLine { text, region } = self;
        RecognizedLine {
            text,
            region: region.and_then(image_region_into_core),
        }
    }
}

/// A region as the platform reported it, or `None` for one it reported
/// outside the image — a crop of that would show nothing, and core admits
/// only regions it can crop to. Logged: a detector naming a box off its own
/// image is worth knowing about.
fn image_region_into_core(
    region: crate::types::BridgeImageRegion,
) -> Option<bae_core::signals::ImageRegion> {
    let crate::types::BridgeImageRegion {
        x,
        y,
        width,
        height,
    } = region;
    let inside = bae_core::signals::ImageRegion::new(x, y, width, height);
    if inside.is_none() {
        tracing::warn!("artwork analyzer reported a region outside its image: {region:?}");
    }
    inside
}
