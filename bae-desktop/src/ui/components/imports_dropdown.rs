//! Imports dropdown wrapper
//!
//! Thin wrapper that bridges ActiveImportsState context to ImportsDropdownView.

use super::active_imports_context::use_active_imports;
use crate::ui::{image_url, AppContext, Route};
use bae_core::db::ImportOperationStatus;
use bae_ui::display_types::{ActiveImport as DisplayActiveImport, ImportStatus};
use bae_ui::ImportsDropdownView;
use dioxus::prelude::*;

/// Dropdown showing list of active imports with progress
#[component]
pub fn ImportsDropdown(mut is_open: Signal<bool>) -> Element {
    let active_imports = use_active_imports();
    let imports = active_imports.imports.read();
    let navigator = use_navigator();
    let app_context = use_context::<AppContext>();

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

    // Build a map from import_id to release_id for navigation
    let release_ids: std::collections::HashMap<String, Option<String>> = imports
        .iter()
        .map(|i| (i.import_id.clone(), i.release_id.clone()))
        .collect();

    rsx! {
        ImportsDropdownView {
            imports: display_imports,
            is_open: *is_open.read(),
            on_close: move |_| is_open.set(false),
            on_import_click: {
                let release_ids = release_ids.clone();
                move |import_id: String| {
                    is_open.set(false);
                    if let Some(Some(rid)) = release_ids.get(&import_id) {
                        navigator
                            .push(Route::AlbumDetail {
                                album_id: rid.clone(),
                                release_id: String::new(),
                            });
                    }
                }
            },
            on_import_dismiss: {
                let library_manager = app_context.library_manager.clone();
                move |import_id: String| {
                    active_imports.dismiss(&import_id);

                    // Also delete from DB so it doesn't reappear after restart
                    let library_manager = library_manager.clone();
                    spawn(async move {
                        if let Err(e) = library_manager.get().delete_import(&import_id).await {
                            tracing::warn!("Failed to delete import from DB: {}", e);
                        }
                    });
                }
            },
            on_clear_all: {
                let library_manager = app_context.library_manager.clone();
                let mut imports_signal = active_imports.imports;
                move |_| {
                    // Collect IDs before clearing the list
                    let import_ids: Vec<String> = imports_signal
                        .read()
                        .iter()
                        .map(|i| i.import_id.clone())
                        .collect();
                    imports_signal.with_mut(|list| list.clear());

                    // Delete all from DB
                    let library_manager = library_manager.clone();
                    spawn(async move {
                        for id in import_ids {
                            if let Err(e) = library_manager.get().delete_import(&id).await {
                                tracing::warn!("Failed to delete import {} from DB: {}", id, e);
                            }
                        }
                    });
                }
            },
        }
    }
}
