use super::*;

mod artist_credits;
mod direct_entry;
mod external;
mod file_metadata;

/// A Discogs release with rich pressing fields, used to assert that a picked
/// release seeds them.
fn discogs_release_rich(title: &str, master_id: &str, tracks: &[&str]) -> DiscogsRelease {
    let tracks: Vec<(&str, &str)> = tracks.iter().map(|title| (*title, "3:00")).collect();
    DiscogsRelease {
        year: Some(1996),
        formats: vec![bae_catalogs::discogs::DiscogsFormat {
            name: "CD".to_string(),
            qty: "1".to_string(),
            descriptions: vec!["Album".to_string(), "Reissue".to_string(), "Promo".to_string()],
            text: None,
        }],
        labels: vec![
            ReleaseLabel::of(Some("Label A"), Some("AB 100")),
            ReleaseLabel::of(Some("Label B"), Some("CL 719")),
        ],
        master_id: Some(master_id.to_string()),
        ..bae_catalogs::test_releases::discogs_test_release(
            &synthetic_release_id(title),
            title,
            &tracks,
        )
    }
}
