//! Each apply's step row, as the apply's own steps write it.

use bae_catalogs::catalog::Catalog;
use bae_store::test_fixtures::empty_db;
use bae_store::DbError;
use bae_util::test_channels::HANG_GUARD;

use crate::tables::ImportTables;
use crate::{CatalogApplyId, CatalogApplyStep, CatalogApplySteps};

type StepQuery = coven::LiveQuery<Option<CatalogApplyStep>, bae_store::DbError>;

/// The steps of the apply `id` names, over `tables`.
#[expect(clippy::disallowed_methods, reason = "test code: it takes the runtime the test runs on")]
fn shown(id: &CatalogApplyId, tables: &ImportTables) -> CatalogApplySteps {
    CatalogApplySteps::shown(
        id.clone(),
        tables.clone(),
        bae_diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    )
}

/// Pull from `query` until it delivers `wanted`.
async fn step_until(query: &mut StepQuery, wanted: Option<CatalogApplyStep>) {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let step =
                query.next_inline().await.expect("the query stays open").expect("the query reads");
            if step == wanted {
                return;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the apply's row never held {wanted:?}"));
}

/// The apply's row holds a read's step from when the read starts until it
/// ends, and holds nothing between reads.
#[tokio::test]
async fn the_row_holds_each_step_while_its_read_runs() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = ImportTables::new(db);
    let apply = CatalogApplyId::new(&coven::UuidIds);
    let steps = shown(&apply, &tables);
    let mut query = tables.subscribe_catalog_apply_step(&apply);
    step_until(&mut query, None).await;

    let reading = CatalogApplyStep::ReadingRelease(Catalog::Discogs);
    let (finish, finished) = tokio::sync::oneshot::channel::<()>();
    let watch = async {
        step_until(&mut query, Some(reading)).await;
        finish.send(()).expect("the read waits");
    };
    let read = async {
        finished.await.expect("the test tells the read to end");
        Ok::<_, DbError>(())
    };
    let (read, ()) = tokio::join!(steps.during(reading, read), watch);
    read.expect("the apply writes and deletes the step's row");
    step_until(&mut query, None).await;

    let cover = CatalogApplyStep::FetchingCover(Catalog::MusicBrainz);
    let fetched = steps.during(cover, async { Ok::<_, DbError>("the cover") }).await;
    assert_eq!(fetched.expect("the apply writes and deletes the step's row"), "the cover");
    // The query may never see a step this short, so a new one reads the row
    // as the step left it.
    let mut after = tables.subscribe_catalog_apply_step(&apply);
    step_until(&mut after, None).await;
}

/// An apply dropped while a read runs leaves no step in its row.
#[tokio::test]
async fn a_dropped_apply_leaves_no_step() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = ImportTables::new(db);
    let apply = CatalogApplyId::new(&coven::UuidIds);
    let steps = shown(&apply, &tables);
    let mut query = tables.subscribe_catalog_apply_step(&apply);
    let reading = CatalogApplyStep::ReadingRelease(Catalog::MusicBrainz);

    let read = steps.during(reading, std::future::pending::<Result<(), DbError>>());
    tokio::select! {
        _ = read => unreachable!("the read never ends"),
        () = step_until(&mut query, Some(reading)) => {}
    }
    step_until(&mut query, None).await;
}

/// The library opens with no apply's step: no apply outlives the process
/// that wrote it.
#[tokio::test]
async fn the_open_empties_every_step() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = ImportTables::new(db);
    let apply = CatalogApplyId::new(&coven::UuidIds);
    let mut query = tables.subscribe_catalog_apply_step(&apply);
    // A step whose apply neither ended nor was dropped, as a process that
    // ended mid-step leaves it.
    let reading = CatalogApplyStep::FetchingCover(Catalog::Bandcamp);
    tables.start_catalog_apply_step(&apply, reading).await.unwrap();
    step_until(&mut query, Some(reading)).await;

    tables.clear_catalog_apply_steps().await.unwrap();
    step_until(&mut query, None).await;
}

/// A step replaces the step its apply's row holds, so a row that a step left
/// behind never blocks the next one.
#[tokio::test]
async fn a_step_replaces_the_step_its_row_holds() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = ImportTables::new(db);
    let apply = CatalogApplyId::new(&coven::UuidIds);
    let reading = CatalogApplyStep::ReadingRelease(Catalog::Discogs);
    tables.start_catalog_apply_step(&apply, reading).await.unwrap();

    let cover = CatalogApplyStep::FetchingCover(Catalog::Discogs);
    tables.start_catalog_apply_step(&apply, cover).await.unwrap();

    let mut query = tables.subscribe_catalog_apply_step(&apply);
    step_until(&mut query, Some(cover)).await;
}
