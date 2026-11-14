//! What the import list's reads cost: its windows at the top, middle and end
//! of lists of each measured size, reached by a jump and by scrolling, and
//! what typing a search and opening a view at its first selected row cost.
//! Measurements, not checks; each is ignored and run in release.

use super::*;

/// The reads of the import list's Found tab at each measured offset, in each
/// order: a window scrolled to, a closed range read from the key at the
/// offset (`scroll`); a jump to the offset, `key_at` and then the range read
/// from the key it gives (`jump`); and the rows before the key at the offset
/// (`position`), the count the screen follows to place a window.
async fn measure_import_list(db: &Store, n: u64) {
    measure_view(db, n, "import_list", "", String::new(), crate::PendingFilter::All).await;
}

/// The same reads of a list holding `n` rows under one view, reported under
/// the list name `name`, its orders named with `variant` after them.
async fn measure_view(
    db: &Store,
    n: u64,
    name: &str,
    variant: &str,
    filter_text: String,
    pending_filter: crate::PendingFilter,
) {
    let tables = crate::tables::ImportTables::new(db.clone());
    use crate::{ImportListOrder, ImportListView};
    for order in [ImportListOrder::PathAscending, ImportListOrder::NewestFirst] {
        let list = tables
            .open_import_list(ImportListView {
                tab: crate::TriageTab::Pending,
                filter_text: filter_text.clone(),
                pending_filter,
                order,
            })
            .await
            .unwrap();
        let order_name = format!("{order:?}{variant}");
        for (at, offset) in offsets(n) {
            let key = tables.import_key_at(list, offset).await.unwrap().expect("the list has rows");
            let end = match offset + LIMIT < n {
                true => tables.import_key_at(list, offset + LIMIT).await.unwrap(),
                false => None,
            };
            let range = ListWindow::Forward { from: Some(key.clone()), until: end, limit: LIMIT };
            let mut scroll = Vec::with_capacity(RUNS);
            let mut jump = Vec::with_capacity(RUNS);
            let mut position = Vec::with_capacity(RUNS);
            let mut rows = 0;
            for _ in 0..RUNS {
                let started = Instant::now();
                rows =
                    tables.read_import_window(list, range.clone()).await.unwrap().read.rows.len();
                scroll.push(started.elapsed());

                let started = Instant::now();
                let reached = tables.import_key_at(list, offset).await.unwrap().unwrap();
                let jumped = ListWindow::Forward { from: Some(reached), until: None, limit: LIMIT };
                tables.read_import_window(list, jumped).await.unwrap();
                jump.push(started.elapsed());

                let started = Instant::now();
                tables
                    .subscribe_import_position(list, key.clone())
                    .next()
                    .await
                    .unwrap()
                    .result
                    .unwrap();
                position.push(started.elapsed());
            }
            for (reach, mut runs) in [("scroll", scroll), ("jump", jump), ("position", position)] {
                runs.sort();
                report(name, &order_name, reach, at, offset, n, (runs[RUNS / 2], rows));
            }
        }
        tables.close_import_list(list).await.unwrap();
    }
}

/// The sizes the import list is measured at: a queue as large as any library
/// a person holds reaches the last.
pub(super) const SIZES: [u64; 3] = [20_000, 100_000, 500_000];

/// What one window of the import list costs to read, at the top, the middle
/// and the end of the list, at [`SIZES`],
/// reached by a jump and by scrolling through the list's live query. A
/// measurement, not a check: run it in release with
///
/// ```sh
/// scripts/cargo.sh test --release -p bae-import --features test-utils --lib -- \
///     import_list_window_cost --ignored --nocapture --test-threads 1
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn import_list_window_cost() {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let mut held = 0;
    for n in SIZES {
        let took = grow_import_list(&db, tmp.path(), held, n).await;
        eprintln!("WINDOW_COST scanned n={n} in {:.1} s", took.as_secs_f64());
        held = n;
        measure_import_list(&db, n).await;
        measure_import_list_narrowed(&db).await;
        measure_import_list_typed(&db).await;
        measure_import_list_first_selected(&db).await;
    }
    measure_import_list_headers(tmp.path()).await;
}

/// [`import_list_window_cost`]'s reads of a text-filtered list alone, and
/// what typing its search costs, each size grown in a store of its own. A
/// measurement, not a check: run it in release with
///
/// ```sh
/// scripts/cargo.sh test --release -p bae-import --features test-utils --lib -- \
///     import_list_search_window_cost --ignored --nocapture --test-threads 1
/// ```
#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn import_list_search_window_cost() {
    for n in SIZES {
        let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
        // In watched folders of 20,000 rows each: a scan's write passes the
        // rows of its watched folder, so one folder of 500,000 would take
        // hours to fill.
        let mut took = Duration::ZERO;
        for start in (0..n).step_by(20_000) {
            took += grow_import_list(&db, tmp.path(), start, (start + 20_000).min(n)).await;
        }
        eprintln!("WINDOW_COST scanned n={n} in {:.1} s", took.as_secs_f64());
        let text = "Album 1";
        let view =
            found_view(text, crate::PendingFilter::All, crate::ImportListOrder::PathAscending);
        let left = rows_shown(&db, view).await;
        measure_view(
            &db,
            left,
            "import_list_narrowed",
            "+text",
            text.to_string(),
            crate::PendingFilter::All,
        )
        .await;
        measure_import_list_typed(&db).await;
    }
}

/// How many rows a list holding `view` shows, headers included.
pub(super) async fn rows_shown(db: &Store, view: crate::ImportListView) -> u64 {
    let tables = crate::tables::ImportTables::new(db.clone());
    let list = tables.open_import_list(view).await.unwrap();
    let total = tables.read_import_list_for_test(list).await.unwrap().total_count;
    tables.close_import_list(list).await.unwrap();
    total
}

pub(super) fn found_view(
    filter_text: &str,
    pending_filter: crate::PendingFilter,
    order: crate::ImportListOrder,
) -> crate::ImportListView {
    crate::ImportListView {
        tab: crate::TriageTab::Pending,
        filter_text: filter_text.to_string(),
        pending_filter,
        order,
    }
}

/// [`measure_import_list`]'s reads with the list narrowed: by a filter text
/// that matches the albums numbered 1xxxxx, about one in ten, and by the
/// pending filter NotLookedUp. Each reports under `import_list_narrowed`,
/// the narrowing named after the order, and `n` the rows it leaves.
async fn measure_import_list_narrowed(db: &Store) {
    use crate::{ImportListOrder::PathAscending, PendingFilter};
    let text = "Album 1";
    for (variant, filter_text, pending_filter) in
        [("+text", text, PendingFilter::All), ("+NotLookedUp", "", PendingFilter::NotLookedUp)]
    {
        let left = rows_shown(db, found_view(filter_text, pending_filter, PathAscending)).await;
        let all = rows_shown(db, found_view("", PendingFilter::All, PathAscending)).await;
        eprintln!(
            "WINDOW_COST narrowing variant={variant} filter_text={filter_text:?} left={left} of={all}"
        );
        measure_view(
            db,
            left,
            "import_list_narrowed",
            variant,
            filter_text.to_string(),
            pending_filter,
        )
        .await;
    }
}

/// What typing a search costs: the write that sets the list's search and
/// the read of its first window that follows, the median of [`RUNS`] searches
/// each typed after the list's search was cleared, for a search that keeps
/// about one release in ten (`+text`) and one that keeps every release
/// (`+every`), reported under `import_list_typed` with `n` the items the
/// search keeps.
async fn measure_import_list_typed(db: &Store) {
    use crate::{ImportListOrder::PathAscending, PendingFilter};
    let tables = crate::tables::ImportTables::new(db.clone());
    for (variant, text) in [("+text", "Album 1"), ("+every", "album")] {
        let list = tables
            .open_import_list(found_view("", PendingFilter::All, PathAscending))
            .await
            .unwrap();
        let mut runs = Vec::with_capacity(RUNS);
        let mut kept = 0;
        for _ in 0..RUNS {
            let started = Instant::now();
            tables
                .set_import_list_view(list, found_view(text, PendingFilter::All, PathAscending))
                .await
                .unwrap();
            let mut window = tables.subscribe_import_window(list, first(LIMIT));
            window.next().await.unwrap().result.unwrap();
            runs.push(started.elapsed());
            kept = tables.subscribe_import_count(list).next().await.unwrap().unwrap().rows;
            tables
                .set_import_list_view(list, found_view("", PendingFilter::All, PathAscending))
                .await
                .unwrap();
        }
        tables.close_import_list(list).await.unwrap();
        runs.sort();
        report(
            "import_list_typed",
            &format!("{PathAscending:?}{variant}"),
            "type",
            "top",
            0,
            kept,
            (runs[RUNS / 2], LIMIT as usize),
        );
    }
}

/// The write that opens a view at its first selected row, with one row
/// selected at the middle of the list and one at its end: the median of
/// [`RUNS`] writes to each of two orders, reported under
/// `import_list_first_selected`. The middle row is the first selected in
/// ascending order and the end row in descending order, so `at` names that
/// row and `offset` the place the write opened the view at.
async fn measure_import_list_first_selected(db: &Store) {
    use crate::{ImportListOrder::PathAscending, ImportListOrder::PathDescending, PendingFilter};
    let tables = crate::tables::ImportTables::new(db.clone());
    let list =
        tables.open_import_list(found_view("", PendingFilter::All, PathAscending)).await.unwrap();
    let total = rows_shown(db, found_view("", PendingFilter::All, PathAscending)).await;
    let candidates = async |offset: u64| -> Vec<crate::CandidateKey> {
        let key = tables.import_key_at(list, offset).await.unwrap().unwrap();
        tables
            .read_import_window(list, from(&key, 10))
            .await
            .unwrap()
            .read
            .rows
            .iter()
            .filter_map(|item| match item {
                crate::ImportListItem::Candidate { row, .. } => Some(row.candidate_key.clone()),
                _ => None,
            })
            .collect()
    };
    let middle = candidates(total / 2).await.remove(0);
    let end = candidates(total - 10).await.pop().unwrap();
    tables
        .change_import_list_selection(list, SelectionChange::Replace { keys: vec![middle, end] })
        .await
        .unwrap();
    let mut runs = [Vec::with_capacity(RUNS), Vec::with_capacity(RUNS)];
    let mut opened = [0, 0];
    for run in 0..2 * RUNS + 2 {
        let which = (run % 2 == 1) as usize;
        let order = [PathDescending, PathAscending][which];
        let started = Instant::now();
        let opening = tables
            .set_import_list_view(list, found_view("", PendingFilter::All, order))
            .await
            .unwrap();
        let took = started.elapsed();
        if run >= 2 {
            runs[which].push(took);
        }
        opened[which] = match opening {
            Some(crate::ImportListOpening::At { position, .. }) => position,
            Some(crate::ImportListOpening::Top) | None => 0,
        };
    }
    for (which, order, at) in [(1, PathAscending, "middle"), (0, PathDescending, "end")] {
        runs[which].sort();
        report(
            "import_list_first_selected",
            &format!("{order:?}"),
            "open",
            at,
            opened[which],
            total,
            (runs[which][RUNS / 2], 1),
        );
    }
    tables.close_import_list(list).await.unwrap();
}

/// The window reads of a list of groups of five albums, every group open,
/// at three header counts, reported under `import_list_headers`, and the
/// same reads of lists with no group holding as many rows, named
/// `+flat` after the order.
async fn measure_import_list_headers(tmp: &std::path::Path) {
    let mut held = 0;
    let (grouped, _grouped_dir) = empty_db(crate::TEST_SCHEMA).await;
    for headers in [100, 1_000, 5_000] {
        let albums = headers * 5;
        grow_grouped_import_list(&grouped, tmp, held, albums).await;
        held = albums;
        let rows = rows_shown(
            &grouped,
            found_view("", crate::PendingFilter::All, crate::ImportListOrder::PathAscending),
        )
        .await;
        eprintln!("WINDOW_COST headers headers={headers} albums={albums} rows_shown={rows}");
        measure_view(
            &grouped,
            rows,
            "import_list_headers",
            "",
            String::new(),
            crate::PendingFilter::All,
        )
        .await;
        let (flat, flat_dir) = empty_db(crate::TEST_SCHEMA).await;
        grow_flat_import_list(&flat, flat_dir.path(), 0, rows).await;
        eprintln!(
            "WINDOW_COST flat rows_shown={}",
            rows_shown(
                &flat,
                found_view("", crate::PendingFilter::All, crate::ImportListOrder::PathAscending)
            )
            .await
        );
        measure_view(
            &flat,
            rows,
            "import_list_headers",
            "+flat",
            String::new(),
            crate::PendingFilter::All,
        )
        .await;
    }
}
