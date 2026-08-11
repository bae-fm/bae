/// A release folder with a real FLAC and a zero-byte FLAC must be
/// rejected. Mixing valid and zero-byte audio poisons the candidate.
#[test]
fn zero_byte_audio_in_release_skips_candidate() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/01.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Album/02.flac".into(),
            kind: FileKind::ZeroByteFlac,
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// A `.flac.part` sidecar next to a real FLAC suppresses the release.
#[test]
fn partial_marker_sidecar_skips_release() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/01.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Album/02.flac.part".into(),
            kind: FileKind::PartialMarker("part"),
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// A folder holding only partial markers (no real audio) must
/// also yield no candidates. Today this passes because no audio is
/// detected at all; the test pins the intent so if marker-only becomes
/// "looks like audio" we notice.
#[test]
fn partial_marker_only_no_real_audio_skips() {
    let result = run_scenario(vec![FixtureEntry::File {
        rel_path: "Album/01.flac.part".into(),
        kind: FileKind::PartialMarker("part"),
    }]);
    assert!(result.top_level_paths().is_empty());
}

/// An I/O fault while validating an audio file (here: the file vanished
/// after the tree was built) is a system error, not "corrupt" — it must
/// surface, not be collapsed to `false` and silently drop the whole
/// release while mis-logging the cause as corruption. The Ok(false)
/// corruption path (covered elsewhere) still skips the candidate.
#[test]
fn io_error_validating_audio_surfaces_not_swallowed() {
    let tree = CandidateFileIndex::new(vec![FileEntry {
        path: PathBuf::from("Album/01.flac"),
        size: 1024,
    }]);
    // fs_root is an empty dir, so Album/01.flac does not exist on disk:
    // is_valid_audio's open fails with a genuine I/O error.
    let temp = tempfile::TempDir::new().unwrap();
    let result = categorize_files_from_tree(
        &tree,
        &PathBuf::from("Album"),
        temp.path(),
        &StoredCandidateEdits::none(),
        &ScanCancellation::new(),
    );
    assert!(
        result.is_err(),
        "an I/O fault during validation must surface as an error"
    );
}

/// A loose partial-download marker sitting directly at the scan root must
/// not abort the whole scan: complete albums in sibling subfolders still
/// import. The root is a collection, not a release, so a loose marker there
/// belongs to no album and shouldn't suppress its neighbours. (A marker
/// that lives inside a release is still caught by the release-level deep
/// check.)
#[test]
fn loose_marker_at_scan_root_does_not_suppress_sibling_albums() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "loose.flac.part".into(),
            kind: FileKind::PartialMarker("part"),
        },
        FixtureEntry::File {
            rel_path: "AlbumA/01.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "AlbumB/01.flac".into(),
            kind: FileKind::Flac,
        },
    ]);
    let mut paths = result.top_level_paths();
    paths.sort();
    assert_eq!(paths, vec!["AlbumA".to_string(), "AlbumB".to_string()]);
}

/// Every supported partial-marker extension suppresses the release.
/// Table-driven: one subtest per extension.
#[test]
fn each_partial_marker_extension_skips_release() {
    for ext in ["part", "crdownload", "download", "aria2", "partial"] {
        let result = run_scenario(vec![
            FixtureEntry::File {
                rel_path: "Album/01.flac".into(),
                kind: FileKind::Flac,
            },
            FixtureEntry::File {
                rel_path: format!("Album/02.flac.{ext}"),
                kind: FileKind::PartialMarker(ext),
            },
        ]);
        assert!(
            result.top_level_paths().is_empty(),
            "extension .{ext} should suppress the candidate",
        );
    }
}

/// Partial-marker extension matching is case-insensitive.
#[test]
fn partial_marker_extension_case_insensitive() {
    for name in ["02.FLAC.PART", "03.FLAC.CRDownload"] {
        let ext = name.rsplit('.').next().unwrap();
        let result = run_scenario(vec![
            FixtureEntry::File {
                rel_path: "Album/01.flac".into(),
                kind: FileKind::Flac,
            },
            FixtureEntry::File {
                rel_path: format!("Album/{name}"),
                kind: FileKind::PartialMarker(ext),
            },
        ]);
        assert!(
            result.top_level_paths().is_empty(),
            "marker {name} should suppress the candidate",
        );
    }
}

/// A folder whose only audio is a FLAC the validator rejects — malformed
/// STREAMINFO length, or wrong magic bytes — surfaces no valid candidate.
#[test]
fn invalid_flac_audio_yields_no_candidate() {
    for kind in [FileKind::MalformedFlacStreaminfo, FileKind::BrokenFlac] {
        let result = run_scenario(vec![FixtureEntry::File {
            rel_path: "Album/01.flac".into(),
            kind,
        }]);
        assert!(
            result.top_level_paths().is_empty(),
            "{kind:?} must reject the candidate",
        );
    }
}

/// An audio-free parent emits each audio-bearing child, not the parent.
#[test]
fn sibling_audio_folders_emit_separate_candidates() {
    let mut entries = flat_audio("Collection/First", 3, FileKind::Flac);
    entries.extend(flat_audio("Collection/Second", 3, FileKind::Flac));
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    assert_eq!(top.len(), 2);
    assert!(top.iter().any(|p| p == "Collection/First"));
    assert!(top.iter().any(|p| p == "Collection/Second"));
}

/// A folder with only `.avi` yields no candidates and no diagnostic.
#[test]
fn non_audio_folder_emits_no_candidates() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Show/S01E01.avi".into(),
            kind: FileKind::Avi,
        },
        FixtureEntry::File {
            rel_path: "Show/S01E02.avi".into(),
            kind: FileKind::Avi,
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// Loose junk at the scan root (.pdf, .zip, .dmg, .jpg) is
/// ignored; a release subfolder still surfaces.
#[test]
fn loose_junk_at_scan_root_ignored() {
    let mut entries = vec![
        FixtureEntry::File {
            rel_path: "loose.pdf".into(),
            kind: FileKind::Pdf,
        },
        FixtureEntry::File {
            rel_path: "loose.zip".into(),
            kind: FileKind::Zip,
        },
        FixtureEntry::File {
            rel_path: "loose.dmg".into(),
            kind: FileKind::Dmg,
        },
    ];
    entries.extend(flat_audio("Album", 3, FileKind::Flac));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
}

// --- Within-release intricacies (C-series) ---

/// Per-track FLACs surface as TrackFiles / "FLAC".
#[test]
fn flat_flac_release_surfaces_as_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::Flac));
    let c = result.candidate("Album");
    assert_eq!(c.files.format_label, "FLAC");
    assert_eq!(c.files.audio().count(), 3);
    assert!(c.files.track_sheets().next().is_none());
}

/// A CUE next to the FLAC it names binds, and the folder is labelled
/// "CUE+FLAC".
#[test]
fn cue_flac_pair_binds_and_is_labelled() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/Album.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Album/Album.cue".into(),
            kind: FileKind::CueFor {
                stem: "Album.flac",
                n_tracks: 8,
            },
        },
    ]);
    let c = result.candidate("Album");
    assert_eq!(c.files.format_label, "CUE+FLAC");
    assert_eq!(c.files.bound_sheets().len(), 1);
}

// The CUE/APE pair is covered by
// `test_collect_release_candidate_files_cue_ape_track_count` above; not
// duplicated here.

/// MP3 tracks surface as TrackFiles / "MP3".
#[test]
fn mp3_release_surfaces_as_mp3_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::Mp3));
    let c = result.candidate("Album");
    assert_eq!(c.files.format_label, "MP3");
}

/// M4A tracks surface as TrackFiles / "M4A".
#[test]
fn m4a_release_surfaces_as_trackfiles() {
    let result = run_scenario(flat_audio("Album", 3, FileKind::M4a));
    let c = result.candidate("Album");
    assert_eq!(c.files.format_label, "M4A");
    assert_eq!(c.files.audio().count(), 3);
}

/// A multi-FILE CUE resolves as a CUE-backed release; each referenced track
/// file remains attached as an ordered source for that layout.
#[test]
fn multi_file_cue_surfaces_as_cue_backed_release() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    for i in 1..=3 {
        std::fs::write(album.join(format!("0{i}.m4a")), bytes_for(FileKind::M4a)).unwrap();
    }
    std::fs::write(
        album.join("Album.cue"),
        "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n\
             FILE \"01.m4a\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n\
             FILE \"02.m4a\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n\
             FILE \"03.m4a\" WAVE\n  TRACK 03 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();
    let candidates = scan_valid(tmp.path().to_path_buf());
    assert_eq!(candidates.len(), 1);
    let c = &candidates[0];
    assert_eq!(c.files.format_label, "CUE+ALAC");
    assert_eq!(c.files.bound_sheets().len(), 1);
    assert_eq!(
        c.files
            .audio()
            .map(|file| file.file_name.as_str())
            .collect::<Vec<_>>(),
        vec!["01.m4a", "02.m4a", "03.m4a"],
    );
    // A sheet is a sheet, never a document.
    assert!(!c.files.documents().any(|d| d.file_name == "Album.cue"));
}

/// Single-FILE CUE whose own stem differs from the audio file it names —
/// pair detection follows the CUE's `FILE` directive, not the CUE's
/// filename stem. The CUE is the source of truth for what it points at.
#[test]
fn single_file_cue_pairs_by_file_directive_not_stem() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Audio.flac"), bytes_for(FileKind::Flac)).unwrap();
    std::fs::write(
        album.join("Sheet.cue"),
        "PERFORMER \"X\"\nTITLE \"Y\"\nFILE \"Audio.flac\" WAVE\n  \
             TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
             TRACK 02 AUDIO\n    INDEX 01 03:00:00\n",
    )
    .unwrap();
    let candidates = scan_valid(tmp.path().to_path_buf());
    assert_eq!(candidates.len(), 1);
    let bound = candidates[0].files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].audio.file_name, "Audio.flac");
    assert_eq!(bound[0].file.file_name, "Sheet.cue");
}

/// A sheet whose `FILE` directive names missing audio leaves the sheet
/// unbound. The folder still imports: the sheet proposes a layout, it does not
/// dictate one.
#[test]
fn cue_referencing_missing_audio_leaves_the_sheet_unbound() {
    let mut entries = flat_audio("Album", 5, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/Album.cue".into(),
        kind: FileKind::NonPairingCue {
            n_tracks: 5,
            file_reference: "Album.flac",
        },
    });
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    build_fixture(&root, &entries);
    let items = scan_items(root);
    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(candidate.files.bound_sheets().is_empty());
            assert_eq!(candidate.files.audio().count(), 5);
        }
        ScanItem::Invalid(invalid) => panic!("must stay importable: {}", invalid.reason),
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// A release's sidecar subfolders attach to the candidate by category:
/// `booklet/*.png` becomes artwork (keeping its `booklet/` prefix), and
/// `Info/Tracklist.txt` becomes a document.
#[test]
fn subfolder_sidecars_attach_by_category() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/booklet/page1.png".into(),
            kind: FileKind::Png,
        },
        FixtureEntry::File {
            rel_path: "Album/booklet/page2.png".into(),
            kind: FileKind::Png,
        },
        FixtureEntry::File {
            rel_path: "Album/Info/Tracklist.txt".into(),
            kind: FileKind::TracklistTxt,
        },
    ]);
    let result = run_scenario(entries);
    let c = result.candidate("Album");

    let booklet_paths: Vec<_> = c
        .files
        .artwork()
        .filter(|a| a.relative_path.starts_with("booklet/"))
        .map(|a| a.relative_path.as_str())
        .collect();
    assert_eq!(booklet_paths.len(), 2, "booklet artwork: {booklet_paths:?}");

    assert!(
        c.files
            .documents()
            .any(|d| d.relative_path.ends_with("Tracklist.txt")),
        "Info/Tracklist.txt should be a document, got {:?}",
        c.files
            .documents()
            .map(|d| d.relative_path.as_str())
            .collect::<Vec<_>>(),
    );
}

/// `.md5` / `.ffp` sidecars are neither audio nor artwork nor
/// documents; they are omitted from categorization.
#[test]
fn md5_ffp_sidecars_silently_ignored() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/checksums.md5".into(),
            kind: FileKind::Md5,
        },
        FixtureEntry::File {
            rel_path: "Album/checksums.ffp".into(),
            kind: FileKind::Ffp,
        },
    ]);
    let result = run_scenario(entries);
    let c = result.candidate("Album");
    assert!(c
        .files
        .documents()
        .all(|d| { !d.file_name.ends_with(".md5") && !d.file_name.ends_with(".ffp") }));
    assert!(c
        .files
        .artwork()
        .all(|a| { !a.file_name.ends_with(".md5") && !a.file_name.ends_with(".ffp") }));
}

/// `.log` and `.m3u` surface as documents.
#[test]
fn log_m3u_attach_as_documents() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/rip.log".into(),
            kind: FileKind::Log,
        },
        FixtureEntry::File {
            rel_path: "Album/playlist.m3u".into(),
            kind: FileKind::M3u,
        },
    ]);
    let result = run_scenario(entries);
    let docs: Vec<_> = result
        .candidate("Album")
        .files
        .documents()
        .map(|d| d.file_name.as_str())
        .collect();
    for expected in ["rip.log", "playlist.m3u"] {
        assert!(docs.contains(&expected), "missing {expected} in {docs:?}");
    }
}

/// The `.bae/` subdirectory is entirely hidden from the scanner.
#[test]
fn bae_sidecar_hidden_from_scanner() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/.bae/cover-mb.jpg".into(),
        kind: FileKind::Jpeg,
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    // Nothing under .bae/ should leak into artwork or documents.
    assert!(c
        .files
        .artwork()
        .all(|a| !a.relative_path.contains(".bae/")));
    assert!(c
        .files
        .documents()
        .all(|d| !d.relative_path.contains(".bae/")));
}

/// Cyrillic path components scan cleanly and the name is
/// preserved verbatim.
#[test]
fn cyrillic_path_component_scans_cleanly() {
    let result = run_scenario(flat_audio("Studio \u{0410}lbums/Album", 3, FileKind::Flac));
    assert_eq!(result.top_level_paths(), vec!["Studio \u{0410}lbums/Album"],);
}

// ── Layer 2: combination tests ────────────────────────────────────────

/// Sibling folders with different audio layouts surface independently with
/// their own formats.
#[test]
fn sibling_folders_keep_their_own_audio_layouts() {
    let mut entries = flat_audio("Collection/Track Files", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Collection/Cue Image/Audio.flac".into(),
        kind: FileKind::Flac,
    });
    entries.push(FixtureEntry::File {
        rel_path: "Collection/Cue Image/Audio.cue".into(),
        kind: FileKind::CueFor {
            stem: "Audio.flac",
            n_tracks: 10,
        },
    });
    let result = run_scenario(entries);
    let top = result.top_level_paths();
    assert_eq!(top.len(), 2);
    let track_files = &result.candidate("Collection/Track Files").files;
    assert_eq!(track_files.format_label, "FLAC");
    assert!(track_files.bound_sheets().is_empty());
    let cue_image = &result.candidate("Collection/Cue Image").files;
    assert_eq!(cue_image.format_label, "CUE+FLAC");
    assert_eq!(cue_image.bound_sheets().len(), 1);
}

/// A partial marker nested under a release subdirectory (e.g. in
/// `booklet/`) still suppresses the whole release. This exercises the
/// deep walker check.
#[test]
fn partial_marker_in_nested_subdir_stops_release_candidate() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/booklet/02.flac.part".into(),
        kind: FileKind::PartialMarker("part"),
    });
    let result = run_scenario(entries);
    assert!(result.top_level_paths().is_empty());
}

/// A CUE lacking PERFORMER/TITLE still parses. Its `FILE` directive
/// names audio that isn't here, so it stays unbound and the three FLACs import
/// as themselves — the sheet's declared 15 tracks are a claim about audio the
/// folder doesn't have.
#[test]
fn cue_no_header_naming_absent_audio_stays_unbound() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/Album.cue".into(),
        kind: FileKind::CueNoHeader {
            n_tracks: 15,
            file_reference: "Album.flac",
        },
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c.files.bound_sheets().is_empty());
    assert_eq!(c.files.track_count(), 3);
}

/// A CUE with an unquoted FILE directive still pairs when it
/// shares a stem; when the track count exceeds on-disk audio, the
/// mismatch guard fires.
#[test]
fn cue_file_reference_with_unquoted_filename_still_parses() {
    // Stem-matched variant: pair detected even with unquoted FILE.
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Paired/Album.flac".into(),
            kind: FileKind::Flac,
        },
        FixtureEntry::File {
            rel_path: "Paired/Album.cue".into(),
            kind: FileKind::CueUnquoted {
                stem: "Album.flac",
                n_tracks: 6,
            },
        },
    ]);
    assert_eq!(result.candidate("Paired").files.bound_sheets().len(), 1);

    // Non-pairing variant: the directive names audio that isn't here, so the
    // sheet stays unbound and the folder still imports.
    let mut entries = flat_audio("Mismatch", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Mismatch/Album.cue".into(),
        kind: FileKind::CueUnquoted {
            stem: "Album.flac",
            n_tracks: 15,
        },
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Mismatch"]);
    assert!(result.candidate("Mismatch").files.bound_sheets().is_empty());
}

// ── Layer 3: edge / adversarial cases ─────────────────────────────────

/// A multi-FILE CUE referencing a missing audio file describes a layout
/// the folder can't supply, so it doesn't bind at all: a partial binding would
/// carve tracks out of audio that isn't there. The folder still imports, with
/// the present audio as one track.
#[test]
fn multi_file_cue_with_missing_secondary_file_stays_unbound() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), bytes_for(FileKind::Flac)).unwrap();
    // Hand-write the CUE because the DSL doesn't model two-FILE sheets.
    std::fs::write(
            album.join("Album.cue"),
            "PERFORMER \"X\"\nTITLE \"Y\"\nFILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\nFILE \"Missing.flac\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 05:00:00\n",
        )
        .unwrap();
    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(
                candidate.files.bound_sheets().is_empty(),
                "a sheet binds only when every FILE reference resolves",
            );
            assert_eq!(candidate.files.audio().count(), 1);
            assert_eq!(candidate.files.track_count(), 1);
        }
        ScanItem::Invalid(invalid) => panic!("must stay importable: {}", invalid.reason),
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// A sheet that will not parse is a document, not a verdict: its folder still
/// imports, and so does its sibling.
#[test]
fn unparseable_cue_lands_as_a_document_and_siblings_still_scan() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("Bad Album");
    let good = tmp.path().join("Good Album");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::create_dir_all(&good).unwrap();
    std::fs::write(bad.join("Album.flac"), bytes_for(FileKind::Flac)).unwrap();
    std::fs::write(
        bad.join("Album.cue"),
        "FILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Missing Index\"\n",
    )
    .unwrap();
    std::fs::write(good.join("01.flac"), bytes_for(FileKind::Flac)).unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 2);
    let bad = items
        .iter()
        .find_map(|item| match item {
            ScanItem::Valid(candidate) if candidate.name == "Bad Album" => Some(candidate),
            _ => None,
        })
        .expect("the folder with the unparseable sheet still imports");
    assert!(
        bad.files.documents().any(|d| d.file_name == "Album.cue"),
        "an unparseable sheet stays a document",
    );
    assert!(bad.files.track_sheets().next().is_none());
    assert_eq!(bad.files.audio().count(), 1);
    assert!(items.iter().any(|item| matches!(
        item,
        ScanItem::Valid(candidate) if candidate.name == "Good Album"
    )));
}

/// A folder with only a CUE and cover, no audio, yields nothing.
#[test]
fn folder_with_only_cue_and_no_audio() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/Album.cue".into(),
            kind: FileKind::CueFor {
                stem: "Album.flac",
                n_tracks: 5,
            },
        },
        FixtureEntry::File {
            rel_path: "Album/cover.jpg".into(),
            kind: FileKind::Jpeg,
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// A folder with audio plus `.avi` extras still produces an audio candidate.
/// surfaces, `.avi` is ignored.
#[test]
fn folder_with_audio_and_video_mixed() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/bonus.avi".into(),
        kind: FileKind::Avi,
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c.files.documents().all(|d| !d.file_name.ends_with(".avi")));
    assert!(c.files.artwork().all(|a| !a.file_name.ends_with(".avi")));
}

/// A deeply nested release under a chain of single-child wrappers
/// collapses to the leaf.
#[test]
fn deeply_nested_release_scan_root_two_levels_up() {
    let result = run_scenario(flat_audio("A/B/C/Release", 3, FileKind::Flac));
    let top = result.top_level_paths();
    assert_eq!(top, vec!["A/B/C/Release"]);
    assert_eq!(result.candidate(&top[0]).name, "Release");
}

/// A folder with unexpected file types (`.xyz`, `.sh`) alongside
/// audio: candidate surfaces, extras omitted from categorization.
#[test]
fn unexpected_file_types_silently_ignored() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/weird.xyz".into(),
            kind: FileKind::UnrecognizedFile("xyz"),
        },
        FixtureEntry::File {
            rel_path: "Album/script.sh".into(),
            kind: FileKind::UnrecognizedFile("sh"),
        },
    ]);
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c
        .files
        .documents()
        .all(|d| !d.file_name.ends_with(".xyz") && !d.file_name.ends_with(".sh")));
    // They are still listed, under the role for what the scan doesn't
    // recognize — and the release carries them like everything else.
    let other: Vec<_> = c
        .files
        .files
        .iter()
        .filter(|entry| matches!(entry.role, FileRole::Other))
        .map(|entry| entry.file.file_name.as_str())
        .collect();
    assert_eq!(other, vec!["script.sh", "weird.xyz"]);
    assert!(
        c.files.release_files().any(|f| f.file_name == "weird.xyz"),
        "the folder is the release: an unrecognized file is carried, not dropped",
    );
}

/// Zero-byte cover art is an incompleteness signal, so the release is
/// suppressed.
#[test]
fn zero_byte_cover_art_does_not_surface() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/cover.jpg".into(),
        kind: FileKind::ZeroByteJpeg,
    });
    let result = run_scenario(entries);
    assert!(result.top_level_paths().is_empty());
}

// ── Files carry roles ───────────────────────────────────────────────────
//
// Rooted in the folders that were broken, not in the model's own shape.

/// The walkthrough's folder: the sheet was written against a WAV that was later
/// encoded to FLAC. The directive names a file that is not here — a question,
/// not a verdict — so the folder imports, the sheet stays unbound, and the FLAC
/// keeps the audio role.
#[test]
fn sheet_naming_absent_audio_does_not_invalidate_the_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    std::fs::write(
        album.join("Album.cue"),
        make_cue_content("Album.wav", "Album Title"),
    )
    .unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    let candidate = match &items[0] {
        ScanItem::Valid(candidate) => candidate,
        ScanItem::Invalid(invalid) => {
            panic!(
                "folder must stay importable, got invalid: {}",
                invalid.reason
            )
        }
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    };

    let sheets: Vec<_> = candidate.files.track_sheets().collect();
    assert_eq!(sheets.len(), 1);
    assert_eq!(sheets[0].file.file_name, "Album.cue");
    assert_eq!(
        sheets[0].binding,
        &SheetBinding::Unresolved,
        "the directive names audio that is not here",
    );
    assert_eq!(
        candidate
            .files
            .audio()
            .map(|f| f.file_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Album.flac"],
    );
    assert_eq!(
        candidate.files.track_count(),
        1,
        "with no sheet bound, the image is one track",
    );
}

/// Audio no sheet references is kept, not dropped: a bound sheet plus two
/// standalone files leaves all three files on the candidate, hashed and listed.
#[test]
fn audio_no_sheet_references_survives() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    std::fs::write(
        album.join("Album.cue"),
        make_cue_content("Album.flac", "Album Title"),
    )
    .unwrap();
    std::fs::write(album.join("bonus 1.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("bonus 2.flac"), fake_flac()).unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(
        files
            .audio()
            .map(|f| f.relative_path.as_str())
            .collect::<Vec<_>>(),
        vec!["Album.flac", "bonus 1.flac", "bonus 2.flac"],
    );
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].audio.file_name, "Album.flac");

    let carried: Vec<_> = files
        .release_files()
        .map(|f| f.relative_path.as_str())
        .collect();
    for expected in ["Album.cue", "Album.flac", "bonus 1.flac", "bonus 2.flac"] {
        assert!(
            carried.contains(&expected),
            "{expected} must survive the scan, got {carried:?}",
        );
    }
}

/// A folder whose only disc-ID source is its rip log becomes a candidate, and
/// the log's TOC still yields the disc ID with the sheet unbound. Before roles
/// the scan refused the folder first, so the log never got the chance.
#[test]
fn folder_identifies_from_its_rip_log_with_the_sheet_unbound() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(fixtures.join("test_album.log"), album.join("rip.log")).unwrap();
    std::fs::copy(
        fixtures.join("flac/01 Test Track 1.flac"),
        album.join("01 Test Track 1.flac"),
    )
    .unwrap();
    std::fs::copy(
        fixtures.join("flac/02 Test Track 2.flac"),
        album.join("02 Test Track 2.flac"),
    )
    .unwrap();
    std::fs::write(
        album.join("Album.cue"),
        make_cue_content("Album.wav", "Album Title"),
    )
    .unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    assert!(
        matches!(&items[0], ScanItem::Valid(_)),
        "folder must be a candidate so its log can identify it",
    );

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert!(files.bound_sheets().is_empty());
    assert!(
        crate::import::discid::compute_discid_from_categorized(&files).is_some(),
        "the rip log's TOC still yields a disc ID with the sheet unbound",
    );
}

/// A sheet that will not parse is not a sheet: it lands as a document, and its
/// audio keeps its role.
#[test]
fn unparseable_sheet_lands_as_a_document() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    // No INDEX: the sheet does not parse.
    std::fs::write(
        album.join("Album.cue"),
        "FILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Missing Index\"\n",
    )
    .unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(
                candidate
                    .files
                    .documents()
                    .any(|d| d.file_name == "Album.cue"),
                "an unparseable sheet stays a document",
            );
            assert!(candidate.files.track_sheets().next().is_none());
            assert_eq!(candidate.files.audio().count(), 1);
        }
        ScanItem::Invalid(invalid) => {
            panic!(
                "an unparseable sheet must not invalidate: {}",
                invalid.reason
            )
        }
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// The hash covers every file the release uploads, including audio no sheet
/// references — which the either/or silently dropped from both. This is the
/// test that fails if that omission is ever reintroduced.
#[test]
fn content_hash_covers_audio_no_sheet_references() {
    let build = |dir: &Path, with_bonus: bool| {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("Album.flac"), fake_flac()).unwrap();
        std::fs::write(
            dir.join("Album.cue"),
            make_cue_content("Album.flac", "Album Title"),
        )
        .unwrap();
        if with_bonus {
            std::fs::write(dir.join("bonus.flac"), fake_flac()).unwrap();
        }
        collect_release_candidate_files_with_scope(
            dir,
            crate::import::ReleaseFileScope::Recursive,
            &StoredCandidateEdits::none(),
        )
        .expect("scan should succeed")
        .content_hash()
    };
    let tmp = tempfile::tempdir().unwrap();
    let plain = build(&tmp.path().join("Plain"), false);
    let with_bonus = build(&tmp.path().join("Bonus"), true);
    assert_ne!(
        plain, with_bonus,
        "audio no sheet references must count toward the hash",
    );
}

/// The folder is the release, so an unrecognized sidecar is carried like every
/// other file: it becomes a row the import writes, and it counts toward the
/// hash. The pair that fails if someone narrows either set back — and they must
/// stay one set, or the fingerprint stops describing the payload it identifies.
#[test]
fn an_unrecognized_sidecar_is_carried_and_hashed() {
    const SIDECARS: [&str; 5] = ["rip.accurip", "rip.ffp", "rip.md5", "rip.nfo", "rip.sfv"];

    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("Bare");
    let with_sidecars = tmp.path().join("Sidecars");
    for dir in [&bare, &with_sidecars] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("01.flac"), fake_flac()).unwrap();
    }
    for sidecar in SIDECARS {
        std::fs::write(with_sidecars.join(sidecar), b"scene notes").unwrap();
    }

    let bare_files = collect_release_candidate_files_with_scope(
        &bare,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    let sidecar_files = collect_release_candidate_files_with_scope(
        &with_sidecars,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    assert_eq!(
        sidecar_files
            .files
            .iter()
            .filter(|entry| matches!(entry.role, FileRole::Other))
            .map(|entry| entry.file.file_name.as_str())
            .collect::<Vec<_>>(),
        SIDECARS.to_vec(),
        "each sidecar is listed under the role for what the scan doesn't recognize",
    );

    // The rows the import writes come from the same iterator the hash covers.
    let carried: Vec<_> = crate::import::handle::flatten_categorized_files(&sidecar_files)
        .into_iter()
        .map(|file| file.file_name)
        .collect();
    for sidecar in SIDECARS {
        assert!(
            carried.contains(&sidecar.to_string()),
            "{sidecar} must become a file row, got {carried:?}",
        );
    }

    assert_ne!(
        bare_files.content_hash(),
        sidecar_files.content_hash(),
        "a file the release carries must count toward the hash",
    );
}

/// The directive binds, not the filename: a sheet and the audio it names pair
/// even when their names have nothing in common.
#[test]
fn a_binding_survives_a_rename_of_the_sheet() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Audio.flac"), fake_flac()).unwrap();
    std::fs::write(
        album.join("Completely Unrelated.cue"),
        make_cue_content("Audio.flac", "Album Title"),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].file.file_name, "Completely Unrelated.cue");
    assert_eq!(bound[0].audio.file_name, "Audio.flac");
    assert_eq!(
        bound[0].audio.relative_path, "Audio.flac",
        "`describes` names the audio by its file id",
    );
}

/// Widening the model must not swallow real defects: audio that will not decode
/// and a folder with no audio at all still invalidate.
#[test]
fn corrupt_audio_and_empty_folders_still_invalidate() {
    let tmp = tempfile::tempdir().unwrap();
    let corrupt = tmp.path().join("Corrupt");
    std::fs::create_dir_all(&corrupt).unwrap();
    // Non-empty so the folder is still detected as a leaf, but the bytes are
    // not FLAC — the file will not decode.
    std::fs::write(corrupt.join("01.flac"), b"not a flac at all").unwrap();
    let items = scan_items(corrupt);
    assert_eq!(items.len(), 1);
    assert!(
        matches!(
            &items[0],
            ScanItem::Invalid(InvalidCandidate {
                reason: InvalidReason::CorruptAudioFile { .. },
                ..
            })
        ),
        "corrupt audio is still a real defect",
    );

    let empty = tmp.path().join("Empty");
    std::fs::create_dir_all(&empty).unwrap();
    std::fs::write(empty.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    assert!(
        scan_items(empty.clone()).is_empty(),
        "a folder with no audio has nothing to import",
    );
    assert!(
        matches!(
            collect_release_candidate_files_with_scope(
                &empty,
                crate::import::ReleaseFileScope::Recursive,
                &StoredCandidateEdits::none()
            ),
            Err(crate::import::ImportError::InvalidFolder(
                InvalidReason::NoValidAudio
            ))
        ),
        "categorizing an audio-less folder names NoValidAudio",
    );
}

/// The scan proposes one cover from the conventional filenames; every other
/// image is artwork, and both are the release's images.
#[test]
fn the_scan_proposes_one_cover_from_the_conventional_names() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    for image in ["back.jpg", "cover.jpg", "folder.jpg"] {
        std::fs::write(album.join(image), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    }

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(
        cover_names(&files),
        vec!["cover.jpg"],
        "one image leads the release — the first conventional name in file order",
    );
    assert_eq!(files.artwork().count(), 3, "all three are still images");
}

/// A release-root image outranks a nested one. Sorting by relative path puts
/// `Artwork/front.jpg` ahead of `cover.jpg`, so taking the first conventional
/// name outright would propose the file inside the subfolder.
#[test]
fn a_root_level_cover_outranks_one_in_a_subfolder() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(album.join("Artwork")).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("Artwork/front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(album.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(cover_names(&files), vec!["cover.jpg"]);

    // With nothing at the root, the nested one leads.
    let nested_only = tmp.path().join("Nested");
    std::fs::create_dir_all(nested_only.join("Artwork")).unwrap();
    std::fs::write(nested_only.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(
        nested_only.join("Artwork/front.jpg"),
        [0xFF, 0xD8, 0xFF, 0xE0],
    )
    .unwrap();
    let files = collect_release_candidate_files_with_scope(
        &nested_only,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(cover_names(&files), vec!["front.jpg"]);
}
