//! A library release's files' own tags, as its editor applies them: read
//! from the release's audio files where this device holds their originals,
//! and laid onto the release's tracks.

use super::ImportServiceHandle;
use std::collections::HashMap;

impl ImportServiceHandle {
    /// What the audio files of `form`'s release and its CUE sheets say about
    /// it, read as the import reads a folder's (§3.6), laid onto the form's
    /// tracks: each track takes what its file's tags, or its sheet's entry,
    /// say about it. The read refuses with
    /// [`crate::ImportError::FileNotOnThisDevice`] when this device holds no
    /// original of one of the release's audio files or sheets, and with
    /// [`crate::ImportError::MetadataTrackCount`] when the files describe
    /// another number of tracks than one file's tracks in the release.
    pub async fn release_file_tag_values(
        &self,
        form: &crate::ReleaseEditForm,
    ) -> Result<crate::LibraryDraft, crate::ImportError> {
        let release_id = form.release_id.as_str();
        let records = &self.services.records;
        let files = records.get_files_for_release(release_id).await?;
        let decisions = self.services.tables.load_release_sheet_decisions(release_id).await?;
        let mut library_files = Vec::new();
        for file in files.iter().filter(|file| {
            matches!(
                file.role,
                bae_records::ReleaseFileRole::Audio | bae_records::ReleaseFileRole::TrackSheet
            )
        }) {
            let original = records.external_file(&file.id).await?.ok_or_else(|| {
                crate::ImportError::FileNotOnThisDevice { name: file.original_filename.clone() }
            })?;
            library_files
                .push(crate::extraction::release::library_file(file.clone(), original.path));
        }
        let reader = self.services.file_tags.clone();
        let lease = bae_store::lease_one(&self.services.files).await;
        let (values, units) = self
            .services
            .diagnostics
            .blocking("read a release's file tags", move || {
                let categorized = crate::folder_scanner::categorize_library_files(
                    library_files,
                    decisions,
                    &lease,
                );
                let audio_files = categorized.audio().cloned().collect::<Vec<_>>();
                let snapshot = crate::file_tag_snapshot::extract_file_tag_snapshot(
                    &audio_files,
                    0,
                    0,
                    reader.as_ref(),
                    &lease,
                )?;
                let values =
                    crate::file_tag_mapper::file_metadata_values(&categorized, &snapshot, None)?;
                Ok::<_, crate::ImportError>((
                    values,
                    crate::audio_layout::audio_units(&categorized),
                ))
            })
            .await??;
        let names: HashMap<&str, &str> =
            files.iter().map(|file| (file.id.as_str(), file.original_filename.as_str())).collect();
        let order = tracks_by_unit(form, &names, &units)?;
        Ok(crate::reading_draft::library_draft(
            in_release_order(values, &order),
            form.edit.tracks.iter().map(|track| track.id.clone()).collect(),
        ))
    }
}

/// For each of the form's tracks, in the form's order, the index of the
/// audio unit it plays among `units`: a file's tracks pair with that file's
/// units in the order they play, a sheet's slices in the sheet's order and
/// a track's start in the file. Refused when a file's units and its tracks
/// differ in number.
fn tracks_by_unit(
    form: &crate::ReleaseEditForm,
    names: &HashMap<&str, &str>,
    units: &[crate::AudioFile],
) -> Result<Vec<usize>, crate::ImportError> {
    let mut units_of: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, unit) in units.iter().enumerate() {
        units_of.entry(unit.file_id()).or_default().push(index);
    }
    let tracks = &form.opened.display.tracks;
    let mut tracks_of: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, track) in tracks.iter().enumerate() {
        let name = names.get(track.source.file_id.as_str()).copied().ok_or_else(|| {
            crate::ImportError::Internal {
                detail: format!(
                    "release {} holds no file {}",
                    form.release_id, track.source.file_id
                ),
            }
        })?;
        tracks_of.entry(name).or_default().push(index);
    }
    let count_differs = || crate::ImportError::MetadataTrackCount {
        folder_tracks: tracks.len() as u32,
        release_tracks: units.len() as u32,
    };
    if units_of.len() != tracks_of.len() {
        return Err(count_differs());
    }
    let mut order = vec![0; tracks.len()];
    for (name, mut track_indexes) in tracks_of {
        let unit_indexes = units_of.get(name).ok_or_else(count_differs)?;
        if unit_indexes.len() != track_indexes.len() {
            return Err(count_differs());
        }
        track_indexes.sort_by_key(|&index| tracks[index].start_ms);
        for (track, unit) in track_indexes.into_iter().zip(unit_indexes) {
            order[track] = *unit;
        }
    }
    Ok(order)
}

/// `values` with its tracks, one per audio unit, put in the form's order:
/// track `i` is the one `order[i]` names.
fn in_release_order(
    mut values: bae_catalogs::release_values::ReleaseValues,
    order: &[usize],
) -> bae_catalogs::release_values::ReleaseValues {
    let mut by_unit: Vec<_> = values.tracks.drain(..).map(Some).collect();
    values.tracks = order
        .iter()
        .map(|&unit| by_unit[unit].take().expect("each unit pairs with one track"))
        .collect();
    values
}
