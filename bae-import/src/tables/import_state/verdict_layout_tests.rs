//! A verdict's rows as the write that stores them, or that changes what they
//! are judged against, lays them out.

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

/// Striking the number the text prints out of the candidate's choices judges
/// its rows again: the row that agreed on it no longer does.
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

/// New signals for a candidate whose verdict stands judge its rows against
/// the text they hold.
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

/// Two records of one pressing store one row, the lead first, and the row's
/// text.
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

/// The cards the stored rows answer, as the pane reads them.
async fn stored_cards(
    db: &Store,
    hash: &str,
    verdict: &TerminalVerdict,
) -> bae_identify::VerdictCards {
    crate::tables::ImportTables::new(db.clone()).load_verdict_cards(hash, verdict).await.unwrap()
}

/// The statement answers the cards the rules fold, with each row's badges
/// and counts.
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

/// The rows agreement set aside are counted across every card.
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
    assert!(!cards.offers_shared_album, "one offered row is not several pressings");
}
