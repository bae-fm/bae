//! Export domain operations for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Reconstruct a release's original source folder on disk, byte-for-byte.
    /// Every release file (audio, CUE, LOG, cover scans, …) is written verbatim
    /// from its coven blob to `<target_dir>/<folder>/<file.original_filename>`,
    /// where `<folder>` is the release's source folder name and
    /// `original_filename` is the file's path within that folder (e.g.
    /// `CD1/CDImage.ape`). A Remote release fetches each blob from the cloud/cache.
    /// Returns the created `<target_dir>/<folder>` path.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn export_release_files(
        &self,
        release_id: &str,
        target_dir: &Path,
    ) -> Result<PathBuf, LibraryError> {
        let release = self
            .get_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("release not found: {release_id}")))?;
        let folder = release.source_folder_name.ok_or_else(|| {
            LibraryError::Import(format!(
                "release {release_id} has no source folder name; cannot reconstruct its folder"
            ))
        })?;

        let release_dir = target_dir.join(&folder);
        let files = self.database.get_files_for_release(release_id).await?;
        info!(
            release_id,
            folder = folder.as_str(),
            file_count = files.len(),
            "Exporting release files verbatim"
        );

        for file in &files {
            let bytes = self.read_release_blob(file).await?;
            let file_path = release_dir.join(&file.original_filename);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&file_path, &bytes)?;
        }

        Ok(release_dir)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn export_release(
        &self,
        release_id: &str,
        target_dir: &Path,
    ) -> Result<(), LibraryError> {
        ExportService::export_release(release_id, target_dir, self)
            .await
            .map_err(LibraryError::Import)
    }

    /// Assemble everything `ExportService::export_track` needs for a
    /// track in one pass: source audio bytes, tag fields, cover image path,
    /// neighbour counts, and the raw audio-format aggregate for decoding.
    /// Cloud-only tracks download + decrypt here — export never requires a
    /// local copy.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn get_export_track_plan(
        &self,
        track_id: &str,
    ) -> Result<ExportTrackPlan, LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;

        let audio_bytes =
            crate::storage::local::transfer::read_release_file_bytes(&meta.audio_file, self)
                .await
                .map_err(|e| {
                    LibraryError::TrackMapping(format!(
                        "Couldn't read audio for track {track_id}: {e}"
                    ))
                })?;

        let album = self.database.get_album_for_release(&meta.release).await?;

        let album_artists = self.database.get_artists_for_album(&album.id).await?;
        let artist = join_artist_names(&album_artists);

        let release_tracks = self
            .database
            .get_tracks_for_release(&meta.release.id)
            .await?;
        let total_tracks = release_tracks.len();
        let has_multiple_sides = release_tracks
            .iter()
            .map(|t| t.side)
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1;
        let disc = if has_multiple_sides {
            Some(meta.track.side)
        } else {
            None
        };

        let year = meta.release.pressing.year.or(album.year);

        let cover_image_bytes = match album.primary_release_id.as_deref() {
            Some(rid) => match self.cover_ref(rid).await? {
                Some(image) => self.read_image_blob(&image).await?,
                None => None,
            },
            None => None,
        };

        let is_digital =
            crate::util::format::is_digital_format(meta.release.pressing.format.as_deref());

        let tags = ExportTags {
            title: meta.track.title.clone(),
            artist,
            album: album.title,
            year,
            disc,
        };

        let track_number = meta.track.track_number;

        Ok(ExportTrackPlan {
            audio_bytes,
            tags,
            cover_image_bytes,
            track_number,
            total_tracks,
            is_digital,
            audio_meta: meta,
        })
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn export_track(
        &self,
        track_id: &str,
        output_path: &Path,
        format: crate::library::ExportFormat,
    ) -> Result<(), LibraryError> {
        let plan = self.get_export_track_plan(track_id).await?;
        ExportService::export_track(plan, output_path, format)
            .await
            .map_err(LibraryError::Import)
    }
}
