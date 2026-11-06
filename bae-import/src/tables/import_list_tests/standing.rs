//! What runs for a release a test writes: its identification and its import,
//! the import's commit into the library, and the release deleted from it.

use super::*;

impl Library {
    /// Each change to the identification of the release at `display_path`,
    /// in order, each its own write.
    pub(crate) fn identification(
        &self,
        display_path: &str,
        changes: Vec<crate::candidate_runtime::IdentificationChange>,
    ) -> Write {
        let tables = self.tables();
        let key = self.key(display_path);
        Write::step(async move {
            for change in changes {
                tables.change_candidate_identifications(vec![(key.clone(), change)]).await.unwrap();
            }
        })
    }

    /// The files of the release at `display_path` as its identification
    /// names them.
    pub(crate) fn files(&self, display_path: &str) -> crate::CandidateFiles {
        crate::CandidateFiles { content_hash: self.hash(display_path), file_edit_revision: 0 }
    }

    /// A run of identification `stage` (`queued`, `running`, `finalizing`,
    /// `reading_pick`) for the release at `display_path`, the queue having
    /// admitted it unasked.
    pub(crate) fn identifying(&self, display_path: &str, stage: &str) -> Write {
        use crate::candidate_runtime::IdentificationChange;
        let run = crate::IdentifyRunId::of_number(1);
        let files = self.files(display_path);
        let mut changes = vec![IdentificationChange::Admit {
            files: files.clone(),
            admission: crate::Admission::Automatic,
        }];
        if stage != "queued" {
            changes.push(IdentificationChange::RunStarted { files, run });
        }
        match stage {
            "queued" | "running" => {}
            "finalizing" => {
                changes.push(IdentificationChange::RunAnswered { run, reads_pick: false })
            }
            "reading_pick" => {
                changes.push(IdentificationChange::RunAnswered { run, reads_pick: true })
            }
            other => panic!("no identification stands at {other}"),
        }
        self.identification(display_path, changes)
    }

    /// A run of the release at `display_path` whose answer bae could not
    /// write, saying `error`.
    pub(crate) fn not_written(&self, display_path: &str, error: &str) -> Write {
        use crate::candidate_runtime::IdentificationChange;
        let run = crate::IdentifyRunId::of_number(1);
        let files = self.files(display_path);
        self.identification(
            display_path,
            vec![
                IdentificationChange::Admit {
                    files: files.clone(),
                    admission: crate::Admission::Automatic,
                },
                IdentificationChange::RunStarted { files: files.clone(), run },
                IdentificationChange::AnswerFailed {
                    files,
                    run,
                    failure: crate::SaveFailure::NotWritten { error: error.to_string() },
                },
            ],
        )
    }

    /// The import `import-{display_path}` claiming the release at
    /// `display_path`, and moving to `step` as the import step column names
    /// it.
    pub(crate) fn importing(&self, display_path: &str, step: &str) -> Write {
        self.importing_with(display_path, step, None)
    }

    /// [`Library::importing`], the claim making `identification` with it.
    pub(crate) fn importing_with(
        &self,
        display_path: &str,
        step: &str,
        identification: Option<crate::candidate_runtime::IdentificationChange>,
    ) -> Write {
        let tables = self.tables();
        let key = self.key(display_path);
        let import_id = format!("import-{display_path}");
        let claim = Write::step(async move {
            let claim =
                tables.claim_candidate_import(&key, &import_id, identification).await.unwrap();
            assert!(
                matches!(claim, crate::tables::ImportClaim::Claimed { .. }),
                "the import claims the release"
            );
        });
        match import_step(step) {
            crate::ImportStep::Preparing(crate::PrepareStep::Queued) => claim,
            _ => claim + self.import_moved(display_path, step),
        }
    }

    /// The library opened again: nothing runs for any candidate.
    pub(crate) fn reopened(&self) -> Write {
        let tables = self.tables();
        Write::step(async move {
            tables.clear_candidate_standings().await.unwrap();
        })
    }

    /// The import that claimed the release at `display_path` moving to
    /// `step`.
    pub(crate) fn import_moved(&self, display_path: &str, step: &str) -> Write {
        let tables = self.tables();
        let key = self.key(display_path);
        let import_id = format!("import-{display_path}");
        let step = import_step(step);
        Write::step(async move {
            tables.move_candidate_import(&key, &import_id, Some(step)).await.unwrap();
        })
    }

    /// The import that claimed the release at `display_path` ending without
    /// a release.
    pub(crate) fn import_ended(&self, display_path: &str) -> Write {
        let tables = self.tables();
        let key = self.key(display_path);
        let import_id = format!("import-{display_path}");
        Write::step(async move {
            tables.move_candidate_import(&key, &import_id, None).await.unwrap();
        })
    }

    /// The failure the last import of the release at `display_path` left.
    pub(crate) fn import_failed(&self, display_path: &str) -> Write {
        let tables = self.tables();
        let hash = self.hash(display_path);
        Write::step(async move {
            tables
                .save_import_candidate_failure(
                    &hash,
                    0,
                    &crate::ImportFailure::error_only(
                        "it broke",
                        bae_store::test_fixtures::fixed_now(),
                    ),
                )
                .await
                .unwrap();
        })
    }

    /// The release at `display_path` imported by this device, as an import's
    /// commit writes it: its album and release, the release written at
    /// `created_at` (RFC 3339), and the hash of the files it was read from.
    /// What puts it on Done.
    pub(crate) fn imported(&self, display_path: &str, created_at: &str) -> Write {
        self.imported_as(display_path, created_at, "Album Title")
    }

    /// [`Library::imported`], its album titled `album_title`.
    pub(crate) fn imported_as(
        &self,
        display_path: &str,
        created_at: &str,
        album_title: &str,
    ) -> Write {
        self.import_of(
            self.candidate(display_path),
            display_path,
            created_at,
            album_title,
            Vec::new(),
        )
    }

    /// `candidate`, the release scanned at `display_path`, imported by this
    /// device as [`Library::imported`] imports one, its album titled
    /// `album_title` and the release filed under `records`.
    pub(crate) fn import_of(
        &self,
        candidate: FolderCandidate,
        display_path: &str,
        created_at: &str,
        album_title: &str,
        records: Vec<bae_catalogs::records::ReleaseRecord>,
    ) -> Write {
        let db = self.db.clone();
        let created_at = chrono::DateTime::parse_from_rfc3339(created_at)
            .expect("an RFC 3339 time")
            .with_timezone(&chrono::Utc);
        let album = bae_records::DbAlbum {
            id: self.album_id(display_path),
            title: album_title.to_string(),
            year: None,
            is_compilation: false,
            created_at,
        };
        let release = bae_records::DbRelease {
            created_at,
            ..bae_records::DbRelease::new_test(&album.id, &self.release_id(display_path))
        };
        Write::step(async move {
            commit_import(&db, &candidate, Some(&album), &release, &[], &records).await;
        })
    }

    /// The id of the album [`Self::imported`] writes for `display_path`.
    pub(crate) fn album_id(&self, display_path: &str) -> String {
        bae_util::test_ids::test_uuid(&format!("album {display_path}"))
    }

    /// The release [`Self::imported`] wrote for `display_path` deleted from
    /// the library, as the library deletes a release.
    pub(crate) fn release_deleted(&self, display_path: &str) -> Write {
        let records = self.records();
        let release_id = self.release_id(display_path);
        Write::step(async move {
            bae_records::test_fixtures::delete_release(&records, &release_id).await;
        })
    }

    /// The id of the release [`Self::imported`] writes for `display_path`.
    pub(crate) fn release_id(&self, display_path: &str) -> String {
        bae_util::test_ids::test_uuid(&format!("release {display_path}"))
    }
}

/// The candidate whose files hash `content_hash` as the pane reads it now.
pub(crate) async fn as_read(tables: &ImportTables, content_hash: &str) -> crate::CandidateAsRead {
    let state = tables
        .load_import_candidate_state(content_hash)
        .await
        .unwrap()
        .expect("a scan stored the candidate");
    crate::CandidateAsRead {
        content_hash: content_hash.to_string(),
        file_edit_revision: state.file_edits.revision,
        metadata_revision: state.metadata_revision,
    }
}

/// An import of `candidate` as this device's import commits it: the import
/// claims the candidate, then writes `album` (`None` for one the library
/// holds) and `release` with `tracks`, and records the hash of the files it
/// read, in one write.
pub(crate) async fn commit_import(
    db: &Store,
    candidate: &FolderCandidate,
    album: Option<&bae_records::DbAlbum>,
    release: &bae_records::DbRelease,
    tracks: &[crate::TrackFile],
    records: &[bae_catalogs::records::ReleaseRecord],
) {
    let tables = ImportTables::new(db.clone());
    let key = candidate.key();
    let content_hash = candidate.files.content_hash();
    let import_id = format!("import of {content_hash}");
    tables.claim_candidate_import(&key, &import_id, None).await.unwrap();
    let read = as_read(&tables, &content_hash).await;
    let mediums = [bae_records::DbReleaseMedium::new_test(&release.id)];
    tables
        .finalize_import_atomic(
            &coven::UuidIds,
            crate::tables::ImportCommitGuard::Candidate {
                candidate_key: key,
                source: crate::release_candidate::CandidateSource {
                    path: candidate.path.clone(),
                    scope: candidate.scope,
                    parts: Vec::new(),
                },
                expectation: crate::service::ImportExpectation {
                    candidate: read,
                    file_tag_snapshot: None,
                },
                import_id,
            },
            album,
            release,
            tracks,
            crate::tables::ImportRows { mediums: &mediums, records, ..Default::default() },
            Vec::new(),
            None,
            crate::tables::NewArtistImages::default(),
        )
        .await
        .unwrap();
}

/// `candidate` imported by this device into an album of its own, as an
/// import's commit writes it: the release it wrote.
pub(crate) async fn imported_release(
    db: &Store,
    candidate: &FolderCandidate,
) -> bae_records::DbRelease {
    let album = bae_records::DbAlbum {
        id: bae_util::test_ids::test_uuid(&format!("album {}", candidate.name)),
        title: "Album Title".to_string(),
        year: None,
        is_compilation: false,
        created_at: bae_store::test_fixtures::fixed_now(),
    };
    let release = bae_records::DbRelease::new_test(
        &album.id,
        &bae_util::test_ids::test_uuid(&format!("release {}", candidate.name)),
    );
    commit_import(db, candidate, Some(&album), &release, &[], &[]).await;
    release
}

/// The import step the `import_step` column names `column`.
fn import_step(column: &str) -> crate::ImportStep {
    use crate::{ImportPhase, ImportStep, PrepareStep};
    match column {
        "queued" => ImportStep::Preparing(PrepareStep::Queued),
        "validating_source_files" => ImportStep::Preparing(PrepareStep::ValidatingSourceFiles),
        "fetching_artist_pictures" => ImportStep::Preparing(PrepareStep::FetchingArtistPictures),
        "reading_files" => ImportStep::Running(ImportPhase::ReadingFiles),
        "measuring_loudness" => ImportStep::Running(ImportPhase::MeasuringLoudness),
        "finalizing" => ImportStep::Running(ImportPhase::Finalizing),
        other => panic!("no import step is called {other}"),
    }
}
