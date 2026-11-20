uniffi::setup_scaffolding!();

#[macro_use]
extern crate bae_mirror;

#[cfg(feature = "cast")]
mod airplay_prompt;
mod app_dir;
#[cfg(any(test, not(desktop)))]
mod audio_focus;
mod bridge_utils;
mod cancellation;
#[cfg(all(feature = "dev-reset", unix))]
mod dev_reset;
mod from_app;
mod handle;
mod host;
// A phone calls into its host for its audio focus and sign-in presenter, every
// casting host for the code an AirPlay receiver asks for, and every host for
// the device's time zone.
mod host_calls;
mod init;
mod keyed_values;
mod local_time_zone;
mod oauth;
mod operation_runtime;
mod screens;
mod setup;
#[cfg(desktop)]
mod signals;
mod types;

#[cfg(feature = "cast")]
pub use airplay_prompt::*;
pub use app_dir::*;
#[cfg(any(test, not(desktop)))]
pub use audio_focus::*;
pub use cancellation::BridgeCancellation;
#[cfg(all(feature = "dev-reset", unix))]
pub use dev_reset::*;
pub use handle::*;
pub use host::*;
pub use init::*;
pub use keyed_values::*;
pub use local_time_zone::*;
pub use oauth::*;
pub use screens::*;
pub use setup::*;
pub use types::*;
