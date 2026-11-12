//! Binding a track sheet to its audio: what the scan proposes, what a
//! person's binding overrules, the codecs and timing a sheet can use, and
//! the discs bound sheets lay out.

use super::*;
use crate::folder_scanner::tests::*;

/// A per-track sheet whose references name nothing here — the tracks were
/// renamed after the rip — takes the audio files beside it in name order when
/// there are exactly as many of them as it has references.
#[test]
fn a_sheet_naming_nothing_takes_the_audio_beside_it_in_name_order() {
    let (_tmp, album) = album_dir();
    for name in ["01. Track One.flac", "02. Track Two.flac"] {
        copy_cue_flac(&album, name);
    }
    std::fs::write(
        album.join("album.cue"),
        "FILE \"01 -First.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n\
         FILE \"02 -Second.wav\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();

    let files = scan_files(&album);

    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Resolved {
            files: vec![
                SheetAudioFile {
                    file_reference: "01 -First.wav".to_string(),
                    file_id: "01. Track One.flac".to_string(),
                },
                SheetAudioFile {
                    file_reference: "02 -Second.wav".to_string(),
                    file_id: "02. Track Two.flac".to_string(),
                },
            ],
        },
    );
    assert_eq!(files.track_count(), 2);
}

/// The last resort needs the count to match: one reference beside two audio
/// files is not a pairing anyone asked for.
#[test]
fn a_sheet_naming_nothing_beside_a_different_count_stays_unbound() {
    let (_tmp, album) = album_dir();
    for name in ["01. Track One.flac", "02. Track Two.flac"] {
        copy_cue_flac(&album, name);
    }
    std::fs::write(
        album.join("album.cue"),
        "FILE \"Range.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();

    let files = scan_files(&album);

    assert!(files.carving_sheets().is_empty());
    assert_eq!(files.track_count(), 2);
}

/// A codec the CUE path cannot seek inside is refused where the choice is
/// offered, with the codec named — never handed to the user as a choice that
/// fails at commit. The FLAC beside it stays offerable, so this is the refusal
/// and not an empty picker.
#[test]
fn audio_a_sheet_cannot_use_is_refused_at_offer_time_with_the_codec_named() {
    let (_tmp, album) = cue_flac_album("cd.flac", "cd.wav", 12);
    std::fs::copy(
        bae_audio::test_fixtures::audio_format_dir().join("placeholder-mp3.mp3"),
        album.join("cd.mp3"),
    )
    .unwrap();

    let files = scan_files(&album);
    let options = files.sheet_binding_options("cd.cue");

    assert_eq!(
        options[0].options,
        vec![
            SheetBindingOption {
                file_id: "cd.flac".to_string(),
                offer: SheetBindingOffer::Offered,
            },
            SheetBindingOption {
                file_id: "cd.mp3".to_string(),
                offer: SheetBindingOffer::RefusedCodec { codec: "MP3".to_string() },
            },
        ],
        "the MP3 is refused with its codec named, not offered and rejected later",
    );
}

/// Clearing a binding leaves the sheet describing nothing. It does **not**
/// restore the scan's proposal: someone who cleared a binding is saying the
/// guess was wrong, and re-guessing it is the one answer that is certainly not
/// what they asked for.
#[test]
fn clearing_a_binding_leaves_it_unbound_rather_than_re_guessed() {
    let (_tmp, album) = cue_flac_album("cd.flac", "cd.wav", 12);

    let proposed = scan_files(&album);
    assert_eq!(
        proposed.track_count(),
        12,
        "the unique same-stem audio makes the scan propose the binding",
    );

    let cleared = scan_with_binding(&album, &proposed, "cd.cue", None);

    assert_eq!(
        cleared.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved { files: Vec::new() },
        "the sheet the user cleared describes nothing, proposal or not",
    );
    assert_eq!(cleared.track_count(), 1);
    assert_uniform_source_audio(
        &cleared,
        bae_records::source_audio::SourceAudioLayout::File,
        "FLAC",
    );
    assert!(cleared.bound_sheets().is_empty());
}

/// A binding whose audio leaves the folder is not silently kept. Removing the
/// file changes the file set, so it changes the hash the decision is stored
/// under, so the decision is unreachable and the candidate derives from what is
/// actually there. The behaviour is what matters; the hash is only how.
#[test]
fn a_binding_whose_audio_disappears_is_not_kept() {
    let (_tmp, album) = cue_flac_album("cd.flac", "cd.wav", 12);
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        album.join("bonus.flac"),
    )
    .unwrap();

    let scanned = scan_files(&album);
    let stored = stored_binding(&scanned, "cd.cue", Some("cd.flac"));
    assert_eq!(
        collect_release_candidate_files_with_scope(
            &album,
            crate::ReleaseFileScope::Recursive,
            &stored
        )
        .expect("scan")
        .track_count(),
        13,
        "the binding contributes twelve tracks alongside the loose bonus track",
    );

    std::fs::remove_file(album.join("cd.flac")).unwrap();

    let after = collect_release_candidate_files_with_scope(
        &album,
        crate::ReleaseFileScope::Recursive,
        &stored,
    )
    .expect("scan");
    assert_eq!(
        after.track_sheets().next().unwrap().binding,
        &SheetBinding::Resolved {
            files: vec![SheetAudioFile {
                file_reference: "cd.wav".to_string(),
                file_id: "bonus.flac".to_string(),
            }],
        },
        "the folder derives from what is on disk, with no memory of the removed pairing: \
         the one audio file left beside the sheet is what it takes",
    );
}

/// The stored bindings a fresh scan of `folder` would apply, if the user had
/// made this one decision about `files`.
fn stored_binding(
    files: &CategorizedFiles,
    sheet_file_id: &str,
    audio_file_id: Option<&str>,
) -> StoredCandidateEdits {
    let mut edits = SheetBindingEdits::default();
    edits.set_reference(
        sheet_file_id.to_string(),
        files
            .track_sheets()
            .find(|sheet| sheet.file.relative_path == sheet_file_id)
            .unwrap()
            .sheet
            .audio_file_references()[0]
            .to_owned(),
        match audio_file_id {
            Some(file_id) => UserSheetBinding::Describes { file_id: file_id.to_string() },
            None => UserSheetBinding::Cleared,
        },
    );
    StoredCandidateEdits::new(HashMap::from([(
        files.content_hash(),
        CandidateFileEdits { sheet_bindings: edits, ..Default::default() },
    )]))
}

/// Re-scan `folder` as it reads once the user has made one binding decision.
fn scan_with_binding(
    folder: &Path,
    files: &CategorizedFiles,
    sheet_file_id: &str,
    audio_file_id: Option<&str>,
) -> CategorizedFiles {
    collect_release_candidate_files_with_scope(
        folder,
        crate::ReleaseFileScope::Recursive,
        &stored_binding(files, sheet_file_id, audio_file_id),
    )
    .expect("scan")
}

/// Two sheets for one image is the usual rip leftover, so the first in file
/// order carves it and the other is ignored rather than both being set aside.
#[test]
fn the_first_of_competing_cues_carves_the_audio() {
    let (_tmp, album) = cue_flac_album("cd.flac", "cd.flac", 3);
    std::fs::write(
        album.join("alternative.cue"),
        make_cue_content_n_tracks("cd.flac", "Album Title", 2),
    )
    .unwrap();
    let files = scan_files(&album);
    assert_eq!(files.track_count(), 2, "alternative.cue sorts first and carves");
    assert_eq!(files.carving_sheets().len(), 1);
    assert_eq!(files.bound_sheets().len(), 2);
}

#[test]
fn multi_file_cue_offers_bindings_for_each_reference() {
    let (_tmp, album) = album_dir();
    copy_cue_flac(&album, "first.flac");
    copy_cue_flac(&album, "second.flac");
    std::fs::write(album.join("disc.cue"),
        "FILE \"first.flac\" WAVE\n TRACK 01 AUDIO\n INDEX 01 00:00:00\nFILE \"missing.wav\" WAVE\n TRACK 02 AUDIO\n INDEX 01 00:00:00\n").unwrap();
    let files = scan_files(&album);
    assert_eq!(files.track_count(), 2);
    assert!(!files.sheet_binding_options("disc.cue").is_empty());
}

/// Nobody has said which cue is which disc, so each bound sheet takes its
/// position among the folder's bound sheets, in path order. An unbound sheet
/// takes no place in that count — it carves nothing either way.
#[test]
fn bound_sheets_take_their_positions_as_discs_by_default() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    std::fs::write(root.join("alpha.flac"), fake_flac()).unwrap();
    std::fs::write(
        root.join("alpha.cue"),
        make_cue_content_n_tracks("alpha.flac", "Album Title", 2),
    )
    .unwrap();
    std::fs::write(root.join("beta.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("beta.cue"), make_cue_content_n_tracks("beta.flac", "Album Title", 3))
        .unwrap();
    // Names audio the folder does not hold, so it never binds.
    std::fs::write(root.join("zeta.cue"), make_cue_content_n_tracks("zeta.flac", "Album Title", 4))
        .unwrap();

    let files = scan_files(root);

    assert_eq!(
        files
            .track_sheets()
            .map(|sheet| (sheet.file.relative_path.as_str(), sheet.disc))
            .collect::<Vec<_>>(),
        vec![
            ("alpha.cue", SheetDisc::Disc { number: 1 }),
            ("beta.cue", SheetDisc::Disc { number: 2 }),
            ("zeta.cue", SheetDisc::Ignored),
        ],
    );
}

/// A stored assignment wins over the position the folder would hand out, and it
/// survives being read back off disk by a later scan.
#[test]
fn a_stored_disc_assignment_overrules_the_default_position() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    std::fs::write(root.join("alpha.flac"), fake_flac()).unwrap();
    std::fs::write(
        root.join("alpha.cue"),
        make_cue_content_n_tracks("alpha.flac", "Album Title", 2),
    )
    .unwrap();
    std::fs::write(root.join("beta.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("beta.cue"), make_cue_content_n_tracks("beta.flac", "Album Title", 3))
        .unwrap();

    let scanned = scan_files(root);

    let mut sheet_discs = SheetDiscEdits::default();
    sheet_discs.set("alpha.cue".to_string(), SheetDisc::Disc { number: 2 });
    sheet_discs.set("beta.cue".to_string(), SheetDisc::Ignored);
    let edits = CandidateFileEdits { sheet_discs, ..Default::default() };

    let reopened = collect_release_candidate_files_with_scope(
        root,
        crate::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::new(HashMap::from([(scanned.content_hash(), edits)])),
    )
    .expect("scan should succeed");

    assert_eq!(
        reopened
            .track_sheets()
            .map(|sheet| (sheet.file.relative_path.as_str(), sheet.disc))
            .collect::<Vec<_>>(),
        vec![("alpha.cue", SheetDisc::Disc { number: 2 }), ("beta.cue", SheetDisc::Ignored),],
    );
    // The ignored sheet carves nothing: its container is loose audio alongside
    // the two tracks carved by the other sheet.
    assert_eq!(reopened.carving_sheets().len(), 1);
    assert_eq!(reopened.track_count(), 3);
}

/// A CUE paired with an audio file whose codec can't back single-file CUE
/// playback (MP3, Vorbis) costs the sheet its binding — bae can't carve tracks
/// out of that container — but the folder still imports: the audio keeps its
/// role and becomes one track with its own source-audio descriptor.
#[test]
fn cue_with_unsupported_codec_leaves_the_sheet_unbound() {
    for (folder, audio_name, fixture, codec, source_codec) in [
        ("MP3 Album", "album.mp3", "placeholder-mp3.mp3", "MP3", "MP3"),
        ("Ogg Album", "album.ogg", "placeholder-vorbis.ogg", "Vorbis", "Vorbis"),
    ] {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();

        let album = root.join(folder);
        std::fs::create_dir(&album).unwrap();
        std::fs::write(album.join("album.cue"), make_cue_content(audio_name, "Test Album"))
            .unwrap();
        std::fs::write(album.join(audio_name), audio_format_fixture(fixture)).unwrap();

        let candidates = scan_valid(root.to_path_buf());
        assert_eq!(candidates.len(), 1, "{folder}: one valid candidate");
        let files = &candidates[0].files;
        assert!(
            files.bound_sheets().is_empty(),
            "{folder}: the sheet must not bind to a codec bae can't carve",
        );
        // The refusal keeps the codec, so the pane can explain why the sheet
        // does not carve the audio.
        let sheets: Vec<_> = files.track_sheets().collect();
        assert_eq!(sheets.len(), 1);
        assert_eq!(
            sheets[0].binding,
            &SheetBinding::RefusedCodec { codec: codec.to_string() },
            "{folder}: the refusal names the probed codec",
        );
        assert_eq!(files.audio().count(), 1, "{folder}: the audio keeps its role",);
        assert_eq!(files.track_count(), 1, "{folder}: it imports as one track");
        assert_uniform_source_audio(
            files,
            bae_records::source_audio::SourceAudioLayout::File,
            source_codec,
        );
    }
}

/// A CUE paired with a codec that can back CUE playback yields a
/// valid candidate with a CUE-backed descriptor. PCM, WavPack, and DSD are
/// otherwise untested positive arms of the codec match.
#[test]
fn cue_with_supported_codec_yields_valid_source_audio() {
    for (folder, audio_name, fixture, codec) in [
        ("PCM Album", "album.wav", "placeholder-pcm.wav", "PCM"),
        ("WavPack Album", "album.wv", "placeholder-wavpack.wv", "WavPack"),
        ("DSD Album", "album.dsf", "placeholder-dsd.dsf", "DSD"),
    ] {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join(folder);
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("album.cue"),
            format!(
                "PERFORMER \"Test Artist\"\nTITLE \"Test Album\"\nFILE \"{audio_name}\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Track One\"\n    INDEX 01 00:00:00\n"
            ),
        )
        .unwrap();
        std::fs::write(root.join(audio_name), audio_format_fixture(fixture)).unwrap();

        let candidates = scan_valid(root);
        assert_eq!(candidates.len(), 1, "{folder}: one valid candidate");
        assert_uniform_source_audio(
            &candidates[0].files,
            bae_records::source_audio::SourceAudioLayout::Cue,
            codec,
        );
    }
}

/// A CUE-paired audio file that clears the header-only magic check but can't be
/// probed (no playable stream) surfaces the folder as an invalid candidate — it
/// must NOT abort the whole watched-root walk. Same failure class as an
/// unsupported codec, triggered instead by one corrupt/incomplete file. A
/// sibling FLAC release under the same root still scans.
#[test]
fn cue_with_unprobeable_audio_is_invalid_and_siblings_still_scan() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    let bad = root.join("Truncated Album");
    std::fs::create_dir(&bad).unwrap();
    std::fs::write(bad.join("album.cue"), make_cue_content("album.flac", "Test Album")).unwrap();
    std::fs::write(bad.join("album.flac"), header_only_flac_unprobeable()).unwrap();

    let good = root.join("FLAC Album");
    std::fs::create_dir(&good).unwrap();
    std::fs::write(good.join("01 Track.flac"), fake_flac()).unwrap();

    let items = scan_items(root.to_path_buf());
    assert_eq!(items.len(), 2, "both leaves surface");

    let invalid = items
        .iter()
        .find_map(|i| match i {
            ScanItem::Invalid(inv) if inv.name == "Truncated Album" => Some(inv),
            _ => None,
        })
        .expect("expected an invalid candidate for the unprobeable CUE audio");
    assert_eq!(invalid.reason(), InvalidReason::DamagedAudio { count: 1 });

    let sibling_scanned =
        items.iter().any(|i| matches!(i, ScanItem::Valid(c) if c.name == "FLAC Album"));
    assert!(sibling_scanned, "sibling FLAC release still scans");
}

/// A FLAC with valid `fLaC` magic + well-formed STREAMINFO shape but no audio
/// frames and `total_samples = 0` (streaming-length unknown). Its header looks
/// intact yet it has no usable duration, so the FFmpeg probe can't identify a
/// playable stream — the shape of a download truncated right after the
/// STREAMINFO block.
fn header_only_flac_unprobeable() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"fLaC");
    // STREAMINFO block header: last-block=0, type=0, length=34.
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 34]);
    // STREAMINFO data (34 bytes): 44100 Hz, 2ch, 16-bit, total_samples=0.
    buf.extend_from_slice(&[0x10, 0x00]); // min block size 4096
    buf.extend_from_slice(&[0x10, 0x00]); // max block size 4096
    buf.extend_from_slice(&[0x00, 0x00, 0x00]); // min frame size
    buf.extend_from_slice(&[0x00, 0x00, 0x00]); // max frame size
    buf.push(0x0A); // sample_rate >> 12
    buf.push(0xC4); // (sample_rate >> 4) & 0xFF
    buf.push(0x42); // (sample_rate & 0x0F)<<4 | (ch-1)<<1 | (bps-1)>>4
    buf.push(0xF0); // (bps-1 & 0x0F)<<4 | total_samples high nibble
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // total_samples low 32 bits
    buf.extend_from_slice(&[0u8; 16]); // MD5 signature
    assert_eq!(buf.len(), 42);
    buf
}

/// A CUE next to the FLAC it names binds, and the folder reports a CUE-backed
/// FLAC source descriptor.
#[test]
fn cue_flac_pair_binds_and_reports_source_audio() {
    let result = run_scenario(vec![
        file("Album/Album.flac", FileKind::Flac),
        file("Album/Album.cue", FileKind::CueFor { stem: "Album.flac", n_tracks: 8 }),
    ]);
    let c = result.candidate("Album");
    assert_uniform_source_audio(
        &c.files,
        bae_records::source_audio::SourceAudioLayout::Cue,
        "FLAC",
    );
    assert_eq!(c.files.bound_sheets().len(), 1);
}

/// Audio no sheet references is kept, not dropped: a bound sheet plus two
/// standalone files leaves all three files on the candidate, hashed and listed.
#[test]
fn audio_no_sheet_references_survives() {
    let (_tmp, album) = album_dir();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("Album.cue"), make_cue_content("Album.flac", "Album Title")).unwrap();
    std::fs::write(album.join("bonus 1.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("bonus 2.flac"), fake_flac()).unwrap();

    let files = scan_files(&album);
    assert_eq!(
        files.audio().map(|f| f.relative_path.as_str()).collect::<Vec<_>>(),
        vec!["Album.flac", "bonus 1.flac", "bonus 2.flac"],
    );
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].audio_files[0].1.file_name, "Album.flac");

    let carried: Vec<_> = files.release_files().map(|f| f.relative_path.as_str()).collect();
    for expected in ["Album.cue", "Album.flac", "bonus 1.flac", "bonus 2.flac"] {
        assert!(carried.contains(&expected), "{expected} must survive the scan, got {carried:?}",);
    }
}

/// A folder whose only disc-ID source is its rip log becomes a candidate, and
/// the log's TOC still yields the disc ID with the sheet unbound. Before roles
/// the scan refused the folder first, so the log never got the chance.
#[test]
fn folder_identifies_from_its_rip_log_with_the_sheet_unbound() {
    let (tmp, album) = album_dir();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(fixtures.join("logs/test_album.log"), album.join("rip.log")).unwrap();
    std::fs::copy(fixtures.join("flac/01 Test Track 1.flac"), album.join("01 Test Track 1.flac"))
        .unwrap();
    std::fs::copy(fixtures.join("flac/02 Test Track 2.flac"), album.join("02 Test Track 2.flac"))
        .unwrap();
    std::fs::write(album.join("Album.cue"), make_cue_content("Album.wav", "Album Title")).unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    assert!(
        matches!(&items[0], ScanItem::Valid(_)),
        "folder must be a candidate so its log can identify it",
    );

    let files = scan_files(&album);
    assert!(files.bound_sheets().is_empty());
    assert!(
        crate::rip_reading::read_rip_artifacts(&files).unwrap().disc_id.computed().len() == 1,
        "the rip log's TOC still yields a disc ID with the sheet unbound",
    );
}

/// The directive binds, not the filename: a sheet and the audio it names pair
/// even when their names have nothing in common.
#[test]
fn a_binding_survives_a_rename_of_the_sheet() {
    let (_tmp, album) = album_dir();
    std::fs::write(album.join("Audio.flac"), fake_flac()).unwrap();
    std::fs::write(
        album.join("Completely Unrelated.cue"),
        make_cue_content("Audio.flac", "Album Title"),
    )
    .unwrap();

    let files = scan_files(&album);
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].file.file_name, "Completely Unrelated.cue");
    assert_eq!(bound[0].audio_files[0].1.file_name, "Audio.flac");
    assert_eq!(
        bound[0].audio_files[0].1.relative_path, "Audio.flac",
        "the binding names the audio by its file id",
    );
}
