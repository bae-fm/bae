//! How many files an import holds open at once.
//!
//! Every open of the person's files takes a place from the installation's
//! budget, so an import holds no more files open than the budget has places,
//! however many tracks it has. The test gives the library a budget of one place,
//! imports a three-disc folder of 300 tracks, and requires every track to import,
//! the most files open at once to be one, and none of them to remain open once
//! the import is done.

use std::fs;
use std::path::Path;
use std::time::Instant;

const DISCS: usize = 3;
const TRACKS_PER_DISC: usize = 100;

fn write_box_set(album_dir: &Path) {
    let flac = fs::read(bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac"))
        .expect("FLAC fixture");
    for disc in 1..=DISCS {
        let disc_dir = album_dir.join(format!("CD{disc}"));
        fs::create_dir_all(&disc_dir).unwrap();
        for track in 1..=TRACKS_PER_DISC {
            fs::write(disc_dir.join(format!("{track:03} - Track Title {track}.flac")), &flac)
                .unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_box_set_imports_with_a_budget_of_one_place() {
    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
    let budget = coven::FileBudget::new(std::num::NonZeroUsize::MIN);
    let watch = budget.watch();
    let (library, album_dir, _temp) =
        bae_library::test_libraries::setup_test_library_with_album_dir_within(
            fakes.providers(),
            bae_util::http::Http::for_test(),
            budget,
        )
        .await;
    write_box_set(&album_dir);

    let release = bae_catalogs::discogs::DiscogsRelease {
        tracklist: (1..=DISCS)
            .flat_map(|disc| {
                (1..=TRACKS_PER_DISC).map(move |track| {
                    bae_catalogs::test_releases::discogs_track(
                        &format!("{disc}-{track}"),
                        &format!("Track Title {track}"),
                        "3:00",
                    )
                })
            })
            .collect(),
        ..bae_catalogs::test_releases::discogs_test_release(
            "test-box-set-open-files",
            "Album Title",
            &[],
        )
    };
    let release_key = fakes.seed_discogs_test_release(release);
    let import_handle = library.start_discogs_import().await;

    let started = Instant::now();
    let import_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
    import_handle
        .send_command(bae_import::test_imports::folder_import(
            &import_id,
            album_dir.clone(),
            bae_import::test_imports::discogs_release(release_key),
        ))
        .await
        .expect("import command is accepted");
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let result = bae_import::test_imports::try_wait_for_import_complete(&mut progress_rx).await;
    eprintln!(
        "{} tracks imported in {:.1?} with at most {} files open at once",
        DISCS * TRACKS_PER_DISC,
        started.elapsed(),
        watch.most_holding()
    );

    let (release_id, _album_id) = result
        .unwrap_or_else(|error| panic!("the box set import failed with a budget of one: {error}"));
    assert_eq!(watch.most_holding(), 1, "the import held more files open than its budget");
    let tracks =
        library.records().get_tracks_for_release(&release_id).await.expect("read imported tracks");
    assert_eq!(tracks.len(), DISCS * TRACKS_PER_DISC);

    // Every source file the import opened is closed once it is done.
    coven::assert_no_open_files_under(&album_dir);
}
