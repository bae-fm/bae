//! Confirmation wrapper - reads context and delegates to ConfirmationView

use crate::db::DbStorageProfile;
use crate::import::{MatchCandidate, MatchSource};
use crate::ui::import_context::state::SelectedCover;
use crate::ui::import_context::ImportContext;
use crate::ui::local_file_url;
use crate::ui::Route;
use bae_ui::components::import::ConfirmationView;
use bae_ui::display_types::{ArtworkFile, MatchSourceType, StorageProfileInfo};
use dioxus::prelude::*;
use std::rc::Rc;

/// Convert bae MatchCandidate to bae-ui display type
pub fn to_display_candidate(candidate: &MatchCandidate) -> bae_ui::display_types::MatchCandidate {
    let (source_type, format, country, label, catalog_number, original_year) =
        match &candidate.source {
            MatchSource::MusicBrainz(release) => (
                MatchSourceType::MusicBrainz,
                release.format.clone(),
                release.country.clone(),
                release.label.clone(),
                release.catalog_number.clone(),
                release.first_release_date.clone(),
            ),
            MatchSource::Discogs(result) => (
                MatchSourceType::Discogs,
                result.format.as_ref().map(|v| v.join(", ")),
                result.country.clone(),
                result.label.as_ref().map(|v| v.join(", ")),
                None, // Discogs search results don't have catalog number
                None,
            ),
        };

    bae_ui::display_types::MatchCandidate {
        title: candidate.title(),
        artist: match &candidate.source {
            MatchSource::MusicBrainz(r) => r.artist.clone(),
            MatchSource::Discogs(r) => r.title.split(" - ").next().unwrap_or("").to_string(),
        },
        year: candidate.year(),
        cover_url: candidate.cover_art_url(),
        format,
        country,
        label,
        catalog_number,
        source_type,
        original_year,
    }
}

/// Convert bae SelectedCover to bae-ui display type
fn to_display_selected_cover(
    cover: &SelectedCover,
    source_name: &str,
) -> bae_ui::display_types::SelectedCover {
    match cover {
        SelectedCover::Remote { url, .. } => bae_ui::display_types::SelectedCover::Remote {
            url: url.clone(),
            source: source_name.to_string(),
        },
        SelectedCover::Local { filename } => bae_ui::display_types::SelectedCover::Local {
            filename: filename.clone(),
        },
    }
}

#[component]
pub fn Confirmation(
    confirmed_candidate: ReadSignal<Option<MatchCandidate>>,
    on_edit: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let navigator = use_navigator();
    let import_context = use_context::<Rc<ImportContext>>();
    let is_importing = import_context.is_importing();
    let preparing_step = import_context.preparing_step();
    let folder_files = import_context.folder_files();
    let folder_path = import_context.folder_path();
    let mut storage_profiles: Signal<Vec<DbStorageProfile>> = use_signal(Vec::new);
    let selected_profile_id = import_context.storage_profile_id();

    // Load storage profiles on mount
    {
        let import_context = import_context.clone();
        use_effect(move || {
            let import_context = import_context.clone();
            spawn(async move {
                match import_context
                    .library_manager
                    .get()
                    .get_all_storage_profiles()
                    .await
                {
                    Ok(profiles) => {
                        storage_profiles.set(profiles.clone());
                        if selected_profile_id.read().is_none() {
                            if let Some(default) = profiles.iter().find(|p| p.is_default) {
                                import_context.set_storage_profile_id(Some(default.id.clone()));
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load storage profiles: {}", e);
                    }
                }
            });
        });
    }

    let selected_cover = import_context.selected_cover();

    // Auto-select remote cover if none selected
    {
        let import_context = import_context.clone();
        use_effect(move || {
            if selected_cover.read().is_none() {
                if let Some(candidate) = confirmed_candidate.read().as_ref() {
                    if let Some(url) = candidate.cover_art_url() {
                        let source = match &candidate.source {
                            MatchSource::MusicBrainz(_) => "musicbrainz",
                            MatchSource::Discogs(_) => "discogs",
                        };
                        import_context.set_remote_cover(&url, source);
                    }
                }
            }
        });
    }

    let Some(candidate) = confirmed_candidate.read().as_ref().cloned() else {
        return rsx! {};
    };

    let remote_cover_url = candidate.cover_art_url();
    let folder_path_str = folder_path.read().clone();

    // Convert artwork files to ArtworkFile with resolved URLs
    let artwork_files: Vec<ArtworkFile> = folder_files
        .read()
        .artwork
        .iter()
        .map(|f| {
            let path = format!("{}/{}", folder_path_str, f.name);
            ArtworkFile {
                name: f.name.clone(),
                display_url: local_file_url(&path),
            }
        })
        .collect();

    // Compute display cover URL
    let display_cover_url = match selected_cover.read().as_ref() {
        Some(SelectedCover::Local { filename }) => {
            let path = format!("{}/{}", folder_path_str, filename);
            Some(local_file_url(&path))
        }
        Some(SelectedCover::Remote { url, .. }) => Some(url.clone()),
        None => remote_cover_url.clone(),
    };

    // Convert storage profiles to display type
    let profiles: Vec<StorageProfileInfo> = storage_profiles
        .read()
        .iter()
        .map(|p| StorageProfileInfo {
            id: p.id.clone(),
            name: p.name.clone(),
            is_default: p.is_default,
        })
        .collect();

    let display_candidate = to_display_candidate(&candidate);
    let cover_source_name = match &candidate.source {
        MatchSource::MusicBrainz(_) => "musicbrainz",
        MatchSource::Discogs(_) => "discogs",
    };
    let display_selected_cover = selected_cover
        .read()
        .as_ref()
        .map(|c| to_display_selected_cover(c, cover_source_name));
    let preparing_text = preparing_step
        .read()
        .as_ref()
        .map(|s| s.display_text().to_string());

    rsx! {
        ConfirmationView {
            candidate: display_candidate,
            selected_cover: display_selected_cover,
            display_cover_url,
            artwork_files,
            remote_cover_url,
            storage_profiles: profiles,
            selected_profile_id: selected_profile_id.read().clone(),
            is_importing: *is_importing.read(),
            preparing_step_text: preparing_text,
            on_select_remote_cover: {
                let import_context = import_context.clone();
                let source = match &candidate.source {
                    MatchSource::MusicBrainz(_) => "musicbrainz",
                    MatchSource::Discogs(_) => "discogs",
                };
                let source = source.to_string();
                move |url: String| {
                    import_context.set_remote_cover(&url, &source);
                }
            },
            on_select_local_cover: {
                let import_context = import_context.clone();
                move |filename: String| {
                    import_context.set_local_cover(&filename);
                }
            },
            on_storage_profile_change: {
                let import_context = import_context.clone();
                move |profile_id: Option<String>| {
                    import_context.set_storage_profile_id(profile_id);
                }
            },
            on_edit: move |_| on_edit.call(()),
            on_confirm: move |_| on_confirm.call(()),
            on_configure_storage: move |_| {
                navigator.push(Route::Settings {});
            },
        }
    }
}
