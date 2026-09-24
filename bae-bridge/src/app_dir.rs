//! bae's own directory, as the host names it.

use std::sync::Arc;

/// bae's own directory, built from the home directory the host passes in. The
/// host builds it first at process start and hands it to `BridgeHost`; nothing
/// reads `$HOME`.
#[derive(uniffi::Object)]
pub struct BridgeAppDir {
    inner: bae_core::config::AppDir,
}

impl BridgeAppDir {
    pub(crate) fn core(&self) -> &bae_core::config::AppDir {
        &self.inner
    }
}

#[uniffi::export]
impl BridgeAppDir {
    /// bae's directory under `home`: the platform's home directory on the
    /// desktop, the app's private data directory on mobile.
    #[uniffi::constructor]
    pub fn new(home: String) -> Arc<Self> {
        Arc::new(Self {
            inner: bae_core::config::AppDir::under_home(std::path::Path::new(&home)),
        })
    }
}
