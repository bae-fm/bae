//! Folder selector wrapper - handles drag/drop and file dialog, delegates UI to FolderSelectorView

use bae_ui::FolderSelectorView;
use dioxus::prelude::*;
use rfd::AsyncFileDialog;
use std::path::Path;

#[component]
pub fn FolderSelector(on_select: EventHandler<String>, on_error: EventHandler<String>) -> Element {
    let mut is_dragging = use_signal(|| false);
    let mut drag_counter = use_signal(|| 0i32);

    let on_drag_enter = move |evt: dioxus::html::DragEvent| {
        evt.prevent_default();
        let current = *drag_counter.read();
        drag_counter.set(current + 1);
        is_dragging.set(true);
    };

    let on_drag_over = move |evt: dioxus::html::DragEvent| {
        evt.prevent_default();
    };

    let on_drag_leave = move |evt: dioxus::html::DragEvent| {
        evt.prevent_default();
        let current: i32 = *drag_counter.read();
        drag_counter.set(current.saturating_sub(1));
        if *drag_counter.read() == 0 {
            is_dragging.set(false);
        }
    };

    let on_drop = {
        let mut is_dragging = is_dragging;
        let mut drag_counter = drag_counter;
        move |evt: dioxus::html::DragEvent| {
            tracing::info!("Drop event triggered!");
            evt.prevent_default();
            is_dragging.set(false);
            drag_counter.set(0);

            let data = evt.data();
            tracing::info!("Got drag data");

            use dioxus::html::HasFileData;
            let files = data.files();
            tracing::info!("Files count: {}", files.len());

            if files.is_empty() {
                tracing::warn!("No files in drop event");
                on_error.call("No files or folders were dropped.".to_string());
                return;
            }

            let first_file = &files[0];
            let first_path = first_file.path();
            tracing::info!("First file path: {}", first_path.display());

            let path = Path::new(&first_path);
            let resolved_path = if path.is_absolute() {
                tracing::info!("Path is absolute: {}", path.display());
                path.to_path_buf()
            } else {
                tracing::info!("Path is relative, trying to resolve: {}", path.display());
                std::env::current_dir()
                    .ok()
                    .and_then(|cwd| {
                        tracing::info!("Current dir: {}", cwd.display());
                        cwd.join(path).canonicalize().ok()
                    })
                    .or_else(|| {
                        tracing::warn!("Failed to resolve relative path");
                        None
                    })
                    .unwrap_or_else(|| {
                        tracing::warn!("Using original path as fallback");
                        path.to_path_buf()
                    })
            };

            tracing::info!(
                "Resolved path: {}, exists: {}, is_dir: {}",
                resolved_path.display(),
                resolved_path.exists(),
                resolved_path.is_dir()
            );

            if resolved_path.exists() && resolved_path.is_dir() {
                tracing::info!("Calling on_select with: {}", resolved_path.display());
                on_select.call(resolved_path.to_string_lossy().to_string());
            } else if resolved_path.exists() && resolved_path.is_file() {
                tracing::info!("Dropped item is a file, using parent directory");
                if let Some(parent) = resolved_path.parent() {
                    if parent.is_dir() {
                        tracing::info!("Calling on_select with parent: {}", parent.display());
                        on_select.call(parent.to_string_lossy().to_string());
                    } else {
                        tracing::warn!("Parent is not a directory");
                        on_error.call("Please drop a folder, not individual files.".to_string());
                    }
                } else {
                    tracing::warn!("File has no parent directory");
                    on_error.call("Please drop a folder containing your music files.".to_string());
                }
            } else {
                tracing::error!(
                    "Could not resolve path: {} (exists: {}, is_dir: {})",
                    resolved_path.display(),
                    resolved_path.exists(),
                    resolved_path.is_dir()
                );
                on_error.call(
                    "Could not resolve folder path from drag-and-drop. Please use the 'Select Folder' button below."
                        .to_string(),
                );
            }
        }
    };

    let on_button_click = move |_| {
        spawn(async move {
            if let Some(folder_handle) = AsyncFileDialog::new()
                .set_title("Select Music Folder")
                .pick_folder()
                .await
            {
                let folder_path = folder_handle.path().to_string_lossy().to_string();
                on_select.call(folder_path);
            }
        });
    };

    // Wrap the view in a drop zone
    rsx! {
        div {
            ondragenter: on_drag_enter,
            ondragover: on_drag_over,
            ondragleave: on_drag_leave,
            ondrop: on_drop,
            FolderSelectorView {
                is_dragging: *is_dragging.read(),
                on_select_click: on_button_click,
            }
        }
    }
}
