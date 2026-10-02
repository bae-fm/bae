//! The import list a fixture names: watched folders whose last read found
//! the candidates it lists, each in the state the fixture gives it, written
//! as the scan and identification write their rows.

use super::*;
use crate::identify::TerminalVerdict;
use crate::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, ReleaseFileScope, ScanItem,
    ScannedAudio, ScannedFile,
};
use crate::import::Catalog;

/// A watched folder of a [`LibraryFixture`] and what its last read found.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FixtureWatchedFolder {
    /// Where the folder is. Nothing need be on disk there: the library holds
    /// what the fixture says the folder's last read found.
    pub path: String,
    /// The candidates the import list lists under the folder.
    pub candidates: Vec<FixtureCandidate>,
}

/// One candidate of a [`FixtureWatchedFolder`]: a folder of FLAC tracks, in
/// one state of the import list's Found tab.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FixtureCandidate {
    /// The candidate's folder, `/`-separated below the watched folder.
    pub folder: String,
    /// The file names of its tracks, in order.
    pub tracks: Vec<String>,
    pub state: FixtureCandidateState,
}

/// The state a [`FixtureCandidate`] is in, each one the import list's Found
/// tab reads from the stored rows: what identification and import left
/// behind. Identifying and importing are what is running for a candidate,
/// which no stored row holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureCandidateState {
    /// Nothing looked the folder up.
    NotLookedUp,
    /// The lookup found the folder in no catalog.
    NeedsYou,
    /// Linked to a MusicBrainz release titled as the folder, its draft read
    /// from that release.
    Identified,
    /// The lookup found one release for the folder, and the candidate is
    /// linked to none.
    Unmatched,
    /// A catalog could not be reached.
    LookupError,
    /// bae broke on its own side and the lookup ended there.
    Error,
    /// The last import of the candidate failed.
    ImportError,
}

/// The made-up failure a [`FixtureCandidateState::Error`] candidate states.
pub const FIXTURE_LOOKUP_FAILURE: &str = "the fixture's lookup failed";
/// The made-up failure a [`FixtureCandidateState::ImportError`] candidate
/// states.
pub const FIXTURE_IMPORT_FAILURE: &str = "the fixture's import failed";

impl LibraryManager {
    /// Watch `folders`, each read once and finding its candidates, and put
    /// each candidate in its state. `file_count` numbers every track file the
    /// fixture writes, so each one is a different size and no two candidates
    /// share their content.
    pub(super) async fn write_fixture_watched_folders(
        &self,
        folders: &[FixtureWatchedFolder],
    ) -> Result<(), LibraryFixtureError> {
        let mut file_count = 0;
        for folder in folders {
            let root = crate::import::watched_folder::canonical_absolute_root(&folder.path)
                .map_err(|error| LibraryFixtureError::Invalid(error.to_string()))?;
            self.database
                .add_watched_import_folder(&root)
                .await
                .map_err(LibraryError::from)?;
            let generation = self
                .database
                .begin_folder_scan(&root, crate::import::VolumeKind::Local)
                .await
                .map_err(LibraryError::from)?;
            let candidates: Vec<(FolderCandidate, FixtureCandidateState)> = folder
                .candidates
                .iter()
                .map(|candidate| {
                    let scanned = fixture_candidate(&root, candidate, &mut file_count);
                    (scanned, candidate.state)
                })
                .collect();
            for (candidate, _) in &candidates {
                self.database
                    .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
                    .await
                    .map_err(LibraryError::from)?;
            }
            self.database
                .finish_folder_scan(&root, generation, None)
                .await
                .map_err(LibraryError::from)?;
            for (candidate, state) in &candidates {
                self.write_fixture_candidate_state(candidate, *state)
                    .await?;
            }
        }
        Ok(())
    }

    /// Store what puts `candidate` in `state`, as identification and import
    /// store it.
    async fn write_fixture_candidate_state(
        &self,
        candidate: &FolderCandidate,
        state: FixtureCandidateState,
    ) -> Result<(), LibraryFixtureError> {
        let content_hash = candidate.files.content_hash();
        let track_count = candidate.files.files.len() as u32;
        match state {
            FixtureCandidateState::NotLookedUp => Ok(()),
            FixtureCandidateState::NeedsYou => {
                self.store_fixture_verdict(
                    candidate,
                    TerminalVerdict::NotFoundAnywhere { ledger: None },
                )
                .await
            }
            FixtureCandidateState::Unmatched => {
                let release = fixture_release(candidate, self.ids.new_id());
                self.store_fixture_verdict(
                    candidate,
                    TerminalVerdict::of_pick(release, track_count),
                )
                .await
            }
            FixtureCandidateState::LookupError => {
                self.store_fixture_verdict(
                    candidate,
                    TerminalVerdict::Failed {
                        failures: vec![crate::identify::IdentifyFailure::DiscId(
                            crate::signals::LookupFailure::Network,
                        )],
                        findings: crate::identify::Findings::default(),
                        track_count,
                        ledger: None,
                    },
                )
                .await
            }
            FixtureCandidateState::Error => {
                self.store_fixture_verdict(
                    candidate,
                    TerminalVerdict::Error {
                        failure: crate::signals::InternalFailure {
                            detail: FIXTURE_LOOKUP_FAILURE.to_string(),
                        },
                    },
                )
                .await
            }
            FixtureCandidateState::Identified => {
                let release_id = self.ids.new_id();
                let record = crate::import::MetadataRef::new(Catalog::MusicBrainz, &release_id);
                let document = musicbrainz_release(candidate, &release_id);
                let release = crate::import::payloads::ReleasePayloads::for_test(
                    record.clone(),
                    document.to_string(),
                    Vec::new(),
                )
                .extract()
                .map_err(|error| LibraryFixtureError::Invalid(error.to_string()))?;
                self.database
                    .save_source_release(&release)
                    .await
                    .map_err(LibraryError::from)?;
                // The draft as a pick reads it from the release: titled as it.
                let mut draft = self
                    .database
                    .load_import_candidate_pane_rows(&content_hash)
                    .await
                    .map_err(LibraryError::from)?
                    .draft
                    .release_edit();
                draft.album_title = candidate.name.clone();
                self.preparations
                    .replace_metadata(
                        &content_hash,
                        &candidate.path.to_string_lossy(),
                        &draft,
                        Some(&crate::import::MetadataProvenance::ExternalRelease {
                            record: record.clone(),
                        }),
                    )
                    .await?;
                self.preparations
                    .replace_link(
                        &content_hash,
                        Some(&crate::import::ReleaseLink::Pressing(
                            crate::import::PressingLink {
                                record,
                                partners: Vec::new(),
                            },
                        )),
                    )
                    .await?;
                Ok(())
            }
            FixtureCandidateState::ImportError => {
                self.database
                    .save_import_candidate_failure(
                        &content_hash,
                        candidate.file_edit_revision,
                        &crate::import::ImportFailure {
                            reason: crate::import::ImportFailureReason::Error {
                                detail: FIXTURE_IMPORT_FAILURE.to_string(),
                            },
                            failed_at: self.clock.now(),
                            artist_identity_conflict: None,
                        },
                    )
                    .await
                    .map_err(LibraryError::from)?;
                Ok(())
            }
        }
    }

    /// Store `verdict` for `candidate` as a run that read nothing from its
    /// files stores it.
    async fn store_fixture_verdict(
        &self,
        candidate: &FolderCandidate,
        verdict: TerminalVerdict,
    ) -> Result<(), LibraryFixtureError> {
        let stored = self
            .preparations
            .store_verdict(&crate::db::NewImportCandidateVerdict {
                content_hash: candidate.files.content_hash(),
                file_edit_revision: candidate.file_edit_revision,
                folder_path: candidate.path.to_string_lossy().into_owned(),
                verdict,
                signals: crate::signals::Signals {
                    origin: crate::signals::AudioOrigin::default(),
                    disc_id: crate::signals::DiscIdSignal::Absent,
                    barcode: crate::signals::BarcodeSignal::Absent,
                    text: crate::signals::TextSignal::Settled {
                        catalogs: Vec::new(),
                        free_text: Vec::new(),
                    },
                    text_pool: Vec::new(),
                    isrcs: Vec::new(),
                    track_titles: Vec::new(),
                },
                pick: None,
            })
            .await?;
        if !stored {
            return Err(LibraryFixtureError::Invalid(format!(
                "the verdict of {} did not land on its candidate",
                candidate.path.display()
            )));
        }
        Ok(())
    }
}

/// `candidate` as the scan reads it under `root`: its tracks FLAC files of
/// three minutes, each sized by the running `file_count`.
fn fixture_candidate(
    root: &str,
    candidate: &FixtureCandidate,
    file_count: &mut u64,
) -> FolderCandidate {
    let path = candidate
        .folder
        .split('/')
        .fold(PathBuf::from(root), |folder, part| folder.join(part));
    let files = candidate
        .tracks
        .iter()
        .map(|track| {
            *file_count += 1;
            let mut file =
                ScannedFile::new(path.join(track), track.clone(), 1_000 + *file_count, 1);
            file.source_audio = Some(ScannedAudio {
                content_type: crate::util::content_type::ContentType::Flac,
                duration_ms: 180_000,
                format: crate::album_detail::AudioFormat {
                    codec: crate::util::content_type::ContentType::Flac
                        .display_name()
                        .to_string(),
                    sample_rate_hz: 44_100,
                    bits_per_sample: Some(16),
                    bitrate_kbps: None,
                    channels: 2,
                },
            });
            CandidateFile {
                file,
                role: FileRole::Audio,
            }
        })
        .collect();
    FolderCandidate {
        path: path.clone(),
        file_root: path,
        name: candidate.folder.clone(),
        files: CategorizedFiles {
            files,
            parts: Vec::new(),
        },
        watched_folder_path: root.to_string(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: candidate.folder.clone(),
        grouping: None,
    }
}

/// A MusicBrainz release `release_id` titled as `candidate`'s folder, listing
/// its tracks.
fn fixture_release(
    candidate: &FolderCandidate,
    release_id: String,
) -> crate::import::search::MetadataResult {
    crate::import::search::MetadataResult {
        source: Catalog::MusicBrainz,
        release_id,
        title: candidate.name.clone(),
        artist: None,
        year: None,
        labels: Vec::new(),
        area: None,
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        barcodes: Vec::new(),
        media: crate::pressing::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: None,
        album_links: crate::import::album_links::AlbumLinks::NotAsked,
        source_tracks: Some(crate::import::search::SourceTracks::Listed {
            count: candidate.files.files.len() as u32,
        }),
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes: Vec::new(),
    }
}

/// The MusicBrainz document of release `release_id`, titled as `candidate`'s
/// folder, with one track per track file.
fn musicbrainz_release(candidate: &FolderCandidate, release_id: &str) -> serde_json::Value {
    let tracks: Vec<serde_json::Value> = candidate
        .files
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            serde_json::json!({
                "id": format!("{release_id}-track-{index}"),
                "position": index + 1,
                "number": (index + 1).to_string(),
                "title": file.file.file_name,
                "recording": { "id": format!("{release_id}-recording-{index}"), "title": file.file.file_name },
            })
        })
        .collect();
    serde_json::json!({
        "id": release_id,
        "title": candidate.name,
        "artist-credit": [{ "name": "Artist Name", "artist": { "id": format!("{release_id}-artist"), "name": "Artist Name" } }],
        "cover-art-archive": { "count": 0, "artwork": false, "front": false, "back": false, "darkened": false },
        "media": [{ "position": 1, "tracks": tracks }],
    })
}
