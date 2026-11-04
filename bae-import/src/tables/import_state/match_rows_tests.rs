//! The release-match rows as stored and read back.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use bae_catalogs::catalog::LookupCatalog;
use bae_db::test_fixtures::empty_db;
use bae_db::watched_path::host_root;
use bae_identify::{Findings, NarrowedOut, TerminalVerdict};

/// A stored verdict's match reads its album's first year from the catalog
/// release it names, as that release is stored now: none while it is not
/// stored, whatever the run read, and a later fetch's once one stores it.
#[tokio::test]
async fn a_match_reads_its_album_first_year_from_its_stored_catalog_release() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut findings = sample_findings();
    findings.matches[0].album_first_year = Some(1998);
    let verdict = TerminalVerdict::Found { findings, track_count: 11, ledger: None };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();
    let first_year = || async {
        let loaded = tables.load_import_candidate_states().await.unwrap();
        let TerminalVerdict::Found { findings, .. } =
            &loaded[&hash].identify.as_ref().expect("a stored verdict reads back").verdict
        else {
            panic!("the stored verdict reads back as found");
        };
        findings.matches[0].album_first_year
    };
    assert_eq!(first_year().await, None);

    for year in [1998, 2001] {
        let mut release = fetched_release("rel-1");
        release.metadata.album.first_year = Some(year);
        release.metadata.album.year = Some(year);
        tables.save_source_release(&release).await.unwrap();
        assert_eq!(first_year().await, Some(year));
    }
}

/// Only a run's ranking reads a release's track titles and notes, so a stored
/// verdict keeps neither: its releases read back without them.
#[tokio::test]
async fn a_stored_verdict_keeps_no_track_titles_or_notes() {
    let read_back = TerminalVerdict::Found {
        findings: Findings { named_notes: Vec::new(), ..sample_findings() },
        track_count: 11,
        ledger: None,
    };
    let mut live = read_back.clone();
    let TerminalVerdict::Found { findings, .. } = &mut live else {
        unreachable!("built as a found verdict");
    };
    findings.matches[0].track_titles = vec!["Track One".to_string(), "Track Two".to_string()];
    findings.matches[0].notes = vec!["Pressed By Plant Name".to_string()];

    let (read, _) = stored_and_read(&live).await;
    assert_eq!(read, read_back);
}

/// Everything the rows and cards are grouped by reads back, so a stored verdict
/// groups as its run did.
#[tokio::test]
async fn round_trip_preserves_the_evidence_the_rows_are_paired_by() {
    use bae_catalogs::album_links::AlbumLinks;
    use bae_catalogs::catalog::Catalog;
    use bae_catalogs::records::MetadataRef;
    use bae_types::pressing::{
        DiscogsDetail, Medium, Packaging, Region, ReleaseArea, ReleaseStatus, StatedFormat,
        StatedMedia,
    };

    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut musicbrainz = sample_match();
    musicbrainz.barcodes = vec!["012345678905".to_string()];
    musicbrainz.labels = vec![
        bae_types::pressing::ReleaseLabel::of(Some("Label Name"), Some("CAT-1")),
        bae_types::pressing::ReleaseLabel::of(Some("Second Label"), None),
        bae_types::pressing::ReleaseLabel::of(None, Some("CAT-2")),
    ];
    musicbrainz.media = StatedMedia::PerMedium(vec![Some(Medium::Cd), None]);
    musicbrainz.status = Some(ReleaseStatus::Official);
    musicbrainz.packaging = Some(Packaging::JewelCase);
    musicbrainz.links =
        vec![MetadataRef::new(Catalog::Discogs, "42"), MetadataRef::new(Catalog::Discogs, "43")];
    let album = AlbumLinks::Read(vec![
        MetadataRef::new(Catalog::Discogs, "9"),
        MetadataRef::new(Catalog::Discogs, "7"),
        MetadataRef::new(Catalog::Discogs, "8"),
    ]);
    musicbrainz.album_links = album.clone();
    let mut discogs = sample_match();
    discogs.source = LookupCatalog::Discogs;
    discogs.release_id = "42".to_string();
    discogs.source_group_id = Some("7".to_string());
    discogs.barcodes = vec!["0 12345 67890 5".to_string(), "5051961234567".to_string()];
    discogs.media = StatedMedia::Formats(vec![
        StatedFormat { medium: Some(Medium::Cd), quantity: 2 },
        StatedFormat { medium: None, quantity: 1 },
    ]);
    discogs.area = Some(ReleaseArea::Region(Region::UkAndEurope));
    discogs.status = Some(ReleaseStatus::Promotion);
    discogs.discogs_details = vec![DiscogsDetail::Reissue, DiscogsDetail::Size12In];
    // A cover with every copy the archive serves, one with Discogs's one
    // thumbnail, and one served at its one size.
    musicbrainz.cover_art =
        Some(bae_catalogs::remote_image::RemoteCover::musicbrainz_release("rel-1"));
    discogs.cover_art = bae_catalogs::discogs::remote_cover_from_urls(
        Some("https://images.example/front.jpg"),
        Some("https://images.example/front-150.jpg"),
        "release",
        42,
    );
    let mut undescribed = sample_match();
    undescribed.cover_art = bae_catalogs::discogs::remote_cover_from_urls(
        Some("https://images.example/only.jpg"),
        None,
        "release",
        2,
    );
    undescribed.release_id = "rel-2".to_string();
    undescribed.year = Some(2001);
    undescribed.media = StatedMedia::Undescribed;
    undescribed.album_links = album;
    let mut unread = sample_match();
    unread.release_id = "rel-3".to_string();
    unread.source_group_id = Some("group-2".to_string());
    unread.album_links = AlbumLinks::Unread;
    let mut linked = sample_match();
    linked.source = LookupCatalog::Discogs;
    linked.release_id = "44".to_string();
    linked.source_group_id = Some("7".to_string());
    let matches = vec![musicbrainz, discogs, undescribed, unread, linked];
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            pressings: bae_identify::release_group::form_rows(&matches),
            matches: matches.clone(),
            narrowed_out: NarrowedOut::default(),
            medium_conflict: None,
            named_notes: Vec::new(),
        },
        track_count: 11,
        ledger: None,
    };
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), &verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    let stored = &loaded[&hash].identify.as_ref().expect("a stored verdict reads back").verdict;
    assert_eq!(*stored, verdict);
    let TerminalVerdict::Found { findings: Findings { matches: stored_matches, .. }, .. } = stored
    else {
        panic!("the verdict found releases");
    };
    let live = bae_identify::release_group::group_results(
        bae_identify::release_group::unranked(matches.clone()),
        None,
    );
    let replayed = bae_identify::release_group::group_results(
        bae_identify::release_group::unranked(stored_matches.clone()),
        None,
    );
    assert_eq!(replayed, live);
    assert_eq!(
        live.len(),
        2,
        "the pair joins the two groups on one card; the unread group is its own"
    );
    assert_eq!(
        live[0].pressings().map(|pressing| pressing.releases.len()).collect::<Vec<_>>(),
        vec![2, 1, 1],
        "the linked pair is one row, the others their own"
    );
    assert_eq!(bae_identify::release_group::pressing_count(matches), 4);
}
