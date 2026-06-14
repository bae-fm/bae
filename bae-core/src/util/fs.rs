use std::path::{Path, PathBuf};

/// Move a directory to a new location. Tries rename (fast, same volume),
/// falls back to recursive copy + delete.
pub fn move_directory(source: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let folder_name = source
        .file_name()
        .ok_or_else(|| "Invalid source path".to_string())?;
    let dest = dest_dir.join(folder_name);

    if dest.exists() {
        return Err(format!("Destination already exists: {}", dest.display()));
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create destination directory: {e}"))?;
    }

    if std::fs::rename(source, &dest).is_err() {
        copy_dir_recursive(source, &dest).map_err(|e| format!("Failed to move directory: {e}"))?;
        std::fs::remove_dir_all(source)
            .map_err(|e| format!("Failed to clean up source after copy: {e}"))?;
    }

    Ok(dest)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}
