use super::*;
use crate::CoverSelection;

fn image(byte: u8) -> bae_catalogs::remote_image::RemoteImage {
    bae_catalogs::remote_image::RemoteImage {
        bytes: vec![byte],
        content_type: bae_util::content_type::ContentType::Jpeg,
    }
}

/// A source that brings no image of its own says nothing about the
/// cover, so the stored one stands — a remote selection with the bytes
/// prepared for it, which is what the candidate commits with.
#[test]
fn a_source_with_no_image_keeps_the_stored_cover_and_its_bytes() {
    let folders_own = CoverSelection::Local("cover.jpg".to_string());
    let remote = CoverSelection::Remote(
        bae_catalogs::remote_image::RemoteImageSet::original(
            "https://example.invalid/front".to_string(),
        ),
        bae_catalogs::catalog::Catalog::Discogs,
    );
    let nothing = || PreparedCover { selection: None, image: None };

    let kept = settled_cover(
        nothing(),
        PreparedCover { selection: Some(folders_own.clone()), image: None },
    );
    assert_eq!(kept.selection, Some(folders_own.clone()));
    assert_eq!(kept.image, None);

    let kept = settled_cover(
        nothing(),
        PreparedCover { selection: Some(remote.clone()), image: Some(image(1)) },
    );
    assert_eq!(kept.selection, Some(remote.clone()));
    assert_eq!(kept.image, Some(image(1)));

    let none_either_way = settled_cover(nothing(), nothing());
    assert_eq!(none_either_way.selection, None);
    assert_eq!(none_either_way.image, None);

    let replaced = settled_cover(
        PreparedCover { selection: Some(remote.clone()), image: Some(image(2)) },
        PreparedCover { selection: Some(folders_own), image: None },
    );
    assert_eq!(replaced.selection, Some(remote));
    assert_eq!(replaced.image, Some(image(2)));
}
