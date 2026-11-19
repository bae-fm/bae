//! What each part of the pane reads for one candidate, through the domains'
//! own writes: the folder's counts and audio, the library albums that hold
//! the linked release, the album an import files under, the failures the
//! pane says in lines of their own, and the covers the candidate's picker
//! offers.

use super::*;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::records::{LookupRef, MetadataRef, ReleaseRecord};

/// The release `mb-linked`, which MusicBrainz files under the group
/// `mb-group` and whose document says the Cover Art Archive serves its
/// front.
fn linked_release() -> bae_catalogs::source_release::SourceRelease {
    let mut release = musicbrainz_release("mb-linked", "Linked Album");
    release["release-group"] = serde_json::json!({ "id": "mb-group" });
    release["cover-art-archive"] = serde_json::json!({
        "count": 1, "artwork": true, "front": true, "back": false, "darkened": false
    });
    bae_catalogs::payloads::ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::MusicBrainz, "mb-linked"),
        release.to_string(),
        Vec::new(),
    )
    .extract()
    .unwrap()
}

/// A scanned folder linked to `mb-linked`, its draft read from that release.
async fn linked(db: &Store, root: &std::path::Path) -> crate::folder_scanner::FolderCandidate {
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = scanned(db, root, "Album").await;
    let hash = candidate.files.content_hash();
    tables.save_source_release(&linked_release()).await.unwrap();
    let record = LookupRef::new(LookupCatalog::MusicBrainz, "mb-linked");
    let draft = tables.load_import_candidate_pane_rows(&hash).await.unwrap().draft.release_edit();
    tables
        .replace_metadata(
            &hash,
            &candidate.key(),
            &draft,
            Some(&MetadataProvenance::ExternalRelease { record: record.clone() }),
            &[],
        )
        .await
        .unwrap();
    tables
        .replace_link(
            &hash,
            Some(&crate::ReleaseLink::Pressing(bae_identify::PressingLink {
                record,
                partners: vec![],
            })),
            &[],
        )
        .await
        .unwrap();
    candidate
}

/// A library album titled `title`, made at `created_at`, holding one release
/// that `record` describes.
async fn library_album(
    db: &Store,
    title: &str,
    created_at: &str,
    record: ReleaseRecord,
) -> bae_records::DbAlbum {
    let records = bae_records::test_fixtures::test_records(db);
    let created_at = bae_util::timestamp::parse_rfc3339(created_at).unwrap();
    let album = bae_records::DbAlbum {
        id: bae_util::test_ids::test_uuid(title),
        title: title.to_string(),
        year: None,
        is_compilation: false,
        created_at,
    };
    records.insert_album(&album).await.unwrap();
    let release = bae_records::DbRelease {
        id: bae_util::test_ids::test_uuid(&format!("{title} release")),
        album_id: album.id.clone(),
        release_name: None,
        pressing: bae_types::pressing::Pressing::blank(),
        source_folder_name: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at,
    };
    records.insert_release(&release).await.unwrap();
    records.insert_release_records(&release.id, &[record]).await.unwrap();
    album
}

/// A MusicBrainz pressing `key` of the group `mb-group`.
fn group_pressing(key: &str) -> ReleaseRecord {
    ReleaseRecord::new(&MetadataRef::new(Catalog::MusicBrainz, key), Some("mb-group".to_string()))
}

async fn pane_draft(
    db: &Store,
    candidate: &crate::folder_scanner::FolderCandidate,
) -> DraftProjection {
    crate::tables::ImportTables::new(db.clone())
        .load_import_candidate(&candidate.key())
        .await
        .unwrap()
        .expect("the scanned candidate has a pane")
        .draft()
        .expect("a candidate not imported holds a draft")
        .clone()
}

/// The pane counts the tracks and summarizes the audio of the files the scan
/// stored, and the cover picker lists the folder's images it can show.
#[tokio::test]
async fn the_pane_counts_and_summarizes_the_folder_s_files() {
    use crate::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScannedFile};
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let folder = folder_below(&root, "Album");
    let file = |name: &str, role: FileRole| CandidateFile {
        file: ScannedFile::new(folder.join(name), name.to_string(), 1_000, 1),
        role,
    };
    let audio = |name: &str| CandidateFile {
        file: ScannedFile::new(folder.join(name), name.to_string(), 1_000, 1)
            .with_test_flac_audio(),
        role: FileRole::Audio,
    };
    let candidate = candidate_with(
        &root,
        "Album",
        CategorizedFiles {
            files: vec![
                audio("01.flac"),
                audio("02.flac"),
                file("booklet.bmp", FileRole::Artwork),
                file("cover.jpg", FileRole::Artwork),
                file("drawing.svg", FileRole::Artwork),
                file("notes.txt", FileRole::Other),
                file("scan.webp", FileRole::Artwork),
            ],
            parts: Vec::new(),
        },
        crate::folder_scanner::ReleaseFileScope::Recursive,
    );
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    let detail = tables
        .load_import_candidate(&candidate.key())
        .await
        .unwrap()
        .expect("the scanned candidate has a pane")
        .resolve();

    let files = &detail.files;
    assert_eq!(files.track_count, 2);
    assert_eq!(
        files.source_audio,
        Some(bae_records::source_audio::SourceAudioSummary::Uniform {
            descriptor: bae_records::source_audio::SourceAudioDescriptor {
                layout: bae_records::source_audio::SourceAudioLayout::File,
                format: bae_types::audio_format::AudioFormat {
                    codec: "FLAC".to_string(),
                    sample_rate_hz: 44_100,
                    bits_per_sample: Some(16),
                    bitrate_kbps: None,
                    channels: 2,
                },
            },
        })
    );
    assert_eq!(
        files.cover_files.iter().map(|file| file.name.as_str()).collect::<Vec<_>>(),
        vec!["cover.jpg", "scan.webp"]
    );
    assert_eq!(
        files.images.iter().map(|image| image.file_id.as_str()).collect::<Vec<_>>(),
        vec!["booklet.bmp", "cover.jpg", "drawing.svg", "scan.webp"]
    );
    // Every image keeps its role, whether or not a picker can show it.
    let images = files
        .files
        .iter()
        .filter(|file| file.role == crate::PaneFileRole::Artwork)
        .map(|file| file.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(images, vec!["booklet.bmp", "cover.jpg", "drawing.svg", "scan.webp"]);
    assert_eq!(files.files.len(), 7);
}

/// The pane summarizes a folder whose audio files disagree by naming each
/// fact they disagree on, with the values in the order the files show them.
#[tokio::test]
async fn the_pane_summarizes_audio_files_that_disagree() {
    use crate::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScannedFile};
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let folder = folder_below(&root, "Album");
    let flac = CandidateFile {
        file: ScannedFile::new(folder.join("01.flac"), "01.flac".to_string(), 1_000, 1)
            .with_test_flac_audio(),
        role: FileRole::Audio,
    };
    let mut mp3 = ScannedFile::new(folder.join("02.mp3"), "02.mp3".to_string(), 1_000, 1);
    mp3.source_audio = Some(crate::folder_scanner::ScannedAudio {
        content_type: bae_util::content_type::ContentType::Mp3,
        container: bae_util::audio_container::AudioContainer::Mp3,
        duration_ms: 3_000,
        format: bae_types::audio_format::AudioFormat {
            codec: bae_util::content_type::ContentType::Mp3.display_name().to_string(),
            sample_rate_hz: 48_000,
            bits_per_sample: None,
            bitrate_kbps: Some(320),
            channels: 2,
        },
    });
    let candidate = candidate_with(
        &root,
        "Album",
        CategorizedFiles {
            files: vec![flac, CandidateFile { file: mp3, role: FileRole::Audio }],
            parts: Vec::new(),
        },
        crate::folder_scanner::ReleaseFileScope::Recursive,
    );
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    let detail = tables
        .load_import_candidate(&candidate.key())
        .await
        .unwrap()
        .expect("the scanned candidate has a pane")
        .resolve();

    use bae_records::source_audio::{SourceAudioDifference, SourceAudioSummary};
    assert_eq!(
        detail.files.source_audio,
        Some(SourceAudioSummary::Mixed {
            differences: vec![
                SourceAudioDifference::Codec {
                    codecs: vec!["FLAC".to_string(), "MP3".to_string()]
                },
                SourceAudioDifference::SampleRate { sample_rates_hz: vec![44_100, 48_000] },
            ],
        })
    );
}

/// The pane says which library albums hold the linked release: the album
/// holding a copy of the pressing first, then the albums holding another
/// pressing of its group.
#[tokio::test]
async fn the_pane_lists_the_library_albums_holding_the_linked_release() {
    let (db, _tmp, root) = watched_root().await;
    let candidate = linked(&db, &root).await;
    assert_eq!(
        pane_draft(&db, &candidate).await.picked_library_status,
        Some(bae_records::LibraryStatus { release_id: "mb-linked".to_string(), albums: vec![] }),
        "the library holds nothing of the group yet"
    );

    let sibling =
        library_album(&db, "Sibling Album", "2020-01-01T00:00:00Z", group_pressing("mb-other"))
            .await;
    let copy =
        library_album(&db, "Copy Album", "2021-01-01T00:00:00Z", group_pressing("mb-linked")).await;

    assert_eq!(
        pane_draft(&db, &candidate).await.picked_library_status,
        Some(bae_records::LibraryStatus {
            release_id: "mb-linked".to_string(),
            albums: vec![
                bae_records::LibraryAlbumMatch {
                    album_id: copy.id,
                    album_title: copy.title,
                    holds_release: true,
                },
                bae_records::LibraryAlbumMatch {
                    album_id: sibling.id,
                    album_title: sibling.title,
                    holds_release: false,
                },
            ],
        })
    );
}

/// With nothing picked, an import files under the album the linked release's
/// records name: one holding a copy of the pressing over one holding another
/// pressing of its group, and a new album while the library holds neither.
#[tokio::test]
async fn the_pane_names_the_album_the_linked_release_s_records_name() {
    let (db, _tmp, root) = watched_root().await;
    let candidate = linked(&db, &root).await;
    let named = |album: &bae_records::DbAlbum| crate::DraftAlbum::Named {
        album_id: album.id.clone(),
        line: crate::AlbumLine::TitleAlone { title: album.title.clone() },
    };
    assert_eq!(pane_draft(&db, &candidate).await.album, crate::DraftAlbum::New);

    let sibling =
        library_album(&db, "Sibling Album", "2020-01-01T00:00:00Z", group_pressing("mb-other"))
            .await;
    assert_eq!(pane_draft(&db, &candidate).await.album, named(&sibling));

    let copy =
        library_album(&db, "Copy Album", "2021-01-01T00:00:00Z", group_pressing("mb-linked")).await;
    assert_eq!(pane_draft(&db, &candidate).await.album, named(&copy));
}

/// A ledger whose Disc ID cell failed with HTTP 503 and whose Discogs cell
/// for catalog number AB-1 timed out.
fn failing_ledger() -> bae_identify::IdentifyRunView {
    use bae_catalogs::LookupFailure;
    bae_identify::IdentifyRunView {
        providers: vec![LookupCatalog::MusicBrainz],
        disc_id: bae_identify::DiscIdStepView::Read {
            discs: vec![bae_identify::DiscIdRow {
                disc_id: "disc-1".to_string(),
                lookup: bae_identify::LookupView::Failed {
                    failure: LookupFailure::Provider { status: Some(503) },
                },
            }],
        },
        barcode: bae_identify::BarcodeStepView::Absent,
        catalog: bae_identify::CatalogStepView::Numbers {
            scanning: false,
            rows: vec![bae_identify::SignalValueRow {
                value: "AB-1".to_string(),
                excluded: false,
                cells: vec![
                    bae_identify::ProviderCell {
                        source: LookupCatalog::MusicBrainz,
                        lookup: bae_identify::LookupView::NoMatch,
                    },
                    bae_identify::ProviderCell {
                        source: LookupCatalog::Discogs,
                        lookup: bae_identify::LookupView::Failed {
                            failure: LookupFailure::Timeout,
                        },
                    },
                ],
            }],
            candidates: Vec::new(),
        },
        isrc: bae_identify::IsrcStepView::Absent,
        search: bae_identify::SearchStepView::NotNeeded,
    }
}

/// The pane shows a failed verdict's failures on lines of their own only when
/// no cell of its ledger shows them: the Disc ID lookup's failure and the
/// catalog number's are on their cells, and the cover's has no cell.
#[tokio::test]
async fn the_pane_says_only_the_failures_its_ledger_does_not_show() {
    use bae_catalogs::search::SourceFailure;
    use bae_catalogs::LookupFailure;
    use bae_identify::IdentifyFailure;
    let (db, _tmp, root) = watched_root().await;
    let candidate = scanned(&db, &root, "Album").await;
    let ledger = failing_ledger();
    let cover = IdentifyFailure::Cover(SourceFailure {
        source: Catalog::MusicBrainz,
        failure: LookupFailure::Network,
    });
    store_verdict(
        &db,
        &candidate,
        TerminalVerdict::Failed {
            failures: vec![
                IdentifyFailure::DiscId(LookupFailure::Provider { status: Some(503) }),
                // The same lookup failing in another way is a failure no cell shows.
                IdentifyFailure::DiscId(LookupFailure::Timeout),
                IdentifyFailure::Catalog(SourceFailure {
                    source: Catalog::Discogs,
                    failure: LookupFailure::Timeout,
                }),
                cover.clone(),
            ],
            findings: bae_identify::Findings::default(),
            track_count: 1,
            ledger: Some(ledger),
        },
    )
    .await;

    let detail = crate::tables::ImportTables::new(db.clone())
        .load_import_candidate(&candidate.key())
        .await
        .unwrap()
        .expect("the scanned candidate has a pane");

    let Some(bae_identify::IdentifyStateView::Failed {
        failure_lines: Some(failure_lines), ..
    }) = detail.stored_verdict
    else {
        panic!("a failed verdict resumes failed with lines, got {:?}", detail.stored_verdict);
    };
    assert_eq!(failure_lines.all, vec![IdentifyFailure::DiscId(LookupFailure::Timeout), cover]);
}

/// A failed verdict whose ledger cells show every failure has no failure
/// lines, so the pane draws no lines and no Retry beneath its band.
#[tokio::test]
async fn a_verdict_whose_cells_show_every_failure_has_no_failure_lines() {
    use bae_catalogs::search::SourceFailure;
    use bae_catalogs::LookupFailure;
    use bae_identify::IdentifyFailure;
    let (db, _tmp, root) = watched_root().await;
    let candidate = scanned(&db, &root, "Album").await;
    store_verdict(
        &db,
        &candidate,
        TerminalVerdict::Failed {
            failures: vec![
                IdentifyFailure::DiscId(LookupFailure::Provider { status: Some(503) }),
                IdentifyFailure::Catalog(SourceFailure {
                    source: Catalog::Discogs,
                    failure: LookupFailure::Timeout,
                }),
            ],
            findings: bae_identify::Findings::default(),
            track_count: 1,
            ledger: Some(failing_ledger()),
        },
    )
    .await;

    let detail = crate::tables::ImportTables::new(db.clone())
        .load_import_candidate(&candidate.key())
        .await
        .unwrap()
        .expect("the scanned candidate has a pane");

    let Some(bae_identify::IdentifyStateView::Failed { failure_lines, .. }) = detail.stored_verdict
    else {
        panic!("a failed verdict resumes failed, got {:?}", detail.stored_verdict);
    };
    assert_eq!(failure_lines, None);
}

/// A failed verdict with no ledger has no cell that shows a failure, so the
/// pane shows each failure in a line of its own. Below the offered releases,
/// the pane lists the picked release's failed fetches separately, then names
/// each other failed lookup once as a search with missing results, in the
/// order the run first hit each one.
#[tokio::test]
async fn the_pane_says_every_failure_of_a_verdict_with_no_ledger() {
    use bae_catalogs::search::SourceFailure;
    use bae_catalogs::LookupFailure;
    use bae_identify::{IdentifyFailure, LookupStep, MissingSearch};
    let (db, _tmp, root) = watched_root().await;
    let candidate = scanned(&db, &root, "Album").await;
    let failed = |source, failure| SourceFailure { source, failure };
    let cover = IdentifyFailure::Cover(failed(Catalog::MusicBrainz, LookupFailure::Network));
    let failures = vec![
        IdentifyFailure::Search(failed(Catalog::Discogs, LookupFailure::Network)),
        cover.clone(),
        IdentifyFailure::DiscId(LookupFailure::Timeout),
        IdentifyFailure::Search(failed(Catalog::Discogs, LookupFailure::Timeout)),
        IdentifyFailure::Pressing(failed(Catalog::MusicBrainz, LookupFailure::Network)),
    ];
    store_verdict(
        &db,
        &candidate,
        TerminalVerdict::Failed {
            failures: failures.clone(),
            findings: bae_identify::Findings::default(),
            track_count: 1,
            ledger: None,
        },
    )
    .await;

    let detail = crate::tables::ImportTables::new(db.clone())
        .load_import_candidate(&candidate.key())
        .await
        .unwrap()
        .expect("the scanned candidate has a pane");

    let Some(bae_identify::IdentifyStateView::Failed {
        failure_lines: Some(failure_lines), ..
    }) = detail.stored_verdict
    else {
        panic!("a failed verdict resumes failed with lines, got {:?}", detail.stored_verdict);
    };
    assert_eq!(failure_lines.all, failures);
    assert_eq!(failure_lines.pick_failures, Some(vec![cover]));
    let missing = |step, catalog| MissingSearch { step, catalog };
    assert_eq!(
        failure_lines.missing_searches,
        vec![
            missing(LookupStep::TitleSearch, Catalog::Discogs),
            missing(LookupStep::DiscId, Catalog::MusicBrainz),
            missing(LookupStep::Pressing, Catalog::MusicBrainz),
        ]
    );
}

/// A failed verdict with no failed fetch of the picked release has no block
/// of pick failures, so the pane draws no such lines and no Retry for them.
#[tokio::test]
async fn a_verdict_with_no_failed_fetch_of_its_pick_has_no_pick_failures() {
    use bae_catalogs::LookupFailure;
    use bae_identify::IdentifyFailure;
    let (db, _tmp, root) = watched_root().await;
    let candidate = scanned(&db, &root, "Album").await;
    let failures = vec![IdentifyFailure::DiscId(LookupFailure::Timeout)];
    store_verdict(
        &db,
        &candidate,
        TerminalVerdict::Failed {
            failures: failures.clone(),
            findings: bae_identify::Findings::default(),
            track_count: 1,
            ledger: None,
        },
    )
    .await;

    let detail = crate::tables::ImportTables::new(db.clone())
        .load_import_candidate(&candidate.key())
        .await
        .unwrap()
        .expect("the scanned candidate has a pane");

    let Some(bae_identify::IdentifyStateView::Failed {
        failure_lines: Some(failure_lines), ..
    }) = detail.stored_verdict
    else {
        panic!("a failed verdict resumes failed with lines, got {:?}", detail.stored_verdict);
    };
    assert_eq!(failure_lines.all, failures);
    assert_eq!(failure_lines.pick_failures, None);
}

/// The candidate's picker offers the covers the linked release states, each
/// with its downscaled copies smallest first; the picker does not offer the
/// group's front, which nothing states, beside a stated one.
#[tokio::test]
async fn the_picker_offers_the_covers_the_linked_release_states() {
    let (db, _tmp, root) = watched_root().await;
    let candidate = linked(&db, &root).await;
    let asked = crate::CoverGalleryRequest {
        owner: crate::CoverGalleryOwner::Candidate(candidate.key()),
        filter: crate::CoverFilter::Catalog(Catalog::MusicBrainz),
        focus: None,
    };
    let mut live =
        crate::tables::ImportTables::new(db.clone()).subscribe_cover_gallery(Some(asked.clone()));
    let gallery = loop {
        let event = live.next_inline().await.expect("the read runs");
        if event.request.as_ref() == Some(&asked) {
            break event.result.unwrap().expect("the live query answers the request").gallery;
        }
    };

    let front = "https://coverartarchive.org/release/mb-linked/front";
    let image = bae_catalogs::remote_image::RemoteImageSet {
        url: front.to_string(),
        downscaled: [250, 500, 1200]
            .into_iter()
            .map(|max_edge| bae_catalogs::remote_image::DownscaledCopy {
                url: format!("{front}-{max_edge}"),
                max_edge,
            })
            .collect(),
    };
    assert_eq!(
        (gallery.remote, gallery.files),
        (
            Some(crate::CoverSection {
                items: vec![crate::GalleryCover {
                    key: crate::CoverKey::Remote(front.to_string()),
                    label: crate::GalleryLabel::Remote {
                        label: bae_catalogs::remote_image::CoverLabel::Lead {
                            release_group: false
                        },
                        source: Catalog::MusicBrainz,
                        selection: crate::CoverSelection::Remote(
                            image.clone(),
                            Catalog::MusicBrainz
                        ),
                    },
                    image: crate::CoverImageSource::Remote { image },
                }],
                count: 1,
            }),
            None
        )
    );
}

/// The candidate's picker reads only that candidate's rows, so coven reruns
/// its read for a fetch of that candidate's covers and for no write of
/// another folder: the scan that finds it, and the draft and link its
/// identification stores.
#[tokio::test]
async fn another_folder_s_scan_and_link_leave_the_picker_s_read_unmarked() {
    let (db, _tmp, root) = watched_root().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let shown = linked(&db, &root).await;
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let mut live = tables.subscribe_cover_gallery(Some(crate::CoverGalleryRequest {
        owner: crate::CoverGalleryOwner::Candidate(shown.key()),
        filter: crate::CoverFilter::All,
        focus: None,
    }));
    live.next_inline().await.expect("the read runs").result.unwrap().expect("the picker reads");

    let other = crate::CandidateKey::folder(&folder_below(&root, "Other"));
    let other = key_candidate(&root, &other);
    tables.save_folder_scan_item(&root, generation, &ScanItem::Valid(other.clone())).await.unwrap();
    let hash = other.files.content_hash();
    assert_ne!(hash, shown.files.content_hash(), "the folders hold different files");
    let record = LookupRef::new(LookupCatalog::MusicBrainz, "mb-linked");
    let draft = tables.load_import_candidate_pane_rows(&hash).await.unwrap().draft.release_edit();
    tables
        .replace_metadata(
            &hash,
            &other.key(),
            &draft,
            Some(&MetadataProvenance::ExternalRelease { record: record.clone() }),
            &[],
        )
        .await
        .unwrap();
    tables
        .replace_link(
            &hash,
            Some(&crate::ReleaseLink::Pressing(bae_identify::PressingLink {
                record,
                partners: vec![],
            })),
            &[],
        )
        .await
        .unwrap();
    assert!(
        !live.is_marked_for_rerun(),
        "another folder's writes reached none of the picker's rows"
    );

    let shown_hash = shown.files.content_hash();
    let link = tables.candidate_cover_link(&shown_hash).await.unwrap();
    tables.replace_candidate_covers(&shown_hash, link, Some(Vec::new())).await.unwrap();
    assert!(live.is_marked_for_rerun(), "a fetch of the candidate's covers changes what it reads");
}
