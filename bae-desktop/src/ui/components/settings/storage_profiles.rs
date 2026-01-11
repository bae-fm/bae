//! Storage Profiles section wrapper - handles persistence, delegates UI to StorageProfilesSectionView

use crate::ui::use_library_manager;
use bae_core::db::{DbStorageProfile, StorageLocation};
use bae_ui::{StorageProfile, StorageProfilesSectionView};
use dioxus::prelude::*;
use tracing::{error, info};

/// Convert bae StorageLocation to bae-ui StorageLocation
fn to_display_location(loc: StorageLocation) -> bae_ui::StorageLocation {
    match loc {
        StorageLocation::Local => bae_ui::StorageLocation::Local,
        StorageLocation::Cloud => bae_ui::StorageLocation::Cloud,
    }
}

/// Convert bae-ui StorageLocation to bae StorageLocation
fn from_display_location(loc: bae_ui::StorageLocation) -> StorageLocation {
    match loc {
        bae_ui::StorageLocation::Local => StorageLocation::Local,
        bae_ui::StorageLocation::Cloud => StorageLocation::Cloud,
    }
}

/// Convert DbStorageProfile to bae-ui StorageProfile
fn to_display_profile(p: &DbStorageProfile) -> StorageProfile {
    StorageProfile {
        id: p.id.clone(),
        name: p.name.clone(),
        location: to_display_location(p.location),
        location_path: p.location_path.clone(),
        encrypted: p.encrypted,
        is_default: p.is_default,
        cloud_bucket: p.cloud_bucket.clone(),
        cloud_region: p.cloud_region.clone(),
        cloud_endpoint: p.cloud_endpoint.clone(),
        cloud_access_key: p.cloud_access_key.clone(),
        cloud_secret_key: p.cloud_secret_key.clone(),
    }
}

/// Storage Profiles section - CRUD for profiles
#[component]
pub fn StorageProfilesSection() -> Element {
    let library_manager = use_library_manager();
    let mut profiles = use_signal(Vec::<DbStorageProfile>::new);
    let mut editing_profile = use_signal(|| Option::<DbStorageProfile>::None);
    let mut is_creating = use_signal(|| false);
    let mut is_loading = use_signal(|| true);
    let mut refresh_trigger = use_signal(|| 0u32);

    let lm = library_manager.clone();
    use_effect(move || {
        let _ = *refresh_trigger.read();
        let lm = lm.clone();
        spawn(async move {
            is_loading.set(true);
            match lm.get_all_storage_profiles().await {
                Ok(p) => profiles.set(p),
                Err(e) => error!("Failed to load storage profiles: {}", e),
            }
            is_loading.set(false);
        });
    });

    let display_profiles: Vec<StorageProfile> =
        profiles.read().iter().map(to_display_profile).collect();

    let display_editing = editing_profile.read().as_ref().map(to_display_profile);

    // Handle save from the view - profile data comes back as bae-ui::StorageProfile
    let handle_save = {
        let lm = library_manager.clone();
        move |profile: StorageProfile| {
            let lm = lm.clone();
            let is_new = profile.id.is_empty();
            spawn(async move {
                let result = if is_new {
                    // Create new profile
                    let location = from_display_location(profile.location);
                    let db_profile = if location == StorageLocation::Local {
                        DbStorageProfile::new_local(
                            &profile.name,
                            &profile.location_path,
                            profile.encrypted,
                        )
                    } else {
                        let endpoint = profile.cloud_endpoint.as_deref();
                        DbStorageProfile::new_cloud(
                            &profile.name,
                            profile.cloud_bucket.as_deref().unwrap_or(""),
                            profile.cloud_region.as_deref().unwrap_or(""),
                            endpoint,
                            profile.cloud_access_key.as_deref().unwrap_or(""),
                            profile.cloud_secret_key.as_deref().unwrap_or(""),
                            profile.encrypted,
                        )
                    }
                    .with_default(profile.is_default);
                    lm.insert_storage_profile(&db_profile).await
                } else {
                    // Update existing profile
                    let mut db_profile = DbStorageProfile {
                        id: profile.id.clone(),
                        name: profile.name.clone(),
                        location: from_display_location(profile.location),
                        location_path: profile.location_path.clone(),
                        encrypted: profile.encrypted,
                        is_default: profile.is_default,
                        cloud_bucket: profile.cloud_bucket.clone(),
                        cloud_region: profile.cloud_region.clone(),
                        cloud_endpoint: profile.cloud_endpoint.clone(),
                        cloud_access_key: profile.cloud_access_key.clone(),
                        cloud_secret_key: profile.cloud_secret_key.clone(),
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    };

                    if db_profile.location == StorageLocation::Local {
                        db_profile.cloud_bucket = None;
                        db_profile.cloud_region = None;
                        db_profile.cloud_endpoint = None;
                        db_profile.cloud_access_key = None;
                        db_profile.cloud_secret_key = None;
                    }

                    lm.update_storage_profile(&db_profile).await
                };

                match result {
                    Ok(()) => {
                        info!("Saved storage profile: {}", profile.name);
                        is_creating.set(false);
                        editing_profile.set(None);
                        refresh_trigger.set(refresh_trigger() + 1);
                    }
                    Err(e) => {
                        error!("Failed to save profile: {}", e);
                    }
                }
            });
        }
    };

    let handle_delete = {
        let lm = library_manager.clone();
        move |profile_id: String| {
            let lm = lm.clone();
            spawn(async move {
                match lm.delete_storage_profile(&profile_id).await {
                    Ok(()) => {
                        info!("Deleted profile: {}", profile_id);
                        refresh_trigger.set(refresh_trigger() + 1);
                    }
                    Err(e) => error!("Failed to delete profile: {}", e),
                }
            });
        }
    };

    let handle_set_default = {
        let lm = library_manager.clone();
        move |profile_id: String| {
            let lm = lm.clone();
            spawn(async move {
                match lm.set_default_storage_profile(&profile_id).await {
                    Ok(()) => {
                        info!("Set default profile: {}", profile_id);
                        refresh_trigger.set(refresh_trigger() + 1);
                    }
                    Err(e) => error!("Failed to set default profile: {}", e),
                }
            });
        }
    };

    let handle_edit = move |profile: StorageProfile| {
        // Find the original DbStorageProfile
        if let Some(db_profile) = profiles.read().iter().find(|p| p.id == profile.id).cloned() {
            editing_profile.set(Some(db_profile));
            is_creating.set(false);
        }
    };

    rsx! {
        StorageProfilesSectionView {
            profiles: display_profiles,
            is_loading: *is_loading.read(),
            editing_profile: display_editing,
            is_creating: *is_creating.read(),
            on_create: move |_| {
                is_creating.set(true);
                editing_profile.set(None);
            },
            on_edit: handle_edit,
            on_delete: handle_delete,
            on_set_default: handle_set_default,
            on_save: handle_save,
            on_cancel_edit: move |_| {
                editing_profile.set(None);
                is_creating.set(false);
            },
        }
    }
}
