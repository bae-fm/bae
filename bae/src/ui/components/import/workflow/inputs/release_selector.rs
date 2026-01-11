//! Release selector wrapper - handles context state, delegates UI to ReleaseSelectorView

use crate::ui::import_context::ImportContext;
use bae_ui::components::import::ReleaseSelectorView;
use bae_ui::display_types::DetectedRelease;
use dioxus::prelude::*;
use std::rc::Rc;
use tracing::warn;

#[component]
pub fn ReleaseSelector() -> Element {
    let import_context = use_context::<Rc<ImportContext>>();
    let detected_releases = import_context.detected_releases();
    let mut selected_indices = use_signal(Vec::<usize>::new);

    // Convert to display type
    let display_releases: Vec<DetectedRelease> = detected_releases
        .read()
        .iter()
        .map(|r| DetectedRelease {
            name: r.name.clone(),
            path: r.path.display().to_string(),
        })
        .collect();

    let on_selection_change = move |indices: Vec<usize>| {
        selected_indices.set(indices);
    };

    let on_import = {
        let import_context = import_context.clone();
        move |indices: Vec<usize>| {
            if indices.is_empty() {
                return;
            }
            let import_context = import_context.clone();
            spawn(async move {
                import_context.set_selected_release_indices(indices.clone());
                import_context.set_current_release_index(0);
                if let Err(e) = import_context.load_selected_release(indices[0]).await {
                    warn!("Failed to load selected release: {}", e);
                    import_context.set_import_error_message(Some(e));
                }
            });
        }
    };

    rsx! {
        ReleaseSelectorView {
            releases: display_releases,
            selected_indices: selected_indices.read().clone(),
            on_selection_change,
            on_import,
        }
    }
}
