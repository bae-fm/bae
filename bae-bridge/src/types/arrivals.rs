//! Files still arriving from another device, and the pictures waiting on them.

/// A file another device attached but hasn't uploaded yet, opaque to the UI,
/// which hands it back to `wait_for_arrival`. Wraps
/// `bae_core::library::ArrivingFile`.
#[derive(Debug, uniffi::Object)]
pub struct BridgeArrivingFile(pub(crate) bae_core::library::ArrivingFile);

/// An image to show: its bytes, or its file still arriving from another
/// device, which the UI waits for with `wait_for_arrival` and then reads
/// again. Arriving is no failure: the UI shows its placeholder and logs
/// nothing above debug. Mirrors `bae_core::library::ImageRead`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeImageRead {
    Bytes {
        bytes: Vec<u8>,
    },
    Arriving {
        file: std::sync::Arc<BridgeArrivingFile>,
    },
}

impl BridgeImageRead {
    pub(crate) fn from_core(read: bae_core::library::ImageRead) -> Self {
        match read {
            bae_core::library::ImageRead::Bytes(bytes) => Self::Bytes { bytes },
            bae_core::library::ImageRead::Arriving(file) => Self::Arriving {
                file: std::sync::Arc::new(BridgeArrivingFile(file)),
            },
        }
    }
}
