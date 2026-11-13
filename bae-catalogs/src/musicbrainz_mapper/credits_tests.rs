use crate::musicbrainz::{MbArtistCredit, MbArtistRef, MbMedium, MbWork};
use crate::musicbrainz_mapper::tests::{
    credit, discogs_artist_document, make_mb_track, make_response, map, own_credits, work,
};
use crate::musicbrainz_mapper::*;

#[test]
fn nested_recording_work_imports_composer_work_graph() {
    let composer_ref = MbArtistRef {
        id: Some("composer-artist-a".to_string()),
        name: Some("Composer Name A".to_string()),
        sort_name: Some("Composer Name A".to_string()),
    };
    let lyricist_ref = MbArtistRef {
        id: Some("lyricist-artist-a".to_string()),
        name: Some("Lyricist Name A".to_string()),
        sort_name: Some("Lyricist Name A".to_string()),
    };
    let child_work = MbWork {
        id: "mb-work-child-a".to_string(),
        title: "Work Part A".to_string(),
        disambiguation: None,
        work_type: Some("part".to_string()),
        relations: vec![MbRelation {
            target_type: Some("artist".to_string()),
            relation_type: Some("composer".to_string()),
            artist: Some(composer_ref.clone()),
            target_credit: Some("Composer Name A".to_string()),
            ..MbRelation::default()
        }],
    };
    let parent_work = MbWork {
        id: "mb-work-parent-a".to_string(),
        title: "Work Title A".to_string(),
        disambiguation: Some("work disambiguation".to_string()),
        work_type: Some("work".to_string()),
        relations: vec![
            MbRelation {
                target_type: Some("artist".to_string()),
                relation_type: Some("composer".to_string()),
                artist: Some(composer_ref),
                target_credit: Some("Composer Name A".to_string()),
                ..MbRelation::default()
            },
            MbRelation {
                target_type: Some("artist".to_string()),
                relation_type: Some("lyricist".to_string()),
                artist: Some(lyricist_ref),
                target_credit: Some("Lyricist Name A".to_string()),
                ..MbRelation::default()
            },
            MbRelation {
                target_type: Some("work".to_string()),
                relation_type: Some("parts".to_string()),
                direction: Some("forward".to_string()),
                work: Some(child_work),
                ..MbRelation::default()
            },
        ],
    };
    let mut track_one = make_mb_track("1", "Track Title 1");
    track_one.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(parent_work.clone()),
        ..MbRelation::default()
    }];
    let mut track_two = make_mb_track("2", "Track Title 2");
    track_two.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(parent_work),
        ..MbRelation::default()
    }];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track_one, track_two],
    }]);

    let values = map(&response, vec![]).unwrap();

    assert_eq!(values.works.works.len(), 2);
    assert_eq!(values.tracks[0].works, ["mb-work-parent-a"]);
    assert_eq!(values.tracks[1].works, ["mb-work-parent-a"]);
    assert!(values.works.parts.contains(&crate::works::WorkPart {
        parent: "mb-work-parent-a".to_string(),
        child: "mb-work-child-a".to_string(),
    }));

    // Each work credits the composer, and no work credits the lyricist.
    let composer = Some("composer-artist-a");
    for id in ["mb-work-parent-a", "mb-work-child-a"] {
        let composers = &work(&values, id).composers;
        assert_eq!(composers.len(), 1, "{id} credits one composer");
        assert_eq!(composers[0].musicbrainz_artist_id.as_deref(), composer);
    }
    // The composer is no album or track artist.
    assert!(values.album_credits.iter().all(|credit| credit
        .credit
        .musicbrainz_artist_id
        .as_deref()
        != composer));
    assert!(values.tracks.iter().all(|track| own_credits(track).is_empty()));
}

#[test]
fn recording_linking_the_same_work_twice_produces_one_track_work_link() {
    // A MusicBrainz recording can carry more than one performance relation to
    // the same work (e.g. a full and a partial performance of one work). Each
    // relation would push a track_work link, and two links for the same
    // (track_id, work_id) violate track_works' UNIQUE(track_id, work_id) at
    // finalize. The mapper must emit that pair only once.
    let work = MbWork {
        id: "mb-work-a".to_string(),
        title: "Work Title A".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations: vec![],
    };
    let mut track = make_mb_track("1", "Track Title 1");
    track.recording.as_mut().unwrap().relations = vec![
        MbRelation {
            target_type: Some("work".to_string()),
            relation_type: Some("performance".to_string()),
            work: Some(work.clone()),
            ..MbRelation::default()
        },
        MbRelation {
            target_type: Some("work".to_string()),
            relation_type: Some("performance".to_string()),
            work: Some(work),
            ..MbRelation::default()
        },
    ];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let values = map(&response, vec![]).unwrap();

    assert_eq!(
        values.tracks[0].works,
        ["mb-work-a"],
        "the track performs the work once, however many relations name it"
    );
    assert_eq!(values.works.works.len(), 1);
}

#[test]
fn track_level_artist_credit_creates_and_links_a_new_artist() {
    // Track 2 credits a guest distinct from the release artist. The
    // track-artist loop must create that artist once and link it to track 2
    // only; track 1 (no credits) gets no track-artist rows.
    let mut featured = make_mb_track("2", "Track 2 (feat. Guest)");
    featured.artist_credit = vec![credit("artist-guest", "Guest")];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1"), featured],
    }]);

    let values = map(&response, vec![]).unwrap();

    let guest = own_credits(&values.tracks[1]);
    assert_eq!(guest.len(), 1);
    assert_eq!(guest[0].credit.musicbrainz_artist_id.as_deref(), Some("artist-guest"));
    assert_eq!(guest[0].credit.name, "Guest");
    assert_eq!(
        values.tracks[0].credits,
        TrackCredits::AlbumArtists,
        "a track with no credits credits the album's artists"
    );
}

#[test]
fn track_artist_credit_name_is_used_when_artist_payload_name_is_missing() {
    let mut track = make_mb_track("1", "Track 1");
    let mut name_only_credit = credit("track-credit-name-only", "Track Credit Artist Name");
    name_only_credit.artist.as_mut().unwrap().name = None;
    track.artist_credit = vec![name_only_credit];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let values = map(&response, vec![]).unwrap();

    let credits = own_credits(&values.tracks[0]);
    assert_eq!(credits.len(), 1);
    assert_eq!(credits[0].credit.musicbrainz_artist_id.as_deref(), Some("track-credit-name-only"));
    assert_eq!(credits[0].credit.name, "Track Credit Artist Name");
}

#[test]
fn track_level_artist_credit_dedupes_against_release_artist_by_mb_id() {
    // The track credits the release artist again by the same MB id; the
    // commit must make them one artist rather than a duplicate.
    let mut t = make_mb_track("1", "Track 1");
    t.artist_credit = vec![credit("artist-1", "Test Artist")];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![t],
    }]);

    let values = map(&response, vec![]).unwrap();

    // The track credits the release artist's own MusicBrainz id; which one
    // library artist both credits are is the commit's to decide.
    let credits = own_credits(&values.tracks[0]);
    assert_eq!(credits.len(), 1);
    assert_eq!(credits[0].credit.musicbrainz_artist_id.as_deref(), Some("artist-1"));
    assert_eq!(values.album_credits[0].credit.musicbrainz_artist_id.as_deref(), Some("artist-1"));
}

#[test]
fn known_mb_artist_id_does_not_merge_into_same_name_artist_without_id() {
    let mut response = make_response(vec![{
        let mut track = make_mb_track("1", "Track 1");
        track.recording.as_mut().unwrap().relations = vec![MbRelation {
            target_type: Some("artist".to_string()),
            relation_type: Some("composer".to_string()),
            artist: Some(MbArtistRef {
                id: Some("composer-artist-a".to_string()),
                name: Some("Artist Name A".to_string()),
                sort_name: None,
            }),
            ..MbRelation::default()
        }];
        MbMedium { discs: vec![], format: Some("CD".to_string()), tracks: vec![track] }
    }]);
    response.artist_credit[0].artist = None;

    let values = map(&response, vec![]).unwrap();

    // The release artist reads with no id, the composer of the same name
    // with its own, credited on the track.
    assert_eq!(values.album_credits[0].credit.name, "Artist Name A");
    assert_eq!(values.album_credits[0].credit.musicbrainz_artist_id, None);
    let composer = &values.tracks[0].roles[0].artist;
    assert_eq!(composer.name, "Artist Name A");
    assert_eq!(composer.musicbrainz_artist_id.as_deref(), Some("composer-artist-a"));
}

/// An id-less track credit sharing the release artist's name reads with no
/// id, apart from the release artist's credit, which carries one.
#[test]
fn id_less_track_credit_reads_apart_from_the_id_bearing_release_artist() {
    // Release artist "Artist Name A" carries MB id "artist-1".
    let mut track = make_mb_track("1", "Track 1");
    track.artist_credit = vec![MbArtistCredit {
        joinphrase: None,
        name: "Artist Name A".to_string(),
        artist: Some(MbArtistRef {
            id: None,
            name: Some("Artist Name A".to_string()),
            sort_name: None,
        }),
    }];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let values = map(&response, vec![]).unwrap();

    assert_eq!(values.album_credits[0].credit.musicbrainz_artist_id.as_deref(), Some("artist-1"));
    let credits = own_credits(&values.tracks[0]);
    assert_eq!(credits.len(), 1);
    assert_eq!(credits[0].credit.name, "Artist Name A");
    assert_eq!(credits[0].credit.musicbrainz_artist_id, None);
}

#[test]
fn known_mb_artist_ids_keep_same_name_artists_separate() {
    let mut track = make_mb_track("1", "Track 1");
    track.artist_credit = vec![credit("artist-1", "Artist Name A")];
    track.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("artist".to_string()),
        relation_type: Some("composer".to_string()),
        artist: Some(MbArtistRef {
            id: Some("artist-2".to_string()),
            name: Some("Artist Name A".to_string()),
            sort_name: None,
        }),
        ..MbRelation::default()
    }];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let values = map(&response, vec![]).unwrap();

    // Two artists of one name, each read with its own MusicBrainz id.
    let credits = own_credits(&values.tracks[0]);
    assert_eq!(credits[0].credit.name, "Artist Name A");
    assert_eq!(credits[0].credit.musicbrainz_artist_id.as_deref(), Some("artist-1"));
    let composer = &values.tracks[0].roles[0].artist;
    assert_eq!(composer.name, "Artist Name A");
    assert_eq!(composer.musicbrainz_artist_id.as_deref(), Some("artist-2"));
}

/// A "parts" work relation with direction "backward" names the current
/// work's *parent*, not a child: the related work becomes the parent in
/// work_parts (the forward direction is covered by the composer-work-graph
/// test above).
#[test]
fn backward_work_parts_relation_treats_related_work_as_parent() {
    let parent_work = MbWork {
        id: "mb-work-parent-a".to_string(),
        title: "Parent Work".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations: vec![],
    };
    let child_work = MbWork {
        id: "mb-work-child-a".to_string(),
        title: "Child Work".to_string(),
        disambiguation: None,
        work_type: Some("part".to_string()),
        relations: vec![MbRelation {
            target_type: Some("work".to_string()),
            relation_type: Some("parts".to_string()),
            direction: Some("backward".to_string()),
            work: Some(parent_work),
            ..MbRelation::default()
        }],
    };
    let mut track = make_mb_track("1", "Track 1");
    track.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(child_work),
        ..MbRelation::default()
    }];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let values = map(&response, vec![]).unwrap();
    assert!(
        values.works.parts.contains(&crate::works::WorkPart {
            parent: "mb-work-parent-a".to_string(),
            child: "mb-work-child-a".to_string(),
        }),
        "backward relation should make the related work the parent: {:?}",
        values.works.parts
    );
}

/// A release artist takes its discogs_artist_id from the cross-referenced
/// Discogs release, matched on name case-insensitively.
#[test]
fn release_artist_gets_discogs_id_by_case_insensitive_name() {
    let mut response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);
    response.relations = vec![serde_json::from_value(serde_json::json!({
        "type": "discogs", "target-type": "url",
        "url": {"resource": "https://www.discogs.com/release/99"}
    }))
    .expect("Discogs relationship fixture parses")];

    let values = map(&response, vec![discogs_artist_document("ARTIST NAME A")]).unwrap();

    let release_artist = &values.album_credits[0].credit;
    assert_eq!(release_artist.musicbrainz_artist_id.as_deref(), Some("artist-1"));
    assert_eq!(release_artist.discogs_artist_id.as_deref(), Some("7"));
}

/// Supplemental data cannot assign identities without a source relationship.
#[test]
fn unrelated_discogs_document_does_not_supply_an_artist_identity() {
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);

    let values = map(&response, vec![discogs_artist_document("ARTIST NAME A")]).unwrap();

    assert_eq!(values.album_credits[0].credit.discogs_artist_id, None);
}

/// No Discogs artist name matches the release artist -> no cross-ref id.
#[test]
fn release_artist_discogs_id_is_none_when_no_name_matches() {
    let mut response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![make_mb_track("1", "Track 1")],
    }]);
    response.relations = vec![serde_json::from_value(serde_json::json!({
        "type": "discogs", "target-type": "url",
        "url": {"resource": "https://www.discogs.com/release/99"}
    }))
    .expect("Discogs relationship fixture parses")];

    let values = map(&response, vec![discogs_artist_document("Different Artist")]).unwrap();

    let release_artist = &values.album_credits[0].credit;
    assert_eq!(release_artist.musicbrainz_artist_id.as_deref(), Some("artist-1"));
    assert_eq!(release_artist.discogs_artist_id, None);
}

/// A work composer relation with no artist payload is skipped with a
/// warning, not silently dropped; the work itself still imports.
#[test]
fn work_composer_relation_without_artist_payload_is_logged_and_skipped() {
    let work = MbWork {
        id: "mb-work-a".to_string(),
        title: "Work Title".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations: vec![MbRelation {
            target_type: Some("artist".to_string()),
            relation_type: Some("composer".to_string()),
            artist: None,
            ..MbRelation::default()
        }],
    };
    let mut track = make_mb_track("1", "Track 1");
    track.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(work),
        ..MbRelation::default()
    }];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let mut parsed = None;
    let logs = bae_util::test_logs::capture_warn_logs(|| {
        parsed = Some(map(&response, vec![]).unwrap());
    });
    let values = parsed.unwrap();

    assert_eq!(
        values.works.works[0].musicbrainz_work_id, "mb-work-a",
        "the work reads; only its malformed composer relation is dropped",
    );
    assert!(values.works.works[0].composers.is_empty());
    assert!(
        logs.contains("work artist relation without artist payload"),
        "expected a skip warning, got: {logs}"
    );
}

/// A work reached from two tracks is read once per release: its malformed
/// composer relation is logged a single time, and its row lands once, no
/// matter how many tracks reference it.
#[test]
fn work_referenced_by_two_tracks_logs_skip_once() {
    let work = MbWork {
        id: "mb-work-a".to_string(),
        title: "Work Title".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations: vec![MbRelation {
            target_type: Some("artist".to_string()),
            relation_type: Some("composer".to_string()),
            artist: None,
            ..MbRelation::default()
        }],
    };
    let performance = |work: MbWork| MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(work),
        ..MbRelation::default()
    };
    let mut track_one = make_mb_track("1", "Track 1");
    track_one.recording.as_mut().unwrap().relations = vec![performance(work.clone())];
    let mut track_two = make_mb_track("2", "Track 2");
    track_two.recording.as_mut().unwrap().relations = vec![performance(work)];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track_one, track_two],
    }]);

    let mut parsed = None;
    let logs = bae_util::test_logs::capture_warn_logs(|| {
        parsed = Some(map(&response, vec![]).unwrap());
    });
    let values = parsed.unwrap();

    // The work reads once and both tracks perform it.
    assert_eq!(values.works.works.len(), 1);
    assert!(values.tracks.iter().all(|track| track.works == ["mb-work-a"]));
    // The skip line is logged exactly once, not once per referencing track.
    assert_eq!(
        logs.matches("work artist relation without artist payload").count(),
        1,
        "expected the work's skip to log once per release, got: {logs}"
    );
}

/// A work "parts" relation with no work payload is skipped with a warning;
/// no work-part link is produced.
#[test]
fn work_parts_relation_without_work_payload_is_logged_and_skipped() {
    let work = MbWork {
        id: "mb-work-a".to_string(),
        title: "Work Title".to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations: vec![MbRelation {
            target_type: Some("work".to_string()),
            relation_type: Some("parts".to_string()),
            direction: Some("forward".to_string()),
            work: None,
            ..MbRelation::default()
        }],
    };
    let mut track = make_mb_track("1", "Track 1");
    track.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(work),
        ..MbRelation::default()
    }];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let mut parsed = None;
    let logs = bae_util::test_logs::capture_warn_logs(|| {
        parsed = Some(map(&response, vec![]).unwrap());
    });
    let values = parsed.unwrap();

    assert!(values.works.parts.is_empty());
    assert!(
        logs.contains("work parts relation without work payload"),
        "expected a skip warning, got: {logs}"
    );
}

/// A track artist credit whose name resolves to nothing (empty credit and
/// no artist-payload name) is skipped with a warning; the track still maps
/// and no track-artist row is produced.
#[test]
fn track_artist_credit_without_resolvable_name_is_logged_and_skipped() {
    let mut track = make_mb_track("1", "Track 1");
    track.artist_credit = vec![MbArtistCredit {
        joinphrase: None,
        name: String::new(),
        artist: Some(MbArtistRef {
            id: Some("artist-nameless".to_string()),
            name: None,
            sort_name: None,
        }),
    }];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let mut parsed = None;
    let logs = bae_util::test_logs::capture_warn_logs(|| {
        parsed = Some(map(&response, vec![]).unwrap());
    });
    let values = parsed.unwrap();

    assert_eq!(values.tracks.len(), 1);
    assert!(own_credits(&values.tracks[0]).is_empty());
    assert!(logs.contains("unresolvable artist name"), "expected a skip warning, got: {logs}");
}

/// A track credited "Artist A feat. Artist B" keeps both credits, the join
/// after the first, and the name the credit prints where it is not the
/// artist's own; a credit naming no artist object keeps its printed name.
#[test]
fn a_track_credit_keeps_each_printed_name_and_join() {
    let mut track = make_mb_track("1", "Track 1");
    track.artist_credit = vec![
        MbArtistCredit {
            joinphrase: Some(" feat. ".to_string()),
            ..credit("artist-a", "Artist A")
        },
        MbArtistCredit {
            name: "B Printed".to_string(),
            joinphrase: Some(" & ".to_string()),
            ..credit("artist-b", "Artist B")
        },
        MbArtistCredit {
            name: "Nobody Named".to_string(),
            artist: None,
            joinphrase: Some("".to_string()),
        },
    ];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let values = map(&response, vec![]).unwrap();

    let TrackCredits::Own(credits) = &values.tracks[0].credits else {
        panic!("the track credits its own artists");
    };
    let stated: Vec<(&str, &str, Option<&str>)> = credits
        .iter()
        .map(|stated| {
            (stated.credit.name.as_str(), stated.credit.printed(), stated.join.as_deref())
        })
        .collect();
    assert_eq!(
        stated,
        [
            ("Artist A", "Artist A", Some(" feat. ")),
            ("Artist B", "B Printed", Some(" & ")),
            ("Nobody Named", "Nobody Named", None),
        ]
    );
    assert_eq!(
        crate::credits::SourceCredit::line(credits).plain_text(),
        "Artist A feat. B Printed & Nobody Named"
    );
}

/// A work's composer and a recording's composer are their artist by the
/// artist's own name, printing the name the relation credits.
#[test]
fn role_credits_name_the_artist_and_print_the_relation_s_credit() {
    let artist = MbArtistRef {
        id: Some("composer-artist-a".to_string()),
        name: Some("Composer Name A".to_string()),
        sort_name: Some("Composer Name A".to_string()),
    };
    let other = MbArtistRef {
        id: Some("composer-artist-b".to_string()),
        name: Some("Composer Name B".to_string()),
        sort_name: Some("Composer Name B".to_string()),
    };
    let relation = |artist: &MbArtistRef, credit: Option<&str>| MbRelation {
        target_type: Some("artist".to_string()),
        relation_type: Some("composer".to_string()),
        artist: Some(artist.clone()),
        target_credit: credit.map(str::to_string),
        ..MbRelation::default()
    };
    let composed_work = MbWork {
        id: "mb-work-a".to_string(),
        title: "Work Title A".to_string(),
        disambiguation: None,
        work_type: None,
        relations: vec![relation(&artist, Some("Composer Printed Name")), relation(&other, None)],
    };
    let mut track = make_mb_track("1", "Track Title 1");
    track.recording.as_mut().unwrap().relations = vec![MbRelation {
        target_type: Some("work".to_string()),
        relation_type: Some("performance".to_string()),
        work: Some(composed_work),
        ..MbRelation::default()
    }];
    let response = make_response(vec![MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks: vec![track],
    }]);

    let values = map(&response, vec![]).unwrap();

    let composers = &work(&values, "mb-work-a").composers;
    assert_eq!(composers[0].name, "Composer Name A");
    assert_eq!(composers[0].credited_as.as_deref(), Some("Composer Printed Name"));
    assert_eq!(composers[1].name, "Composer Name B");
    assert_eq!(composers[1].credited_as, None);
}
