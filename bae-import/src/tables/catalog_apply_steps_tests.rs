//! Each apply's step row, as the apply's own steps write it.

use bae_catalogs::catalog::Catalog;
use bae_store::test_fixtures::empty_db;
use bae_util::test_channels::HANG_GUARD;

use crate::tables::ImportTables;
use crate::{CatalogApplyId, CatalogApplyStep, CatalogApplySteps};

type StepQuery = coven::LiveQuery<Option<CatalogApplyStep>, bae_store::DbError>;

/// The steps of the apply `id` names, over `tables`.
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
            let step = query.next().await.expect("the query stays open").expect("the query reads");
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
    let (read, ()) = tokio::join!(steps.during(reading, finished), watch);
    read.expect("the read ends when it is told to");
    step_until(&mut query, None).await;

    let cover = CatalogApplyStep::FetchingCover(Catalog::MusicBrainz);
    let fetched = steps.during(cover, async { "the cover" }).await;
    assert_eq!(fetched, "the cover");
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

    let read = steps.during(reading, std::future::pending::<()>());
    tokio::select! {
        () = read => unreachable!("the read never ends"),
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
