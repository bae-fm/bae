use crate::config::use_config;
use crate::AppContext;
use dioxus::prelude::*;
use tracing::{error, info};

/// Cloud Storage section - S3 configuration form
#[component]
pub fn CloudStorageSection() -> Element {
    let config = use_config();
    let app_context = use_context::<AppContext>();

    // Clone config values for use in closures
    let initial_bucket = config.s3_config.bucket_name.clone();
    let initial_region = config.s3_config.region.clone();
    let initial_endpoint = config.s3_config.endpoint_url.clone().unwrap_or_default();
    let initial_access = config.s3_config.access_key_id.clone();
    let initial_secret = config.s3_config.secret_access_key.clone();

    let mut bucket_name = use_signal(|| initial_bucket.clone());
    let mut region = use_signal(|| initial_region.clone());
    let mut endpoint = use_signal(|| initial_endpoint.clone());
    let mut access_key = use_signal(|| initial_access.clone());
    let mut secret_key = use_signal(|| initial_secret.clone());

    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);
    let mut show_secrets = use_signal(|| false);

    let has_changes = {
        let ep = endpoint.read().clone();

        *bucket_name.read() != initial_bucket
            || *region.read() != initial_region
            || ep != initial_endpoint
            || *access_key.read() != initial_access
            || *secret_key.read() != initial_secret
    };

    let save_changes = move |_| {
        let new_bucket = bucket_name.read().clone();
        let new_region = region.read().clone();
        let new_endpoint = endpoint.read().clone();
        let new_access_key = access_key.read().clone();
        let new_secret_key = secret_key.read().clone();
        let mut config = app_context.config.clone();

        spawn(async move {
            is_saving.set(true);
            save_error.set(None);

            config.s3_config.bucket_name = new_bucket;
            config.s3_config.region = new_region;
            config.s3_config.endpoint_url = if new_endpoint.is_empty() {
                None
            } else {
                Some(new_endpoint)
            };
            config.s3_config.access_key_id = new_access_key;
            config.s3_config.secret_access_key = new_secret_key;

            match config.save() {
                Ok(()) => {
                    info!("Saved cloud storage settings");
                    is_editing.set(false);
                }
                Err(e) => {
                    error!("Failed to save config: {}", e);
                    save_error.set(Some(e.to_string()));
                }
            }

            is_saving.set(false);
        });
    };

    // Clone values for cancel closure
    let cancel_bucket = initial_bucket.clone();
    let cancel_region = initial_region.clone();
    let cancel_endpoint = initial_endpoint.clone();
    let cancel_access = initial_access.clone();
    let cancel_secret = initial_secret.clone();

    let cancel_edit = move |_| {
        bucket_name.set(cancel_bucket.clone());
        region.set(cancel_region.clone());
        endpoint.set(cancel_endpoint.clone());
        access_key.set(cancel_access.clone());
        secret_key.set(cancel_secret.clone());
        is_editing.set(false);
        save_error.set(None);
    };

    // Clone values for display in read-only mode
    let display_bucket = initial_bucket.clone();
    let display_region = initial_region.clone();
    let display_endpoint = config.s3_config.endpoint_url.clone();

    rsx! {
        div { class: "max-w-2xl",
            h2 { class: "text-xl font-semibold text-white mb-6", "Cloud Storage" }

            div { class: "bg-gray-800 rounded-lg p-6",
                div { class: "flex items-center justify-between mb-6",
                    div {
                        h3 { class: "text-lg font-medium text-white", "S3-Compatible Storage" }
                        p { class: "text-sm text-gray-400 mt-1", "AWS S3, MinIO, or compatible object storage" }
                    }
                    if !*is_editing.read() {
                        button {
                            class: "px-3 py-1.5 text-sm bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                            onclick: move |_| is_editing.set(true),
                            "Edit"
                        }
                    }
                }

                if *is_editing.read() {
                    div { class: "space-y-4",
                        // Bucket name
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2", "Bucket Name" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                placeholder: "my-music-bucket",
                                value: "{bucket_name}",
                                oninput: move |e| bucket_name.set(e.value())
                            }
                        }

                        // Region
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2", "Region" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                placeholder: "us-east-1",
                                value: "{region}",
                                oninput: move |e| region.set(e.value())
                            }
                        }

                        // Endpoint (optional)
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2", "Custom Endpoint (optional)" }
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                placeholder: "https://minio.example.com",
                                value: "{endpoint}",
                                oninput: move |e| endpoint.set(e.value())
                            }
                            p { class: "text-xs text-gray-500 mt-1", "Leave empty for AWS S3" }
                        }

                        // Credentials header with toggle
                        div { class: "flex items-center justify-between pt-2",
                            span { class: "text-sm font-medium text-gray-400", "Credentials" }
                            button {
                                class: "text-sm text-indigo-400 hover:text-indigo-300",
                                onclick: move |_| {
                                    let current = *show_secrets.read();
                                    show_secrets.set(!current);
                                },
                                if *show_secrets.read() { "Hide" } else { "Show" }
                            }
                        }

                        // Access key
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2", "Access Key ID" }
                            input {
                                r#type: if *show_secrets.read() { "text" } else { "password" },
                                class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500 font-mono",
                                placeholder: "AKIAIOSFODNN7EXAMPLE",
                                value: "{access_key}",
                                oninput: move |e| access_key.set(e.value())
                            }
                        }

                        // Secret key
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2", "Secret Access Key" }
                            input {
                                r#type: if *show_secrets.read() { "text" } else { "password" },
                                class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500 font-mono",
                                placeholder: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
                                value: "{secret_key}",
                                oninput: move |e| secret_key.set(e.value())
                            }
                        }

                        if let Some(error) = save_error.read().as_ref() {
                            div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                                "{error}"
                            }
                        }

                        div { class: "flex gap-3 pt-2",
                            button {
                                class: "px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-500 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: !has_changes || *is_saving.read(),
                                onclick: save_changes,
                                if *is_saving.read() { "Saving..." } else { "Save" }
                            }
                            button {
                                class: "px-4 py-2 bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                                onclick: cancel_edit,
                                "Cancel"
                            }
                        }
                    }
                } else {
                    div { class: "space-y-3",
                        div { class: "flex justify-between py-2 border-b border-gray-700",
                            span { class: "text-gray-400", "Bucket" }
                            span { class: "text-white font-mono", "{display_bucket}" }
                        }
                        div { class: "flex justify-between py-2 border-b border-gray-700",
                            span { class: "text-gray-400", "Region" }
                            span { class: "text-white font-mono", "{display_region}" }
                        }
                        div { class: "flex justify-between py-2 border-b border-gray-700",
                            span { class: "text-gray-400", "Endpoint" }
                            if let Some(ep) = &display_endpoint {
                                span { class: "text-white font-mono", "{ep}" }
                            } else {
                                span { class: "text-gray-500 italic", "AWS S3 (default)" }
                            }
                        }
                        div { class: "flex justify-between py-2",
                            span { class: "text-gray-400", "Credentials" }
                            span { class: "px-3 py-1 bg-green-900 text-green-300 rounded-full text-sm", "Configured" }
                        }
                    }
                }

                // Note
                div { class: "mt-6 p-4 bg-gray-700/50 rounded-lg",
                    p { class: "text-sm text-gray-400",
                        "Credentials are stored securely in your system's keychain (macOS) or credential manager (Windows/Linux)."
                    }
                }
            }
        }
    }
}
