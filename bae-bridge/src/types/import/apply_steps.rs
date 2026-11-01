use crate::types::{BridgeCatalog, BridgeMessage, BridgeMessageArg};

/// A network read an apply of a catalog release waits on: an import pick or
/// the release editor's. Mirrors `bae_core::import::CatalogApplyStep`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCatalogApplyStep {
    /// Reading a release's documents from its catalog.
    ReadingRelease { catalog: BridgeCatalog },
    /// Fetching the cover the release is read with from the catalog that
    /// offers it.
    FetchingCover { catalog: BridgeCatalog },
}

impl BridgeCatalogApplyStep {
    pub(crate) fn from_core(step: bae_core::import::CatalogApplyStep) -> Self {
        use bae_core::import::CatalogApplyStep as S;
        match step {
            S::ReadingRelease(catalog) => Self::ReadingRelease {
                catalog: BridgeCatalog::from_core(catalog),
            },
            S::FetchingCover(catalog) => Self::FetchingCover {
                catalog: BridgeCatalog::from_core(catalog),
            },
        }
    }
}

/// The line an apply shows while it waits on `step`, naming the catalog.
#[uniffi::export]
pub fn bridge_catalog_apply_step_line(step: BridgeCatalogApplyStep) -> BridgeMessage {
    let (key, catalog) = match step {
        BridgeCatalogApplyStep::ReadingRelease { catalog } => {
            ("core.import.apply_step.reading_release", catalog)
        }
        BridgeCatalogApplyStep::FetchingCover { catalog } => {
            ("core.import.apply_step.fetching_cover", catalog)
        }
    };
    BridgeMessage {
        key: key.to_string(),
        args: vec![BridgeMessageArg::new("catalog", catalog.name().to_string())],
    }
}
