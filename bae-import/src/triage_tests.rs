use super::*;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::catalog::LookupCatalog;

/// The pane states the found release's failed check while identification's
/// pick or nobody's draft stands; a person's or the tags' valid draft is
/// their answer and states none.
#[test]
fn a_person_s_draft_states_no_folder_check() {
    let unchecked = VerdictSummary {
        kind: bae_identify::VerdictKind::Found,
        pressing_count: 1,
        lead: Some(bae_identify::TracklistFit::ListsNothing),
        medium_conflict: None,
        unread_document: false,
        kept_own_draft: false,
    };
    let check = Some(FolderCheck::SourceTracksUnknown);
    for (author, draft_valid, stated) in [
        (MetadataAuthor::Identification, true, check.clone()),
        (MetadataAuthor::Nobody, false, check.clone()),
        (MetadataAuthor::Person, false, check.clone()),
        (MetadataAuthor::Person, true, None),
        (MetadataAuthor::Prefill, true, None),
    ] {
        assert_eq!(
            stated_folder_check(author, draft_valid, Some(&unchecked)),
            stated,
            "{author:?}, valid: {draft_valid}"
        );
    }
}

fn a_draft() -> TriageMetadataSummary {
    TriageMetadataSummary {
        album_title: "Album Title".to_string(),
        album_artists: bae_types::credits::CreditLine::default(),
    }
}

/// The records a pick's documents describe a release in, as the reader
/// of those documents hands them over.
fn described_in() -> Vec<bae_catalogs::records::ReleaseRecord> {
    vec![
        bae_catalogs::records::ReleaseRecord::new(
            &bae_catalogs::records::MetadataRef::new(Catalog::MusicBrainz, "mb-1".to_string()),
            None,
        ),
        bae_catalogs::records::ReleaseRecord::new(
            &bae_catalogs::records::MetadataRef::new(Catalog::Discogs, "discogs-1".to_string()),
            None,
        ),
    ]
}

#[test]
fn a_row_with_no_draft_is_its_folder_and_nothing_else() {
    assert_eq!(TriageReading::of(None, None, Vec::new()), TriageReading::Unidentified);
}

#[test]
fn a_draft_of_an_unlinked_candidate_names_no_source() {
    assert_eq!(
        TriageReading::of(Some(&a_draft()), None, Vec::new()),
        TriageReading::Prefilled,
        "a draft with no release link is a draft, wherever it was read from"
    );
}

/// A link reads as identified in exactly the records its documents were
/// read into — the reading names them, it does not derive them.
#[test]
fn a_link_reads_as_identified_in_the_records_it_is_handed() {
    let pick = ReleaseLink::Pressing(bae_identify::PressingLink {
        record: bae_catalogs::records::LookupRef::new(
            LookupCatalog::MusicBrainz,
            "mb-1".to_string(),
        ),
        partners: Vec::new(),
    });
    assert_eq!(
        TriageReading::of(Some(&a_draft()), Some(&pick), described_in()),
        TriageReading::Identified { records: described_in() }
    );
}

/// An album link reads as the album, pressing unknown, in its records.
#[test]
fn an_album_link_reads_as_its_album() {
    let album = bae_identify::AlbumLink::new([bae_catalogs::records::MetadataRef::new(
        Catalog::MusicBrainz,
        "group-1",
    )])
    .expect("one album");
    let records = album.records();
    assert_eq!(
        TriageReading::of(Some(&a_draft()), Some(&ReleaseLink::Album(album)), records.clone()),
        TriageReading::IdentifiedAlbum { records }
    );
}
