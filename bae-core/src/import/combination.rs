//! A selected set of folders, in the order its release will play.

use super::folder_scanner::{CategorizedFiles, FileRole, FolderCandidate, SheetBinding, SheetDisc};
use super::{AudioFile, ImportError, TrackUserEdit};
use std::collections::{BTreeMap, BTreeSet, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombinationTrackOrder {
    SeparateDiscs,
    Continuous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombinationAction {
    Combine,
    Separate,
}

/// The source revisions shown by the review. Reordering uses this exact set;
/// commit compares it with storage before creating the combined candidate.
#[derive(Debug, Clone)]
pub struct CombinationReview {
    candidates: Vec<FolderCandidate>,
}

impl CombinationReview {
    pub(crate) fn new(candidates: Vec<FolderCandidate>) -> Result<Self, ImportError> {
        CandidateCombination::prepare(&candidates, CombinationTrackOrder::SeparateDiscs)?;
        Ok(Self { candidates })
    }

    pub fn candidate_keys(&self) -> Vec<String> {
        self.candidates
            .iter()
            .map(|candidate| candidate.path.to_string_lossy().into_owned())
            .collect()
    }

    pub fn preview(
        &self,
        keys: &[String],
        order: CombinationTrackOrder,
    ) -> Result<CandidateCombination, ImportError> {
        CandidateCombination::prepare(&self.ordered_candidates(keys)?, order)
    }

    pub(crate) fn ordered_candidates(
        &self,
        keys: &[String],
    ) -> Result<Vec<FolderCandidate>, ImportError> {
        let distinct: HashSet<&str> = keys.iter().map(String::as_str).collect();
        if keys.len() != self.candidates.len() || distinct.len() != keys.len() {
            return Err(ImportError::Internal {
                detail: "the combination order must contain every selected folder once".into(),
            });
        }
        keys.iter()
            .map(|key| {
                self.candidates
                    .iter()
                    .find(|candidate| candidate.path.to_string_lossy() == key.as_str())
                    .cloned()
                    .ok_or_else(|| ImportError::Internal {
                        detail: format!("{key} was not part of the reviewed selection"),
                    })
            })
            .collect()
    }
}

/// One source folder's place in a combined release. The prefix is a release
/// file identity, not a directory created on the source filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombinationPart {
    pub candidate_key: String,
    pub folder_name: String,
    pub file_prefix: String,
    pub first_disc: u32,
    pub disc_count: u32,
    pub track_count: u32,
}

/// The same file set and physical track layout the review and commit consume.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateCombination {
    pub parts: Vec<CombinationPart>,
    pub files: CategorizedFiles,
    pub tracks: Vec<TrackUserEdit>,
}

impl CandidateCombination {
    pub(crate) fn from_stored(
        parts: Vec<CombinationPart>,
        files: CategorizedFiles,
        order: CombinationTrackOrder,
    ) -> Result<Self, ImportError> {
        let mut tracks = super::track_slots::direct_entry_track_rows(&files);
        let mut numbers = BTreeMap::<i32, i32>::new();
        for track in &mut tracks {
            let audio = track.file.as_ref().expect("physical tracks have audio");
            let part = parts
                .iter()
                .find(|part| audio.file_id().starts_with(&part.file_prefix))
                .ok_or_else(|| ImportError::Internal {
                    detail: format!(
                        "{} has no source folder in this combination",
                        audio.file_id()
                    ),
                })?;
            track.side = match (order, audio) {
                (CombinationTrackOrder::Continuous, _) => 1,
                (CombinationTrackOrder::SeparateDiscs, AudioFile::Standalone { .. }) => {
                    i32::try_from(part.first_disc).map_err(|_| ImportError::Internal {
                        detail: "combined disc number exceeds the track range".into(),
                    })?
                }
                (CombinationTrackOrder::SeparateDiscs, AudioFile::SheetSlice { .. }) => track.side,
            };
            let number = numbers.entry(track.side).or_default();
            *number = number.checked_add(1).ok_or_else(|| ImportError::Internal {
                detail: "combined track number exceeds the track range".into(),
            })?;
            track.track_number = Some(*number);
        }
        Ok(Self {
            parts,
            files,
            tracks,
        })
    }

    pub fn prepare(
        candidates: &[FolderCandidate],
        order: CombinationTrackOrder,
    ) -> Result<Self, ImportError> {
        if candidates.len() < 2 {
            return Err(ImportError::Internal {
                detail: "combining a release requires at least two folders".into(),
            });
        }
        let mut seen_keys = HashSet::new();
        let mut seen_files = HashSet::new();
        let mut parts = Vec::new();
        let mut files = Vec::new();
        let mut next_source_disc = 1u32;
        for (position, candidate) in candidates.iter().enumerate() {
            let key = candidate.path.to_string_lossy().into_owned();
            if !seen_keys.insert(key.clone()) {
                return Err(ImportError::Internal {
                    detail: format!("{key} was selected more than once"),
                });
            }
            let prefix = format!("{:02} - {}/", position + 1, candidate.name);
            super::watched_folder::validate_relative_path(prefix.trim_end_matches('/'))?;
            let part_tracks = super::track_slots::direct_entry_track_rows(&candidate.files);
            if part_tracks.is_empty() {
                return Err(ImportError::Internal {
                    detail: format!("{key} has no playable tracks to combine"),
                });
            }
            let source_discs = part_tracks
                .iter()
                .map(|track| track.side)
                .collect::<BTreeSet<_>>();
            let disc_count = u32::try_from(source_discs.len()).map_err(|_| numbering_overflow())?;
            let track_count = u32::try_from(part_tracks.len()).map_err(|_| numbering_overflow())?;
            // Physical sheet order stays distinct even when editable numbering
            // is continuous across the selected folders.
            let discs = source_discs
                .into_iter()
                .enumerate()
                .map(|(offset, original)| {
                    let offset = u32::try_from(offset).map_err(|_| numbering_overflow())?;
                    let assigned = next_source_disc
                        .checked_add(offset)
                        .ok_or_else(numbering_overflow)?;
                    i32::try_from(assigned).map_err(|_| numbering_overflow())?;
                    Ok((original, assigned))
                })
                .collect::<Result<BTreeMap<_, _>, ImportError>>()?;
            for entry in &candidate.files.files {
                if !seen_files.insert(entry.file.path.clone()) {
                    return Err(ImportError::Internal {
                        detail: format!(
                            "{} belongs to more than one selected folder",
                            entry.file.path.display()
                        ),
                    });
                }
                let mut entry = entry.clone();
                entry.file.relative_path = format!("{prefix}{}", entry.file.relative_path);
                entry.file.dir_prefix = Some(match entry.file.dir_prefix {
                    Some(directory) => format!("{prefix}{directory}"),
                    None => prefix.clone(),
                });
                if let FileRole::TrackSheet { binding, disc, .. } = &mut entry.role {
                    match binding {
                        SheetBinding::Resolved { files } => {
                            for audio in files {
                                audio.file_id = format!("{prefix}{}", audio.file_id);
                            }
                        }
                        SheetBinding::Override { file } => {
                            file.file_id = format!("{prefix}{}", file.file_id);
                        }
                        SheetBinding::Unresolved | SheetBinding::RefusedCodec { .. } => {}
                    }
                    if let SheetDisc::Disc { number } = disc {
                        let original = i32::try_from(*number).map_err(|_| numbering_overflow())?;
                        if let Some(assigned) = discs.get(&original) {
                            *number = *assigned;
                        }
                    }
                }
                files.push(entry);
            }
            parts.push(CombinationPart {
                candidate_key: key,
                folder_name: candidate.name.clone(),
                file_prefix: prefix,
                first_disc: match order {
                    CombinationTrackOrder::SeparateDiscs => next_source_disc,
                    CombinationTrackOrder::Continuous => 1,
                },
                disc_count: match order {
                    CombinationTrackOrder::SeparateDiscs => disc_count,
                    CombinationTrackOrder::Continuous => 1,
                },
                track_count,
            });
            next_source_disc = next_source_disc
                .checked_add(disc_count)
                .ok_or_else(numbering_overflow)?;
        }
        Self::from_stored(parts, CategorizedFiles { files }, order)
    }
}

fn numbering_overflow() -> ImportError {
    ImportError::Internal {
        detail: "combined disc or track number exceeds the supported range".into(),
    }
}

#[cfg(test)]
#[path = "combination_tests.rs"]
mod tests;
