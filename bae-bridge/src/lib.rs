#![deny(unreachable_pub, dead_code)]

uniffi::setup_scaffolding!();

mod bridge_utils;
#[cfg(feature = "cloudkit")]
mod cloudkit;
mod handle;
mod init;
mod live_subscription;
mod operation_runtime;
mod setup;
#[cfg(feature = "desktop")]
mod signals;
mod types;
#[cfg(feature = "desktop")]
mod utils;

#[cfg(feature = "cloudkit")]
pub use cloudkit::*;
pub use handle::*;
pub use init::*;
pub use live_subscription::*;
pub use setup::*;
pub use types::*;
#[cfg(feature = "desktop")]
pub use utils::*;

#[cfg(feature = "cloudkit")]
pub(crate) use cloudkit::get_cloudkit_ops;

/// No CloudKit driver can be registered without the `cloudkit` feature, so
/// there are never any CloudKit ops.
#[cfg(not(feature = "cloudkit"))]
pub(crate) fn get_cloudkit_ops() -> Option<std::sync::Arc<dyn coven::CloudKitOps>> {
    None
}
