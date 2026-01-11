use crate::ui::AppContext;
use bae_core::db::{DbImport, ImportOperationStatus};
use bae_core::import::{ImportProgress, PrepareStep};
use dioxus::prelude::*;
/// Represents a single import operation being tracked in the UI
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveImport {
    pub import_id: String,
    pub album_title: String,
    pub artist_name: String,
    pub status: ImportOperationStatus,
    pub current_step: Option<PrepareStep>,
    pub progress_percent: Option<u8>,
    pub release_id: Option<String>,
    /// External cover art URL (ephemeral, shown during import)
    pub cover_art_url: Option<String>,
    /// Stored cover image ID (shown after import complete)
    pub cover_image_id: Option<String>,
}
impl From<DbImport> for ActiveImport {
    fn from(db_import: DbImport) -> Self {
        Self {
            import_id: db_import.id,
            album_title: db_import.album_title,
            artist_name: db_import.artist_name,
            status: db_import.status,
            current_step: None,
            progress_percent: None,
            release_id: db_import.release_id,
            cover_art_url: None,
            cover_image_id: None,
        }
    }
}
/// Shared state for active imports across the app
#[derive(Clone, Copy)]
pub struct ActiveImportsState {
    pub imports: Signal<Vec<ActiveImport>>,
    pub is_loading: Signal<bool>,
}
impl ActiveImportsState {
    /// Dismiss/remove an import from the list (used by ImportsDropdown)
    pub fn dismiss(&self, import_id: &str) {
        let mut imports = self.imports;
        imports.with_mut(|list| {
            list.retain(|i| i.import_id != import_id);
        });
    }
}
/// Provider component for active imports state
#[component]
pub fn ActiveImportsProvider(children: Element) -> Element {
    let imports = use_signal(Vec::new);
    let is_loading = use_signal(|| true);
    let state = ActiveImportsState {
        imports,
        is_loading,
    };
    use_context_provider(|| state);
    let app_context = use_context::<AppContext>();
    let library_manager = app_context.library_manager.clone();
    let import_handle = app_context.import_handle.clone();
    use_effect({
        let library_manager = library_manager.clone();
        let mut imports = state.imports;
        let mut is_loading = state.is_loading;
        move || {
            let library_manager = library_manager.clone();
            spawn(async move {
                match library_manager.get().get_active_imports().await {
                    Ok(db_imports) => {
                        let active: Vec<ActiveImport> =
                            db_imports.into_iter().map(ActiveImport::from).collect();
                        imports.set(active);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load active imports: {}", e);
                    }
                }
                is_loading.set(false);
            });
        }
    });
    use_effect({
        let import_handle = import_handle.clone();
        let imports = state.imports;
        move || {
            let import_handle = import_handle.clone();
            spawn(async move {
                let mut progress_rx = import_handle.subscribe_all_imports();
                while let Some(event) = progress_rx.recv().await {
                    handle_progress_event(imports, event);
                }
            });
        }
    });
    rsx! {
        {children}
    }
}
/// Handle a progress event and update the imports state
fn handle_progress_event(imports: Signal<Vec<ActiveImport>>, event: ImportProgress) {
    let mut imports = imports;
    match event {
        ImportProgress::Preparing {
            import_id,
            step,
            album_title,
            artist_name,
            cover_art_url,
        } => {
            imports.with_mut(|list| {
                if let Some(import) = list.iter_mut().find(|i| i.import_id == import_id) {
                    import.current_step = Some(step);
                    import.status = ImportOperationStatus::Preparing;
                } else {
                    list.push(ActiveImport {
                        import_id,
                        album_title,
                        artist_name,
                        status: ImportOperationStatus::Preparing,
                        current_step: Some(step),
                        progress_percent: None,
                        release_id: None,
                        cover_art_url,
                        cover_image_id: None,
                    });
                }
            });
        }
        ImportProgress::Started { id, import_id, .. } => {
            if let Some(ref iid) = import_id {
                imports.with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.status = ImportOperationStatus::Importing;
                        import.current_step = None;
                        import.progress_percent = Some(0);
                        if import.release_id.is_none() {
                            import.release_id = Some(id.clone());
                        }
                    }
                });
            }
        }
        ImportProgress::Progress {
            percent, import_id, ..
        } => {
            if let Some(ref iid) = import_id {
                imports.with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.progress_percent = Some(percent);
                    }
                });
            }
        }
        ImportProgress::Complete {
            import_id,
            release_id,
            cover_image_id,
            ..
        } => {
            if let Some(ref iid) = import_id {
                imports.with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.status = ImportOperationStatus::Complete;
                        import.progress_percent = Some(100);
                        if release_id.is_some() {
                            import.release_id = release_id.clone();
                        }
                        if cover_image_id.is_some() {
                            import.cover_image_id = cover_image_id.clone();
                        }
                    }
                });
            }
        }
        ImportProgress::Failed { import_id, .. } => {
            if let Some(ref iid) = import_id {
                imports.with_mut(|list| {
                    if let Some(import) = list.iter_mut().find(|i| &i.import_id == iid) {
                        import.status = ImportOperationStatus::Failed;
                    }
                });
            }
        }
    }
}
/// Hook to access active imports state (used by TitleBar components)
pub fn use_active_imports() -> ActiveImportsState {
    use_context::<ActiveImportsState>()
}
