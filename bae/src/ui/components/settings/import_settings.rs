use crate::config::use_config;
use crate::AppContext;
use dioxus::prelude::*;
use tracing::{error, info};

/// Import settings section - worker pools and chunk size
#[component]
pub fn ImportSettingsSection() -> Element {
    let config = use_config();
    let app_context = use_context::<AppContext>();

    let mut encrypt_workers = use_signal(|| config.max_import_encrypt_workers.to_string());
    let mut upload_workers = use_signal(|| config.max_import_upload_workers.to_string());
    let mut db_write_workers = use_signal(|| config.max_import_db_write_workers.to_string());
    let mut chunk_size = use_signal(|| (config.chunk_size_bytes / 1024).to_string()); // Show in KB

    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let has_changes = {
        let ew = encrypt_workers.read().parse::<usize>().unwrap_or(0);
        let uw = upload_workers.read().parse::<usize>().unwrap_or(0);
        let dw = db_write_workers.read().parse::<usize>().unwrap_or(0);
        let cs = chunk_size.read().parse::<usize>().unwrap_or(0) * 1024;

        ew != config.max_import_encrypt_workers
            || uw != config.max_import_upload_workers
            || dw != config.max_import_db_write_workers
            || cs != config.chunk_size_bytes
    };

    let save_changes = move |_| {
        let ew = encrypt_workers
            .read()
            .parse::<usize>()
            .unwrap_or(config.max_import_encrypt_workers);
        let uw = upload_workers
            .read()
            .parse::<usize>()
            .unwrap_or(config.max_import_upload_workers);
        let dw = db_write_workers
            .read()
            .parse::<usize>()
            .unwrap_or(config.max_import_db_write_workers);
        let cs = chunk_size.read().parse::<usize>().unwrap_or(1024) * 1024;

        let mut config = app_context.config.clone();

        spawn(async move {
            is_saving.set(true);
            save_error.set(None);

            config.max_import_encrypt_workers = ew;
            config.max_import_upload_workers = uw;
            config.max_import_db_write_workers = dw;
            config.chunk_size_bytes = cs;

            match config.save() {
                Ok(()) => {
                    info!("Saved import settings");
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

    let cancel_edit = move |_| {
        encrypt_workers.set(config.max_import_encrypt_workers.to_string());
        upload_workers.set(config.max_import_upload_workers.to_string());
        db_write_workers.set(config.max_import_db_write_workers.to_string());
        chunk_size.set((config.chunk_size_bytes / 1024).to_string());
        is_editing.set(false);
        save_error.set(None);
    };

    rsx! {
        div { class: "max-w-2xl",
            h2 { class: "text-xl font-semibold text-white mb-6", "Import Settings" }

            div { class: "bg-gray-800 rounded-lg p-6",
                div { class: "flex items-center justify-between mb-6",
                    div {
                        h3 { class: "text-lg font-medium text-white", "Worker Pools" }
                        p { class: "text-sm text-gray-400 mt-1", "Parallel processing for import operations" }
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
                        // Encrypt workers
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2", "Encryption Workers" }
                            input {
                                r#type: "number",
                                min: "1",
                                max: "64",
                                class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                value: "{encrypt_workers}",
                                oninput: move |e| encrypt_workers.set(e.value())
                            }
                            p { class: "text-xs text-gray-500 mt-1", "CPU-bound tasks (recommended: 2x CPU cores)" }
                        }

                        // Upload workers
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2", "Upload Workers" }
                            input {
                                r#type: "number",
                                min: "1",
                                max: "100",
                                class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                value: "{upload_workers}",
                                oninput: move |e| upload_workers.set(e.value())
                            }
                            p { class: "text-xs text-gray-500 mt-1", "I/O-bound tasks (recommended: 20)" }
                        }

                        // DB write workers
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2", "Database Writers" }
                            input {
                                r#type: "number",
                                min: "1",
                                max: "50",
                                class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                value: "{db_write_workers}",
                                oninput: move |e| db_write_workers.set(e.value())
                            }
                            p { class: "text-xs text-gray-500 mt-1", "I/O-bound tasks (recommended: 10)" }
                        }

                        // Chunk size
                        div {
                            label { class: "block text-sm font-medium text-gray-400 mb-2", "Chunk Size (KB)" }
                            input {
                                r#type: "number",
                                min: "64",
                                max: "16384",
                                step: "64",
                                class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                value: "{chunk_size}",
                                oninput: move |e| chunk_size.set(e.value())
                            }
                            p { class: "text-xs text-gray-500 mt-1", "Size of encrypted chunks (recommended: 1024 KB = 1 MB)" }
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
                            span { class: "text-gray-400", "Encryption Workers" }
                            span { class: "text-white font-mono", "{config.max_import_encrypt_workers}" }
                        }
                        div { class: "flex justify-between py-2 border-b border-gray-700",
                            span { class: "text-gray-400", "Upload Workers" }
                            span { class: "text-white font-mono", "{config.max_import_upload_workers}" }
                        }
                        div { class: "flex justify-between py-2 border-b border-gray-700",
                            span { class: "text-gray-400", "Database Writers" }
                            span { class: "text-white font-mono", "{config.max_import_db_write_workers}" }
                        }
                        div { class: "flex justify-between py-2",
                            span { class: "text-gray-400", "Chunk Size" }
                            span { class: "text-white font-mono", "{config.chunk_size_bytes / 1024} KB" }
                        }
                    }
                }

                // Note
                div { class: "mt-6 p-4 bg-gray-700/50 rounded-lg",
                    p { class: "text-sm text-gray-400",
                        "Changes take effect on the next import. Higher worker counts may improve speed but use more resources."
                    }
                }
            }
        }
    }
}
