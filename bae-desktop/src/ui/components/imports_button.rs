//! Imports button wrapper
//!
//! Thin wrapper that bridges ActiveImportsState context to ImportsButtonView.

use super::active_imports_context::use_active_imports;
use crate::ui::image_url;
use bae_core::db::ImportOperationStatus;
use bae_ui::display_types::{ActiveImport as DisplayActiveImport, ImportStatus};
use bae_ui::ImportsButtonView;
use dioxus::prelude::*;

/// Button in title bar that shows active imports count and toggles dropdown
#[component]
pub fn ImportsButton(mut is_open: Signal<bool>) -> Element {
    let active_imports = use_active_imports();
    let imports = active_imports.imports.read();

    // Convert to display types
    let display_imports: Vec<DisplayActiveImport> = imports
        .iter()
        .map(|i| {
            let cover_url = i
                .cover_image_id
                .as_ref()
                .map(|id| image_url(id))
                .or_else(|| i.cover_art_url.clone());

            DisplayActiveImport {
                import_id: i.import_id.clone(),
                album_title: i.album_title.clone(),
                artist_name: i.artist_name.clone(),
                status: match i.status {
                    ImportOperationStatus::Preparing => ImportStatus::Preparing,
                    ImportOperationStatus::Importing => ImportStatus::Importing,
                    ImportOperationStatus::Complete => ImportStatus::Complete,
                    ImportOperationStatus::Failed => ImportStatus::Failed,
                },
                current_step_text: i.current_step.map(|s| s.display_text().to_string()),
                progress_percent: i.progress_percent,
                release_id: i.release_id.clone(),
                cover_url,
            }
        })
        .collect();

    rsx! {
        ImportsButtonView {
            imports: display_imports,
            is_open: *is_open.read(),
            on_toggle: move |_| {
                let current = *is_open.read();
                is_open.set(!current);
            },
        }
    }
}
