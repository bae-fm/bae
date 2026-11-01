//! The objects an open or a setup call reports its steps through, made on
//! the host so that the app pulls their streams on the host's runtime.

use std::sync::Arc;

use super::BridgeHost;
use crate::{BridgeLibraryOpening, BridgeLibrarySetup};

#[uniffi::export]
impl BridgeLibraryOpening {
    /// An opening whose stream the app pulls on `host`'s runtime.
    #[uniffi::constructor]
    pub fn new(host: Arc<BridgeHost>) -> Arc<Self> {
        crate::screens::library_opening(host.operations.clone())
    }
}

#[uniffi::export]
impl BridgeLibrarySetup {
    /// A setup whose stream the app pulls on `host`'s runtime.
    #[uniffi::constructor]
    pub fn new(host: Arc<BridgeHost>) -> Arc<Self> {
        crate::screens::library_setup(host.operations.clone())
    }
}
