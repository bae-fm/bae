//! A library release's files' own tags, as its editor applies them: read from
//! its audio files on this device and its CUE sheets through coven.

use super::ImportServiceHandle;
use crate::ImportError;
use std::collections::HashMap;

impl ImportServiceHandle {
    /// What `form`'s release's audio files and CUE sheets say about each of
    /// its tracks, read as the import reads a folder's (§3.6). Refused with
    /// [`crate::ImportError::FileNotOnThisDevice`] when this device holds no
    /// original of an audio file, and [`crate::ImportError::MetadataTrackCount`]
    /// when the files describe another number of tracks than a file holds.
    pub async fn release_file_tag_values(
        &self,
        form: &crate::ReleaseEditForm,
    ) -> Result<crate::LibraryDraft, crate::ImportError> {
        let release_id = form.release_id.as_str();
        let records = &self.services.records;
        let files = records.release_audio_and_sheet_originals(release_id).await?;
        let decisions = self.services.tables.load_release_sheet_decisions(release_id).await?;
        let names: HashMap<String, String> = files
            .iter()
            .map(|(file, _)| (file.id.clone(), file.original_filename.clone()))
            .collect();
        let lease = bae_store::lease_one(&self.services.files).await;
        let sheet = |file: &bae_records::DbFile| crate::rip_reading::opens_library_file(&file.role);
        let sheets: Vec<_> = files.iter().map(|(file, _)| file).filter(|f| sheet(f)).collect();
        let documents =
            crate::library_documents::LibraryDocuments::read(records, &lease, &sheets).await?;
        let originals = files
            .iter()
            .filter(|(file, _)| !sheet(file))
            .map(|(f, original)| match original {
                Some(path) => Ok((f.original_filename.clone(), path.clone())),
                None => Err(ImportError::FileNotOnThisDevice { name: f.original_filename.clone() }),
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let library_files: Vec<_> = files
            .into_iter()
            .map(|(file, _)| crate::extraction::release::library_file(file))
            .collect();
        let reader = self.services.file_tags.clone();
        let (values, units) = self
            .services
            .diagnostics
            .blocking("read a release's file tags", move || {
                let categorized = crate::folder_scanner::categorize_library_files(
                    library_files,
                    &documents,
                    decisions,
                );
                let mut audio_files = categorized.audio().cloned().collect::<Vec<_>>();
                for file in &mut audio_files {
                    file.version.path = originals[&file.relative_path].clone();
                }
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
        let order = tracks_by_unit(form, &names, &units)?;
        Ok(crate::reading_draft::library_draft(
            in_release_order(values, &order),
            form.edit.tracks.iter().map(|track| track.id.clone()).collect(),
        ))
    }
}

/// For each of the form's tracks, in order, the index of the unit it plays
/// among `units`: a file's tracks pair with its units in play order, by start.
/// Refused when a file's units and its tracks differ in number.
fn tracks_by_unit(
    form: &crate::ReleaseEditForm,
    names: &HashMap<String, String>,
    units: &[crate::AudioFile],
) -> Result<Vec<usize>, crate::ImportError> {
    let mut units_of: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, unit) in units.iter().enumerate() {
        units_of.entry(unit.file_id()).or_default().push(index);
    }
    let tracks = &form.opened.display.tracks;
    let mut tracks_of: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, track) in tracks.iter().enumerate() {
        let name = names.get(&track.source.file_id).map(String::as_str).ok_or_else(|| {
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

/// `values` with its tracks, one per unit, in the form's order (`order[i]`).
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
