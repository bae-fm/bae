//! The library image rows an import stages for the pictures of the artists
//! it creates.

use super::*;

fn artist(id: &str, discogs_artist_id: Option<&str>) -> bae_db::DbArtist {
    bae_db::DbArtist {
        id: id.to_string(),
        name: id.to_string(),
        sort_name: None,
        discogs_artist_id: discogs_artist_id.map(str::to_string),
        musicbrainz_artist_id: None,
        created_at: chrono::DateTime::UNIX_EPOCH,
    }
}

fn image(discogs_artist_id: &str) -> DiscogsArtistImage {
    DiscogsArtistImage::Image { discogs_artist_id: discogs_artist_id.into(), jpeg: vec![1, 2, 3] }
}

#[test]
fn each_new_discogs_artist_with_a_picture_gets_its_row() {
    let artists = [artist("a", Some("1")), artist("b", Some("2")), artist("c", None)];
    let fetched = [image("1"), DiscogsArtistImage::Nothing { discogs_artist_id: "2".into() }];
    let rows = artist_image_rows(&artists, &fetched, chrono::DateTime::UNIX_EPOCH).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, [1, 2, 3]);
}

#[test]
fn a_new_artist_no_picture_was_fetched_for_means_the_library_changed() {
    let artists = [artist("a", Some("1"))];
    let error = artist_image_rows(&artists, &[], chrono::DateTime::UNIX_EPOCH).unwrap_err();
    assert!(matches!(error, crate::ImportError::LibraryChanged));
}

#[test]
fn two_answers_for_one_artist_are_refused() {
    let error = artist_image_rows(&[], &[image("1"), image("1")], chrono::DateTime::UNIX_EPOCH)
        .unwrap_err();
    assert!(matches!(error, crate::ImportError::Internal { .. }));
}
