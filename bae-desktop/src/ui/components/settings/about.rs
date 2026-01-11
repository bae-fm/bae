//! About section wrapper - handles library stats, delegates UI to AboutSectionView

use crate::ui::use_library_manager;
use crate::updater;
use bae_ui::AboutSectionView;
use dioxus::prelude::*;

const VERSION: &str = env!("BAE_VERSION");

/// About section - version info and library stats
#[component]
pub fn AboutSection() -> Element {
    let library_manager = use_library_manager();
    let mut album_count = use_signal(|| 0usize);

    let lm = library_manager.clone();
    use_effect(move || {
        let lm = lm.clone();
        spawn(async move {
            if let Ok(albums) = lm.get_albums().await {
                album_count.set(albums.len());
            }
        });
    });

    rsx! {
        AboutSectionView {
            version: VERSION.to_string(),
            album_count: *album_count.read(),
            on_check_updates: move |_| {
                updater::check_for_updates();
            },
        }
    }
}
