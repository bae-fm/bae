//! The import pane's candidate through the app: the pane's live read moves
//! with what core does to the candidate it shows.

use crate::test_import_selection::{open, scan_albums};
use bae_import::{ArtistAssignment, CandidateAction, CandidateStage, ImportCandidateDetail};
use bae_util::test_channels::HANG_GUARD;
use tempfile::TempDir;

/// The pane's values until one `accept` admits.
async fn until(
    pane: &bae_library::LiveDetail<ImportCandidateDetail, bae_import::CandidateKey>,
    accept: impl Fn(&ImportCandidateDetail) -> bool,
) -> ImportCandidateDetail {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let snapshot = pane.next().await.expect("the pane answers");
            if let Some(detail) = snapshot.value.filter(|detail| accept(detail)) {
                return detail;
            }
        }
    })
    .await
    .expect("the pane gets there")
}

/// A candidate imported while its pane is open moves the pane to the release
/// it became: the pane's next value is the imported stage, with the release,
/// and it offers nothing but showing the folder.
#[tokio::test(flavor = "multi_thread")]
async fn an_import_moves_the_open_pane_to_the_release() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let services = open(&tmp.path().join("db")).await;
    let collection = tmp.path().join("Collection");
    scan_albums(&services, &collection, 1).await;
    let key = bae_import::CandidateKey::folder(&collection.join("Album 0"));
    services.import_select_candidate_file_tags(key.clone()).await.unwrap();
    services
        .import_set_candidate_album_artists(
            &key,
            vec![bae_import::Credited::plain(ArtistAssignment::named("Artist Name"))],
        )
        .await
        .unwrap();

    let pane = services.subscribe_import_candidate();
    pane.set(Some(key.clone())).unwrap();
    let drafting =
        until(&pane, |detail| detail.live.actions.contains(&CandidateAction::Import)).await;
    assert!(matches!(drafting.stage, CandidateStage::Drafting(_)));

    assert_eq!(services.pane_start_import(&key).await.unwrap(), bae_import::PaneOutcome::Done);

    let imported =
        until(&pane, |detail| matches!(detail.stage, CandidateStage::Imported { .. })).await;
    let CandidateStage::Imported { release, .. } = &imported.stage else { unreachable!() };
    assert_eq!(
        imported.import_status,
        Some(bae_import::CandidateImportStatus::Complete { release: release.clone() })
    );
    assert_eq!(imported.live.actions, vec![CandidateAction::RevealFolder]);
    services.close().await.unwrap();
}
