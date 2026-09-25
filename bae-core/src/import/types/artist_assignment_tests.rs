use super::*;

fn library_artist(id: &str) -> ExistingArtist {
    ExistingArtist {
        artist_id: id.to_string(),
        name: "Artist Name".to_string(),
        sort_name: None,
        musicbrainz_artist_id: None,
        discogs_artist_id: None,
    }
}

fn resolved(name: &str, resolution: CreditResolution) -> ResolvedCredit {
    let ArtistAssignment::Credit { credit } = ArtistAssignment::named(name) else {
        unreachable!("a named assignment is a credit")
    };
    ResolvedCredit { credit, resolution }
}

#[test]
fn a_picked_artist_stands_in_the_library_whatever_was_resolved() {
    let picked = ArtistAssignment::picked(library_artist("picked"));

    assert_eq!(picked.standing(&[]), Some(ArtistStanding::Library));
}

#[test]
fn a_credit_stands_as_it_resolved() {
    let resolutions = [
        resolved(
            "Artist One",
            CreditResolution::Library {
                artist: library_artist("one"),
            },
        ),
        resolved("Artist Two", CreditResolution::New),
        resolved(
            "Artist Three",
            CreditResolution::Ambiguous {
                artists: vec![library_artist("a"), library_artist("b")],
            },
        ),
    ];

    assert_eq!(
        ArtistAssignment::named("Artist One").standing(&resolutions),
        Some(ArtistStanding::Library)
    );
    assert_eq!(
        ArtistAssignment::named("Artist Two").standing(&resolutions),
        Some(ArtistStanding::New)
    );
    assert_eq!(
        ArtistAssignment::named("Artist Three").standing(&resolutions),
        Some(ArtistStanding::Choose {
            choices: vec![library_artist("a"), library_artist("b")]
        })
    );
    assert_eq!(
        ArtistAssignment::named("Artist Four").standing(&resolutions),
        None,
        "a credit nothing resolved has no standing to show"
    );
}

#[test]
fn a_field_summarizes_its_artists() {
    let resolutions = [
        resolved("New One", CreditResolution::New),
        resolved("New Two", CreditResolution::New),
        resolved(
            "Shared",
            CreditResolution::Conflicting {
                artists: vec![library_artist("a"), library_artist("b")],
            },
        ),
    ];
    let picked = ArtistAssignment::picked(library_artist("picked"));

    assert_eq!(
        artists_standing(std::slice::from_ref(&picked), &resolutions),
        Some(ArtistsStanding::Library)
    );
    assert_eq!(
        artists_standing(
            &[
                ArtistAssignment::named("New One"),
                ArtistAssignment::named("New Two")
            ],
            &resolutions
        ),
        Some(ArtistsStanding::New)
    );
    assert_eq!(
        artists_standing(
            &[picked.clone(), ArtistAssignment::named("New One")],
            &resolutions
        ),
        Some(ArtistsStanding::SomeNew { count: 1 })
    );
    assert_eq!(
        artists_standing(&[ArtistAssignment::named("Shared")], &resolutions),
        Some(ArtistsStanding::Choose { choices: 2 })
    );
    assert_eq!(
        artists_standing(&[picked, ArtistAssignment::named("Shared")], &resolutions),
        Some(ArtistsStanding::SomeToChoose { count: 1 })
    );
    assert_eq!(artists_standing(&[], &resolutions), None);
}
