uniffi::setup_scaffolding!();

#[macro_use]
extern crate bae_mirror;

#[cfg(target_os = "android")]
mod android_tls;
mod app_dir;
#[cfg(any(test, not(desktop)))]
mod audio_focus;
mod bridge_utils;
#[cfg(all(feature = "dev-reset", unix))]
mod dev_reset;
mod handle;
mod host;
mod init;
mod keyed_values;
mod live_subscription;
mod oauth;
mod operation_runtime;
mod setup;
#[cfg(feature = "desktop")]
mod signals;
mod types;
#[cfg(feature = "desktop")]
mod utils;

pub use app_dir::*;
#[cfg(any(test, not(desktop)))]
pub use audio_focus::*;
#[cfg(all(feature = "dev-reset", unix))]
pub use dev_reset::*;
pub use handle::*;
pub use host::*;
pub use init::*;
pub use keyed_values::*;
pub use live_subscription::*;
pub use oauth::*;
pub use setup::*;
pub use types::*;
#[cfg(feature = "desktop")]
pub use utils::*;
