#[cfg(target_os = "macos")]
mod macos_window;
#[cfg(target_os = "macos")]
pub use macos_window::{
    set_playback_repeat_mode, setup_app_menu, setup_macos_window_activation,
    setup_transparent_titlebar,
};
#[cfg(not(target_os = "macos"))]
pub fn setup_macos_window_activation() {}
#[cfg(not(target_os = "macos"))]
pub fn setup_transparent_titlebar() {}
#[cfg(not(target_os = "macos"))]
pub fn setup_app_menu() {}
#[cfg(not(target_os = "macos"))]
pub fn set_playback_repeat_mode(_mode: bae_core::playback::RepeatMode) {}
