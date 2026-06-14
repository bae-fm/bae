pub mod client;
pub mod models;
pub use client::DiscogsClient;
pub use models::*;

/// Split a Discogs "Artist - Album" title into its trimmed `(artist, album)`
/// halves. Discogs packs both into one title field separated by " - ". Returns
/// `None` when there's no separator; callers pick their own fallback.
pub(crate) fn split_title(title: &str) -> Option<(&str, &str)> {
    title.split_once(" - ").map(|(a, b)| (a.trim(), b.trim()))
}

/// Map `Some("")` → `None`. Discogs returns empty strings (`""`)
/// instead of `null` for missing URL fields (`thumb`, `cover_image`,
/// etc.); serde deserializes those into `Some(String::new())` which
/// then leaks downstream as a "valid" URL until something tries to
/// parse it. Apply with
/// `#[serde(default, deserialize_with = "empty_string_as_none")]` on
/// any `Option<String>` field where empty has only one meaning
/// (absence).
pub(crate) fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Ok(Option::<String>::deserialize(deserializer)?.filter(|s| !s.is_empty()))
}
