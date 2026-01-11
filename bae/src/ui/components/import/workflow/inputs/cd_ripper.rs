//! CD ripper wrapper - handles drive detection and delegates UI to CdRipperView

use crate::cd::CdDrive;
use bae_ui::components::import::CdRipperView;
use bae_ui::display_types::CdDriveInfo;
use dioxus::prelude::*;
use std::path::PathBuf;

fn to_display_drive(drive: &CdDrive) -> CdDriveInfo {
    CdDriveInfo {
        device_path: drive.device_path.to_string_lossy().to_string(),
        name: drive.name.clone(),
    }
}

#[component]
pub fn CdRipper(on_drive_select: EventHandler<PathBuf>, on_error: EventHandler<String>) -> Element {
    let mut drives = use_signal(Vec::<CdDrive>::new);
    let mut selected_drive = use_signal(|| None::<PathBuf>);
    let mut is_scanning = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            is_scanning.set(true);
            match CdDrive::detect_drives() {
                Ok(detected_drives) => {
                    if let Some(first_drive) = detected_drives.first() {
                        let path = first_drive.device_path.clone();
                        selected_drive.set(Some(path.clone()));
                        on_drive_select.call(path.clone());
                    }
                    drives.set(detected_drives);
                }
                Err(e) => {
                    #[cfg(target_os = "macos")]
                    {
                        let error_msg = if e.to_string().contains("Permission denied")
                            || e.to_string().contains("Operation not permitted")
                        {
                            format!(
                                "Failed to access CD drive: {}\n\nOn macOS, you may need to grant Full Disk Access permission:\n1. Open System Settings → Privacy & Security → Full Disk Access\n2. Add this application to the list",
                                e,
                            )
                        } else {
                            format!("Failed to detect CD drives: {}", e)
                        };
                        on_error.call(error_msg);
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        on_error.call(format!("Failed to detect CD drives: {}", e));
                    }
                }
            }
            is_scanning.set(false);
        });
    });

    let display_drives: Vec<CdDriveInfo> = drives.read().iter().map(to_display_drive).collect();
    let selected_path = selected_drive
        .read()
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());

    rsx! {
        CdRipperView {
            is_scanning: *is_scanning.read(),
            drives: display_drives,
            selected_drive: selected_path,
            on_drive_select: move |path: String| {
                let path_buf = PathBuf::from(&path);
                selected_drive.set(Some(path_buf.clone()));
                on_drive_select.call(path_buf);
            },
        }
    }
}
