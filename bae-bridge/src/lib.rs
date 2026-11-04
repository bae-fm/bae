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
// Only a phone calls into its host: its audio focus and sign-in presenter.
#[cfg(any(test, not(desktop)))]
mod host_calls;
mod init;
mod keyed_values;
mod oauth;
mod operation_runtime;
mod path_bytes;
mod screens;
mod setup;
#[cfg(feature = "desktop")]
mod signals;
mod types;

pub use app_dir::*;
#[cfg(any(test, not(desktop)))]
pub use audio_focus::*;
#[cfg(all(feature = "dev-reset", unix))]
pub use dev_reset::*;
pub use handle::*;
pub use host::*;
pub use init::*;
pub use keyed_values::*;
pub use oauth::*;
pub use screens::*;
pub use setup::*;
pub use types::*;
