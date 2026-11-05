//! The objects an open, a setup call, a close or an apply of a catalog
//! release reports its steps through, made on the host so that the app pulls
//! their streams on the host's runtime.

use std::sync::Arc;

use super::BridgeHost;
#[cfg(desktop)]
use crate::BridgeCatalogApply;
use crate::{BridgeLibraryClosing, BridgeLibraryOpening, BridgeLibrarySetup};

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

#[uniffi::export]
impl BridgeLibraryClosing {
    /// A closing whose stream the app pulls on `host`'s runtime, which runs
    /// on after the library's own has ended.
    #[uniffi::constructor]
    pub fn new(host: Arc<BridgeHost>) -> Arc<Self> {
        crate::screens::library_closing(host.operations.clone())
    }
}

#[cfg(desktop)]
#[uniffi::export]
impl BridgeCatalogApply {
    /// An apply whose stream the app pulls on `host`'s runtime.
    #[uniffi::constructor]
    pub fn new(host: Arc<BridgeHost>) -> Arc<Self> {
        crate::screens::catalog_apply(host.operations.clone())
    }
}
