//! The kept seeds of the screen-cost scenarios: a library of 2,000 (S),
//! 20,000 (M) and 100,000 (L) albums, each size grown from the one before,
//! written through each owning domain's own row writes: records and their
//! files, the transfer queues, the playback queue, people, ratings and
//! playlists.

use bae_records::test_fixtures::shaped_library;
use bae_store::test_fixtures::kept_store::KeptSeeds;
use bae_store::Store;
use std::path::Path;

/// The album counts of S, M and L.
pub(super) const SIZES: [u64; 3] = [2_000, 20_000, 100_000];

/// Bumped when [`seed`] writes different rows, so the kept seeds are made
/// again.
const SEED_VERSION: u32 = 1;

/// How many albums one write holds.
const WRITE_ALBUMS: u64 = 5_000;

/// How many releases the output queue holds.
const OUTPUTS: u64 = 20;

/// How many tracks Up Next holds.
const UP_NEXT: u64 = 50;

/// The sizes to measure: [`SIZES`], or the ones `SCREEN_SIZES` names, comma
/// separated album counts (`SCREEN_SIZES=2000`).
pub(super) fn sizes() -> Vec<u64> {
    match std::env::var("SCREEN_SIZES") {
        Ok(text) => text
            .split(',')
            .map(|n| n.parse().expect("SCREEN_SIZES lists album counts"))
            .inspect(|n| assert!(SIZES.contains(n), "{n} is not a seed size"))
            .collect(),
        Err(_) => SIZES.to_vec(),
    }
}

pub(super) const SEEDS: KeptSeeds = KeptSeeds {
    schema: bae_library::SCHEMA,
    version: SEED_VERSION,
    sizes: &SIZES,
    seed: |store, originals, start, end| Box::pin(seed(store, originals, start, end)),
};

/// The id of album `i`, as an SQL expression over `i`.
fn album_id_sql() -> String {
    "printf('%08x-0002-4000-8000-%012x', i, 0)".to_string()
}

/// Albums `start..end` of the shaped library and everything the other
/// domains hold of them. The first call (`start` 0) also makes what has no
/// album number: the playlists, Up Next and the playing track.
pub(super) async fn seed(db: &Store, originals: &Path, start: u64, end: u64) {
    let records = bae_records::test_fixtures::test_records(db);
    let mut from = start;
    while from < end {
        let to = (from + WRITE_ALBUMS).min(end);
        db.write(move |sql| {
            shaped_library::insert_on(&sql, from, to)?;
            Ok::<(), bae_records::IdentityWriteError>(())
        })
        .await
        .unwrap();
        from = to;
    }
    let (release, track) =
        (shaped_library::first_release_id_sql(), shaped_library::first_track_id_sql());
    let first_track = shaped_library::track_id(0, 0, 0);
    db.write(move |sql| {
        bae_transfers::tables::bulk::queue_shaped_downloads_on(&sql, start, end, &release)?;
        bae_transfers::tables::bulk::queue_shaped_outputs_on(&sql, start, end, OUTPUTS, &release)?;
        bae_playback::tables::window_cost::fill_context_lane_on(&sql, start, end, &track)?;
        if start == 0 {
            bae_playback::tables::window_cost::shaped_playback_on(&sql, UP_NEXT, &track)?;
            bae_playback::tables::window_cost::shaped_now_playing_on(&sql, &first_track)?;
        }
        bae_people::people::shaped_people::insert_on(&sql, start, end, &album_id_sql(), &track)?;
        Ok::<(), bae_store::DbError>(())
    })
    .await
    .unwrap();
    put_files(&records, originals, start, end).await;
}

/// The shaped files of albums `start..end`, each staged from an original of
/// its size under `originals`, one original for each size.
async fn put_files(records: &bae_records::Records, originals: &Path, start: u64, end: u64) {
    std::fs::create_dir_all(originals).unwrap();
    let mut from = start;
    while from < end {
        let to = (from + WRITE_ALBUMS).min(end);
        let files = shaped_library::files_of(from, to)
            .into_iter()
            .map(|shaped| {
                let original = originals.join(format!("{}.bin", shaped.size));
                if !original.exists() {
                    std::fs::File::create_new(&original).unwrap().set_len(shaped.size).unwrap();
                }
                let place = if shaped.queued_to_upload {
                    coven::Place::Cloud
                } else {
                    coven::Place::DeviceOnly
                };
                (shaped.file, original, place)
            })
            .collect();
        records.insert_files_placed_for_test(files).await.unwrap();
        from = to;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seed of S holds what the shape says of every domain's rows.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "makes the kept seed of S, which takes a while"]
    async fn the_seed_of_s_holds_every_domains_rows() {
        let (db, _copy) = SEEDS.seeded_copy(SIZES[0]).await;
        let count = |table: &'static str| {
            let db = db.clone();
            async move { bae_store::test_fixtures::row_count(&db, table).await }
        };
        assert_eq!(count("albums").await, 2_000);
        assert!(count("release_files").await > 8_000);
        assert!(count("download_queue").await >= 20);
        assert_eq!(count("output_queue").await, OUTPUTS as usize);
        assert_eq!(count("playback_manual").await, UP_NEXT as usize);
        assert!(count("playback_context").await > 0);
        assert_eq!(count("people").await, 3);
        assert_eq!(count("playlists").await, 201);
        assert_eq!(count("playlist_entries").await as u64, {
            (0..200).map(|n| 10 + (n * 37) % 491).sum::<u64>() + 10_000
        });
        assert!(count("ratings").await > 500);
        let queued: i64 = db
            .read(|sql| {
                sql.query_row("SELECT COUNT(*) FROM coven_upload_queue", [], |row| row.get(0))
                    .map_err(Into::into)
            })
            .await
            .unwrap();
        assert!(queued > 0, "some files wait to upload");
    }

    /// Making the kept seed of L, which the measuring run reads from, holds
    /// the shape's counts at 100,000 albums. It grows from M's, so it adds
    /// the 80,000 albums beyond it.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "makes the kept seed of L, which takes a long while"]
    async fn the_seed_of_l_holds_every_domains_rows() {
        let (db, _copy) = SEEDS.seeded_copy(SIZES[2]).await;
        let count = |table: &'static str| {
            let db = db.clone();
            async move { bae_store::test_fixtures::row_count(&db, table).await }
        };
        assert_eq!(count("albums").await, 100_000);
        assert!(count("tracks").await > 1_000_000);
        assert_eq!(count("output_queue").await, OUTPUTS as usize);
        assert_eq!(count("playback_manual").await, UP_NEXT as usize);
        assert_eq!(count("playlists").await, 201);
    }
}
