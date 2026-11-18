use super::*;
use bae_store::list_windows::SortValue;

/// The key of the row at `position` of a list whose rows sort by their
/// position.
fn key(position: u64) -> OrderKey {
    OrderKey::new(vec![SortValue::Integer(position as i64)])
}

/// The keys of the rows from `start` up to `end`.
fn keys(start: u64, end: u64) -> Vec<OrderKey> {
    (start..end).map(key).collect()
}

/// A window from the list's top is opened at once; one further down needs
/// the key there, read once by offset.
#[test]
fn a_new_view_opens_its_window_a_screen_above_the_rows_on_screen() {
    assert_eq!(wanted(Visible::new(0, 9), None), Move::To(ListWindow::top(30)));
    assert_eq!(wanted(Visible::new(5, 14), None), Move::To(ListWindow::top(30)));
    assert_eq!(wanted(Visible::new(100, 109), None), Move::Jump { position: 90, limit: 30 });
}

/// The window stays while the rows on screen keep more than half a screen
/// from each edge the list goes on past, and starts again a screen above
/// them once they come nearer: from a row it holds, or else from the row
/// there, found by offset.
#[test]
fn the_window_moves_when_the_rows_on_screen_near_its_edge() {
    let window = ListWindow { from: Some(key(90)), limit: 30 };
    let held = keys(90, 120);
    let placed = || Some(Placed { window: &window, start: 90, keys: &held });
    // The rows on screen sit in the window's middle.
    assert_eq!(wanted(Visible::new(100, 109), placed()), Move::Stay);
    assert_eq!(wanted(Visible::new(95, 104), placed()), Move::Stay);
    // Within half a screen of the window's last row.
    assert_eq!(
        wanted(Visible::new(106, 115), placed()),
        Move::To(ListWindow { from: Some(key(96)), limit: 30 })
    );
    // Within half a screen of the window's first row: the window holds no
    // row a screen above them, so the view finds that row by offset.
    assert_eq!(wanted(Visible::new(93, 102), placed()), Move::Jump { position: 83, limit: 30 });
    // Rows far past the window: no row it holds is a screen above them.
    assert_eq!(wanted(Visible::new(500, 509), placed()), Move::Jump { position: 490, limit: 30 });
    // Near the top, the window starts at the list's top.
    assert_eq!(wanted(Visible::new(3, 12), placed()), Move::To(ListWindow::top(30)));
}

/// A window that holds fewer rows than its limit reaches the list's end, so
/// rows on screen near its end move nothing; one at the list's top moves
/// for no row near its top.
#[test]
fn the_window_stays_at_the_list_s_ends() {
    let window = ListWindow { from: Some(key(90)), limit: 30 };
    let held = keys(90, 112);
    let placed = Some(Placed { window: &window, start: 90, keys: &held });
    assert_eq!(wanted(Visible::new(102, 111), placed), Move::Stay);

    let top = ListWindow::top(30);
    let held = keys(0, 30);
    let placed = Some(Placed { window: &top, start: 0, keys: &held });
    assert_eq!(wanted(Visible::new(0, 9), placed), Move::Stay);
}

/// A screen that shows more rows than the window was sized for moves the
/// window to a larger one.
#[test]
fn a_larger_screen_grows_the_window() {
    let window = ListWindow::top(30);
    let held = keys(0, 30);
    let placed = Some(Placed { window: &window, start: 0, keys: &held });
    assert_eq!(wanted(Visible::new(0, 19), placed), Move::To(ListWindow::top(60)));
}

/// The artist list read through its view, over a library of artists named in
/// order: the window around the rows on screen, and the position of its
/// first row, each a live read.
mod artists {
    use super::*;
    use bae_records::test_fixtures::library::RecordsFixture;
    use bae_records::test_fixtures::test_records;
    use bae_store::live_query::CancellableLiveQuery;
    use bae_store::test_fixtures::temp_db;
    use bae_store::Store;
    use bae_util::test_channels::HANG_GUARD;
    use tokio_util::sync::CancellationToken;

    fn artist(at: usize) -> String {
        format!("6b2d4f60-8a1c-4e3b-9d5f-7a9c1e3b5d{at:02}")
    }

    /// One hundred artists, "Artist Name 000" to "Artist Name 099", each
    /// with an album and its release.
    async fn hundred_artists() -> (Store, tempfile::TempDir) {
        let (db, dir) = temp_db(crate::TEST_SCHEMA).await;
        let mut library = RecordsFixture::new();
        for at in 0..100 {
            let album = format!("6b2d4f60-8a1c-4e3b-9d5f-7a9c1e3b6d{at:02}");
            let release = format!("6b2d4f60-8a1c-4e3b-9d5f-7a9c1e3b7d{at:02}");
            library = library
                .artist(&artist(at), &format!("Artist Name {at:03}"))
                .album(&album, &format!("Album {at:03}"), None, &[&artist(at)])
                .release(&album, &release);
        }
        library.write(&test_records(&db)).await;
        (db, dir)
    }

    fn live<Request, Projection>(
        query: coven::ReconfigurableLiveQuery<Request, Projection, bae_store::DbError>,
    ) -> CancellableLiveQuery<Request, Projection, bae_store::DbError>
    where
        Request: Clone + PartialEq + Send + Sync + 'static,
        Projection: Clone + PartialEq + Send + 'static,
    {
        CancellableLiveQuery::new(query, CancellationToken::new(), coven::task::HeardBugs::shared())
    }

    /// The artist list's reads under its default sort.
    fn reads(db: &Store) -> ListReads {
        let views = crate::LibraryViews::new(db.clone());
        let (for_window, for_position) = (views.clone(), views.clone());
        ListReads {
            window: Box::new(move |window| {
                crate::LiveListWindow::new(
                    live(for_window.subscribe_artist_window(&[], window)),
                    |_, read| Some(crate::artist::resolve_artist_window(read)),
                )
            }),
            position: Box::new(move |key| {
                crate::LiveListPosition::new(
                    live(for_position.subscribe_artist_position(&[], key)),
                    |read, rows_before| {
                        Some(crate::ListPosition { key: read.request.clone(), rows_before })
                    },
                )
            }),
            key_at: Box::new(move |position| {
                let views = views.clone();
                Box::pin(async move {
                    views.artist_key_at(&[], position).await.map_err(LiveQueryError::Query)
                })
            }),
        }
    }

    /// The names of a value's rows.
    fn names(value: &ListViewValue) -> Vec<String> {
        let ListRows::Artists(rows) = &value.rows else {
            panic!("the artist list's window holds artists");
        };
        rows.iter().map(|row| row.artist.name.clone()).collect()
    }

    fn named(range: std::ops::Range<usize>) -> Vec<String> {
        range.map(|at| format!("Artist Name {at:03}")).collect()
    }

    /// The view's next value whose first row is at `start`.
    async fn value_at(view: &ListViewHandle, start: u64) -> ListViewValue {
        tokio::time::timeout(HANG_GUARD, async {
            loop {
                let value = view.next().await.expect("the list's reads stay open");
                if value.start == start {
                    return value;
                }
            }
        })
        .await
        .expect("the view delivers the window at its start")
    }

    /// Write an artist named `name`, with an album and its release, each id
    /// ending in `tail`.
    async fn write_artist(db: &Store, tail: &str, name: &str) {
        let artist = format!("6b2d4f60-8a1c-4e3b-9d5f-7a9c1e3b5e{tail}");
        let album = format!("6b2d4f60-8a1c-4e3b-9d5f-7a9c1e3b6e{tail}");
        let release = format!("6b2d4f60-8a1c-4e3b-9d5f-7a9c1e3b7e{tail}");
        RecordsFixture::new()
            .artist(&artist, name)
            .album(&album, "Album Title", None, &[&artist])
            .release(&album, &release)
            .write(&test_records(db))
            .await;
    }

    /// The view opens its window a screen above the rows on screen, found by
    /// offset; an artist written above the window moves its rows down one
    /// place and leaves them as they were; rows on screen near the window's
    /// end move it once, to a screen above them; and rows on screen one
    /// further leave it where it is.
    #[tokio::test]
    async fn the_view_keeps_its_window_around_the_rows_on_screen() {
        let (db, _dir) = hundred_artists().await;
        let (view, run) =
            ListView::open(reads(&db), Visible::new(50, 59), CancellationToken::new());
        #[expect(
            clippy::disallowed_methods,
            reason = "the test joins the view's task, so a panic in it fails the test"
        )]
        let task = tokio::spawn(run);

        let first = tokio::time::timeout(HANG_GUARD, view.next()).await.unwrap().unwrap();
        assert_eq!(first.start, 40);
        assert_eq!(names(&first), named(40..70));

        // "Artist Name 000a" sorts between the first two artists.
        write_artist(&db, "ff", "Artist Name 000a").await;
        let shifted = value_at(&view, 41).await;
        assert_eq!(names(&shifted), named(40..70));

        // The window holds positions 41 to 70; rows 80 to 89 come within half
        // a screen of its end, so it starts again a screen above them, at
        // the row it holds at 70, artist 69.
        view.set_visible(Visible::new(80, 89));
        let moved = value_at(&view, 70).await;
        assert_eq!(names(&moved), named(69..99));
        // One row further keeps the window where it is: the view takes the
        // rows on screen before the next answer of its window, and the
        // window's next value, after an artist written inside it, still
        // starts at 70.
        view.set_visible(Visible::new(81, 90));
        write_artist(&db, "fe", "Artist Name 075a").await;
        let written = "Artist Name 075a".to_string();
        let stayed = tokio::time::timeout(HANG_GUARD, async {
            loop {
                let value = view.next().await.expect("the list's reads stay open");
                assert_eq!(value.start, 70, "the window stays where it is");
                if names(&value).contains(&written) {
                    return value;
                }
            }
        })
        .await
        .expect("the window delivers the artist written inside it");
        let mut expected = named(69..76);
        expected.push("Artist Name 075a".to_string());
        expected.extend(named(76..98));
        assert_eq!(names(&stayed), expected);

        view.cancel();
        assert!(matches!(view.next().await, Err(LiveQueryError::Cancelled)));
        drop(view);
        tokio::time::timeout(HANG_GUARD, task)
            .await
            .expect("the view's task ends with its handle")
            .expect("the view's task does not panic");
    }
}
