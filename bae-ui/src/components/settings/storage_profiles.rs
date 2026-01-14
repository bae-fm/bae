//! Storage profiles section view

use crate::components::icons::{CheckIcon, PencilIcon, PlusIcon, TrashIcon};
use dioxus::prelude::*;

/// Storage location type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageLocation {
    #[default]
    Local,
    Cloud,
}

impl StorageLocation {
    pub fn label(&self) -> &'static str {
        match self {
            StorageLocation::Local => "Local",
            StorageLocation::Cloud => "Cloud",
        }
    }
}

/// Storage profile display data
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StorageProfile {
    pub id: String,
    pub name: String,
    pub location: StorageLocation,
    pub location_path: String,
    pub encrypted: bool,
    pub is_default: bool,
    pub cloud_bucket: Option<String>,
    pub cloud_region: Option<String>,
    pub cloud_endpoint: Option<String>,
    pub cloud_access_key: Option<String>,
    pub cloud_secret_key: Option<String>,
}

/// Storage profiles section view
#[component]
pub fn StorageProfilesSectionView(
    profiles: Vec<StorageProfile>,
    is_loading: bool,
    editing_profile: Option<StorageProfile>,
    is_creating: bool,
    on_create: EventHandler<()>,
    on_edit: EventHandler<StorageProfile>,
    on_delete: EventHandler<String>,
    on_set_default: EventHandler<String>,
    on_save: EventHandler<StorageProfile>,
    on_cancel_edit: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "max-w-2xl",
            div { class: "flex items-center justify-between mb-6",
                h2 { class: "text-xl font-semibold text-white", "Storage Profiles" }
                if !is_creating && editing_profile.is_none() {
                    button {
                        class: "px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-500 transition-colors flex items-center gap-2",
                        onclick: move |_| on_create.call(()),
                        PlusIcon { class: "w-5 h-5" }
                        "New Profile"
                    }
                }
            }

            if is_creating {
                StorageProfileEditorView { profile: None, on_save, on_cancel: on_cancel_edit }
            } else if let Some(ref profile) = editing_profile {
                StorageProfileEditorView {
                    profile: Some(profile.clone()),
                    on_save,
                    on_cancel: on_cancel_edit,
                }
            }

            if !is_creating && editing_profile.is_none() {
                if is_loading {
                    div { class: "bg-gray-800 rounded-lg p-6 text-center text-gray-400",
                        "Loading profiles..."
                    }
                } else if profiles.is_empty() {
                    div { class: "bg-gray-800 rounded-lg p-6 text-center",
                        p { class: "text-gray-400 mb-4", "No storage profiles configured" }
                        p { class: "text-sm text-gray-500",
                            "Create a profile to define how releases are stored."
                        }
                    }
                } else {
                    div { class: "space-y-3",
                        for profile in profiles.iter() {
                            ProfileCard {
                                key: "{profile.id}",
                                profile: profile.clone(),
                                on_edit,
                                on_delete,
                                on_set_default,
                            }
                        }
                    }
                }
            }

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
    profile: StorageProfile,
    on_edit: EventHandler<StorageProfile>,
    on_delete: EventHandler<String>,
    on_set_default: EventHandler<String>,
) -> Element {
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
                            span { class: "px-2 py-0.5 bg-indigo-900 text-indigo-300 rounded text-xs",
                                "Default"
                            }
                        }
                    }
                    div { class: "flex flex-wrap gap-2 mt-2",
                        span { class: "px-2 py-1 bg-gray-700 text-gray-300 rounded text-xs",
                            "{profile.location.label()}"
                        }
                        if profile.encrypted {
                            span { class: "px-2 py-1 bg-green-900 text-green-300 rounded text-xs",
                                "Encrypted"
                            }
                        }
                    }
                    p { class: "text-sm text-gray-500 mt-2 font-mono", "{profile.location_path}" }
                }
                div { class: "flex items-center gap-2",
                    if !profile.is_default {
                        button {
                            class: "p-2 text-gray-400 hover:text-white hover:bg-gray-700 rounded-lg transition-colors",
                            title: "Set as default",
                            onclick: {
                                let pid = profile_id_for_default.clone();
                                move |_| on_set_default.call(pid.clone())
                            },
                            CheckIcon { class: "w-5 h-5" }
                        }
                    }
                    button {
                        class: "p-2 text-gray-400 hover:text-white hover:bg-gray-700 rounded-lg transition-colors",
                        title: "Edit",
                        onclick: {
                            let p = profile_for_edit.clone();
                            move |_| on_edit.call(p.clone())
                        },
                        PencilIcon { class: "w-5 h-5" }
                    }
                    button {
                        class: "p-2 text-gray-400 hover:text-red-400 hover:bg-gray-700 rounded-lg transition-colors",
                        title: "Delete",
                        onclick: move |_| show_delete_confirm.set(true),
                        TrashIcon { class: "w-5 h-5" }
                    }
                }
            }

            if *show_delete_confirm.read() {
                div { class: "mt-4 p-3 bg-red-900/30 border border-red-700 rounded-lg",
                    p { class: "text-sm text-red-300 mb-3",
                        "Are you sure you want to delete this profile?"
                    }
                    div { class: "flex gap-2",
                        button {
                            class: "px-3 py-1.5 bg-red-600 text-white rounded hover:bg-red-500 transition-colors text-sm",
                            onclick: {
                                let pid = profile_id_for_delete.clone();
                                move |_| {
                                    on_delete.call(pid.clone());
                                    show_delete_confirm.set(false);
                                }
                            },
                            "Delete"
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

/// Profile editor form view
#[component]
pub fn StorageProfileEditorView(
    profile: Option<StorageProfile>,
    on_save: EventHandler<StorageProfile>,
    on_cancel: EventHandler<()>,
) -> Element {
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
    let mut cloud_bucket = use_signal(|| {
        profile
            .as_ref()
            .and_then(|p| p.cloud_bucket.clone())
            .unwrap_or_default()
    });
    let mut cloud_region = use_signal(|| {
        profile
            .as_ref()
            .and_then(|p| p.cloud_region.clone())
            .unwrap_or_default()
    });
    let mut cloud_endpoint = use_signal(|| {
        profile
            .as_ref()
            .and_then(|p| p.cloud_endpoint.clone())
            .unwrap_or_default()
    });
    let mut cloud_access_key = use_signal(|| {
        profile
            .as_ref()
            .and_then(|p| p.cloud_access_key.clone())
            .unwrap_or_default()
    });
    let mut cloud_secret_key = use_signal(|| {
        profile
            .as_ref()
            .and_then(|p| p.cloud_secret_key.clone())
            .unwrap_or_default()
    });
    let mut show_secrets = use_signal(|| false);
    let mut encrypted = use_signal(|| profile.as_ref().map(|p| p.encrypted).unwrap_or(true));
    let mut is_default = use_signal(|| profile.as_ref().map(|p| p.is_default).unwrap_or(false));
    let mut validation_error = use_signal(|| Option::<String>::None);

    let existing_id = profile.as_ref().map(|p| p.id.clone());

    let handle_save = move |_| {
        validation_error.set(None);

        let new_name = name.read().clone();
        let new_location = *location.read();
        let new_location_path = location_path.read().clone();
        let new_cloud_bucket = cloud_bucket.read().clone();
        let new_cloud_region = cloud_region.read().clone();
        let new_cloud_endpoint = cloud_endpoint.read().clone();
        let new_cloud_access_key = cloud_access_key.read().clone();
        let new_cloud_secret_key = cloud_secret_key.read().clone();
        let new_encrypted = *encrypted.read();
        let new_is_default = *is_default.read();

        // Validation
        if new_name.trim().is_empty() {
            validation_error.set(Some("Name is required".to_string()));
            return;
        }

        if new_location == StorageLocation::Local {
            if new_location_path.trim().is_empty() {
                validation_error.set(Some("Directory path is required".to_string()));
                return;
            }
        } else {
            if new_cloud_bucket.trim().is_empty() {
                validation_error.set(Some("Bucket name is required".to_string()));
                return;
            }
            if new_cloud_region.trim().is_empty() {
                validation_error.set(Some("Region is required".to_string()));
                return;
            }
            if new_cloud_access_key.trim().is_empty() {
                validation_error.set(Some("Access key is required".to_string()));
                return;
            }
            if new_cloud_secret_key.trim().is_empty() {
                validation_error.set(Some("Secret key is required".to_string()));
                return;
            }
        }

        let profile = StorageProfile {
            id: existing_id.clone().unwrap_or_default(),
            name: new_name,
            location: new_location,
            location_path: if new_location == StorageLocation::Local {
                new_location_path
            } else {
                String::new()
            },
            encrypted: new_encrypted,
            is_default: new_is_default,
            cloud_bucket: if new_location == StorageLocation::Cloud {
                Some(new_cloud_bucket)
            } else {
                None
            },
            cloud_region: if new_location == StorageLocation::Cloud {
                Some(new_cloud_region)
            } else {
                None
            },
            cloud_endpoint: if new_location == StorageLocation::Cloud
                && !new_cloud_endpoint.trim().is_empty()
            {
                Some(new_cloud_endpoint)
            } else {
                None
            },
            cloud_access_key: if new_location == StorageLocation::Cloud {
                Some(new_cloud_access_key)
            } else {
                None
            },
            cloud_secret_key: if new_location == StorageLocation::Cloud {
                Some(new_cloud_secret_key)
            } else {
                None
            },
        };

        on_save.call(profile);
    };

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-6 mb-6",
            h3 { class: "text-lg font-medium text-white mb-4",
                if is_edit {
                    "Edit Profile"
                } else {
                    "New Profile"
                }
            }
            div { class: "space-y-4",
                div {
                    label { class: "block text-sm font-medium text-gray-400 mb-2", "Name" }
                    input {
                        r#type: "text",
                        autocomplete: "off",
                        class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                        placeholder: "My Storage Profile",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }

                div {
                    label { class: "block text-sm font-medium text-gray-400 mb-2", "Storage Type" }
                    div { class: "flex flex-col gap-3",
                        label { class: "flex items-center gap-2 cursor-pointer",
                            input {
                                r#type: "radio",
                                name: "location",
                                class: "text-indigo-600 focus:ring-indigo-500",
                                checked: *location.read() == StorageLocation::Cloud,
                                onchange: move |_| location.set(StorageLocation::Cloud),
                            }
                            span { class: "text-white", "Cloud (S3)" }
                        }
                        label { class: "flex items-center gap-2 cursor-pointer",
                            input {
                                r#type: "radio",
                                name: "location",
                                class: "text-indigo-600 focus:ring-indigo-500",
                                checked: *location.read() == StorageLocation::Local,
                                onchange: move |_| location.set(StorageLocation::Local),
                            }
                            span { class: "text-white", "Local Filesystem" }
                        }
                    }
                }

                if *location.read() == StorageLocation::Local {
                    div {
                        label { class: "block text-sm font-medium text-gray-400 mb-2",
                            "Directory Path"
                        }
                        input {
                            r#type: "text",
                            autocomplete: "off",
                            class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                            placeholder: "/path/to/storage",
                            value: "{location_path}",
                            oninput: move |e| location_path.set(e.value()),
                        }
                    }
                } else {
                    div {
                        label { class: "block text-sm font-medium text-gray-400 mb-2",
                            "Bucket Name"
                        }
                        input {
                            r#type: "text",
                            autocomplete: "off",
                            class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                            placeholder: "my-music-bucket",
                            value: "{cloud_bucket}",
                            oninput: move |e| cloud_bucket.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-gray-400 mb-2",
                            "Region"
                        }
                        input {
                            r#type: "text",
                            autocomplete: "off",
                            class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                            placeholder: "us-east-1",
                            value: "{cloud_region}",
                            oninput: move |e| cloud_region.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-gray-400 mb-2",
                            "Custom Endpoint (optional)"
                        }
                        input {
                            r#type: "text",
                            autocomplete: "off",
                            class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                            placeholder: "https://minio.example.com",
                            value: "{cloud_endpoint}",
                            oninput: move |e| cloud_endpoint.set(e.value()),
                        }
                        p { class: "text-xs text-gray-500 mt-1", "Leave empty for AWS S3" }
                    }

                    div { class: "flex items-center justify-between",
                        span { class: "text-sm font-medium text-gray-400", "Credentials" }
                        button {
                            r#type: "button",
                            class: "text-sm text-indigo-400 hover:text-indigo-300",
                            onclick: move |_| show_secrets.toggle(),
                            if *show_secrets.read() {
                                "Hide"
                            } else {
                                "Show"
                            }
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-gray-400 mb-2",
                            "Access Key ID"
                        }
                        input {
                            r#type: if *show_secrets.read() { "text" } else { "password" },
                            autocomplete: "off",
                            class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500 font-mono",
                            placeholder: "AKIAIOSFODNN7EXAMPLE",
                            value: "{cloud_access_key}",
                            oninput: move |e| cloud_access_key.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-gray-400 mb-2",
                            "Secret Access Key"
                        }
                        input {
                            r#type: if *show_secrets.read() { "text" } else { "password" },
                            autocomplete: "off",
                            class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500 font-mono",
                            placeholder: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
                            value: "{cloud_secret_key}",
                            oninput: move |e| cloud_secret_key.set(e.value()),
                        }
                    }
                }

                div { class: "space-y-3",
                    label { class: "flex items-start gap-3 cursor-pointer",
                        input {
                            r#type: "checkbox",
                            class: "rounded text-indigo-600 focus:ring-indigo-500 bg-gray-700 border-gray-600 mt-0.5",
                            checked: *encrypted.read(),
                            onchange: move |e| encrypted.set(e.checked()),
                        }
                        div {
                            span { class: "text-white block", "Encrypted" }
                            span { class: "text-xs text-gray-500",
                                "AES-256 encryption. Data is unreadable without your key."
                            }
                        }
                    }
                }

                div {
                    label { class: "flex items-center gap-2 cursor-pointer",
                        input {
                            r#type: "checkbox",
                            class: "rounded text-indigo-600 focus:ring-indigo-500 bg-gray-700 border-gray-600",
                            checked: *is_default.read(),
                            onchange: move |e| is_default.set(e.checked()),
                        }
                        span { class: "text-white", "Set as default" }
                    }
                }

                if let Some(error) = validation_error.read().as_ref() {
                    div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                        "{error}"
                    }
                }

                div { class: "flex gap-3 pt-2",
                    button {
                        class: "px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-500 transition-colors",
                        onclick: handle_save,
                        "Save"
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
