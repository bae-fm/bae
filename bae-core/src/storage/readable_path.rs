//! Readable cloud blob keys for browsable homes.
//!
//! An opaque home keys every managed blob by a hash of its id
//! (`storage/{ab}/{cd}/{id}` for audio, `images/{ab}/{cd}/{id}` for images),
//! so its cloud objects are unfindable by a human browsing the bucket. A
//! browsable home instead lays its blobs out under readable paths a user can
//! open in Finder/Explorer:
//!
//! - audio:        `storage/{artist}/{album}/{filename}`
//! - cover image:  `{artist}/{album}/cover.{ext}`   (under the `images` namespace)
//! - artist image: `{artist}/artist.{ext}`          (under the `images` namespace)
//!
//! Audio keys carry the `storage/` prefix — bae's audio namespace, the same root
//! an opaque home's `storage/{ab}/{cd}/{id}` shards live under — so a readable
//! key can never land under one of coven's reserved root prefixes (`heads/`,
//! `changes/`, `membership/`, `auth/keys/`), which coven enumerates with
//! `list()`. Without it, an artist literally named `heads` would write
//! `heads/Album/Track.flac`, which `list("heads/")` returns and the head parser
//! rejects — wedging every sync. Image keys stay under coven's `images/`
//! namespace (coven prepends it), which coven never enumerates.
//!
//! The key a blob lands at is computed once — the moment the blob is first
//! destined for the cloud — and stored on the synced `cloud_path` column of its
//! row (`release_files` for audio, `library_images` for images). Every later
//! upload, read, delete, and pull uses the stored value verbatim, so a metadata
//! rename never moves a blob and every device addresses it the same way.
//!
//! This module is the pure half: sanitizing path components, formatting the
//! three key shapes, and disambiguating a collision. Resolving the artist /
//! album names and querying the already-taken keys for collision-safety needs a
//! database connection and lives in `crate::db` (`resolve_*_cloud_path`).

use crate::util::content_type::ContentType;

/// Longest a single path component may be after sanitizing, measured in UTF-8
/// BYTES (not scalar values — a CJK or emoji name is 3-4 bytes per character, so
/// a char-counted cap could blow the byte limit). A browsable key is
/// `storage/artist/album/file` — a namespace plus three components — so at
/// 200 bytes each the whole key stays well under the 1024-byte object-key limit
/// S3 imposes, while leaving each component readable.
const MAX_COMPONENT_BYTES: usize = 200;

/// The file extension for an image blob's readable name. Browsable covers and
/// artist images are named `cover.{ext}` / `artist.{ext}`, so the extension must
/// be a short, predictable token a user recognizes — not the full content-type
/// extension table (which would emit `bin`/`svg`/etc. for shapes that never
/// reach here). The three formats bae actually stores map to their familiar
/// extension; anything else falls back to a neutral `img`.
pub fn image_extension(content_type: &ContentType) -> &'static str {
    match content_type {
        ContentType::Jpeg => "jpg",
        ContentType::Png => "png",
        ContentType::Webp => "webp",
        _ => "img",
    }
}

/// Sanitize one path component (an artist, album, filename, or the literal
/// `cover`/`artist` stem) so it is safe to write as a folder/file name a user
/// opens in Finder or Explorer, and so it can never inject extra path levels.
///
/// `fallback_id` replaces a component that sanitizes to empty (e.g. an
/// all-whitespace, all-dot, or all-separator name) — the row's id is always
/// present and keeps the key unique and addressable. The component is finally
/// capped to [`MAX_COMPONENT_BYTES`], preserving a trailing `.ext` when one is
/// present so a truncated filename keeps its type suffix.
///
/// Underscore runs are collapsed to a single `_` and leading/trailing `_` are
/// trimmed so the component never carries a `__` or a boundary `_`. That keeps
/// the readable key injective under the `/`→`__` flattening Google Drive and
/// OneDrive apply: every `__` in a flattened key is then a real path separator,
/// so two distinct keys can't flatten to the same object name and silently
/// overwrite each other. (A name whose underscores collapse to match another's
/// collapses to the same readable key and is caught by the collision check, not
/// lost.)
pub fn sanitize_component(raw: &str, fallback_id: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_was_space = false;
    let mut last_was_underscore = false;
    for ch in raw.chars() {
        // Path separators, control chars, and the Windows/macOS-hostile
        // punctuation set all collapse to `_` so the component is one safe
        // folder/file name on every platform a browsable home is opened on.
        let replaced = if ch == '/'
            || ch == '\\'
            || (ch as u32) < 0x20
            || matches!(ch, ':' | '*' | '?' | '"' | '<' | '>' | '|')
        {
            '_'
        } else {
            ch
        };
        if replaced.is_whitespace() {
            // Collapse internal whitespace runs to a single space.
            if !last_was_space {
                out.push(' ');
            }
            last_was_space = true;
            last_was_underscore = false;
        } else if replaced == '_' {
            // Collapse underscore runs to a single `_` (keeps the key injective
            // under Google Drive / OneDrive `/`→`__` flattening).
            if !last_was_underscore {
                out.push('_');
            }
            last_was_underscore = true;
            last_was_space = false;
        } else {
            out.push(replaced);
            last_was_space = false;
            last_was_underscore = false;
        }
    }
    // Trim leading/trailing whitespace, dots, and underscores: a leading dot
    // hides the entry on Unix and a trailing dot is rejected on Windows, and a
    // boundary `_` adjacent to the `__` separator would re-create an ambiguous
    // `___` after flattening.
    let trimmed = out.trim_matches(|c: char| c.is_whitespace() || c == '.' || c == '_');
    if trimmed.is_empty() {
        return fallback_id.to_string();
    }
    cap_bytes(trimmed)
}

/// Cap a component to [`MAX_COMPONENT_BYTES`] UTF-8 bytes, preserving a trailing
/// file extension when one is present so a truncated filename keeps its type
/// suffix (`<long name>.flac` stays `.flac`). Truncation lands on a character
/// boundary, so a multi-byte name is never cut mid-character.
fn cap_bytes(s: &str) -> String {
    if s.len() <= MAX_COMPONENT_BYTES {
        return s.to_string();
    }
    // Split off a short, real-looking extension (`.` + up to 8 ASCII-alphanumeric
    // chars) so it can be reattached after truncating the stem.
    if let Some(dot) = s.rfind('.') {
        let ext = &s[dot + 1..];
        if !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            let stem = &s[..dot];
            // Reserve room (in bytes) for the dot + extension within the cap.
            let budget = MAX_COMPONENT_BYTES.saturating_sub(ext.len() + 1).max(1);
            return format!("{}.{ext}", truncate_to_bytes(stem, budget));
        }
    }
    truncate_to_bytes(s, MAX_COMPONENT_BYTES)
}

/// The longest char-aligned prefix of `s` that fits in `max` UTF-8 bytes, with
/// any trailing whitespace/dot/underscore trimmed — so truncating mid-name can't
/// re-introduce a boundary `_` (which would break the `__`-flatten injectivity)
/// or a trailing dot. The result is therefore `<= max` bytes.
fn truncate_to_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end]
        .trim_end_matches(|c: char| c.is_whitespace() || c == '.' || c == '_')
        .to_string()
}

/// The readable cloud key for an audio file: `storage/{artist}/{album}/{filename}`.
/// The `storage/` prefix is bae's audio namespace (an opaque home shards audio
/// under the same prefix), keeping the key clear of coven's reserved root
/// prefixes. Each component is sanitized independently; an empty component falls
/// back to the row id (`file_id` for the filename, else the release id) so the
/// key is always non-empty and addressable.
pub fn audio_key(
    artist: &str,
    album: &str,
    filename: &str,
    release_id: &str,
    file_id: &str,
) -> String {
    let artist = sanitize_component(artist, release_id);
    let album = sanitize_component(album, release_id);
    let filename = sanitize_component(filename, file_id);
    format!("storage/{artist}/{album}/{filename}")
}

/// The readable `cloud_path` for a cover image, RELATIVE to the `images`
/// namespace coven prepends: `{artist}/{album}/cover.{ext}`. The blob's id is
/// the release id, used as the fallback for any empty component.
pub fn cover_cloud_path(
    artist: &str,
    album: &str,
    content_type: &ContentType,
    release_id: &str,
) -> String {
    let artist = sanitize_component(artist, release_id);
    let album = sanitize_component(album, release_id);
    let ext = image_extension(content_type);
    format!("{artist}/{album}/cover.{ext}")
}

/// The readable `cloud_path` for an artist image, RELATIVE to the `images`
/// namespace coven prepends: `{artist}/artist.{ext}`. The blob's id is the
/// artist id, used as the fallback for an empty component.
pub fn artist_cloud_path(artist: &str, content_type: &ContentType, artist_id: &str) -> String {
    let artist = sanitize_component(artist, artist_id);
    let ext = image_extension(content_type);
    format!("{artist}/artist.{ext}")
}

/// Make `candidate` unique among `taken` by inserting ` (2)`, ` (3)`, … before
/// the file extension of its last component until the key is free. A clean
/// import (distinct track filenames within one album) never collides and keeps
/// the plain `Artist/Album/Track.flac`; only a genuine clash — the same artist,
/// album, and filename for a different blob — is disambiguated.
///
/// `taken(key) -> bool` reports whether a key is already claimed by a DIFFERENT
/// blob; the caller backs it with the set of stored `cloud_path` values across
/// `release_files` and `library_images`.
pub fn disambiguate(candidate: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(candidate) {
        return candidate.to_string();
    }
    for n in 2u32.. {
        let next = insert_suffix(candidate, n);
        if !taken(&next) {
            return next;
        }
    }
    unreachable!("disambiguation counter exhausted u32::MAX candidates")
}

/// Insert ` (n)` before the file extension of `key`'s last path component:
/// `Artist/Album/Track.flac` → `Artist/Album/Track (2).flac`. A component with
/// no extension gets the suffix appended (`.../cover` → `.../cover (2)`).
fn insert_suffix(key: &str, n: u32) -> String {
    let (dir, last) = match key.rfind('/') {
        Some(slash) => (&key[..=slash], &key[slash + 1..]),
        None => ("", key),
    };
    match last.rfind('.') {
        // A real extension (non-empty, not a leading dot): suffix the stem.
        Some(dot) if dot > 0 && dot + 1 < last.len() => {
            let stem = &last[..dot];
            let ext = &last[dot + 1..];
            format!("{dir}{stem} ({n}).{ext}")
        }
        _ => format!("{dir}{last} ({n})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_separators_and_hostile_chars() {
        // Slashes and backslashes can't introduce extra path levels.
        assert_eq!(sanitize_component("a/b\\c", "id"), "a_b_c");
        // The Windows/macOS-hostile punctuation set all collapses to `_`.
        assert_eq!(
            sanitize_component("a:b*c?d\"e<f>g|h", "id"),
            "a_b_c_d_e_f_g_h"
        );
    }

    #[test]
    fn sanitize_replaces_control_chars() {
        // Every control char (< 0x20) — tab, newline, bell — becomes `_`.
        assert_eq!(sanitize_component("a\tb\nc\u{0007}d", "id"), "a_b_c_d");
    }

    #[test]
    fn sanitize_collapses_whitespace_and_trims_edges() {
        assert_eq!(sanitize_component("  the   name  ", "id"), "the name");
    }

    #[test]
    fn sanitize_trims_leading_and_trailing_dots() {
        assert_eq!(sanitize_component("...hidden...", "id"), "hidden");
        assert_eq!(sanitize_component(".", "fallback-id"), "fallback-id");
    }

    #[test]
    fn sanitize_empty_falls_back_to_id() {
        // The fallback fires when nothing readable survives: an empty input, one
        // that is all whitespace/dots, or one that is all separators (which
        // become `_` and are then trimmed as boundary underscores).
        assert_eq!(sanitize_component("", "rel-123"), "rel-123");
        assert_eq!(sanitize_component("   ", "rel-123"), "rel-123");
        assert_eq!(sanitize_component(" . . ", "rel-123"), "rel-123");
        assert_eq!(sanitize_component("/\\/", "rel-123"), "rel-123");
    }

    #[test]
    fn sanitize_collapses_underscore_runs_and_trims_boundary() {
        // Runs of `_` (from a literal `__`, or from adjacent hostile chars like
        // `::`) collapse to one, and a boundary `_` is trimmed — so no component
        // carries a `__` or edge `_` that the GDrive/OneDrive `/`→`__` flatten
        // could mistake for a separator.
        assert_eq!(sanitize_component("lo__fi", "id"), "lo_fi");
        assert_eq!(sanitize_component("a::b", "id"), "a_b");
        assert_eq!(sanitize_component("_lead", "id"), "lead");
        assert_eq!(sanitize_component("trail_", "id"), "trail");
    }

    #[test]
    fn sanitize_caps_length_preserving_extension() {
        let long_stem = "a".repeat(300);
        let name = format!("{long_stem}.flac");
        let capped = sanitize_component(&name, "id");
        assert!(capped.len() <= MAX_COMPONENT_BYTES);
        assert!(capped.ends_with(".flac"), "extension preserved: {capped}");
    }

    #[test]
    fn sanitize_caps_length_without_extension() {
        let long = "x".repeat(300);
        let capped = sanitize_component(&long, "id");
        assert_eq!(capped.len(), MAX_COMPONENT_BYTES);
    }

    #[test]
    fn sanitize_truncation_leaves_no_trailing_underscore() {
        // A long name whose byte cap would land on an underscore must not keep a
        // trailing `_`, or the `/`→`__` flatten would alias it with a neighbour.
        let name = "a_".repeat(150); // 300 chars; truncation at 200 lands on `_`
        let capped = sanitize_component(&name, "id");
        assert!(capped.len() <= MAX_COMPONENT_BYTES);
        assert!(
            !capped.ends_with('_'),
            "truncation must not leave a trailing underscore: {capped}"
        );
    }

    #[test]
    fn sanitize_caps_multibyte_by_bytes_on_a_char_boundary() {
        // A multibyte name well under MAX_COMPONENT_BYTES *characters* can still
        // exceed it in bytes; the cap measures bytes and never splits a scalar.
        let cjk = "音".repeat(200); // 3 bytes each = 600 bytes, 200 chars
        let capped = sanitize_component(&cjk, "id");
        assert!(capped.len() <= MAX_COMPONENT_BYTES, "byte length capped");
        assert!(
            capped.chars().all(|c| c == '音'),
            "no character was split mid-scalar: {capped}"
        );
    }

    #[test]
    fn image_extension_maps_known_and_falls_back() {
        assert_eq!(image_extension(&ContentType::Jpeg), "jpg");
        assert_eq!(image_extension(&ContentType::Png), "png");
        assert_eq!(image_extension(&ContentType::Webp), "webp");
        assert_eq!(image_extension(&ContentType::Gif), "img");
        assert_eq!(image_extension(&ContentType::OctetStream), "img");
    }

    #[test]
    fn audio_key_joins_sanitized_components_under_storage_prefix() {
        assert_eq!(
            audio_key(
                "Artist Name",
                "Album Title",
                "01 Track Title.flac",
                "rel-1",
                "file-1"
            ),
            "storage/Artist Name/Album Title/01 Track Title.flac"
        );
    }

    #[test]
    fn cover_and_artist_paths_have_expected_shape() {
        assert_eq!(
            cover_cloud_path("Artist Name", "Album Title", &ContentType::Jpeg, "rel-1"),
            "Artist Name/Album Title/cover.jpg"
        );
        assert_eq!(
            artist_cloud_path("Artist Name", &ContentType::Png, "artist-1"),
            "Artist Name/artist.png"
        );
    }

    #[test]
    fn disambiguate_leaves_free_key_untouched() {
        assert_eq!(
            disambiguate("Artist Name/Album Title/01 Track.flac", |_| false),
            "Artist Name/Album Title/01 Track.flac"
        );
    }

    #[test]
    fn disambiguate_inserts_counter_before_extension() {
        let taken = ["Artist Name/Album Title/01 Track.flac".to_string()];
        let out = disambiguate("Artist Name/Album Title/01 Track.flac", |k| {
            taken.contains(&k.to_string())
        });
        assert_eq!(out, "Artist Name/Album Title/01 Track (2).flac");
    }

    #[test]
    fn disambiguate_skips_to_next_free_counter() {
        let taken = ["A/B/cover.jpg".to_string(), "A/B/cover (2).jpg".to_string()];
        let out = disambiguate("A/B/cover.jpg", |k| taken.contains(&k.to_string()));
        assert_eq!(out, "A/B/cover (3).jpg");
    }

    #[test]
    fn disambiguate_suffixes_extensionless_component() {
        let taken = ["A/artist".to_string()];
        let out = disambiguate("A/artist", |k| taken.contains(&k.to_string()));
        assert_eq!(out, "A/artist (2)");
    }
}
