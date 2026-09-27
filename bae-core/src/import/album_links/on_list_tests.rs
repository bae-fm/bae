use super::*;
use crate::import::types::Catalog;
use crate::pressing::ReleaseLabel;

const GROUP: &str = "mb-group";

/// A release titled `title` printing `barcodes` and `labels`, filed under
/// `album`.
fn printing(
    source: Catalog,
    id: &str,
    album: &str,
    title: &str,
    barcodes: &[&str],
    labels: &[(&str, &str)],
) -> Listed {
    let mut result = MetadataResult::for_test(source, id, Some(album));
    result.title = title.to_string();
    result.barcodes = barcodes.iter().map(|code| code.to_string()).collect();
    result.labels = labels
        .iter()
        .map(|(name, number)| ReleaseLabel::of(Some(name), Some(number)))
        .collect();
    Listed::of(&result)
}

fn ours(title: &str, barcodes: &[&str], labels: &[(&str, &str)]) -> GroupToRead {
    GroupToRead {
        group: GROUP.to_string(),
        releases: vec![printing(
            Catalog::MusicBrainz,
            "mb-1",
            GROUP,
            title,
            barcodes,
            labels,
        )],
    }
}

fn theirs(id: &str, master: &str, title: &str, barcodes: &[&str], labels: &[(&str, &str)]) -> Listed {
    printing(Catalog::Discogs, id, master, title, barcodes, labels)
}

fn by_barcode(release: &str, master: &str) -> AlbumLink {
    AlbumLink {
        album: MetadataRef::new(Catalog::Discogs, master),
        stated: AlbumStatement::Barcode {
            musicbrainz_release: "mb-1".to_string(),
            release: MetadataRef::new(Catalog::Discogs, release),
        },
    }
}

fn by_catalog_number(release: &str, master: &str) -> AlbumLink {
    AlbumLink {
        album: MetadataRef::new(Catalog::Discogs, master),
        stated: AlbumStatement::CatalogNumber {
            musicbrainz_release: "mb-1".to_string(),
            release: MetadataRef::new(Catalog::Discogs, release),
        },
    }
}

/// A MusicBrainz release and a Discogs release printing one barcode, however
/// each spells it, join their albums.
#[test]
fn one_barcode_joins_the_albums() {
    let joined = albums(
        &ours("Album", &["012345678905"], &[]),
        &[
            theirs("dg-1", "700", "Album", &["0 12345 67890 5"], &[]),
            theirs("dg-2", "701", "Album", &["5051961234567"], &[]),
        ],
    );
    assert_eq!(joined, vec![by_barcode("dg-1", "700")]);
}

/// A barcode shared by two titles with no word in common is a code typed
/// onto the wrong record, not one album.
#[test]
fn titles_sharing_no_word_do_not_join() {
    let joined = albums(
        &ours("Album", &["012345678905"], &[("Imprint", "LB 100")]),
        &[theirs(
            "dg-1",
            "700",
            "Other Record",
            &["012345678905"],
            &[("Imprint", "LB 100")],
        )],
    );
    assert!(joined.is_empty());
}

/// A title's bracketed tail and its case, punctuation and accents say nothing
/// about which album it is.
#[test]
fn a_title_s_bracketed_tail_and_spelling_do_not_keep_albums_apart() {
    let joined = albums(
        &ours("Album", &["012345678905"], &[]),
        &[theirs("dg-1", "700", "ALBÚM! (Remastered)", &["012345678905"], &[])],
    );
    assert_eq!(joined, vec![by_barcode("dg-1", "700")]);
}

/// Stop words say nothing about which album a title is: two titles sharing
/// only "the" share no word.
#[test]
fn titles_sharing_only_a_stop_word_do_not_join() {
    let joined = albums(
        &ours("The Album", &["012345678905"], &[]),
        &[theirs("dg-1", "700", "The Record", &["012345678905"], &[])],
    );
    assert!(joined.is_empty());
}

/// With no barcode between them, one catalog number under one label joins
/// the albums — the label's trade word and the number's spacing aside.
#[test]
fn one_catalog_number_under_one_label_joins_the_albums() {
    let joined = albums(
        &ours("Album", &[], &[("Imprint", "LB 100")]),
        &[theirs("dg-1", "700", "Album", &[], &[("Imprint Records", "LB-100")])],
    );
    assert_eq!(joined, vec![by_catalog_number("dg-1", "700")]);
}

/// A label may be anywhere in a record's labels: each label is compared with
/// its own number.
#[test]
fn a_catalog_number_is_compared_with_every_label_a_release_states() {
    let joined = albums(
        &ours("Album", &[], &[("Imprint", "LB 100"), ("Second Press", "SP 5")]),
        &[theirs("dg-1", "700", "Album", &[], &[("Second Press", "SP-5")])],
    );
    assert_eq!(joined, vec![by_catalog_number("dg-1", "700")]);
}

/// The same number under another label is another label's numbering.
#[test]
fn one_catalog_number_under_two_labels_does_not_join() {
    let joined = albums(
        &ours("Album", &[], &[("Imprint", "LB 100")]),
        &[theirs("dg-1", "700", "Album", &[], &[("Other Imprint", "LB 100")])],
    );
    assert!(joined.is_empty());
}

/// A barcode is read before a catalog number: where one joins an album, the
/// albums a catalog number alone would name are not taken.
#[test]
fn a_barcode_is_taken_over_a_catalog_number() {
    let joined = albums(
        &ours("Album", &["012345678905"], &[("Imprint", "LB 100")]),
        &[
            theirs("dg-1", "700", "Album", &[], &[("Imprint", "LB 100")]),
            theirs("dg-2", "701", "Album", &["012345678905"], &[]),
        ],
    );
    assert_eq!(joined, vec![by_barcode("dg-2", "701")]);
}

/// A release its catalog files under no album names none to join.
#[test]
fn a_release_under_no_album_joins_nothing() {
    let mut ungrouped = theirs("dg-1", "700", "Album", &["012345678905"], &[]);
    ungrouped.album = None;
    assert!(albums(&ours("Album", &["012345678905"], &[]), &[ungrouped]).is_empty());
}
