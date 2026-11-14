use super::*;
use crate::folder_scanner::{
    collect_release_candidate_files_with_scope, CandidateFileEdits, SheetBindingOffer,
    SheetBindingOption, SheetDiscEdits, StoredCandidateEdits,
};
use crate::pane::blank_candidate_draft;
use crate::tables::watched_path::host_root;
use crate::types::{AudioFile, CandidateDraft};
use crate::ImportError;
use bae_catalogs::medium_coverage::LENGTH_DISAGREEMENT_MS;
use bae_types::pressing::{made_of, Medium};
use std::fs;
use std::path::Path;

/// 44.1 kHz / 2-channel / 16-bit STREAMINFO declaring one second of audio —
/// enough for the scan's validation and the container probe.
///
/// The 34-byte STREAMINFO packs the sample rate (20 bits), channels − 1
/// (3 bits) and bits-per-sample − 1 (5 bits) across three bytes, then the
/// total sample count and an MD5 signature.
pub(crate) fn synthetic_flac_bytes() -> Vec<u8> {
    const CHANNELS_MINUS_1: u8 = 1;
    const BPS_MINUS_1: u8 = 15;
    let sample_rate: u32 = 44_100;

    let mut buf = Vec::new();
    buf.extend_from_slice(b"fLaC");
    buf.extend_from_slice(&[0x80, 0x00, 0x00, 34]);
    buf.extend_from_slice(&[0x10, 0x00, 0x10, 0x00]);
    buf.extend_from_slice(&[0u8; 6]);
    buf.push((sample_rate >> 12) as u8);
    buf.push(((sample_rate >> 4) & 0xFF) as u8);
    buf.push((((sample_rate & 0x0F) as u8) << 4) | (CHANNELS_MINUS_1 << 1) | (BPS_MINUS_1 >> 4));
    buf.push((BPS_MINUS_1 & 0x0F) << 4);
    buf.extend_from_slice(&44_100u32.to_be_bytes());
    buf.extend_from_slice(&[0u8; 16]);
    buf.resize(18_000, 0);
    buf
}

pub(crate) fn write_flac(path: &Path) {
    fs::write(path, synthetic_flac_bytes()).expect("write flac");
}

/// A sheet naming one container for the whole disc, its entries a fifth of
/// a second apart so every entry but the last has a length of its own.
pub(crate) fn cue_sheet_text(audio_file_name: &str, count: usize) -> String {
    crate::cue_sheet::tests::cue_text(
        Some(("Artist Name", "Album Title")),
        &format!("\"{audio_file_name}\""),
        (0..count).map(|index| {
            (Some(format!("Sheet Track {}", index + 1)), format!("00:00:{:02}", index * 15))
        }),
    )
}

pub(crate) fn scan(root: &Path) -> CategorizedFiles {
    collect_release_candidate_files_with_scope(
        root,
        crate::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan succeeds")
}

fn assign_discs(files: &mut CategorizedFiles, assignments: &[(&str, u32)]) {
    let mut sheet_discs = SheetDiscEdits::default();
    for (sheet_id, number) in assignments {
        sheet_discs.set((*sheet_id).to_string(), SheetDisc::Disc { number: *number });
    }
    files
        .apply_candidate_file_edits(&CandidateFileEdits { sheet_discs, ..Default::default() })
        .expect("disc assignments preserve a valid candidate");
}

fn mappings(table: &MappingTable) -> Vec<&TrackMapping> {
    table.track_sides.iter().flat_map(MappingTrackSide::mappings).collect()
}

/// Every run of the table, side after side.
fn runs(table: &MappingTable) -> Vec<&MappingTrackRun> {
    table.track_sides.iter().flat_map(|side| &side.runs).collect()
}

fn track_file(mapping: &TrackMapping) -> &MappingFile {
    match mapping {
        TrackMapping { source: MappingSource::File(file), .. } => file,
        other => panic!("expected a file row, got {other:?}"),
    }
}

/// A folder's files stored the way a scan stores them, with a draft over
/// them, in a library of its own.
struct StoredFolder {
    _db: bae_store::Store,
    _tmp: tempfile::TempDir,
    tables: crate::tables::ImportTables,
    watched_folder_path: std::path::PathBuf,
    key: crate::CandidateKey,
    hash: String,
}

impl StoredFolder {
    async fn of(files: &CategorizedFiles) -> Self {
        let (db, tmp) = bae_store::test_fixtures::empty_db(crate::TEST_SCHEMA).await;
        let folder = host_root("/music/Album");
        let hash =
            crate::tables::import_state::tests::store_candidate_state(&db, files, &folder).await;
        Self {
            tables: crate::tables::ImportTables::new(db.clone()),
            _db: db,
            _tmp: tmp,
            watched_folder_path: folder.parent().expect("a watched root").to_path_buf(),
            key: crate::CandidateKey::folder(&folder),
            hash,
        }
    }

    /// The table the pane draws over `draft`, which replaces the blank draft
    /// the scan stored.
    async fn table(&self, draft: &CandidateDraft) -> Result<MappingTable, ImportError> {
        self.tables.replace_draft_for_test(&self.hash, draft).await;
        self.tables
            .load_mapping_table_for_test(&self.watched_folder_path, &self.key, &self.hash)
            .await
    }
}

/// The table the pane draws for `files` stored and `draft` over them.
async fn mapping_table(
    files: &CategorizedFiles,
    draft: &CandidateDraft,
) -> Result<MappingTable, ImportError> {
    StoredFolder::of(files).await.table(draft).await
}

/// The table the folder's blank draft renders.
pub(crate) async fn blank_table(files: &CategorizedFiles) -> MappingTable {
    mapping_table(files, &blank_candidate_draft(files))
        .await
        .expect("a blank draft is drawn over its own folder")
}

/// The draft over `files` on `mediums`, each track at its (medium, side,
/// number).
fn numbered_draft(
    files: &CategorizedFiles,
    mediums: Vec<bae_types::mediums::ReleaseMedium>,
    places: &[(u16, u8, u32)],
) -> CandidateDraft {
    let mut draft = blank_candidate_draft(files);
    draft.mediums = mediums;
    for (track, (medium, side, number)) in draft.tracks.iter_mut().zip(places) {
        track.place = bae_types::mediums::TrackPlace {
            medium: std::num::NonZeroU16::new(*medium).expect("mediums count from one"),
            side: std::num::NonZeroU8::new(*side).expect("sides count from one"),
            number: std::num::NonZeroU32::new(*number).expect("numbers count from one"),
        };
    }
    draft
}

/// One record of two sides.
fn record() -> Vec<bae_types::mediums::ReleaseMedium> {
    vec![bae_types::mediums::ReleaseMedium {
        carrier: Some(Medium::Vinyl),
        sides: std::num::NonZeroU8::new(2).expect("two sides"),
    }]
}

fn positions(side: &MappingTrackSide) -> Vec<&str> {
    side.mappings().map(|mapping| mapping.position.as_str()).collect()
}

/// Every audio row is one of the draft's tracks, and a rip log and an image
/// are carried beside them, because a role is a fact about the folder.
#[tokio::test]
async fn audio_rows_are_the_draft_s_tracks_and_the_rest_are_carried() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    write_flac(&tmp.path().join("02.flac"));
    fs::write(tmp.path().join("cover.jpg"), fake_jpeg()).expect("write cover");
    fs::write(tmp.path().join("rip.log"), b"log").expect("write log");

    let files = scan(tmp.path());
    let table = blank_table(&files).await;
    let mappings = mappings(&table);

    assert_eq!(table.images.len(), 1);
    let cover = files
        .files
        .iter()
        .find(|entry| entry.file.relative_path == "cover.jpg")
        .expect("the scan found cover.jpg");
    assert_eq!(
        table.images[0].file,
        cover.file.version.clone(),
        "the gallery draws the image at the version the scan read"
    );
    assert_eq!(track_file(mappings[0]).name, "01.flac");
    assert_eq!(track_file(mappings[1]).name, "02.flac");
    assert_eq!(mapping_tracks(&table), blank_candidate_draft(&files).release_edit().tracks);
    let carried: Vec<(&str, MappingRole)> = table
        .files
        .iter()
        .map(|row| match row {
            MappingFileRow::File(file) => (file.name.as_str(), file.role),
            other => panic!("expected a carried file, got {other:?}"),
        })
        .collect();
    assert_eq!(
        carried,
        [("cover.jpg", MappingRole::Image), ("rip.log", MappingRole::Document)],
        "the image is carried in the scan's order beside the other files"
    );
    // The scan stored the length of each unit with the files.
    assert_eq!(track_file(mappings[0]).duration_ms, Some(1_000));
    assert_eq!(track_file(mappings[0]).role, MappingRole::Audio);
}

/// The table counts the track rows nobody has named: a blank title, or one
/// of only spaces, which committing replaces with the file's name.
#[tokio::test]
async fn mapping_statement_counts_untitled_tracks_as_str_trim_does() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for name in ["01.flac", "02.flac", "03.flac"] {
        write_flac(&tmp.path().join(name));
    }
    let files = scan(tmp.path());
    assert_eq!(blank_table(&files).await.untitled_track_count, 3);
    let mut draft = blank_candidate_draft(&files);
    draft.tracks[0].edit.title = "Intro".into();
    draft.tracks[1].edit.title = "   ".into();
    let table =
        mapping_table(&files, &draft).await.expect("the draft is drawn over its own folder");
    assert_eq!(table.untitled_track_count, 2);

    // Every character `str::trim` removes counts as blank, and a zero-width
    // space, which it keeps, names the track.
    draft.tracks[0].edit.title = "\u{3000}\u{a0}\t".into();
    draft.tracks[1].edit.title = "\u{200b}".into();
    let table = mapping_table(&files, &draft).await.unwrap();
    assert_eq!(table.untitled_track_count, 2);
}

/// With no release applied, a row shows its stored probe and nothing
/// disagrees with it.
#[tokio::test]
async fn a_track_the_release_names_no_length_for_uses_its_stored_probe() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    let files = scan(tmp.path());

    let table = blank_table(&files).await;

    let mapping = mappings(&table)[0];
    assert_eq!(mapping.duration_ms, Some(1_000));
    assert!(!mapping.lengths_disagree);
}

/// A row whose track holds the length its source lists shows that length
/// and says when it is far from its file's; a row with none shows its file's.
#[tokio::test]
async fn mapping_statement_warns_where_lengths_differ_by_more_than_three_seconds() {
    let tmp = tempfile::TempDir::new().unwrap();
    for number in 1..=2 {
        write_flac(&tmp.path().join(format!("{number:02}.flac")));
    }
    let files = scan(tmp.path());
    let mut draft = blank_candidate_draft(&files);
    draft.tracks[0].stated_length_ms = Some(200_000);
    let table = mapping_table(&files, &draft).await.unwrap();
    let rows = mappings(&table);
    assert_eq!(rows[0].duration_ms, Some(200_000));
    assert!(rows[0].lengths_disagree);
    assert_eq!(track_file(rows[0]).duration_ms, Some(1_000));
    assert_eq!(rows[1].duration_ms, Some(1_000));
    assert!(!rows[1].lengths_disagree);

    // Exactly the limit is not a disagreement; a millisecond past it is.
    draft.tracks[0].stated_length_ms = Some(1_000 + LENGTH_DISAGREEMENT_MS);
    draft.tracks[1].stated_length_ms = Some(1_000 + LENGTH_DISAGREEMENT_MS + 1);
    let table = mapping_table(&files, &draft).await.unwrap();
    let rows = mappings(&table);
    assert!(!rows[0].lengths_disagree);
    assert!(rows[1].lengths_disagree);
}

/// Sides past Z are lettered AA, AB, … as `side_letters` writes them.
#[tokio::test]
async fn mapping_statement_letters_sides_past_z() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for number in 1..=28 {
        write_flac(&tmp.path().join(format!("{number:02}.flac")));
    }
    let files = scan(tmp.path());
    let places: Vec<(u16, u8, u32)> = (1..=28).map(|side| (1, side, 1)).collect();
    let draft = numbered_draft(
        &files,
        vec![bae_types::mediums::ReleaseMedium {
            carrier: Some(Medium::Vinyl),
            sides: std::num::NonZeroU8::new(28).unwrap(),
        }],
        &places,
    );
    let table = mapping_table(&files, &draft).await.unwrap();
    let letters: Vec<String> = table
        .track_sides
        .iter()
        .map(|side| match &side.side {
            bae_types::mediums::TrackSide::Sided { side_letter } => side_letter.clone(),
            other => panic!("a record's side, got {other:?}"),
        })
        .collect();
    let expected: Vec<String> = (1..=28).map(bae_types::mediums::side_letters).collect();
    assert_eq!(letters, expected);
    assert_eq!(
        (letters[25].as_str(), letters[26].as_str(), letters[27].as_str()),
        ("Z", "AA", "AB")
    );
}

/// The folder's images are one gallery beside the table rows.
#[tokio::test]
async fn the_folder_s_images_are_a_gallery_beside_the_table_rows() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    fs::write(tmp.path().join("cover.jpg"), fake_jpeg()).expect("write cover");
    fs::write(tmp.path().join("back.jpg"), fake_jpeg()).expect("write back");
    fs::create_dir(tmp.path().join("scans")).expect("scans dir");
    for name in ["scan1.jpg", "scan2.jpg", "scan3.jpg"] {
        fs::write(tmp.path().join("scans").join(name), fake_jpeg()).expect("write scan");
    }

    let table = blank_table(&scan(tmp.path())).await;

    assert_eq!(table.images.len(), 5);
    assert_eq!(
        table.images.iter().map(|image| image.file_id.as_str()).collect::<Vec<_>>(),
        ["back.jpg", "cover.jpg", "scans/scan1.jpg", "scans/scan2.jpg", "scans/scan3.jpg",],
        "the gallery preserves the scan's authoritative order"
    );
    // A directory of images is not collapsed away from the gallery — its
    // files are in it, each with the path a thumbnail reads.
    assert!(table
        .images
        .iter()
        .any(|image| image.file_id == "scans/scan1.jpg" && image.file.path.exists()));
    assert_eq!(table.track_sides.len(), 1);
    assert_eq!(track_file(mappings(&table)[0]).name, "01.flac");
}

/// A bound sheet is one group row over its entries: the entries carry the
/// sheet's own titles and timings on the left, and on the right each is the
/// draft's track on that slice.
#[tokio::test]
async fn a_sheet_s_entries_carry_its_own_titles_and_bind_to_its_slices() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("CDImage.flac"));
    fs::write(tmp.path().join("CDImage.cue"), cue_sheet_text("CDImage.flac", 3))
        .expect("write cue");

    let files = scan(tmp.path());
    let mut draft = blank_candidate_draft(&files);
    for (index, track) in draft.tracks.iter_mut().enumerate() {
        track.edit.title = format!("Track Title {}", index + 1);
    }
    let table = mapping_table(&files, &draft).await.unwrap();

    let [MappingTrackRun::Sheet { sheet, entries }] = runs(&table).as_slice() else {
        panic!("the sheet is the folder's only run, got {:?}", table.track_sides);
    };
    assert_eq!(sheet.sheet_id, "CDImage.cue");
    assert_eq!(sheet.assignment, SheetDisc::Disc { number: 1 });
    assert_eq!(
        sheet.reference_options,
        vec![SheetReferenceOptions {
            file_reference: "CDImage.flac".into(),
            file_id: Some("CDImage.flac".into()),
            options: vec![SheetBindingOption {
                file_id: "CDImage.flac".into(),
                offer: SheetBindingOffer::Offered,
            }],
        }]
    );
    let SheetBound::Describes(container) = &sheet.bound else {
        panic!("expected a bound sheet, got {:?}", sheet.bound);
    };
    assert_eq!(container.name, "CDImage.flac");
    assert_eq!(entries.len(), 3);

    for (index, entry) in entries.iter().enumerate() {
        let MappingSource::SheetEntry(source) = &entry.source else {
            panic!("expected a sheet entry, got {:?}", entry.source);
        };
        assert_eq!(source.index, index as u32);
        assert_eq!(source.number, index as u32 + 1);
        assert_eq!(source.title.as_deref(), Some(&*format!("Sheet Track {}", index + 1)));
        assert_eq!(source.container_id, "CDImage.flac");
        // The first entries end at the next sheet boundary; the final entry
        // ends at the container duration already stored by the scan.
        assert_eq!(source.duration_ms, Some(if index < 2 { 200 } else { 600 }));
        assert_eq!(
            entry.track.file,
            Some(AudioFile::SheetSlice {
                file_id: "CDImage.flac".to_string(),
                sheet_id: "CDImage.cue".to_string(),
                index: index as u32,
            }),
        );
        // The right half is the draft's tracklist, not the sheet's.
        assert_eq!(entry.track.title, format!("Track Title {}", index + 1));
    }
}

#[tokio::test]
async fn standalone_tracks_are_sectioned_by_release_side() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for number in 1..=4 {
        write_flac(&tmp.path().join(format!("{number:02}.flac")));
    }

    let files = scan(tmp.path());
    let draft = numbered_draft(&files, record(), &[(1, 1, 1), (1, 1, 2), (1, 2, 1), (1, 2, 2)]);
    let table = mapping_table(&files, &draft).await.unwrap();

    assert_eq!(table.track_sides.len(), 2);
    assert_eq!(
        table.track_sides[0].side,
        bae_types::mediums::TrackSide::Sided { side_letter: "A".to_string() }
    );
    assert_eq!(
        table.track_sides[1].side,
        bae_types::mediums::TrackSide::Sided { side_letter: "B".to_string() }
    );
    assert_eq!(positions(&table.track_sides[0]), ["A1", "A2"]);
    assert_eq!(positions(&table.track_sides[1]), ["B1", "B2"]);
}

#[tokio::test]
async fn each_cue_is_one_section_on_its_assigned_disc() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for stem in ["alpha", "beta"] {
        write_flac(&tmp.path().join(format!("{stem}.flac")));
        fs::write(
            tmp.path().join(format!("{stem}.cue")),
            cue_sheet_text(&format!("{stem}.flac"), 2),
        )
        .expect("write cue");
    }

    let mut files = scan(tmp.path());
    assign_discs(&mut files, &[("alpha.cue", 2), ("beta.cue", 1)]);
    let draft = numbered_draft(
        &files,
        vec![
            bae_types::mediums::ReleaseMedium::one_sided(Some(Medium::Cd)),
            bae_types::mediums::ReleaseMedium::one_sided(Some(Medium::Cd)),
        ],
        &[(1, 1, 1), (1, 1, 2), (2, 1, 1), (2, 1, 2)],
    );
    let table = mapping_table(&files, &draft).await.unwrap();

    assert_eq!(table.track_sides.len(), 2);
    for (side, (disc, sheet_id)) in
        table.track_sides.iter().zip([(1, "beta.cue"), (2, "alpha.cue")])
    {
        assert_eq!(side.side, bae_types::mediums::TrackSide::Disc { disc });
        let [MappingTrackRun::Sheet { sheet, entries }] = side.runs.as_slice() else {
            panic!("a CUE disc is represented by its sheet and entries");
        };
        assert_eq!(sheet.sheet_id, sheet_id);
        assert_eq!(entries.len(), 2);
        assert_eq!(positions(side), ["1", "2"]);
    }
}

/// A release that lays one sheet's entries across two sides of a record heads
/// each side's run of them with the sheet.
#[tokio::test]
async fn mapping_statement_names_a_split_sheet_again_on_the_next_side() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("LP.flac"));
    fs::write(tmp.path().join("LP.cue"), cue_sheet_text("LP.flac", 4)).expect("write cue");

    let files = scan(tmp.path());
    let draft = numbered_draft(&files, record(), &[(1, 1, 1), (1, 1, 2), (1, 2, 1), (1, 2, 2)]);
    let table = mapping_table(&files, &draft).await.unwrap();

    assert_eq!(table.track_sides.len(), 2);
    for (side, letter) in table.track_sides.iter().zip(["A", "B"]) {
        assert_eq!(
            side.side,
            bae_types::mediums::TrackSide::Sided { side_letter: letter.to_string() }
        );
        let [MappingTrackRun::Sheet { sheet, entries }] = side.runs.as_slice() else {
            panic!("each side's entries sit under the sheet");
        };
        assert_eq!(sheet.sheet_id, "LP.cue");
        assert_eq!(entries.len(), 2);
    }
}

/// A side holding loose files and a sheet's entries is one side, headed once,
/// with a run for the loose rows and a run for the sheet; the sheet's entries
/// that the release places on the next side start that side under the sheet
/// again.
#[tokio::test]
async fn mapping_statement_heads_each_side_once_however_many_runs() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for name in ["01.flac", "02.flac", "LP.flac"] {
        write_flac(&tmp.path().join(name));
    }
    fs::write(tmp.path().join("LP.cue"), cue_sheet_text("LP.flac", 2)).expect("write cue");

    let files = scan(tmp.path());
    let draft = numbered_draft(&files, record(), &[(1, 1, 1), (1, 1, 2), (1, 1, 3), (1, 2, 1)]);
    let table = mapping_table(&files, &draft).await.unwrap();

    let [side_a, side_b] = table.track_sides.as_slice() else {
        panic!("one side per side letter, got {:?}", table.track_sides);
    };
    assert_eq!(side_a.side, bae_types::mediums::TrackSide::Sided { side_letter: "A".to_string() });
    let [MappingTrackRun::Tracks(loose), MappingTrackRun::Sheet { sheet, entries }] =
        side_a.runs.as_slice()
    else {
        panic!("side A holds the loose run, then the sheet: {:?}", side_a.runs);
    };
    assert_eq!(
        loose.iter().map(|row| track_file(row).name.as_str()).collect::<Vec<_>>(),
        ["01.flac", "02.flac"]
    );
    assert_eq!(sheet.sheet_id, "LP.cue");
    assert_eq!(entries.len(), 1);
    assert_eq!(positions(side_a), ["A1", "A2", "A3"]);
    assert_eq!(side_b.side, bae_types::mediums::TrackSide::Sided { side_letter: "B".to_string() });
    let [MappingTrackRun::Sheet { sheet, entries }] = side_b.runs.as_slice() else {
        panic!("side B's entry sits under the sheet: {:?}", side_b.runs);
    };
    assert_eq!(sheet.sheet_id, "LP.cue");
    assert_eq!(entries.len(), 1);
}

/// The tracks the commit writes are the table's own rows, in the order the
/// table lays them out, each addressable on its own.
#[tokio::test]
async fn the_commit_tracks_are_the_table_s_rows_in_order() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("CDImage.flac"));
    fs::write(tmp.path().join("CDImage.cue"), cue_sheet_text("CDImage.flac", 2))
        .expect("write cue");
    write_flac(&tmp.path().join("bonus.flac"));
    fs::write(tmp.path().join("cover.jpg"), fake_jpeg()).expect("write cover");

    let files = scan(tmp.path());
    let table = blank_table(&files).await;

    let tracks = mapping_tracks(&table);
    assert_eq!(
        tracks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        vec!["candidate-track-0", "candidate-track-1", "candidate-track-2"],
    );
    // The cover is not a track, and the bonus file leads the sheet's two
    // slices exactly as the folder's audio units do (case-insensitive name
    // order).
    assert_eq!(
        tracks.iter().map(|t| t.file.clone()).collect::<Vec<_>>(),
        vec![
            Some(AudioFile::Standalone { file_id: "bonus.flac".to_string() }),
            Some(AudioFile::SheetSlice {
                file_id: "CDImage.flac".to_string(),
                sheet_id: "CDImage.cue".to_string(),
                index: 0,
            }),
            Some(AudioFile::SheetSlice {
                file_id: "CDImage.flac".to_string(),
                sheet_id: "CDImage.cue".to_string(),
                index: 1,
            }),
        ],
    );
}

/// A draft that does not play the folder's audio units in order is a stored
/// state that contradicts the folder, and drawing it is refused rather than
/// pairing rows with the wrong audio.
#[tokio::test]
async fn mapping_statement_refuses_a_draft_track_whose_unit_differs() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for number in 1..=2 {
        write_flac(&tmp.path().join(format!("{number:02}.flac")));
    }
    let files = scan(tmp.path());

    let mut swapped = blank_candidate_draft(&files);
    swapped.tracks.swap(0, 1);
    assert!(matches!(mapping_table(&files, &swapped).await, Err(ImportError::Internal { .. })));

    let mut short = blank_candidate_draft(&files);
    short.tracks.pop();
    assert!(matches!(mapping_table(&files, &short).await, Err(ImportError::Internal { .. })));

    let mut long = blank_candidate_draft(&files);
    long.tracks.push(long.tracks[0].clone());
    assert!(matches!(mapping_table(&files, &long).await, Err(ImportError::Internal { .. })));
}

/// A sheet whose `FILE` directive names audio that is not in the folder
/// describes nothing — and says what it was looking for, so the header can
/// state it while it offers the folder's own audio instead. It also carries
/// its own path, which is what opens it in the document viewer.
#[tokio::test]
async fn a_sheet_that_describes_nothing_says_what_it_asked_for() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    // Two audio files beside a one-file sheet: nothing for it to take as a
    // last resort, so it describes nothing.
    write_flac(&tmp.path().join("01.flac"));
    write_flac(&tmp.path().join("02.flac"));
    fs::write(tmp.path().join("CDImage.cue"), cue_sheet_text("CDImage.wav", 3)).expect("write cue");

    let table = blank_table(&scan(tmp.path())).await;
    // The sheet is named where it sits on disk, after the loose audio that
    // sorts before it — a sheet that carves nothing occupies no run.
    let Some(MappingFileRow::Sheet(sheet)) =
        table.files.iter().find(|row| matches!(row, MappingFileRow::Sheet(_)))
    else {
        panic!("expected a sheet row among {:?}", table.files);
    };
    assert_eq!(sheet.bound, SheetBound::Unresolved { requested: vec!["CDImage.wav".to_string()] },);
}

/// The projection keeps the two meanings of a sheet separate: one that carves
/// audio heads those track rows, while one that carves nothing remains a file
/// the user can resolve from the files section.
#[tokio::test]
async fn associated_sheets_group_tracks_and_unassociated_sheets_remain_files() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("disc.flac"));
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.flac", 2))
        .expect("write associated cue");
    fs::write(tmp.path().join("unresolved.cue"), cue_sheet_text("absent.wav", 2))
        .expect("write unassociated cue");

    let table = blank_table(&scan(tmp.path())).await;

    assert!(matches!(
        runs(&table).as_slice(),
        [MappingTrackRun::Sheet { sheet, entries }]
            if sheet.sheet_id == "disc.cue" && entries.len() == 2
    ));
    assert!(matches!(
        table.files.as_slice(),
        [MappingFileRow::Sheet(sheet)] if sheet.sheet_id == "unresolved.cue"
    ));
}

/// Projecting the table reads no audio at all. The lengths come from the
/// layout the scan stored with the files, so re-opening a candidate costs
/// nothing on disk however often it happens — which is what lets the pane
/// draw itself from a query.
#[tokio::test]
async fn projecting_the_table_opens_no_audio() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("CDImage.flac"));
    fs::write(tmp.path().join("CDImage.cue"), cue_sheet_text("CDImage.flac", 2))
        .expect("write cue");
    write_flac(&tmp.path().join("bonus.flac"));

    let files = scan(tmp.path());
    let stored = StoredFolder::of(&files).await;
    let opens_after_probing: Vec<u64> = ["CDImage.flac", "bonus.flac"]
        .iter()
        .map(|name| bae_audio::test_reads::probes(&tmp.path().join(name)))
        .collect();

    let draft = blank_candidate_draft(&files);
    let first = stored.table(&draft).await.unwrap();
    let second = stored.table(&draft).await.unwrap();

    for (index, name) in ["CDImage.flac", "bonus.flac"].iter().enumerate() {
        assert_eq!(
            bae_audio::test_reads::probes(&tmp.path().join(name)),
            opens_after_probing[index],
            "{name} must not be read again by projecting the table",
        );
    }
    assert_eq!(mapping_tracks(&first), mapping_tracks(&second));
}

/// JPEG magic bytes — what the scan's image validation reads.
fn fake_jpeg() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]
}

#[tokio::test]
async fn invalid_cue_times_are_not_reported_as_missing_audio() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("disc.flac"));
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.flac", 7)).unwrap();
    let table = blank_table(&scan(tmp.path())).await;
    let MappingFileRow::Sheet(sheet) = &table.files[0] else {
        panic!("the refused CUE remains listed");
    };
    assert_eq!(sheet.bound, SheetBound::RefusedTiming);
    assert_eq!(
        sheet.reference_options,
        vec![SheetReferenceOptions {
            file_reference: "disc.flac".into(),
            file_id: Some("disc.flac".into()),
            options: vec![SheetBindingOption {
                file_id: "disc.flac".into(),
                offer: SheetBindingOffer::RefusedTiming,
            }],
        }]
    );
}

#[tokio::test]
async fn a_sheet_disc_menu_keeps_the_assigned_disc_available() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("disc.flac"));
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.flac", 2)).unwrap();
    let mut files = scan(tmp.path());
    assign_discs(&mut files, &[("disc.cue", 2)]);
    let table = blank_table(&files).await;
    let [MappingTrackRun::Sheet { sheet, .. }] = runs(&table).as_slice() else {
        panic!("the selected CUE groups its tracks");
    };
    assert!(sheet.disc_options.contains(&2));
}

#[tokio::test]
async fn partial_sheet_keeps_assigned_and_missing_references_even_without_choices() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("first.flac"));
    fs::write(tmp.path().join("album.cue"),
        "FILE \"first.wav\" WAVE\n TRACK 01 AUDIO\n INDEX 01 00:00:00\nFILE \"absent.wav\" WAVE\n TRACK 02 AUDIO\n INDEX 01 00:00:00\n"
    ).unwrap();
    let table = blank_table(&scan(tmp.path())).await;
    let MappingFileRow::Sheet(sheet) = &table.files[0] else {
        panic!("partial CUE remains listed");
    };
    assert_eq!(sheet.bound, SheetBound::Unresolved { requested: vec!["absent.wav".into()] });
    assert_eq!(
        sheet.reference_options,
        vec![
            SheetReferenceOptions {
                file_reference: "first.wav".into(),
                file_id: Some("first.flac".into()),
                options: vec![SheetBindingOption {
                    file_id: "first.flac".into(),
                    offer: SheetBindingOffer::Offered
                }],
            },
            SheetReferenceOptions {
                file_reference: "absent.wav".into(),
                file_id: None,
                options: vec![]
            },
        ]
    );
}

#[tokio::test]
async fn codec_refusal_does_not_invent_a_current_assignment() {
    let tmp = tempfile::TempDir::new().unwrap();
    fs::copy(
        bae_audio::test_fixtures::audio_format_dir().join("placeholder-mp3.mp3"),
        tmp.path().join("disc.mp3"),
    )
    .unwrap();
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.mp3", 1)).unwrap();
    let table = blank_table(&scan(tmp.path())).await;
    let MappingFileRow::Sheet(sheet) = &table.files[0] else {
        panic!("refused CUE remains listed");
    };
    assert_eq!(sheet.bound, SheetBound::RefusedCodec { codec: "MP3".into() });
    assert_eq!(
        sheet.reference_options,
        vec![SheetReferenceOptions {
            file_reference: "disc.mp3".into(),
            file_id: None,
            options: vec![SheetBindingOption {
                file_id: "disc.mp3".into(),
                offer: SheetBindingOffer::RefusedCodec { codec: "MP3".into() }
            }],
        }]
    );
}

#[tokio::test]
async fn the_first_of_competing_sheets_carves_and_the_rest_stay_listed() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("disc.flac"));
    for name in ["first.cue", "second.cue"] {
        fs::write(tmp.path().join(name), cue_sheet_text("disc.flac", 2)).unwrap();
    }
    let table = blank_table(&scan(tmp.path())).await;
    assert!(matches!(
        runs(&table).as_slice(),
        [MappingTrackRun::Sheet { sheet, entries }]
            if sheet.sheet_id == "first.cue" && entries.len() == 2
    ));
    // The sheet that lost the container stays listed, ignored, still bound
    // to it and still offering it, so a person can make it the one that
    // carves instead.
    let [MappingFileRow::Sheet(sheet)] = table.files.as_slice() else {
        panic!("the competing CUE remains listed: {:?}", table.files);
    };
    assert_eq!(sheet.sheet_id, "second.cue");
    assert_eq!(sheet.assignment, SheetDisc::Ignored);
    assert!(
        matches!(&sheet.bound, SheetBound::Describes(container) if container.file_id == "disc.flac")
    );
    assert_eq!(sheet.reference_options[0].file_id.as_deref(), Some("disc.flac"));
}

/// A CUE's slices on a draft whose medium states no carrier read as one flat
/// run: the folder's sheets do not make up sides the draft never named, even
/// where the pressing says it is a record.
#[tokio::test]
async fn source_disc_assignments_do_not_invent_sides() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("disc.flac"));
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.flac", 3)).unwrap();
    let files = scan(tmp.path());
    let mut draft = blank_candidate_draft(&files);
    draft.pressing.facts = made_of(Medium::Vinyl, 1);
    let table = mapping_table(&files, &draft).await.unwrap();
    assert!(
        table
            .track_sides
            .iter()
            .all(|side| { !matches!(side.side, bae_types::mediums::TrackSide::Sided { .. }) }),
        "available CUE audio does not establish vinyl sides"
    );
    let positions: Vec<_> = mappings(&table).into_iter().map(|row| row.position.as_str()).collect();
    assert_eq!(positions, ["1", "2", "3"]);
}

/// A sheet naming two audio files heads its entries as one group, and once
/// ignored stays a listed file still describing both — all without opening
/// either file again.
#[tokio::test]
async fn a_multi_file_sheet_stays_described_when_ignored_without_probing() {
    let tmp = tempfile::TempDir::new().unwrap();
    for name in ["first.flac", "second.flac"] {
        write_flac(&tmp.path().join(name));
    }
    fs::write(tmp.path().join("album.cue"),
        "FILE \"first.wav\" WAVE\n TRACK 01 AUDIO\n INDEX 01 00:00:00\n TRACK 02 AUDIO\n INDEX 01 00:00:15\nFILE \"second.wav\" WAVE\n TRACK 03 AUDIO\n INDEX 01 00:00:00\n"
    ).unwrap();
    let mut files = scan(tmp.path());
    let opens = ["first.flac", "second.flac"]
        .map(|name| bae_audio::test_reads::probes(&tmp.path().join(name)));
    let expected = vec![
        SheetReferenceOptions {
            file_reference: "first.wav".into(),
            file_id: Some("first.flac".into()),
            options: vec![SheetBindingOption {
                file_id: "first.flac".into(),
                offer: SheetBindingOffer::Offered,
            }],
        },
        SheetReferenceOptions {
            file_reference: "second.wav".into(),
            file_id: Some("second.flac".into()),
            options: vec![SheetBindingOption {
                file_id: "second.flac".into(),
                offer: SheetBindingOffer::Offered,
            }],
        },
    ];
    let table = blank_table(&files).await;
    let [MappingTrackRun::Sheet { sheet, entries }] = runs(&table).as_slice() else {
        panic!("the CUE heads its entries");
    };
    assert_eq!(sheet.bound, SheetBound::DescribesFiles { audio_file_count: 2 });
    assert_eq!(sheet.reference_options, expected);
    assert_eq!(entries.len(), 3);

    let mut sheet_discs = SheetDiscEdits::default();
    sheet_discs.set("album.cue".into(), SheetDisc::Ignored);
    files
        .apply_candidate_file_edits(&CandidateFileEdits { sheet_discs, ..Default::default() })
        .unwrap();
    let table = blank_table(&files).await;
    let MappingFileRow::Sheet(sheet) = &table.files[0] else {
        panic!("ignored CUE remains a file");
    };
    assert_eq!(sheet.assignment, SheetDisc::Ignored);
    assert_eq!(sheet.bound, SheetBound::DescribesFiles { audio_file_count: 2 });
    assert_eq!(sheet.reference_options, expected);
    for (name, count) in ["first.flac", "second.flac"].into_iter().zip(opens) {
        assert_eq!(bae_audio::test_reads::probes(&tmp.path().join(name)), count);
    }
}
