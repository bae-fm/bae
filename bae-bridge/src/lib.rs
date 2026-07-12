uniffi::setup_scaffolding!();

mod bridge_utils;
#[cfg(feature = "cloudkit")]
mod cloudkit;
mod handle;
mod init;
mod setup;
#[cfg(feature = "desktop")]
mod signals;
mod types;
mod utils;

pub use handle::*;
pub use types::*;

#[cfg(feature = "cloudkit")]
pub(crate) use cloudkit::get_cloudkit_ops;

/// No CloudKit driver can be registered without the `cloudkit` feature, so
/// there are never any CloudKit ops.
#[cfg(not(feature = "cloudkit"))]
pub(crate) fn get_cloudkit_ops() -> Option<std::sync::Arc<dyn coven::CloudKitOps>> {
    None
}
