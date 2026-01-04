use super::dialog::GlobalDialog;
use super::queue_sidebar::QueueSidebar;
use super::NowPlayingBar;
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
        NowPlayingBar {}
        QueueSidebar {}
        GlobalDialog {}
    }
}
