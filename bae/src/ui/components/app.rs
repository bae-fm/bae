use super::active_imports_context::ActiveImportsProvider;
use super::dialog_context::DialogContext;
use super::library_search_context::LibrarySearchContextProvider;
use super::playback_hooks::PlaybackStateProvider;
use super::queue_sidebar::QueueSidebarState;
use crate::ui::import_context::ImportContextProvider;
#[cfg(target_os = "macos")]
use crate::ui::window_activation::setup_transparent_titlebar;
use crate::ui::{Route, FAVICON, MAIN_CSS, TAILWIND_CSS};
use dioxus::prelude::*;
use tracing::debug;
#[component]
pub fn App() -> Element {
    debug!("Rendering app component");
    use_context_provider(|| QueueSidebarState {
        is_open: Signal::new(false),
    });
    use_context_provider(DialogContext::new);
    #[cfg(target_os = "macos")]
    use_effect(move || {
        setup_transparent_titlebar();
    });
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        PlaybackStateProvider {
            ActiveImportsProvider {
                ImportContextProvider {
                    LibrarySearchContextProvider {
                        div { class: if cfg!(target_os = "macos") { "pb-24 pt-10 h-screen overflow-y-auto" } else { "pb-24 h-screen overflow-y-auto" },
                            Router::<Route> {}
                        }
                    }
                }
            }
        }
    }
}
