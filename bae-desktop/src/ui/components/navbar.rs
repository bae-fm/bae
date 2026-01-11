//! Navbar layout wrapper for desktop app
//!
//! Wraps the shared AppLayoutView with desktop-specific components.

use super::dialog::GlobalDialog;
use super::now_playing_bar::NowPlayingBar;
use super::queue_sidebar::QueueSidebar;
use super::TitleBar;
use crate::ui::Route;
use bae_ui::AppLayoutView;
use dioxus::prelude::*;

/// Layout component that includes title bar, content, playback bar, and sidebar
#[component]
pub fn Navbar() -> Element {
    rsx! {
        AppLayoutView {
            title_bar: rsx! {
                TitleBar {}
            },
            playback_bar: rsx! {
                NowPlayingBar {}
            },
            queue_sidebar: rsx! {
                QueueSidebar {}
            },
            extra: rsx! {
                GlobalDialog {}
            },
            Outlet::<Route> {}
        }
    }
}
