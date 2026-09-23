#![deny(unreachable_pub, dead_code)]

uniffi::setup_scaffolding!();

#[macro_use]
extern crate bae_mirror;

#[cfg(target_os = "android")]
mod android_tls;
mod bridge_utils;
#[cfg(feature = "cloudkit")]
mod cloudkit;
mod handle;
mod host;
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
pub use host::*;
pub use init::*;
pub use live_subscription::*;
pub use setup::*;
pub use types::*;
#[cfg(feature = "desktop")]
pub use utils::*;
