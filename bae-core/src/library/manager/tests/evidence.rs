#[cfg(feature = "test-utils")]
#[tokio::test]
async fn evidence_verification_opens_the_log_with_the_stored_track_results() {
    use crate::import::Verification;
    let (manager, dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    let decoded = crate::text_encoding::decode_text(include_bytes!(
        "../../../../tests/fixtures/logs/test_album.log"
    ));
    let text = decoded.text.as_str();
    let log = crate::import::rip_log::parse_rip_log(text).unwrap();
    let verification = Verification::of(&log);
    assert!(verification.matched_copies().is_some());
    manager
        .database
        .finalize_import_atomic(
            crate::db::ImportCommitGuard::UncheckedTestSetup,
            Some(&album),
            &release,
            &[],
            crate::db::ImportRows {
                verification: Some(&verification),
                ..Default::default()
            },
            Vec::new(),
            None,
            &[],
            None,
            crate::config::HomeStorage::Opaque,
            &[],
        )
        .await
        .unwrap();
    for (name, bytes) in [
        ("toc.log", b"Unrecognized TOC-only log".as_slice()),
        ("rip.log", text.as_bytes()),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            ContentType::PlainText,
            Uuid::new_v4().to_string(),
            Utc::now(),
        );
        manager
            .add_external_file_for_test(&file, &path)
            .await
            .unwrap();
    }
    let subject = EvidenceSubject::Release { id: release.id };
    assert_eq!(
        manager
            .read_evidence(&subject, &EvidenceSelection::Verification)
            .await
            .unwrap(),
        vec![EvidenceContent::Document {
            name: "rip.log".into(),
            text: text.into()
        },]
    );
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn evidence_candidate_opens_its_recorded_cue_and_verification_log() {
    use crate::import::folder_scanner::{
        CandidateFile, CategorizedFiles, FileRole, FolderCandidate, ReleaseFileScope, ScanItem,
        ScannedFile,
    };
    use crate::import::{CandidateAsRead, CandidatePreparations, Verification};
    use crate::signals::{
        BarcodeSignal, DiscIdSignal, SignalOrigin, Signals, SourcedValue, TextSignal,
    };
    let (manager, dir) = setup_test_manager().await;
    let root = dir.path().to_str().unwrap();
    let path = dir.path().join("Album");
    std::fs::create_dir(&path).unwrap();
    let log_bytes = include_bytes!("../../../../tests/fixtures/logs/test_album.log");
    let log_text = crate::text_encoding::decode_text(log_bytes).text;
    let verification = Verification::of(&crate::import::rip_log::parse_rip_log(&log_text).unwrap());
    let cue = "CATALOG 1234567890123\n";
    let files = [
        ("rip.LOG", log_bytes.as_slice()),
        ("album.cue", cue.as_bytes()),
    ]
    .into_iter()
    .map(|(name, bytes)| {
        let file_path = path.join(name);
        std::fs::write(&file_path, bytes).unwrap();
        CandidateFile {
            file: ScannedFile::new(file_path, name.into(), bytes.len() as u64, 1),
            role: FileRole::Document,
            proposed_audio: false,
        }
    })
    .collect();
    let candidate = FolderCandidate {
        path: path.clone(),
        file_root: path.clone(),
        name: "Album".into(),
        files: CategorizedFiles { files },
        watched_folder_path: root.into(),
        scope: ReleaseFileScope::Direct,
        file_edit_revision: 0,
        display_path: "Album".into(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    };
    let key = path.to_str().unwrap().to_string();
    let content_hash = candidate.files.content_hash();
    manager
        .database
        .add_watched_import_folder(root)
        .await
        .unwrap();
    let generation = manager.database.begin_folder_scan(root).await.unwrap();
    manager
        .database
        .save_folder_scan_item(root, generation, &ScanItem::Valid(candidate))
        .await
        .unwrap();
    manager
        .database
        .finish_folder_scan(root, generation, None)
        .await
        .unwrap();
    assert!(CandidatePreparations::new(manager.database.clone())
        .store_verdict(&crate::db::NewImportCandidateVerdict {
            candidate: CandidateAsRead {
                content_hash,
                file_edit_revision: 0,
                metadata_revision: 0
            },
            folder_path: key.clone(),
            verdict: crate::identify::TerminalVerdict::NotFoundAnywhere { ledger: None },
            metadata: None,
            signals: Signals {
                disc_id: DiscIdSignal::Absent {
                    track_count: verification.tracks.len() as u32
                },
                verification: Some(verification),
                barcode: BarcodeSignal::Settled {
                    codes: vec![SourcedValue::in_file(
                        "1234567890123".into(),
                        SignalOrigin::CueSheet,
                        "album.cue".into()
                    )]
                },
                text: TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new()
                },
                text_pool: Vec::new(),
                durations: Default::default(),
            },
        })
        .await
        .unwrap());
    let subject = EvidenceSubject::Candidate { key };
    assert_eq!(
        manager
            .read_evidence(&subject, &EvidenceSelection::Verification)
            .await
            .unwrap(),
        vec![EvidenceContent::Document {
            name: "rip.LOG".into(),
            text: log_text
        }]
    );
}
