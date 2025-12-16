use crate::db::{DbStorageProfile, StorageLocation};
use crate::library::use_library_manager;
use dioxus::prelude::*;
use tracing::{error, info};

/// Storage Profiles section - CRUD for profiles
#[component]
pub fn StorageProfilesSection() -> Element {
    let library_manager = use_library_manager();

    let mut profiles = use_signal(Vec::<DbStorageProfile>::new);
    let mut editing_profile = use_signal(|| Option::<DbStorageProfile>::None);
    let mut is_creating = use_signal(|| false);
    let mut is_loading = use_signal(|| true);
    let mut refresh_trigger = use_signal(|| 0u32);

    // Load profiles on mount and when refresh_trigger changes
    let lm = library_manager.clone();
    use_effect(move || {
        let _ = *refresh_trigger.read(); // React to changes
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

    rsx! {
        div { class: "max-w-2xl",
            div { class: "flex items-center justify-between mb-6",
                h2 { class: "text-xl font-semibold text-white", "Storage Profiles" }
                if !*is_creating.read() && editing_profile.read().is_none() {
                    button {
                        class: "px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-500 transition-colors flex items-center gap-2",
                        onclick: move |_| {
                            is_creating.set(true);
                            editing_profile.set(None);
                        },
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M12 4v16m8-8H4"
                            }
                        }
                        "New Profile"
                    }
                }
            }

            // Editor/Creator form
            if *is_creating.read() {
                ProfileEditor {
                    profile: None,
                    on_save: move |_| {
                        is_creating.set(false);
                        refresh_trigger.set(refresh_trigger() + 1);
                    },
                    on_cancel: move |_| {
                        editing_profile.set(None);
                        is_creating.set(false);
                    }
                }
            } else if let Some(profile) = editing_profile.read().clone() {
                ProfileEditor {
                    profile: Some(profile),
                    on_save: move |_| {
                        editing_profile.set(None);
                        refresh_trigger.set(refresh_trigger() + 1);
                    },
                    on_cancel: move |_| {
                        editing_profile.set(None);
                        is_creating.set(false);
                    }
                }
            }

            // Profile list
            if !*is_creating.read() && editing_profile.read().is_none() {
                if *is_loading.read() {
                    div { class: "bg-gray-800 rounded-lg p-6 text-center text-gray-400",
                        "Loading profiles..."
                    }
                } else if profiles.read().is_empty() {
                    div { class: "bg-gray-800 rounded-lg p-6 text-center",
                        p { class: "text-gray-400 mb-4", "No storage profiles configured" }
                        p { class: "text-sm text-gray-500", "Create a profile to define how releases are stored." }
                    }
                } else {
                    div { class: "space-y-3",
                        for profile in profiles.read().iter() {
                            ProfileCard {
                                key: "{profile.id}",
                                profile: profile.clone(),
                                on_edit: move |p: DbStorageProfile| {
                                    editing_profile.set(Some(p));
                                    is_creating.set(false);
                                },
                                on_refresh: move |_| {
                                    refresh_trigger.set(refresh_trigger() + 1);
                                }
                            }
                        }
                    }
                }
            }

            // Help text
            div { class: "mt-6 p-4 bg-gray-700/50 rounded-lg",
                p { class: "text-sm text-gray-400",
                    "Storage profiles determine how release files are stored. You can have multiple profiles "
                    "for different use cases (e.g., local development, cloud backup). The default profile "
                    "is used for new imports."
                }
            }
        }
    }
}

#[component]
fn ProfileCard(
    profile: DbStorageProfile,
    on_edit: EventHandler<DbStorageProfile>,
    on_refresh: EventHandler<()>,
) -> Element {
    let library_manager = use_library_manager();
    let mut is_deleting = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);

    let profile_for_edit = profile.clone();
    let profile_id_for_default = profile.id.clone();
    let profile_id_for_delete = profile.id.clone();

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-4",
            div { class: "flex items-start justify-between",
                div { class: "flex-1",
                    div { class: "flex items-center gap-3",
                        h3 { class: "text-lg font-medium text-white", "{profile.name}" }
                        if profile.is_default {
                            span { class: "px-2 py-0.5 bg-indigo-900 text-indigo-300 rounded text-xs", "Default" }
                        }
                    }

                    div { class: "flex flex-wrap gap-2 mt-2",
                        span { class: "px-2 py-1 bg-gray-700 text-gray-300 rounded text-xs",
                            match profile.location {
                                StorageLocation::Local => "Local",
                                StorageLocation::Cloud => "Cloud",
                            }
                        }
                        if profile.encrypted {
                            span { class: "px-2 py-1 bg-green-900 text-green-300 rounded text-xs", "Encrypted" }
                        }
                        if profile.chunked {
                            span { class: "px-2 py-1 bg-blue-900 text-blue-300 rounded text-xs", "Chunked" }
                        }
                    }

                    p { class: "text-sm text-gray-500 mt-2 font-mono", "{profile.location_path}" }
                }

                // Actions
                div { class: "flex items-center gap-2",
                    if !profile.is_default {
                        button {
                            class: "p-2 text-gray-400 hover:text-white hover:bg-gray-700 rounded-lg transition-colors",
                            title: "Set as default",
                            onclick: {
                                let lm = library_manager.clone();
                                let pid = profile_id_for_default.clone();
                                move |_| {
                                    let lm = lm.clone();
                                    let pid = pid.clone();
                                    spawn(async move {
                                        match lm.set_default_storage_profile(&pid).await {
                                            Ok(()) => {
                                                info!("Set default profile: {}", pid);
                                                on_refresh.call(());
                                            }
                                            Err(e) => error!("Failed to set default profile: {}", e),
                                        }
                                    });
                                }
                            },
                            svg {
                                class: "w-5 h-5",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M5 13l4 4L19 7"
                                }
                            }
                        }
                    }
                    button {
                        class: "p-2 text-gray-400 hover:text-white hover:bg-gray-700 rounded-lg transition-colors",
                        title: "Edit",
                        onclick: {
                            let p = profile_for_edit.clone();
                            move |_| on_edit.call(p.clone())
                        },
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                            }
                        }
                    }
                    button {
                        class: "p-2 text-gray-400 hover:text-red-400 hover:bg-gray-700 rounded-lg transition-colors",
                        title: "Delete",
                        onclick: move |_| show_delete_confirm.set(true),
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                            }
                        }
                    }
                }
            }

            // Delete confirmation
            if *show_delete_confirm.read() {
                div { class: "mt-4 p-3 bg-red-900/30 border border-red-700 rounded-lg",
                    p { class: "text-sm text-red-300 mb-3", "Are you sure you want to delete this profile?" }
                    div { class: "flex gap-2",
                        button {
                            class: "px-3 py-1.5 bg-red-600 text-white rounded hover:bg-red-500 transition-colors text-sm disabled:opacity-50",
                            disabled: *is_deleting.read(),
                            onclick: {
                                let lm = library_manager.clone();
                                let pid = profile_id_for_delete.clone();
                                move |_| {
                                    let lm = lm.clone();
                                    let pid = pid.clone();
                                    spawn(async move {
                                        is_deleting.set(true);
                                        match lm.delete_storage_profile(&pid).await {
                                            Ok(()) => {
                                                info!("Deleted profile: {}", pid);
                                                on_refresh.call(());
                                            }
                                            Err(e) => error!("Failed to delete profile: {}", e),
                                        }
                                        is_deleting.set(false);
                                        show_delete_confirm.set(false);
                                    });
                                }
                            },
                            if *is_deleting.read() { "Deleting..." } else { "Delete" }
                        }
                        button {
                            class: "px-3 py-1.5 bg-gray-700 text-gray-300 rounded hover:bg-gray-600 transition-colors text-sm",
                            onclick: move |_| show_delete_confirm.set(false),
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProfileEditor(
    profile: Option<DbStorageProfile>,
    on_save: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let library_manager = use_library_manager();
    let is_edit = profile.is_some();

    let mut name = use_signal(|| profile.as_ref().map(|p| p.name.clone()).unwrap_or_default());
    let mut location = use_signal(|| {
        profile
            .as_ref()
            .map(|p| p.location)
            .unwrap_or(StorageLocation::Cloud)
    });
    let mut location_path = use_signal(|| {
        profile
            .as_ref()
            .map(|p| p.location_path.clone())
            .unwrap_or_default()
    });
    let mut encrypted = use_signal(|| profile.as_ref().map(|p| p.encrypted).unwrap_or(true));
    let mut chunked = use_signal(|| profile.as_ref().map(|p| p.chunked).unwrap_or(true));
    let mut is_default = use_signal(|| profile.as_ref().map(|p| p.is_default).unwrap_or(false));

    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-6 mb-6",
            h3 { class: "text-lg font-medium text-white mb-4",
                if is_edit { "Edit Profile" } else { "New Profile" }
            }

            div { class: "space-y-4",
                // Name
                div {
                    label { class: "block text-sm font-medium text-gray-400 mb-2", "Name" }
                    input {
                        r#type: "text",
                        class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                        placeholder: "My Storage Profile",
                        value: "{name}",
                        oninput: move |e| name.set(e.value())
                    }
                }

                // Location type
                div {
                    label { class: "block text-sm font-medium text-gray-400 mb-2", "Storage Type" }
                    div { class: "flex gap-4",
                        label { class: "flex items-center gap-2 cursor-pointer",
                            input {
                                r#type: "radio",
                                name: "location",
                                class: "text-indigo-600 focus:ring-indigo-500",
                                checked: *location.read() == StorageLocation::Cloud,
                                onchange: move |_| location.set(StorageLocation::Cloud)
                            }
                            span { class: "text-white", "Cloud (S3)" }
                        }
                        label { class: "flex items-center gap-2 cursor-pointer",
                            input {
                                r#type: "radio",
                                name: "location",
                                class: "text-indigo-600 focus:ring-indigo-500",
                                checked: *location.read() == StorageLocation::Local,
                                onchange: move |_| location.set(StorageLocation::Local)
                            }
                            span { class: "text-white", "Local Filesystem" }
                        }
                    }
                }

                // Location path
                div {
                    label { class: "block text-sm font-medium text-gray-400 mb-2",
                        if *location.read() == StorageLocation::Cloud {
                            "Bucket Name"
                        } else {
                            "Directory Path"
                        }
                    }
                    input {
                        r#type: "text",
                        class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                        placeholder: if *location.read() == StorageLocation::Cloud { "my-music-bucket" } else { "/path/to/storage" },
                        value: "{location_path}",
                        oninput: move |e| location_path.set(e.value())
                    }
                }

                // Options
                div { class: "flex gap-6",
                    label { class: "flex items-center gap-2 cursor-pointer",
                        input {
                            r#type: "checkbox",
                            class: "rounded text-indigo-600 focus:ring-indigo-500 bg-gray-700 border-gray-600",
                            checked: *encrypted.read(),
                            onchange: move |e| encrypted.set(e.checked())
                        }
                        span { class: "text-white", "Encrypted" }
                    }
                    label { class: "flex items-center gap-2 cursor-pointer",
                        input {
                            r#type: "checkbox",
                            class: "rounded text-indigo-600 focus:ring-indigo-500 bg-gray-700 border-gray-600",
                            checked: *chunked.read(),
                            onchange: move |e| chunked.set(e.checked())
                        }
                        span { class: "text-white", "Chunked" }
                    }
                    label { class: "flex items-center gap-2 cursor-pointer",
                        input {
                            r#type: "checkbox",
                            class: "rounded text-indigo-600 focus:ring-indigo-500 bg-gray-700 border-gray-600",
                            checked: *is_default.read(),
                            onchange: move |e| is_default.set(e.checked())
                        }
                        span { class: "text-white", "Set as default" }
                    }
                }

                if let Some(error) = save_error.read().as_ref() {
                    div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                        "{error}"
                    }
                }

                // Actions
                div { class: "flex gap-3 pt-2",
                    button {
                        class: "px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-500 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: *is_saving.read(),
                        onclick: {
                            let lm = library_manager.clone();
                            let existing_profile = profile.clone();
                            move |_| {
                                let new_name = name.read().clone();
                                let new_location = *location.read();
                                let new_location_path = location_path.read().clone();
                                let new_encrypted = *encrypted.read();
                                let new_chunked = *chunked.read();
                                let new_is_default = *is_default.read();
                                let existing = existing_profile.clone();
                                let lm = lm.clone();

                                spawn(async move {
                                    is_saving.set(true);
                                    save_error.set(None);

                                    // Validation
                                    if new_name.trim().is_empty() {
                                        save_error.set(Some("Name is required".to_string()));
                                        is_saving.set(false);
                                        return;
                                    }
                                    if new_location_path.trim().is_empty() {
                                        save_error.set(Some("Path/bucket is required".to_string()));
                                        is_saving.set(false);
                                        return;
                                    }

                                    let result = if let Some(mut profile) = existing {
                                        // Update existing
                                        profile.name = new_name.clone();
                                        profile.location = new_location;
                                        profile.location_path = new_location_path;
                                        profile.encrypted = new_encrypted;
                                        profile.chunked = new_chunked;
                                        profile.is_default = new_is_default;
                                        lm.update_storage_profile(&profile).await
                                    } else {
                                        // Create new
                                        let profile = DbStorageProfile::new(
                                            &new_name,
                                            new_location,
                                            &new_location_path,
                                            new_encrypted,
                                            new_chunked,
                                        )
                                        .with_default(new_is_default);
                                        lm.insert_storage_profile(&profile).await
                                    };

                                    match result {
                                        Ok(()) => {
                                            info!("Saved storage profile: {}", new_name);
                                            on_save.call(());
                                        }
                                        Err(e) => {
                                            error!("Failed to save profile: {}", e);
                                            save_error.set(Some(e.to_string()));
                                        }
                                    }

                                    is_saving.set(false);
                                });
                            }
                        },
                        if *is_saving.read() { "Saving..." } else { "Save" }
                    }
                    button {
                        class: "px-4 py-2 bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                }
            }
        }
    }
}
