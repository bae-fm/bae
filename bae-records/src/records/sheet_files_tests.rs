//! What a release editor's edits do to a release's tracks, as the statements
//! of `sheet_changes_ctes` work it out.

use super::layout::{laid_mediums_source, laid_out_on, laid_places_source, LayoutParams};
use super::*;
use crate::test_fixtures::{ALBUM_A, RELEASE_A};
use crate::ReleaseFileRole;
use bae_store::Store;
use bae_util::content_type::ContentType;
use std::num::{NonZeroU16, NonZeroU32, NonZeroU8};

/// A track of a fixture release: its place, the audio file it plays, where
/// in that file it starts in seconds (a track of a CUE sheet starts and ends
/// inside an image), and the sheet and `FILE` line that carve it.
struct Track {
    id: &'static str,
    medium: u16,
    file: &'static str,
    start_s: u64,
    carved: Option<(&'static str, &'static str)>,
}

fn loose(id: &'static str, medium: u16, file: &'static str) -> Track {
    Track { id, medium, file, start_s: 0, carved: None }
}

fn carved(
    id: &'static str,
    medium: u16,
    (sheet, line): (&'static str, &'static str),
    file: &'static str,
    start_s: u64,
) -> Track {
    Track { id, medium, file, start_s, carved: Some((sheet, line)) }
}

fn uuid(label: &str) -> String {
    bae_util::test_ids::test_uuid(label)
}

/// A release of `media` one-sided mediums with `tracks` in playing order,
/// each playing its audio file (a file of 600 seconds, named by its label)
/// and carved by its sheet, built through the records' own writes.
async fn release(media: u16, tracks: &[Track]) -> (Store, bae_util::test_files::TestFolder) {
    let (db, folder) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    crate::test_fixtures::album_with_releases(&records, ALBUM_A, "Album", &[RELEASE_A]).await;
    for position in 1..=media {
        records
            .insert_release_medium(&DbReleaseMedium {
                id: uuid(&format!("medium-{position}")),
                release_id: RELEASE_A.to_string(),
                position: NonZeroU16::new(position).unwrap(),
                medium: bae_types::mediums::ReleaseMedium {
                    carrier: None,
                    sides: NonZeroU8::new(1).unwrap(),
                },
                created_at: time::UtcDateTime::now(),
            })
            .await
            .unwrap();
    }
    let mut audio_files: Vec<&str> = tracks.iter().map(|track| track.file).collect();
    audio_files.sort_unstable();
    audio_files.dedup();
    for file in audio_files {
        let image = tracks.iter().any(|track| track.file == file && track.carved.is_some());
        let new = NewReleaseFile {
            id: uuid(file),
            release_id: RELEASE_A.to_string(),
            original_filename: format!("{file}.flac"),
            content_type: ContentType::Flac,
            role: ReleaseFileRole::Audio,
            source_audio: Some(crate::source_audio::SourceAudioFile {
                layout: Some(if image {
                    crate::source_audio::SourceAudioLayout::Cue
                } else {
                    crate::source_audio::SourceAudioLayout::File
                }),
                format: bae_types::audio_format::AudioFormat {
                    codec: "FLAC".to_string(),
                    sample_rate_hz: 44_100,
                    bits_per_sample: Some(16),
                    bitrate_kbps: None,
                    channels: 2,
                },
                content_type: ContentType::Flac,
                container: bae_util::audio_container::AudioContainer::Flac,
                duration_ms: 600_000,
            }),
            created_at: time::UtcDateTime::now(),
        };
        crate::test_fixtures::aggregate_ordering::insert_file(&db, &folder, new, 1).await;
    }
    let mut sheets: Vec<&str> =
        tracks.iter().filter_map(|track| track.carved).map(|c| c.0).collect();
    sheets.sort_unstable();
    sheets.dedup();
    for sheet in sheets {
        let new = NewReleaseFile::new(
            RELEASE_A,
            &format!("{sheet}.cue"),
            ContentType::from_mime("application/x-cue"),
            ReleaseFileRole::TrackSheet,
            uuid(sheet),
            time::UtcDateTime::now(),
        );
        let original = folder.join(uuid(sheet));
        folder
            .open_with(&original, std::fs::OpenOptions::new().write(true).create_new(true))
            .set_len(1)
            .unwrap();
        records.insert_file_for_test(&new, &original).await.unwrap();
    }
    for (index, track) in tracks.iter().enumerate() {
        let number = u32::try_from(index).unwrap() + 1;
        let row = DbTrack {
            medium_id: uuid(&format!("medium-{}", track.medium)),
            side: NonZeroU8::MIN,
            track_number: NonZeroU32::new(number).unwrap(),
            ..DbTrack::new_test(RELEASE_A, &uuid(track.id), track.id, number)
        };
        records.insert_track(&row).await.unwrap();
        let format = DbAudioFormat::new(
            &uuid(track.id),
            ContentType::Flac,
            NonZeroU32::new(44_100).unwrap(),
            NonZeroU32::new(16),
            NonZeroU32::new(2).unwrap(),
            uuid(&format!("format-{}", track.id)),
            time::UtcDateTime::now(),
        );
        let segment = DbAudioSegment {
            id: uuid(&format!("segment-{}", track.id)),
            audio_format_id: format.id.clone(),
            segment_index: 0,
            role: DbAudioSegmentRole::Main,
            file_id: uuid(track.file),
            start_sample: track.start_s * 44_100,
            end_sample: track.carved.map(|_| (track.start_s + 100) * 44_100),
            start_byte: None,
            end_byte: None,
            created_at: time::UtcDateTime::now(),
        };
        records.insert_audio_format_with_segments_for_test(&format, &[segment]).await.unwrap();
        if let Some((sheet, line)) = track.carved {
            let (track_id, sheet_id) = (uuid(track.id), uuid(sheet));
            db.call_sql(move |sql| {
                write::insert_carved_track_row(&sql, &track_id, &sheet_id, line)
            })
            .await
            .unwrap();
        }
    }
    (db, folder)
}

/// The edits that bind each `FILE` line of `no_audio` (sheet label, line) to
/// no audio, and type the titles `titles` (file label, title), as the
/// statements bind them.
fn edits(no_audio: &[(&str, &str)], titles: &[(&str, &str)]) -> String {
    serde_json::to_string(&serde_json::json!({
        "picks": no_audio
            .iter()
            .map(|(sheet, line)| serde_json::json!({
                "sheet_file_id": uuid(sheet),
                "file_reference": line,
                "audio_file_id": null,
            }))
            .collect::<Vec<_>>(),
        "titles": titles
            .iter()
            .map(|(file, title)| (uuid(file), (*title).to_string()))
            .collect::<std::collections::BTreeMap<_, _>>(),
    }))
    .unwrap()
}

async fn changes(db: &Store, edits: String) -> SheetTrackChanges {
    db.read(move |sql| sheet_track_changes_on(&sql, RELEASE_A, &edits)).await.unwrap()
}

/// The ids of `labels`, as the tracks of a fixture are named.
fn ids(labels: &[&str]) -> Vec<String> {
    labels.iter().map(|label| uuid(label)).collect()
}

fn added_ids(changes: &SheetTrackChanges) -> Vec<String> {
    changes.added.iter().map(|added| added.file_id.clone()).collect()
}

/// Edits that bind no line to no audio delete no track and add none.
#[tokio::test]
async fn edits_that_bind_no_line_to_no_audio_delete_and_add_nothing() {
    let (db, _folder) =
        release(1, &[loose("a", 1, "loose"), carved("s1", 1, ("one", "image.flac"), "image", 0)])
            .await;
    let none = changes(&db, "{}".to_string()).await;
    assert_eq!((none.dropped, none.added), (Vec::new(), Vec::new()));
    let empty = changes(&db, edits(&[], &[])).await;
    assert_eq!((empty.dropped, empty.added), (Vec::new(), Vec::new()));
}

/// The tracks under a line with no audio go in release order, and its image, which no
/// track plays then, gets a track.
#[tokio::test]
async fn a_line_with_no_audio_deletes_its_tracks_and_adds_a_track_for_its_image() {
    let (db, _folder) = release(
        1,
        &[
            loose("a", 1, "loose"),
            carved("s1", 1, ("one", "image.flac"), "image", 0),
            carved("s2", 1, ("one", "image.flac"), "image", 100),
            loose("z", 1, "last"),
        ],
    )
    .await;
    let changes = changes(&db, edits(&[("one", "image.flac")], &[])).await;
    assert_eq!(changes.dropped, ids(&["s1", "s2"]));
    assert_eq!(added_ids(&changes), ids(&["image"]));
    assert_eq!(changes.added[0].name, "image.flac");
    assert_eq!(changes.added[0].typed_title, None);
}

/// A sheet whose lines all play no audio adds a track for each file that loses all
/// its audio, in the order the first deleted track of each sat.
#[tokio::test]
async fn a_sheet_of_two_lines_with_no_audio_adds_a_track_for_each_file() {
    let (db, _folder) = release(
        1,
        &[
            carved("b1", 1, ("two", "first.flac"), "first", 0),
            carved("b2", 1, ("two", "second.flac"), "second", 0),
        ],
    )
    .await;
    let changes = changes(
        &db,
        edits(&[("two", "first.flac"), ("two", "second.flac")], &[("second", "Typed")]),
    )
    .await;
    assert_eq!(changes.dropped, ids(&["b1", "b2"]));
    assert_eq!(added_ids(&changes), ids(&["first", "second"]));
    assert_eq!(changes.added[1].typed_title.as_deref(), Some("Typed"));
}

/// A file a track that stays still plays gets no track, whether a loose
/// track or another sheet's carves it.
#[tokio::test]
async fn a_file_a_staying_track_plays_gets_no_track() {
    let (db, _folder) = release(
        1,
        &[
            carved("s1", 1, ("one", "image.flac"), "image", 0),
            carved("t1", 1, ("three", "image.flac"), "image", 200),
            carved("b1", 1, ("two", "whole.flac"), "whole", 0),
            loose("w", 1, "whole"),
        ],
    )
    .await;
    let changes = changes(&db, edits(&[("one", "image.flac"), ("two", "whole.flac")], &[])).await;
    assert_eq!(changes.dropped, ids(&["s1", "b1"]));
    assert_eq!(added_ids(&changes), Vec::<String>::new());
}

/// The edits' picks and layout bound as the editor's statements bind them.
fn params_for(edits: &str) -> (String, String) {
    (edits.to_string(), RELEASE_A.to_string())
}

async fn rows(db: &Store, edits: String) -> Vec<DbReleaseEditRow> {
    let (edits, release) = params_for(&edits);
    db.read(move |sql| {
        let layout = LayoutParams {
            release_id: Some(&release),
            applied: false,
            mediums: "[]",
            places: "[]",
            carving: "[]",
            moves: "[]",
            edits: &edits,
        };
        let mediums = laid_mediums_source();
        let places = laid_places_source();
        Ok(sql.query(
            &release_edit_tracks_sql(
                ":release",
                MediumsSource::Draft { mediums: &mediums, places: &places },
                ":edits",
            ),
            layout.named().as_slice(),
            release_edit_row_of_row,
        )?)
    })
    .await
    .unwrap()
}

/// What each row of the editor's tracks statement is: a track the library
/// holds by its id, or a track the save adds by its file's id, in order.
fn kinds(rows: &[DbReleaseEditRow]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            DbReleaseEditRow::Stored(track) => track.track.id.clone(),
            DbReleaseEditRow::Added(added) => format!("added {}", added.file.file_id),
        })
        .collect()
}

/// The statement lists the tracks that stay and the tracks the save adds in
/// one order: an added track takes the place of the sheet's first track, and
/// the tracks after it number on.
#[tokio::test]
async fn the_editor_lists_an_added_track_where_the_sheet_began() {
    let (db, _folder) = release(
        1,
        &[
            loose("a", 1, "loose"),
            carved("s1", 1, ("one", "image.flac"), "image", 0),
            carved("s2", 1, ("one", "image.flac"), "image", 100),
            loose("z", 1, "last"),
        ],
    )
    .await;
    let rows = rows(&db, edits(&[("one", "image.flac")], &[])).await;
    assert_eq!(kinds(&rows), [uuid("a"), format!("added {}", uuid("image")), uuid("z")],);
    let DbReleaseEditRow::Added(added) = &rows[1] else { panic!("the second row is added") };
    assert_eq!(added.file.name, "image.flac");
    assert_eq!(added.duration_ms, 600_000);
    assert_eq!(added.position.text(), "2");
    let DbReleaseEditRow::Stored(last) = &rows[2] else { panic!("the third row is stored") };
    assert_eq!(last.position.text(), "3", "the side numbers again from one");
}

/// A typed title reaches the added row, and a title for a file no track is
/// added for reaches nothing.
#[tokio::test]
async fn a_typed_title_reaches_the_added_row_only() {
    let (db, _folder) =
        release(1, &[carved("s1", 1, ("one", "image.flac"), "image", 0), loose("z", 1, "last")])
            .await;
    let rows =
        rows(&db, edits(&[("one", "image.flac")], &[("image", "Typed"), ("last", "Stray")])).await;
    let DbReleaseEditRow::Added(added) = &rows[0] else { panic!("the first row is added") };
    assert_eq!(added.file.typed_title.as_deref(), Some("Typed"));
    assert_eq!(rows.len(), 2);
}

/// Several added tracks at one place keep the order their first deleted
/// tracks sat in, ahead of a kept anchor.
#[tokio::test]
async fn added_tracks_keep_the_order_their_first_deleted_tracks_sat_in() {
    let (db, _folder) = release(
        1,
        &[
            loose("a", 1, "loose"),
            carved("b1", 1, ("two", "first.flac"), "first", 0),
            carved("b2", 1, ("two", "second.flac"), "second", 0),
        ],
    )
    .await;
    let rows = rows(&db, edits(&[("two", "first.flac"), ("two", "second.flac")], &[])).await;
    assert_eq!(
        kinds(&rows),
        [uuid("a"), format!("added {}", uuid("first")), format!("added {}", uuid("second"))],
    );
}

/// An added track keeps the disc of the sheet it replaces.
#[tokio::test]
async fn an_added_track_stays_on_the_disc_of_the_sheet() {
    let (db, _folder) = release(
        3,
        &[
            loose("a", 1, "loose"),
            carved("b", 2, ("two", "second.flac"), "second", 0),
            loose("c", 3, "last"),
        ],
    )
    .await;
    let (edits, release) =
        (edits(&[("two", "first.flac"), ("two", "second.flac")], &[]), RELEASE_A.to_string());
    let layout = db
        .read(move |sql| {
            laid_out_on(
                &sql,
                &LayoutParams {
                    release_id: Some(&release),
                    applied: false,
                    mediums: "[]",
                    places: "[]",
                    carving: "[]",
                    moves: "[]",
                    edits: &edits,
                },
            )
        })
        .await
        .unwrap();
    let places: Vec<(String, u16)> = layout
        .tracks
        .iter()
        .map(|track| (track.track_id.clone(), track.place.medium.get()))
        .collect();
    assert_eq!(
        places,
        [(uuid("a"), 1), (uuid("second"), 2), (uuid("c"), 3)],
        "the added track sits on the sheet's disc"
    );
}

/// The names of the Files list's entries for `edits`, each a file by its name
/// or a sheet with no tracks by its name and its lines' text.
async fn carried(db: &Store, edits: String) -> (bool, Vec<String>) {
    let files = db
        .read(move |sql| {
            super::release_files_on(
                &sql,
                ":release",
                ":edits",
                rusqlite::named_params! { ":release": RELEASE_A, ":edits": edits },
            )
        })
        .await
        .unwrap();
    let entries = files
        .carried
        .iter()
        .map(|entry| match entry {
            DbCarriedFile::File(file) => file.original_filename.clone(),
            DbCarriedFile::Sheet(sheet) => format!(
                "{} ({})",
                sheet.name,
                sheet
                    .lines
                    .iter()
                    .map(|line| line.file_reference.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })
        .collect();
    (files.has_others, entries)
}

/// A sheet none of whose lines plays audio takes its place among the files
/// no track plays from, with the side its first track sat on, each line and
/// the audio files the line can play from.
#[tokio::test]
async fn a_sheet_with_no_tracks_takes_its_place_in_the_files_list() {
    let (db, _folder) = release(
        1,
        &[
            carved("b1", 1, ("two", "first.flac"), "first", 0),
            carved("b2", 1, ("two", "second.flac"), "second", 0),
            carved("c1", 1, ("one", "image.flac"), "image", 0),
            loose("z", 1, "last"),
        ],
    )
    .await;
    assert_eq!(carried(&db, edits(&[], &[])).await, (false, Vec::new()));
    let two = [("two", "first.flac"), ("two", "second.flac")];
    let (has_others, entries) = carried(&db, edits(&two, &[])).await;
    assert!(has_others);
    assert_eq!(entries, ["two.cue (first.flac, second.flac)"]);
    let (_, partly) = carried(&db, edits(&[("two", "first.flac")], &[])).await;
    assert_eq!(
        partly,
        Vec::<String>::new(),
        "a sheet with a line that plays audio stays a caption"
    );
    let files = db
        .read(move |sql| {
            super::release_files_on(
                &sql,
                ":release",
                ":edits",
                rusqlite::named_params! { ":release": RELEASE_A, ":edits": edits(&two, &[]) },
            )
        })
        .await
        .unwrap();
    let Some(DbCarriedFile::Sheet(sheet)) = files.carried.first() else { panic!("a sheet") };
    assert_eq!(sheet.place.side, bae_types::mediums::MediumSide::first_of(1));
    assert_eq!(sheet.lines.len(), 2);
    assert!(sheet.lines[0].options.iter().any(|option| option.name == "first.flac"));
}

/// A sheet one of whose lines plays no audio keeps the tracks of its other
/// line, counts only that line, and lists the line that plays none, with the
/// audio files it can play from, on each of those tracks.
#[tokio::test]
async fn a_sheet_with_a_line_of_no_audio_lists_it_on_the_tracks_that_stay() {
    let (db, _folder) = release(
        1,
        &[
            carved("b1", 1, ("two", "first.flac"), "first", 0),
            carved("b2", 1, ("two", "second.flac"), "second", 0),
        ],
    )
    .await;
    let rows = rows(&db, edits(&[("two", "first.flac")], &[])).await;
    let [DbReleaseEditRow::Added(added), DbReleaseEditRow::Stored(stays)] = rows.as_slice() else {
        panic!("the first file gets a track, and the track of the other line stays: {rows:?}")
    };
    assert_eq!(stays.track.id, uuid("b2"));
    assert_eq!(added.file.file_id, uuid("first"));
    let sheet = stays.sheet.as_ref().expect("the sheet carves the track");
    assert_eq!(sheet.line_count, 1);
    let [line] = sheet.unbound_lines.as_slice() else { panic!("one line plays none: {sheet:?}") };
    assert_eq!(line.file_reference, "first.flac");
    assert!(line.options.iter().any(|option| option.name == "first.flac"));
}
