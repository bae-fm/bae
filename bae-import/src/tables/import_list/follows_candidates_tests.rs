//! What a release's stored import state places: its draft, its link, its
//! failure, what is running for it, and the verdict its lookup stored and why
//! a found one picks none of its releases unattended.

use crate::candidate_runtime::IdentificationChange;
use crate::tables::import_list::placement::tests::*;
use crate::tables::import_list::tests::*;
use crate::{ArtistAssignment, CandidateEditField, Credited};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::credits::ArtistCredit;
use bae_catalogs::search::MetadataResult;
use bae_identify::TerminalVerdict;
use bae_records::test_fixtures::artists_apart::{meet_musicbrainz_artist, musicbrainz_artist};
use bae_records::test_fixtures::test_records;

/// The draft counts only while its state describes the folder's current
/// files: it shows once it is not blank, a field the list doesn't read moves
/// nothing, and a pass that read the folder before its file decisions landed
/// shows nothing of it until the next pass reads them.
#[tokio::test]
async fn the_draft_shows_while_its_state_is_current() {
    let lib = Library::new().await;
    lib.run(lib.scan("Album")).await;
    let drafted =
        moves([(lib.release("Album"), Some(found("Album").shown("Album Title", Some(""))))]);
    assert_eq!(lib.moved(lib.draft("Album", "Album Title")).await, drafted);
    assert_eq!(
        lib.moved(lib.typed("Album", CandidateEditField::PressingYear, "2026")).await,
        moves([])
    );
    assert_eq!(lib.moved(lib.file_decision("Album")).await, moves([]));
    assert_eq!(
        lib.moved(lib.scan_revised("Album", 0)).await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
    assert_eq!(lib.moved(lib.scan_revised("Album", 1)).await, drafted);
}

/// A blank draft shows once a source answered it.
#[tokio::test]
async fn a_blank_draft_shows_once_something_answered_it() {
    let lib = Library::new().await;
    lib.run(lib.scan("Album")).await;
    assert_eq!(
        lib.moved(lib.blank_answer("Album", Some(crate::MetadataProvenance::FileMetadata))).await,
        moves([(lib.release("Album"), Some(found("Album").shown("", Some(""))))])
    );
    assert_eq!(
        lib.moved(lib.blank_answer("Album", None)).await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// The MusicBrainz artist the draft below picks.
const PICKED_MUSICBRAINZ: &str = "picked-artist-mbid";

/// A credit printing `name`, its catalogs' artist unknown, as `credited_as`
/// when that differs, followed by `join`.
fn credited(name: &str, credited_as: Option<&str>, join: Option<&str>) -> Credited {
    Credited {
        artist: ArtistAssignment::Credit {
            credit: ArtistCredit {
                credited_as: credited_as.map(str::to_string),
                name: name.to_string(),
                sort_name: None,
                musicbrainz_artist_id: None,
                discogs_artist_id: None,
            },
        },
        join: join.map(str::to_string),
    }
}

/// The name the library holds for artist `artist_id`.
async fn artist_name(db: &bae_store::Store, artist_id: &str) -> String {
    test_records(db)
        .find_artist_by_id(artist_id)
        .await
        .unwrap()
        .expect("the library holds the artist")
        .name
}

/// The draft's album credits print as one line: each credit's printed name,
/// after the join the credit before it states, or a comma. A picked artist
/// prints the credit name the draft names it by while the artist goes by it,
/// else its own name, which another device can rename.
#[test]
fn the_album_credits_print_as_one_line() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        devices.pause().await;
        let lib = Library::on(devices.a().clone()).await;
        meet_musicbrainz_artist(&lib.records(), PICKED_MUSICBRAINZ, "Picked Artist", None).await;
        let picked_id = musicbrainz_artist(PICKED_MUSICBRAINZ);
        lib.run(lib.scan_dated("Album", 100) + lib.draft("Album", "Album Title")).await;
        let shown = |artists: &str| {
            moves([(
                lib.release("Album"),
                Some(found("Album").dated(100).shown("Album Title", Some(artists))),
            )])
        };
        let first = credited("First Artist", None, Some(" & "));
        let second = credited("Second Artist", Some("Printed Second"), None);
        assert_eq!(
            lib.moved(lib.credit("Album", vec![first.clone(), second.clone()])).await,
            shown("First Artist & Printed Second")
        );
        let picked = Credited {
            artist: ArtistAssignment::Picked {
                artist: bae_records::ExistingArtist {
                    artist_id: picked_id.clone(),
                    name: "Picked Artist".to_string(),
                    sort_name: None,
                    musicbrainz_artist_id: Some(PICKED_MUSICBRAINZ.to_string()),
                    discogs_artist_id: None,
                },
                credit_name: Some(bae_records::credits::CreditName::of("Other Name")),
            },
            join: None,
        };
        assert_eq!(
            lib.moved(lib.credit("Album", vec![first.clone(), second.clone(), picked.clone()]))
                .await,
            shown("First Artist & Printed Second, Picked Artist")
        );
        let second = Credited { join: Some(" feat. ".to_string()), ..second };
        assert_eq!(
            lib.moved(lib.credit("Album", vec![first.clone(), second.clone(), picked.clone()]))
                .await,
            shown("First Artist & Printed Second feat. Picked Artist")
        );
        // B, apart, meets the picked artist under another name; the later
        // write's name lands on A.
        meet_musicbrainz_artist(
            &test_records(devices.b()),
            PICKED_MUSICBRAINZ,
            "Renamed Artist",
            None,
        )
        .await;
        assert_eq!(
            lib.moved_after(async {
                devices.resume().await;
                devices
                    .settle("the picked artist's new name", async |_, db| {
                        artist_name(db, &musicbrainz_artist(PICKED_MUSICBRAINZ)).await
                            == "Renamed Artist"
                    })
                    .await;
            })
            .await,
            shown("First Artist & Printed Second feat. Renamed Artist")
        );
        let records = lib.records();
        let other_name = bae_records::credits::CreditName::of("Other Name");
        let entered = |name: &str| bae_store::EnteredName::parse(name).unwrap();
        assert_eq!(
            lib.moved_after(async {
                records.add_artist_credit_name(&picked_id, &entered("Other Name")).await.unwrap()
            })
            .await,
            shown("First Artist & Printed Second feat. Other Name")
        );
        assert_eq!(
            lib.moved_after(async {
                records
                    .rename_artist_credit_name(&picked_id, &other_name.key, &entered("Other NAME"))
                    .await
                    .unwrap()
            })
            .await,
            shown("First Artist & Printed Second feat. Other NAME")
        );
        assert_eq!(
            lib.moved_after(async {
                records.remove_artist_credit_name(&picked_id, &other_name.key).await.unwrap()
            })
            .await,
            shown("First Artist & Printed Second feat. Renamed Artist")
        );
        assert_eq!(
            lib.moved(lib.credit("Album", vec![second, picked])).await,
            shown("Printed Second feat. Renamed Artist")
        );
        // An artist no draft credits renames nothing.
        devices.pause().await;
        meet_musicbrainz_artist(&lib.records(), "other-artist-mbid", "Other Artist", None).await;
        meet_musicbrainz_artist(
            &test_records(devices.b()),
            "other-artist-mbid",
            "Still Other",
            None,
        )
        .await;
        assert_eq!(
            lib.moved_after(async {
                devices.resume().await;
                devices
                    .settle("the other artist's new name", async |_, db| {
                        artist_name(db, &musicbrainz_artist("other-artist-mbid")).await
                            == "Still Other"
                    })
                    .await;
            })
            .await,
            moves([])
        );
    });
}

/// A link to the catalogs makes the release identified and shows its draft.
#[tokio::test]
async fn a_linked_release_is_identified() {
    let lib = Library::new().await;
    lib.run(lib.scan("Album")).await;
    assert_eq!(
        lib.moved(lib.linked("Album")).await,
        moves([(
            lib.release("Album"),
            Some(found("Album").state("identified").shown("", Some("")))
        )])
    );
    assert_eq!(
        lib.moved(lib.link("Album", None)).await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// A failed import keeps its release on Found, saying so, until an import of
/// it lands.
#[tokio::test]
async fn a_failed_import_is_founds_import_error() {
    let lib = Library::new().await;
    lib.run(lib.scan("Album")).await;
    assert_eq!(
        lib.moved(lib.import_failed("Album")).await,
        moves([(lib.release("Album"), Some(found("Album").state("import_error")))])
    );
    assert_eq!(
        lib.moved(lib.imported("Album", "2026-01-01T00:00:00Z")).await,
        moves([(
            lib.release("Album"),
            Some(found("Album").on("done").dated(1_767_225_600_000).shown("Album Title", Some("")))
        )])
    );
}

/// What is running for a Found release decides its state over the tables: an
/// import, then a run, then a run's answer bae could not store.
#[tokio::test]
async fn what_is_running_decides_a_found_releases_state() {
    let lib = Library::new().await;
    lib.run(lib.scan("Album") + lib.verdict("Album", "not_found", None)).await;
    let now = |state: &str| moves([(lib.release("Album"), Some(found("Album").state(state)))]);
    let run = crate::IdentifyRunId::of_number(1);
    assert_eq!(lib.moved(lib.importing("Album", "queued")).await, now("importing"));
    assert_eq!(
        lib.moved(
            lib.import_moved("Album", "reading_files")
                + lib.identification(
                    "Album",
                    vec![IdentificationChange::Admit {
                        files: lib.files("Album"),
                        admission: crate::Admission::Requested,
                    }]
                )
        )
        .await,
        moves([])
    );
    assert_eq!(lib.moved(lib.import_ended("Album")).await, now("identifying"));
    assert_eq!(
        lib.moved(lib.identification(
            "Album",
            vec![
                IdentificationChange::RunStarted { files: lib.files("Album"), run },
                IdentificationChange::RunAnswered { run, reads_pick: true },
            ]
        ))
        .await,
        now("identified")
    );
    assert_eq!(
        lib.moved(lib.identification(
            "Album",
            vec![IdentificationChange::AnswerFailed {
                files: lib.files("Album"),
                run,
                failure: crate::SaveFailure::NotWritten { error: "disk full".to_string() },
            }]
        ))
        .await,
        now("error")
    );
    assert_eq!(
        lib.moved(lib.identification("Album", vec![IdentificationChange::Ended])).await,
        moves([(lib.release("Album"), Some(found("Album").needs_you("not_found")))])
    );
    // Off Found nothing running is a state.
    lib.run(lib.skip("Album")).await;
    assert_eq!(lib.moved(lib.importing("Album", "queued")).await, moves([]));
}

/// A release imported and then deleted from the library holds no draft, and
/// shows its folder's name again.
#[tokio::test]
async fn a_release_whose_draft_is_gone_shows_its_folder() {
    let lib = Library::new().await;
    lib.run(
        lib.scan("Album")
            + lib.draft("Album", "Album Title")
            + lib.imported("Album", "2026-01-01T00:00:00Z"),
    )
    .await;
    assert_eq!(
        lib.moved(lib.release_deleted("Album")).await,
        moves([(lib.release("Album"), Some(found("Album")))])
    );
}

/// A draft stored already written, where none was, shows as soon as it is
/// stored.
#[tokio::test]
async fn a_draft_stored_written_shows_at_once() {
    let lib = Library::new().await;
    lib.run(
        lib.scan("Album")
            + lib.imported("Album", "2026-01-01T00:00:00Z")
            + lib.release_deleted("Album"),
    )
    .await;
    assert_eq!(
        lib.moved(lib.metadata(
            "Album",
            crate::tables::import_state::tests::metadata_draft("Tagged Title", "Artist"),
            None
        ))
        .await,
        moves([(lib.release("Album"), Some(found("Album").shown("Tagged Title", Some("Artist"))))])
    );
}

/// A release whose stored state is current, ready for a verdict.
async fn looked_up() -> Library {
    let lib = Library::new().await;
    lib.run(lib.scan("Album")).await;
    lib
}

/// The one move a write to the verdict of `Album` makes.
fn now(lib: &Library, placed: Placed) -> Moved {
    moves([(lib.release("Album"), Some(placed))])
}

/// The verdict kinds that hold no find place the release by themselves.
#[tokio::test]
async fn each_verdict_kind_places_its_release() {
    for (kind, track_count, placed) in [
        ("not_found", None, found("Album").needs_you("not_found")),
        ("manual_only", Some(11), found("Album").needs_you("nothing_to_look_up")),
        ("failed", Some(11), found("Album").state("lookup_error")),
        ("error", None, found("Album").state("error")),
        // Nothing found that agreement kept.
        ("found", Some(11), found("Album").needs_you("not_found")),
    ] {
        let lib = looked_up().await;
        assert_eq!(
            lib.moved(lib.verdict("Album", kind, track_count)).await,
            now(&lib, placed),
            "{kind}"
        );
    }
}

/// The run's ledger, saying what each step found: nothing anywhere.
fn nothing_found_ledger() -> bae_identify::IdentifyRunView {
    bae_identify::IdentifyRunView {
        providers: vec![LookupCatalog::MusicBrainz],
        disc_id: bae_identify::DiscIdStepView::Absent,
        barcode: bae_identify::BarcodeStepView::Absent,
        catalog: bae_identify::CatalogStepView::NoneFound,
        isrc: bae_identify::IsrcStepView::Absent,
        search: bae_identify::SearchStepView::NotNeeded,
    }
}

/// What the ledger records of a run moves nothing; the person keeping their
/// own draft over the verdict leaves the release unmatched, and a file
/// decision clearing the verdict leaves it not looked up.
#[tokio::test]
async fn keeping_ones_own_draft_leaves_the_release_unmatched() {
    let lib = looked_up().await;
    lib.run(lib.verdict("Album", "not_found", None)).await;
    assert_eq!(
        lib.moved(lib.concluded(
            "Album",
            TerminalVerdict::NotFoundAnywhere { ledger: Some(nothing_found_ledger()) }
        ))
        .await,
        moves([])
    );
    assert_eq!(
        lib.moved(lib.kept_own_draft("Album")).await,
        now(&lib, found("Album").state("unmatched"))
    );
    assert_eq!(lib.moved(lib.file_decision("Album")).await, now(&lib, found("Album")));
}

/// A found verdict picks its one pressing when its lead's tracklist fits the
/// folder; otherwise it says why it picks none.
#[tokio::test]
async fn a_found_verdict_is_judged_by_its_lead() {
    for (source_tracks, placed) in [
        ((Some("listed"), Some(11)), found("Album").state("unmatched")),
        ((Some("listed"), Some(12)), found("Album").needs_you("not_found")),
        ((Some("unreadable"), None), found("Album").needs_you("not_found")),
        ((Some("nothing"), None), found("Album").needs_you("no_tracklist")),
        ((None, None), found("Album").needs_you("no_tracklist")),
    ] {
        let lib = looked_up().await;
        lib.run(lib.verdict("Album", "found", Some(11))).await;
        // Before it found anything, it found nothing that could be the
        // folder's.
        let expected = match placed == found("Album").needs_you("not_found") {
            true => moves([]),
            false => now(&lib, placed),
        };
        assert_eq!(
            lib.moved(
                lib.concluded("Album", found_verdict(11, [(found_release(0, source_tracks), 0)]))
            )
            .await,
            expected,
            "{source_tracks:?}"
        );
    }
}

/// Several pressings are the person's to pick between; the lead is the
/// release at the lowest position agreement kept; a record whose document
/// went unread is a lookup error; and the folder's own files ruling out
/// every release is named over all of it. Each is a lookup run again.
#[tokio::test]
async fn a_found_verdict_names_what_holds_its_pick_back() {
    let lib = looked_up().await;
    let fits = found_release(1, FITS);
    let longer = found_release(0, (Some("listed"), Some(12)));
    lib.run(lib.concluded("Album", found_verdict(11, [(fits.clone(), 0)]))).await;
    assert_eq!(
        lib.moved(
            lib.concluded("Album", found_verdict(11, [(longer.clone(), 1), (fits.clone(), 0)]))
        )
        .await,
        now(&lib, found("Album").needs_you("matches"))
    );
    let mut narrowed = found_verdict(11, [(fits.clone(), 0)]);
    findings_of(&mut narrowed).narrowed_out =
        bae_identify::NarrowedOut { matches: vec![longer.clone()], pressings: vec![0] };
    assert_eq!(
        lib.moved(lib.concluded("Album", narrowed)).await,
        now(&lib, found("Album").state("unmatched"))
    );
    let mut only_ledger_lists = found_verdict(11, [(fits.clone(), 0)]);
    let TerminalVerdict::Found { ledger, .. } = &mut only_ledger_lists else {
        unreachable!("a found verdict")
    };
    *ledger = Some(bae_identify::IdentifyRunView {
        disc_id: bae_identify::DiscIdStepView::Read {
            discs: vec![bae_identify::DiscIdRow {
                disc_id: "disc-1".to_string(),
                lookup: bae_identify::LookupView::Found {
                    count: 1,
                    groups: bae_identify::release_group::group_results(
                        bae_identify::release_group::unranked(vec![longer.clone()]),
                        None,
                    ),
                },
            }],
        },
        ..nothing_found_ledger()
    });
    assert_eq!(
        lib.moved(lib.concluded("Album", only_ledger_lists)).await,
        moves([]),
        "a release only the ledger lists is no more offered than one set aside"
    );
    assert_eq!(
        lib.moved(
            lib.concluded("Album", found_verdict(11, [(longer.clone(), 0), (fits.clone(), 0)]))
        )
        .await,
        now(&lib, found("Album").needs_you("not_found"))
    );
    let unread = MetadataResult {
        document_failure: Some(bae_catalogs::LookupFailure::Network),
        ..found_release(0, FITS)
    };
    assert_eq!(
        lib.moved(
            lib.concluded("Album", found_verdict(11, [(unread.clone(), 0), (fits.clone(), 0)]))
        )
        .await,
        now(&lib, found("Album").state("lookup_error"))
    );
    let mut ruled_out = found_verdict(11, [(unread, 0), (fits.clone(), 0)]);
    findings_of(&mut ruled_out).medium_conflict = Some(bae_identify::MediumConflict::CdRip);
    assert_eq!(
        lib.moved(lib.concluded("Album", ruled_out)).await,
        now(&lib, found("Album").needs_you("not_found"))
    );
    assert_eq!(
        lib.moved(lib.concluded("Album", found_verdict(11, [(fits.clone(), 0)]))).await,
        now(&lib, found("Album").state("unmatched"))
    );
    let retitled = MetadataResult { title: "Retitled".to_string(), ..fits };
    assert_eq!(
        lib.moved(lib.concluded("Album", found_verdict(11, [(retitled, 0)]))).await,
        moves([])
    );
}

/// A verdict counts only through a state at the folder's current files: one
/// stored while a pass that read the folder before its file decisions landed
/// lists it places nothing.
#[tokio::test]
async fn a_verdict_of_older_files_places_nothing() {
    let lib = Library::new().await;
    lib.run(lib.scan("Album") + lib.file_decision("Album") + lib.scan_revised("Album", 0)).await;
    assert_eq!(
        lib.moved(lib.concluded_at("Album", verdict_of("not_found", None), 1)).await,
        moves([])
    );
}
