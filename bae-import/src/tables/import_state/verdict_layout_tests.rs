//! These tests check the layout that each write storing a verdict, or
//! changing its text, stores with the verdict's rows.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::MetadataResult;
use bae_identify::signals::{TextLine, TextOrigin};
use bae_identify::{Findings, TerminalVerdict};
use bae_store::test_fixtures::empty_db;
use bae_store::Store;

/// A release of its own album, whose label prints catalog number `catalog`.
fn release(release_id: &str, catalog: &str) -> MetadataResult {
    MetadataResult {
        release_id: release_id.to_string(),
        source_group_id: Some(format!("group-{release_id}")),
        labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Label"), Some(catalog))],
        ..sample_match()
    }
}

/// A found verdict offering `a` and `b` as two rows.
fn two_rows() -> TerminalVerdict {
    TerminalVerdict::Found {
        findings: Findings {
            matches: vec![release("a", "CAT-A"), release("b", "CAT-B")],
            pressings: vec![0, 1],
            named_notes: Vec::new(),
            ..sample_findings()
        },
        track_count: 11,
        ledger: None,
    }
}

/// Folder text that prints `CAT-A`.
fn printing_cat_a() -> Vec<TextLine> {
    vec![TextLine { text: "Artist - Album [CAT-A]".to_string(), origin: TextOrigin::FolderName }]
}

/// What each release of `hash`'s verdict agrees with the folder's text about,
/// as `release_id catalog-agreement`, in position order.
async fn catalog_agreements(db: &Store, hash: &str) -> Vec<String> {
    let hash = hash.to_string();
    db.read(move |sql| {
        Ok(sql.query(
            "SELECT release_id || ' ' || agrees_catalog FROM import_candidate_match \
             WHERE content_hash = ? ORDER BY position",
            [hash],
            |row| row.get::<_, String>(0),
        )?)
    })
    .await
    .unwrap()
}

/// Where each release of `hash`'s verdict is laid out, as `release_id card
/// record_order`, in position order.
async fn places(db: &Store, hash: &str) -> Vec<String> {
    let hash = hash.to_string();
    db.read(move |sql| {
        Ok(sql.query(
            "SELECT release_id || ' ' || card || ' ' || record_order \
             FROM import_candidate_match WHERE content_hash = ? ORDER BY position",
            [hash],
            |row| row.get::<_, String>(0),
        )?)
    })
    .await
    .unwrap()
}

/// What `hash`'s rows list, as `listing pressing fact value`.
async fn row_statements(db: &Store, hash: &str) -> Vec<String> {
    let hash = hash.to_string();
    db.read(move |sql| {
        Ok(sql.query(
            "SELECT listing || ' ' || pressing || ' ' || fact || ' ' || value \
             FROM import_candidate_row_statement WHERE content_hash = ? \
             ORDER BY listing, pressing, fact, line, ordinal",
            [hash],
            |row| row.get::<_, String>(0),
        )?)
    })
    .await
    .unwrap()
}

/// Store `verdict` with `pool` as the folder's text, for a scanned candidate.
async fn stored_with_text(verdict: &TerminalVerdict, pool: Vec<TextLine>) -> (Store, String) {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    // The library stays open as long as the test holds its handle.
    std::mem::forget(tmp);
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = candidate.content_hash();
    store_candidate_state(&db, &candidate, &host_root("/music/Album")).await;
    let mut row = new_candidate_row(&hash, &host_root("/music/Album"), verdict);
    row.signals.text_pool = pool;
    assert!(crate::tables::ImportTables::new(db.clone()).store_verdict(&row).await.unwrap());
    (db, hash)
}

/// The write that stores a verdict also stores where each release stands in
/// its row and card, which of its fields the text agrees with, and what each
/// card and row says.
#[tokio::test]
async fn a_stored_verdict_holds_its_layout() {
    let (db, hash) = stored_with_text(&two_rows(), printing_cat_a()).await;

    assert_eq!(catalog_agreements(&db, &hash).await, vec!["a 1", "b 0"]);
    assert_eq!(places(&db, &hash).await, vec!["a 0 0", "b 1 0"]);
    assert_eq!(
        row_statements(&db, &hash)
            .await
            .into_iter()
            .filter(|line| line.contains(" catalog_number "))
            .collect::<Vec<_>>(),
        vec!["offered 0 catalog_number CAT-A", "offered 1 catalog_number CAT-B"]
    );
}

/// Discounting the catalog number the text prints makes the write check the
/// rows again, and the row that matched it no longer agrees.
#[tokio::test]
async fn striking_a_number_out_reorders_the_stored_rows() {
    let (db, hash) = stored_with_text(&two_rows(), printing_cat_a()).await;
    assert_eq!(catalog_agreements(&db, &hash).await, vec!["a 1", "b 0"]);

    crate::tables::ImportTables::new(db.clone())
        .save_import_candidate_lookup_choices(
            &hash,
            &bae_identify::LookupChoices {
                discounted_catalogs: vec!["CAT-A".to_string()],
                ..bae_identify::LookupChoices::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(catalog_agreements(&db, &hash).await, vec!["a 0", "b 0"]);
}

/// Storing new signals for a candidate that has a verdict checks the
/// verdict's rows again against the new text.
#[tokio::test]
async fn new_signals_judge_the_verdict_rows_again() {
    let (db, hash) = stored_with_text(&two_rows(), Vec::new()).await;
    assert_eq!(catalog_agreements(&db, &hash).await, vec!["a 0", "b 0"]);

    let mut row = new_candidate_row(&hash, &host_root("/music/Album"), &two_rows());
    row.signals.text_pool = vec![TextLine {
        text: "Artist - Album [CAT-B]".to_string(),
        origin: TextOrigin::FolderName,
    }];
    assert!(crate::tables::ImportTables::new(db.clone()).store_verdict(&row).await.unwrap());

    assert_eq!(catalog_agreements(&db, &hash).await, vec!["a 0", "b 1"]);
}

/// Storing a verdict puts two records of one pressing in one row, the lead
/// first.
#[tokio::test]
async fn two_records_of_one_pressing_lay_out_as_one_row() {
    let mut discogs = release("d", "CAT-A");
    discogs.source = LookupCatalog::Discogs;
    discogs.source_group_id = None;
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            matches: vec![release("a", "CAT-A"), discogs],
            pressings: vec![0, 0],
            named_notes: Vec::new(),
            ..sample_findings()
        },
        track_count: 11,
        ledger: None,
    };
    let (db, hash) = stored_with_text(&verdict, printing_cat_a()).await;

    assert_eq!(places(&db, &hash).await, vec!["a 0 0", "d 0 1"]);
}

/// Loads the verdict's cards from the stored rows, the way the pane reads
/// them.
async fn stored_cards(
    db: &Store,
    hash: &str,
    verdict: &TerminalVerdict,
) -> bae_identify::VerdictCards {
    crate::tables::ImportTables::new(db.clone()).load_verdict_cards(hash, verdict).await.unwrap()
}

/// The statement returns the cards, badges and counts that the rules build
/// from the verdict.
#[tokio::test]
async fn verdict_cards_statement_matches_the_rules() {
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            matches: vec![release("b", "CAT-B"), release("a", "CAT-A")],
            pressings: vec![0, 1],
            named_notes: Vec::new(),
            narrowed_out: bae_identify::NarrowedOut {
                matches: vec![release("c", "CAT-C")],
                pressings: vec![0],
            },
            ..sample_findings()
        },
        track_count: 11,
        ledger: None,
    };
    let (db, hash) = stored_with_text(&verdict, printing_cat_a()).await;

    let read = stored_cards(&db, &hash, &verdict).await;

    let text = bae_identify::CandidateText::of(&printing_cat_a(), &[], &[]);
    let expected =
        bae_identify::view::reference_cards_of(&bae_identify::RunOutcome::Answered(verdict), &text);
    assert_eq!(read, expected);
}

/// A found verdict offering each of `matches` as its own row.
fn each_its_own_row(matches: Vec<MetadataResult>) -> TerminalVerdict {
    let pressings = (0..matches.len() as u32).collect();
    found(matches, pressings, None, 11)
}

/// A release of the album `album`, which `source` files under that album.
fn of_album(
    source: LookupCatalog,
    release_id: &str,
    album: Option<&str>,
    catalog: &str,
) -> MetadataResult {
    MetadataResult {
        source,
        source_group_id: album.map(str::to_string),
        ..release(release_id, catalog)
    }
}

/// What the statement answers as the album `verdict`'s offered rows share.
async fn shared_album_of(verdict: TerminalVerdict) -> Option<bae_identify::SharedAlbum> {
    let (db, hash) = stored_with_text(&verdict, printing_cat_a()).await;
    stored_cards(&db, &hash, &verdict).await.shared_album
}

/// Two offered rows on one album are several pressings of it: the statement
/// answers the album and each row's lead, the row the folder's text agrees
/// with first.
#[tokio::test]
async fn several_pressings_of_one_album_name_it_in_each_catalog() {
    let mb = |release_id: &str, catalog: &str| {
        of_album(LookupCatalog::MusicBrainz, release_id, Some("group-1"), catalog)
    };
    let shared = shared_album_of(each_its_own_row(vec![mb("b", "CAT-B"), mb("a", "CAT-A")]))
        .await
        .expect("two pressings of one album");
    assert_eq!(
        shared.link.albums(),
        [bae_catalogs::records::MetadataRef::new(
            bae_catalogs::catalog::Catalog::MusicBrainz,
            "group-1"
        )]
    );
    assert_eq!(
        shared.leads,
        vec![
            bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "a"),
            bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "b"),
        ],
        "the rows as the cards order them"
    );
}

#[tokio::test]
async fn one_pressing_offers_no_album_to_be_unsure_about() {
    let mb = of_album(LookupCatalog::MusicBrainz, "a", Some("group-1"), "CAT-A");
    assert_eq!(shared_album_of(each_its_own_row(vec![mb])).await, None);
}

#[tokio::test]
async fn pressings_of_two_albums_share_none() {
    let verdict = each_its_own_row(vec![
        of_album(LookupCatalog::MusicBrainz, "a", Some("group-1"), "CAT-A"),
        of_album(LookupCatalog::MusicBrainz, "b", Some("group-2"), "CAT-B"),
    ]);
    assert_eq!(shared_album_of(verdict).await, None);
}

#[tokio::test]
async fn pressings_no_catalog_files_under_an_album_share_none() {
    let verdict = each_its_own_row(vec![
        of_album(LookupCatalog::Discogs, "a", None, "CAT-A"),
        of_album(LookupCatalog::Discogs, "b", None, "CAT-B"),
    ]);
    assert_eq!(shared_album_of(verdict).await, None);
}

/// The cards the statement reads are the cards the rules build by grouping
/// the rows, and within one card the rows are in order of what the text
/// agrees with.
#[tokio::test]
async fn verdict_cards_order_the_rows_of_one_card_by_agreement() {
    let one_album = |release_id: &str, catalog: &str| {
        of_album(LookupCatalog::MusicBrainz, release_id, Some("group-shared"), catalog)
    };
    let verdict = each_its_own_row(vec![one_album("b", "CAT-B"), one_album("a", "CAT-A")]);
    let (db, hash) = stored_with_text(&verdict, printing_cat_a()).await;

    let read = stored_cards(&db, &hash, &verdict).await;

    let text = bae_identify::CandidateText::of(&printing_cat_a(), &[], &[]);
    let expected =
        bae_identify::view::reference_cards_of(&bae_identify::RunOutcome::Answered(verdict), &text);
    assert_eq!(read.groups, expected.groups);
}

/// A card that offers a row with more agreements comes before one that
/// offers fewer, whichever the run listed first.
#[tokio::test]
async fn verdict_cards_statement_ranks_cards_by_their_best_offered_row() {
    let verdict = two_rows_listing_the_unagreed_first();
    let (db, hash) = stored_with_text(&verdict, printing_cat_a()).await;

    let cards = stored_cards(&db, &hash, &verdict).await;

    let ids: Vec<&str> = cards.groups.iter().map(|group| group.id.as_str()).collect();
    assert_eq!(ids, vec!["group-a", "group-b"]);
}

fn two_rows_listing_the_unagreed_first() -> TerminalVerdict {
    TerminalVerdict::Found {
        findings: Findings {
            matches: vec![release("b", "CAT-B"), release("a", "CAT-A")],
            pressings: vec![0, 1],
            named_notes: Vec::new(),
            ..sample_findings()
        },
        track_count: 11,
        ledger: None,
    }
}

/// The statement counts the rows that the agreement check set aside, across
/// every card.
#[tokio::test]
async fn verdict_cards_statement_counts_rows_set_aside() {
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            matches: vec![release("a", "CAT-A")],
            pressings: vec![0],
            named_notes: Vec::new(),
            narrowed_out: bae_identify::NarrowedOut {
                matches: vec![release("c", "CAT-C"), release("d", "CAT-D")],
                pressings: vec![0, 1],
            },
            ..sample_findings()
        },
        track_count: 11,
        ledger: None,
    };
    let (db, hash) = stored_with_text(&verdict, printing_cat_a()).await;

    let cards = stored_cards(&db, &hash, &verdict).await;

    assert_eq!(cards.narrowed_out_count, 2);
    assert!(cards.shared_album.is_none(), "one offered row is not several pressings");
}

/// A found verdict of `matches` (each its own row unless `pressings` says
/// otherwise), for a folder of `track_count` tracks.
fn found(
    matches: Vec<MetadataResult>,
    pressings: Vec<u32>,
    medium_conflict: Option<bae_identify::MediumConflict>,
    track_count: u32,
) -> TerminalVerdict {
    TerminalVerdict::Found {
        findings: Findings {
            matches,
            pressings,
            named_notes: Vec::new(),
            medium_conflict,
            ..sample_findings()
        },
        track_count,
        ledger: None,
    }
}

/// A release of one album whose tracklist is `source_tracks`.
fn listing(
    release_id: &str,
    source_tracks: Option<bae_catalogs::search::SourceTracks>,
) -> MetadataResult {
    MetadataResult { source_tracks, ..release(release_id, "CAT") }
}

/// The statement states, with the cards, the check against the folder the
/// found release failed and the release the verdict picks, for each case the
/// rules tell apart, as the rules judge them.
#[tokio::test]
async fn verdict_cards_statement_states_the_folder_check_and_the_pick() {
    use bae_catalogs::search::SourceTracks::{Listed, Nothing, Unreadable};
    let fits = || listing("fits", Some(Listed { count: 11 }));
    let cases: Vec<(&str, TerminalVerdict)> = vec![
        ("one pressing that fits picks itself", found(vec![fits()], vec![0], None, 11)),
        (
            "two records of one pressing count once",
            found(
                vec![fits(), listing("second", Some(Listed { count: 11 }))],
                vec![0, 0],
                None,
                11,
            ),
        ),
        (
            "several pressings pick none",
            found(vec![fits(), listing("other", Some(Listed { count: 11 }))], vec![0, 1], None, 11),
        ),
        (
            "a tracklist not read yet states its check",
            found(vec![listing("a", None)], vec![0], None, 11),
        ),
        (
            "a source that lists no tracks states its check",
            found(vec![listing("a", Some(Nothing))], vec![0], None, 11),
        ),
        (
            "another track count states no check and picks none",
            found(vec![listing("a", Some(Listed { count: 12 }))], vec![0], None, 11),
        ),
        (
            "an unreadable tracklist states no check and picks none",
            found(vec![listing("a", Some(Unreadable))], vec![0], None, 11),
        ),
        (
            "an unread document picks none",
            found(
                vec![MetadataResult {
                    document_failure: Some(bae_catalogs::LookupFailure::Network),
                    ..fits()
                }],
                vec![0],
                None,
                11,
            ),
        ),
        (
            "a medium the folder rules out states it, over a fitting release",
            found(vec![fits()], vec![0], Some(bae_identify::MediumConflict::CdRip), 11),
        ),
        (
            "a medium the folder rules out states it, over several pressings",
            found(
                vec![fits(), listing("other", Some(Listed { count: 11 }))],
                vec![0, 1],
                Some(bae_identify::MediumConflict::NotCdAudio),
                11,
            ),
        ),
        ("a verdict that found nothing", found(Vec::new(), Vec::new(), None, 11)),
        ("not found anywhere", TerminalVerdict::NotFoundAnywhere { ledger: None }),
        (
            "a failed verdict picks none, however well its findings fit",
            TerminalVerdict::Failed {
                failures: vec![bae_identify::IdentifyFailure::Search(
                    bae_catalogs::search::SourceFailure {
                        source: bae_catalogs::catalog::Catalog::MusicBrainz,
                        failure: bae_catalogs::LookupFailure::Provider { status: Some(503) },
                    },
                )],
                findings: Findings {
                    matches: vec![fits()],
                    pressings: vec![0],
                    named_notes: Vec::new(),
                    ..sample_findings()
                },
                track_count: 11,
                ledger: None,
            },
        ),
    ];
    for (name, verdict) in cases {
        let (db, hash) = stored_with_text(&verdict, Vec::new()).await;
        let read = stored_cards(&db, &hash, &verdict).await;
        let expected = bae_identify::view::reference_cards_of(
            &bae_identify::RunOutcome::Answered(verdict),
            &bae_identify::CandidateText::of(&[], &[], &[]),
        );
        assert_eq!(
            (read.folder_check, read.unattended_pick),
            (expected.folder_check, expected.unattended_pick),
            "{name}"
        );
    }
}

/// The statement names the release of a fitting pressing as the pick, and
/// the failed check for the other two verdicts, not empty values.
#[tokio::test]
async fn verdict_cards_statement_names_the_picked_release_and_the_failed_check() {
    use bae_catalogs::search::SourceTracks::{Listed, Nothing};
    let picked = found(vec![listing("fits", Some(Listed { count: 11 }))], vec![0], None, 11);
    let (db, hash) = stored_with_text(&picked, Vec::new()).await;
    let cards = stored_cards(&db, &hash, &picked).await;
    assert_eq!((cards.folder_check, cards.unattended_pick), (None, Some("fits".to_string())));

    let unchecked = found(vec![listing("a", Some(Nothing))], vec![0], None, 11);
    let (db, hash) = stored_with_text(&unchecked, Vec::new()).await;
    let cards = stored_cards(&db, &hash, &unchecked).await;
    assert_eq!(
        (cards.folder_check, cards.unattended_pick),
        (Some(bae_identify::FolderCheck::SourceTracksUnknown), None)
    );

    let ruled_out = found(
        vec![listing("a", Some(Listed { count: 11 }))],
        vec![0],
        Some(bae_identify::MediumConflict::CdRip),
        11,
    );
    let (db, hash) = stored_with_text(&ruled_out, Vec::new()).await;
    let cards = stored_cards(&db, &hash, &ruled_out).await;
    assert_eq!(
        (cards.folder_check, cards.unattended_pick),
        (
            Some(bae_identify::FolderCheck::MediumDisagrees {
                folder: bae_identify::MediumConflict::CdRip
            }),
            None
        )
    );
}
