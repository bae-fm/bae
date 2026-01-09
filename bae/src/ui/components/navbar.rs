use super::dialog::GlobalDialog;
#[cfg(not(feature = "demo"))]
use super::now_playing_bar::NowPlayingBar;
#[cfg(not(feature = "demo"))]
use super::queue_sidebar::QueueSidebar;
#[cfg(target_os = "macos")]
use super::TitleBar;
use crate::ui::Route;
use dioxus::prelude::*;

/// Layout component that includes title bar and content
#[component]
pub fn Navbar() -> Element {
    rsx! {
        {
            #[cfg(target_os = "macos")]
            {
                rsx! {
                    TitleBar {}
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                rsx! {}
            }
        }
        Outlet::<Route> {}
        {
            #[cfg(not(feature = "demo"))]
            {
                rsx! {
                    NowPlayingBar {}
                    QueueSidebar {}
                }
            }
        }
        GlobalDialog {}
    }
}
