use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// A PNG of `width` x `height` opaque pixels.
fn png_of(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&vec![90u8; width as usize * height as usize * 3]).unwrap();
    drop(writer);
    bytes
}

/// A cache with `budget` bytes over a test folder, with its store and
/// folders, which the caller keeps for the test's length.
#[expect(clippy::disallowed_methods, reason = "test code: it takes the runtime the test runs on")]
async fn cache(
    budget: u64,
) -> (PictureTiles, Store, bae_util::test_files::TestFolder, bae_util::test_files::TestFolder) {
    let (store, store_folder) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let folder = bae_util::test_files::TestFolder::new();
    let tiles = PictureTiles::in_dir(
        folder.join("tiles"),
        budget,
        store.clone(),
        Diagnostics::noop(),
        tokio::runtime::Handle::current(),
    );
    (tiles, store, folder, store_folder)
}

/// An opener of `png` that counts its opens in `opens`.
fn opener(
    png: Vec<u8>,
    opens: &Arc<AtomicUsize>,
) -> impl FnOnce() -> std::future::Ready<Result<Box<dyn PictureOrigin>, OpenError>> + Send + 'static
{
    let opens = opens.clone();
    move || {
        opens.fetch_add(1, Ordering::SeqCst);
        std::future::ready(Ok(Box::new(Cursor::new(png)) as Box<dyn PictureOrigin>))
    }
}

/// How many rows `cached_tiles` holds, and the sum of their bytes.
async fn held(store: &Store) -> (u64, u64) {
    store
        .read(|sql| {
            Ok(sql.query_row(
                "SELECT COUNT(*), COALESCE(SUM(bytes), 0) FROM cached_tiles",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_picture_opened_again_reads_its_kept_tiles() {
    let (tiles, store, _folder, _store_folder) = cache(TILE_CACHE_BUDGET).await;
    let opens = Arc::new(AtomicUsize::new(0));
    let png = png_of(700, 300);

    let first = tiles.pyramid("scan", opener(png.clone(), &opens)).await.unwrap();
    let again = tiles.pyramid("scan", opener(png.clone(), &opens)).await.unwrap();
    let tile = tiles.tile("scan", 0, 1, 0, opener(png, &opens)).await.unwrap();

    assert_eq!(first, Pyramid { width: 700, height: 300, format: CopyFormat::Jpeg });
    assert_eq!(again, first);
    assert_eq!(&tile[..2], [0xFF, 0xD8]);
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    assert_eq!(held(&store).await.0, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tile_outside_the_picture_is_refused() {
    let (tiles, _store, _folder, _store_folder) = cache(TILE_CACHE_BUDGET).await;
    let opens = Arc::new(AtomicUsize::new(0));
    let result = tiles.tile("scan", 0, 2, 0, opener(png_of(700, 300), &opens)).await;
    assert!(matches!(result, Err(TilesError::NoTile { level: 0, x: 2, y: 0 })), "{result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn over_its_budget_the_cache_removes_the_least_recently_opened_picture() {
    let (measure, store, _folder, _store_folder) = cache(TILE_CACHE_BUDGET).await;
    let opens = Arc::new(AtomicUsize::new(0));
    measure.pyramid("measure", opener(png_of(700, 300), &opens)).await.unwrap();
    let (_, one) = held(&store).await;
    drop(measure);

    let (tiles, store, folder, _store_folder) = cache(one * 2).await;
    for name in ["first", "second"] {
        tiles.pyramid(name, opener(png_of(700, 300), &opens)).await.unwrap();
    }
    // Opening the first again makes the second the least recently opened.
    tiles.pyramid("first", opener(png_of(700, 300), &opens)).await.unwrap();
    tiles.pyramid("third", opener(png_of(700, 300), &opens)).await.unwrap();

    assert_eq!(held(&store).await, (2, one * 2));
    assert!(folder.exists(Path::new("tiles").join(folder_for("first"))));
    assert!(!folder.exists(Path::new("tiles").join(folder_for("second"))));
    let before = opens.load(Ordering::SeqCst);
    tiles.pyramid("second", opener(png_of(700, 300), &opens)).await.unwrap();
    assert_eq!(opens.load(Ordering::SeqCst), before + 1, "the second is cut again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_picture_larger_than_the_budget_is_kept_alone() {
    let (tiles, store, folder, _store_folder) = cache(10).await;
    let opens = Arc::new(AtomicUsize::new(0));
    tiles.pyramid("older", opener(png_of(300, 300), &opens)).await.unwrap();

    tiles.pyramid("scan", opener(png_of(700, 300), &opens)).await.unwrap();

    assert_eq!(held(&store).await.0, 1);
    assert!(folder.exists(Path::new("tiles").join(folder_for("scan"))));
    assert!(!folder.exists(Path::new("tiles").join(folder_for("older"))));
    let tile = tiles.tile("scan", 0, 1, 0, opener(png_of(700, 300), &opens)).await.unwrap();
    assert_eq!(&tile[..2], [0xFF, 0xD8]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tile_missing_under_a_live_row_fails_and_is_not_cut_again() {
    let (tiles, _store, folder, _store_folder) = cache(TILE_CACHE_BUDGET).await;
    let opens = Arc::new(AtomicUsize::new(0));
    let png = png_of(700, 300);
    tiles.pyramid("scan", opener(png.clone(), &opens)).await.unwrap();
    folder.remove(Path::new("tiles").join(folder_for("scan")).join("0").join("1_0.jpg"));

    let result = tiles.tile("scan", 0, 1, 0, opener(png, &opens)).await;

    assert!(matches!(result, Err(TilesError::Missing { level: 0, x: 1, y: 0 })), "{result:?}");
    assert_eq!(opens.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cut_that_ends_after_another_kept_the_picture_keeps_the_other_s_tiles() {
    let (tiles, store, _folder, _store_folder) = cache(TILE_CACHE_BUDGET).await;
    let opens = Arc::new(AtomicUsize::new(0));
    let png = png_of(700, 300);
    let first = tiles.cut(folder_for("scan"), opener(png.clone(), &opens)).await.unwrap();
    let second = tiles.cut(folder_for("scan"), opener(png, &opens)).await.unwrap();

    assert_eq!(second, first);
    assert_eq!(opens.load(Ordering::SeqCst), 2, "both cuts ran");
    assert_eq!(held(&store).await.0, 1, "one row names the picture");
}

#[tokio::test(flavor = "multi_thread")]
async fn clearing_removes_every_picture_s_tiles() {
    let (tiles, store, folder, _store_folder) = cache(TILE_CACHE_BUDGET).await;
    let opens = Arc::new(AtomicUsize::new(0));
    tiles.pyramid("scan", opener(png_of(700, 300), &opens)).await.unwrap();

    tiles.clear().await.unwrap();

    assert_eq!(held(&store).await, (0, 0));
    assert!(!folder.exists("tiles"));
}
