//! Smart file display wrapper - handles file reading, delegates UI to SmartFileDisplayView

use super::image_lightbox::ImageLightbox;
use super::text_file_modal::TextFileModal;
use crate::ui::components::import::CategorizedFileInfo;
use crate::ui::local_file_url;
use bae_ui::components::import::SmartFileDisplayView;
use chardetng::EncodingDetector;
use dioxus::prelude::*;
use tracing::warn;

/// Read a text file with automatic encoding detection
async fn read_text_file_with_encoding(path: &str) -> Result<String, String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;

    if let Ok(content) = String::from_utf8(bytes.clone()) {
        return Ok(content);
    }

    let mut detector = EncodingDetector::new();
    detector.feed(&bytes, true);
    let encoding = detector.guess(None, true);
    let (decoded, _, had_errors) = encoding.decode(&bytes);

    if had_errors {
        warn!(
            "Decoding errors occurred while reading {} with encoding {}",
            path,
            encoding.name()
        );
    }

    Ok(decoded.into_owned())
}

#[component]
pub fn SmartFileDisplay(files: CategorizedFileInfo, folder_path: String) -> Element {
    let mut modal_state = use_signal(|| None::<(String, String)>);
    let mut lightbox_index = use_signal(|| None::<usize>);
    let mut show_other_files = use_signal(|| false);

    // Prepare image data with resolved URLs
    let image_data: Vec<(String, String)> = files
        .artwork
        .iter()
        .map(|img| {
            let path = format!("{}/{}", folder_path, img.name);
            let url = local_file_url(&path);
            (img.name.clone(), url)
        })
        .collect();

    let image_count = image_data.len();
    let current_idx = *lightbox_index.read();
    let clamped_lightbox_index = current_idx.and_then(|idx| {
        if image_count == 0 || idx >= image_count {
            lightbox_index.set(None);
            None
        } else {
            Some(idx)
        }
    });

    // Handler for text file clicks - reads file and opens modal
    let on_text_file_click = {
        let folder_path = folder_path.clone();
        move |(filename, _): (String, String)| {
            let filepath = format!("{}/{}", folder_path, filename);
            spawn(async move {
                match read_text_file_with_encoding(&filepath).await {
                    Ok(content) => {
                        modal_state.set(Some((filename, content)));
                    }
                    Err(e) => {
                        warn!("Failed to read file {}: {}", filepath, e);
                    }
                }
            });
        }
    };

    rsx! {
        SmartFileDisplayView {
            files,
            image_data: image_data.clone(),
            show_other_files: *show_other_files.read(),
            on_text_file_click: move |(name, path)| on_text_file_click((name, path)),
            on_image_click: move |idx| lightbox_index.set(Some(idx)),
            on_toggle_other_files: move |_| {
                let current = *show_other_files.read();
                show_other_files.set(!current);
            },
        }

        // Text file modal
        if let Some((filename, content)) = modal_state.read().as_ref() {
            TextFileModal {
                filename: filename.clone(),
                content: content.clone(),
                on_close: move |_| modal_state.set(None),
            }
        }

        // Image lightbox
        if let Some(clamped_idx) = clamped_lightbox_index {
            ImageLightbox {
                images: image_data.clone(),
                current_index: clamped_idx,
                on_close: move |_| lightbox_index.set(None),
                on_navigate: move |new_idx: usize| {
                    let max_idx = image_count.saturating_sub(1);
                    lightbox_index.set(Some(new_idx.min(max_idx)));
                },
            }
        }
    }
}
