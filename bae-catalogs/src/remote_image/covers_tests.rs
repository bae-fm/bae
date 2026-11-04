use super::*;

#[test]
fn cover_art_archive_addresses_are_derived_from_the_entity_id() {
    let base = ARCHIVE;

    let copies = |front: &str| {
        [250, 500, 1200]
            .map(|max_edge| DownscaledCopy { url: format!("{front}-{max_edge}"), max_edge })
            .to_vec()
    };

    let release = RemoteCover::musicbrainz_release("rel-1");
    let front = format!("{base}/release/rel-1/front");
    assert_eq!(release.image.downscaled, copies(&front));
    assert_eq!(release.image.url, front);
    assert_eq!(release.label, "Cover Art Archive");
    assert_eq!(release.source, Catalog::MusicBrainz);

    let group = RemoteCover::musicbrainz_release_group("rg-1");
    let front = format!("{base}/release-group/rg-1/front");
    assert_eq!(group.image.downscaled, copies(&front));
    assert_eq!(group.image.url, front);
    assert_eq!(group.label, "Cover Art Archive (Album)");
}

#[test]
fn a_slot_reads_the_smallest_copy_that_fills_it() {
    let image = RemoteImageSet::with_copies(
        "https://caa.example/front".to_string(),
        vec![
            DownscaledCopy { url: "https://caa.example/front-1200".to_string(), max_edge: 1200 },
            DownscaledCopy { url: "https://caa.example/front-250".to_string(), max_edge: 250 },
            DownscaledCopy { url: "https://caa.example/front-500".to_string(), max_edge: 500 },
        ],
    );

    // A 40-point row on a 2x display, then the 200- and 260-point panes.
    assert_eq!(image.url_covering(Some(80)), "https://caa.example/front-250");
    assert_eq!(image.url_covering(Some(250)), "https://caa.example/front-250");
    assert_eq!(image.url_covering(Some(400)), "https://caa.example/front-500");
    assert_eq!(image.url_covering(Some(520)), "https://caa.example/front-1200");
    // Wider than every copy, and a viewer zooming to the image's own pixels.
    assert_eq!(image.url_covering(Some(2400)), "https://caa.example/front");
    assert_eq!(image.url_covering(None), "https://caa.example/front");
}

#[test]
fn an_image_served_at_one_size_is_read_there_for_every_slot() {
    let image = RemoteImageSet::original("https://discogs.example/only.jpg".to_string());

    assert_eq!(image.url_covering(Some(80)), "https://discogs.example/only.jpg");
    assert_eq!(image.url_covering(Some(520)), "https://discogs.example/only.jpg");
}

#[test]
fn push_unique_cover_dedupes_by_url() {
    let cover = |thumbnail: &str, label: &str| RemoteCover {
        image: RemoteImageSet::with_copies(
            "https://caa.example/cover.jpg".to_string(),
            vec![DownscaledCopy { url: thumbnail.to_string(), max_edge: 250 }],
        ),
        label: label.to_string(),
        source: Catalog::MusicBrainz,
        standing: CoverStanding::Stated,
    };
    let mut covers = vec![cover("https://caa.example/thumb-a.jpg", "Cover Art Archive")];

    push_unique_cover(
        &mut covers,
        cover("https://caa.example/thumb-b.jpg", "Cover Art Archive (Album)"),
    );

    assert_eq!(covers.len(), 1);
    assert_eq!(covers[0].image.downscaled[0].url, "https://caa.example/thumb-a.jpg");
}
