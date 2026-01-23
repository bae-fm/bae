//! Navbar layout wrapper for desktop app
//!
//! Wraps the shared AppLayoutView with desktop-specific components.

use super::now_playing_bar::NowPlayingBar;
use super::queue_sidebar::QueueSidebar;
use super::TitleBar;
use crate::ui::shortcuts::{handle_shortcut, subscribe_nav, NavAction};
use crate::ui::Route;
use bae_ui::AppLayoutView;
use dioxus::prelude::*;

fn execute_nav_action(action: NavAction) {
    match action {
        NavAction::Back => navigator().go_back(),
        NavAction::Forward => navigator().go_forward(),
        NavAction::GoTo(target) => {
            let _ = navigator().push(target.to_route());
        }
    }
}

/// Layout component that includes title bar, content, playback bar, and sidebar
#[component]
pub fn Navbar() -> Element {
    // Listen for menu-triggered navigation (subscribes fresh on each mount)
    use_hook(|| {
        let mut rx = subscribe_nav();
        spawn(async move {
            while let Ok(action) = rx.recv().await {
                execute_nav_action(action);
            }
        });
    });

    rsx! {
        div {
            class: "contents",
            tabindex: 0,
            onkeydown: move |evt| {
                if let Some(action) = handle_shortcut(&evt) {
                    evt.prevent_default();
                    execute_nav_action(action);
                }
            },
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
                Outlet::<Route> {}
            }
        }
    }
}
