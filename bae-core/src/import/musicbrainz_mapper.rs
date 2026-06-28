//! # Import flow
//!
//! Identity rows land in `release_identities` per source, written at
//! commit. The mapper here is concerned with the per-source response →
//! `DbAlbum` / `DbRelease` mapping plus surfacing any cross-link the
//! response carries (MB url-rels → Discogs); the actual
//! `release_identities` writes happen in the commit path.
//!
//! MB → Discogs cross-link: MB releases carry url-rels that routinely
//! include Discogs release URLs. Parse them out here so the commit path
//! can write both an MB and a Discogs identity row from a single MB
//! import. The reverse (Discogs → MB) is less reliable — Discogs API
//! doesn't expose MBIDs as a standard field. Discogs imports typically
//! produce only a Discogs identity row at first; the reverse cross-link
//! is resolved via MB's URL endpoint.

use super::ParsedAlbum;
use crate::db::{DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbTrack, DbTrackArtist};
use crate::import::types::ReleaseIdentity;
use crate::import::MetadataSource;
use crate::musicbrainz::{fetch_release_group_json, ExternalUrls, MbReleaseResponse};
use coven::Clock;
use coven::IdProvider;
use tracing::warn;

/// Extract the leading numeric Discogs release ID from a Discogs release URL.
///
/// MB editors store these URLs in three shapes:
///   - bare numeric: `https://www.discogs.com/release/12345`
///   - trailing slash: `https://www.discogs.com/release/12345/`
///   - slug suffix: `https://www.discogs.com/release/12345-Album-Title`
///
/// Returns `None` if the last path segment doesn't start with digits.
pub(crate) fn extract_discogs_release_id(url: &str) -> Option<String> {
    let trimmed = url.trim_end_matches('/');
    let last = trimmed.rsplit('/').next()?;
    let id: String = last.chars().take_while(|c| c.is_ascii_digit()).collect();
    (!id.is_empty()).then_some(id)
}

/// Fetch a MusicBrainz release. Pure MB: no Discogs client, no cross-ref.
/// Returns the parsed response, the url-rels we'll need for cross-referencing
/// later, and the raw JSON pairs (release + release-group) for archival in
/// `release_metadata`.
///
/// Cross-referencing into Discogs is a separate step — see
/// `crate::discogs::client::enrich_with_discogs_xref`. The split keeps
/// prefetch's API discogs-free; only commit composes both.
pub async fn fetch_mb_response(
    release_id: &str,
) -> Result<(MbReleaseResponse, ExternalUrls, Vec<(String, String)>), String> {
    let (response, external_urls, raw_json) =
        crate::retry::retry_with_backoff(3, "MusicBrainz release fetch", || {
            crate::musicbrainz::lookup_release_by_id(release_id)
        })
        .await
        .map_err(|e| format!("Failed to fetch MusicBrainz release: {}", e))?;

    let mut metadata_pairs = vec![(MetadataSource::MusicBrainz.as_str().to_string(), raw_json)];

    let release_group_id = response.release_group.as_ref().map(|rg| rg.id.as_str());

    if let Some(rg_id) = release_group_id {
        match fetch_release_group_json(rg_id).await {
            Ok(rg_json) => {
                metadata_pairs.push(("musicbrainz_release_group".to_string(), rg_json));
            }
            Err(e) => {
                warn!("Failed to fetch MB release-group: {}", e);
            }
        }
    }

    Ok((response, external_urls, metadata_pairs))
}

/// Map a typed MusicBrainz release response into database models (pure, no I/O).
///
/// `discogs_release`: optional Discogs data resolved from MB url-rels (path 2).
/// When present, the Discogs columns on both `DbAlbum` (master) and `DbRelease`
/// (release) are populated alongside the MB columns.
pub fn map_mb_response_to_db(
    response: &MbReleaseResponse,
    master_year: Option<u32>,
    discogs_release: Option<crate::discogs::DiscogsRelease>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, String> {
    let now = clock.now();
    let mut artists = Vec::new();

    for credit in &response.artist_credit {
        if let Some(artist_obj) = &credit.artist {
            let Some(artist_name) = artist_obj.name.clone() else {
                continue;
            };

            let mb_artist_id = artist_obj.id.clone();

            let sort_name = artist_obj
                .sort_name
                .clone()
                .unwrap_or_else(|| artist_name.clone());

            let discogs_artist_id = discogs_release.as_ref().and_then(|dr| {
                dr.artists
                    .iter()
                    .find(|da| da.name.eq_ignore_ascii_case(&artist_name))
                    .map(|da| da.id.clone())
            });

            let artist = DbArtist {
                id: ids.new_id(),
                name: artist_name,
                sort_name: Some(sort_name),
                discogs_artist_id,
                musicbrainz_artist_id: mb_artist_id,
                created_at: now,
            };
            artists.push(artist);
        }
    }

    if artists.is_empty() {
        let artist_name = response
            .artist_credit
            .first()
            .expect("MusicBrainz release has no artist credits")
            .name
            .clone();
        let artist = DbArtist {
            id: ids.new_id(),
            name: artist_name.clone(),
            sort_name: Some(artist_name),
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        artists.push(artist);
    }

    let primary_artist_id = &artists[0].id;

    let album =
        DbAlbum::from_mb_response(response, master_year, primary_artist_id, ids.new_id(), now);
    let db_release = DbRelease::from_mb_response(&album.id, response, ids.new_id(), now);

    // Identity rows. Always one for MB (every MB release belongs to a
    // release group; absence is a structural bug in the response, not a
    // runtime case). When MB url-rels resolved a Discogs release,
    // contribute a second row so future Discogs imports of the same
    // master attach to this album. Both are Exact: the user committed
    // against the MB pressing, and the cross-link names a specific
    // Discogs pressing.
    let mb_release_group = response
        .release_group
        .as_ref()
        .expect("MusicBrainz release missing release_group");
    let mut identities = vec![ReleaseIdentity {
        source: MetadataSource::MusicBrainz,
        source_group_id: mb_release_group.id.clone(),
        source_release_id: Some(response.id.clone()),
    }];
    if let Some(dr) = discogs_release.as_ref() {
        if let Some(master_id) = dr.master_id.clone() {
            identities.push(ReleaseIdentity {
                source: MetadataSource::Discogs,
                source_group_id: master_id,
                source_release_id: Some(dr.id.clone()),
            });
        }
    }

    // Additional artists (position > 0) go in the junction table
    let album_artists: Vec<DbAlbumArtist> = artists
        .iter()
        .enumerate()
        .skip(1)
        .map(|(position, artist)| {
            DbAlbumArtist::new(&album.id, &artist.id, position as i32, ids.new_id(), now)
        })
        .collect();

    let mut tracks = Vec::new();
    let mut track_artists = Vec::new();

    // Compute side base: each medium contributes 1 or 2 sides depending on format.
    let mut side_base = 0i32;

    for medium in &response.media {
        if medium.tracks.is_empty() {
            return Err(format!(
                "MusicBrainz release {} has a medium with no tracks",
                response.id
            ));
        }

        let is_multi_side_medium = medium
            .format
            .as_deref()
            .is_some_and(|f| f.contains("Vinyl") || f.contains("Cassette"));

        // For multi-side media (vinyl/cassette), derive side from the track's `number` field
        // (e.g., "A1" -> side offset 0, "B2" -> side offset 1). Offsets are relative to the
        // first letter in this medium (so medium 2 with C/D tracks gets offsets 0/1, not 2/3).
        // For single-side media, all tracks = 1 side.
        let mut per_side_count: std::collections::HashMap<i32, i32> =
            std::collections::HashMap::new();

        // Find the first letter in this medium to use as the base for relative offsets
        let medium_base_letter = if is_multi_side_medium {
            medium
                .tracks
                .iter()
                .filter_map(|t| t.number.as_deref()?.chars().next())
                .filter(|c| c.is_ascii_alphabetic())
                .map(|c| c.to_ascii_uppercase() as i32)
                .min()
                .unwrap_or('A' as i32)
        } else {
            'A' as i32
        };

        for track in &medium.tracks {
            let title = track
                .recording
                .as_ref()
                .and_then(|r| r.title.clone())
                .unwrap_or_else(|| "Unknown Track".to_string());

            let (side_offset, track_number) = if is_multi_side_medium {
                // Derive side offset relative to this medium's first letter.
                // A multi-side medium track without a leading side letter (e.g. "A1",
                // "B2") is malformed — there's no way to assign it to a side, and
                // silently grouping it onto offset 0 would corrupt side numbering.
                let side_letter = track
                    .number
                    .as_deref()
                    .and_then(|n| n.chars().next())
                    .filter(|c| c.is_ascii_alphabetic())
                    .ok_or_else(|| {
                        format!(
                            "MusicBrainz multi-side medium track has no side letter: \
                             number={:?}, title={:?}",
                            track.number,
                            track.recording.as_ref().and_then(|r| r.title.as_ref()),
                        )
                    })?;
                let offset = (side_letter.to_ascii_uppercase() as i32) - medium_base_letter;
                let count = per_side_count.entry(offset).or_insert(0);
                *count += 1;
                (offset, *count)
            } else {
                let count = per_side_count.entry(0).or_insert(0);
                *count += 1;
                (0, *count)
            };

            let side = side_base + side_offset + 1;

            let db_track = DbTrack {
                id: ids.new_id(),
                release_id: db_release.id.clone(),
                title,
                side,
                track_number: Some(track_number),
                duration_ms: None,
                discogs_position: track.number.clone(),
                created_at: now,
            };

            for (credit_pos, credit) in track.artist_credit.iter().enumerate() {
                if let Some(artist_obj) = &credit.artist {
                    let Some(artist_name) = artist_obj.name.clone() else {
                        continue;
                    };
                    let mb_artist_id = artist_obj.id.clone();

                    // Find this artist (dedup by MB artist ID, else by name),
                    // or create and insert it — one scan, one predicate.
                    let artist_id = if let Some(existing) = artists.iter().find(|a| {
                        if let (Some(existing_id), Some(ref new_id)) =
                            (&a.musicbrainz_artist_id, &mb_artist_id)
                        {
                            existing_id == new_id
                        } else {
                            a.name.eq_ignore_ascii_case(&artist_name)
                        }
                    }) {
                        existing.id.clone()
                    } else {
                        let sort_name = artist_obj
                            .sort_name
                            .clone()
                            .unwrap_or_else(|| artist_name.clone());

                        let discogs_artist_id = discogs_release.as_ref().and_then(|dr| {
                            dr.artists
                                .iter()
                                .find(|da| da.name.eq_ignore_ascii_case(&artist_name))
                                .map(|da| da.id.clone())
                        });

                        let artist = DbArtist {
                            id: ids.new_id(),
                            name: artist_name.clone(),
                            sort_name: Some(sort_name),
                            discogs_artist_id,
                            musicbrainz_artist_id: mb_artist_id.clone(),
                            created_at: now,
                        };
                        let id = artist.id.clone();
                        artists.push(artist);
                        id
                    };

                    track_artists.push(DbTrackArtist::new(
                        &db_track.id,
                        &artist_id,
                        credit_pos as i32,
                        ids.new_id(),
                        now,
                    ));
                }
            }

            tracks.push(db_track);
        }

        // Advance side_base for the next medium
        if is_multi_side_medium {
            // Each vinyl/cassette medium contributes as many sides as we saw.
            // per_side_count is non-empty: the medium has tracks (checked above)
            // and every track inserts an entry.
            let max_offset = per_side_count
                .keys()
                .copied()
                .max()
                .expect("per_side_count populated by non-empty medium");
            side_base += max_offset + 1;
        } else {
            side_base += 1;
        }
    }

    Ok(ParsedAlbum {
        album,
        release: db_release,
        tracks,
        artists,
        album_artists,
        track_artists,
        identities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::musicbrainz::{
        MbArtistCredit, MbArtistRef, MbMedium, MbRecording, MbReleaseResponse, MbTrack,
    };
    use coven::FixedClock;
    use coven::SequentialIdProvider;

    /// Run the mapper with deterministic fakes. Exercises the real
    /// `map_mb_response_to_db`; only the clock/id inputs are faked.
    fn map(
        response: &MbReleaseResponse,
        master_year: Option<u32>,
        discogs_release: Option<crate::discogs::DiscogsRelease>,
    ) -> Result<ParsedAlbum, String> {
        let clock = FixedClock(
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        let ids = SequentialIdProvider::new("mb");
        map_mb_response_to_db(response, master_year, discogs_release, &clock, &ids)
    }

    fn make_mb_track(number: &str, title: &str) -> MbTrack {
        MbTrack {
            position: None,
            number: Some(number.to_string()),
            title: None,
            length: None,
            recording: Some(MbRecording {
                title: Some(title.to_string()),
            }),
            artist_credit: vec![],
        }
    }

    fn make_response(media: Vec<MbMedium>) -> MbReleaseResponse {
        MbReleaseResponse {
            id: "test-release".to_string(),
            title: "Test Album".to_string(),
            date: Some("2024".to_string()),
            country: None,
            barcode: None,
            artist_credit: vec![MbArtistCredit {
                name: "Test Artist".to_string(),
                artist: Some(MbArtistRef {
                    id: Some("artist-1".to_string()),
                    name: Some("Test Artist".to_string()),
                    sort_name: Some("Artist, Test".to_string()),
                }),
            }],
            release_group: Some(crate::musicbrainz::MbReleaseGroupRef {
                id: "rg-test".to_string(),
                first_release_date: Some("2024".to_string()),
                relations: None,
            }),
            label_info: vec![],
            media,
            relations: vec![],
        }
    }

    #[test]
    fn test_cd_two_media_each_one_side() {
        let response = make_response(vec![
            MbMedium {
                format: Some("CD".to_string()),
                tracks: vec![make_mb_track("1", "Track 1"), make_mb_track("2", "Track 2")],
            },
            MbMedium {
                format: Some("CD".to_string()),
                tracks: vec![make_mb_track("1", "Track 3"), make_mb_track("2", "Track 4")],
            },
        ]);

        let parsed = map(&response, Some(2024), None).unwrap();
        let tracks = &parsed.tracks;

        assert_eq!(tracks.len(), 4);

        // Medium 1 = side 1
        assert_eq!(tracks[0].side, 1);
        assert_eq!(tracks[0].track_number, Some(1));
        assert_eq!(tracks[1].side, 1);
        assert_eq!(tracks[1].track_number, Some(2));

        // Medium 2 = side 2
        assert_eq!(tracks[2].side, 2);
        assert_eq!(tracks[2].track_number, Some(1));
        assert_eq!(tracks[3].side, 2);
        assert_eq!(tracks[3].track_number, Some(2));
    }

    #[test]
    fn test_vinyl_one_medium_two_sides() {
        let response = make_response(vec![MbMedium {
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                make_mb_track("A1", "Track A1"),
                make_mb_track("A2", "Track A2"),
                make_mb_track("B1", "Track B1"),
                make_mb_track("B2", "Track B2"),
            ],
        }]);

        let parsed = map(&response, Some(2024), None).unwrap();
        let tracks = &parsed.tracks;

        assert_eq!(tracks.len(), 4);

        // A tracks = side 1
        assert_eq!(tracks[0].side, 1);
        assert_eq!(tracks[0].track_number, Some(1));
        assert_eq!(tracks[1].side, 1);
        assert_eq!(tracks[1].track_number, Some(2));

        // B tracks = side 2
        assert_eq!(tracks[2].side, 2);
        assert_eq!(tracks[2].track_number, Some(1));
        assert_eq!(tracks[3].side, 2);
        assert_eq!(tracks[3].track_number, Some(2));
    }

    /// 2LP vinyl: two media, each with two sides (A/B and C/D).
    /// Sides must be 1,2,3,4 — not 1,2,3+2,4+2.
    #[test]
    fn test_vinyl_two_media_four_sides() {
        let response = make_response(vec![
            MbMedium {
                format: Some("12\" Vinyl".to_string()),
                tracks: vec![
                    make_mb_track("A1", "Track A1"),
                    make_mb_track("A2", "Track A2"),
                    make_mb_track("B1", "Track B1"),
                    make_mb_track("B2", "Track B2"),
                ],
            },
            MbMedium {
                format: Some("12\" Vinyl".to_string()),
                tracks: vec![
                    make_mb_track("C1", "Track C1"),
                    make_mb_track("C2", "Track C2"),
                    make_mb_track("D1", "Track D1"),
                    make_mb_track("D2", "Track D2"),
                ],
            },
        ]);

        let parsed = map(&response, Some(2024), None).unwrap();
        let tracks = &parsed.tracks;

        assert_eq!(tracks.len(), 8);

        // Medium 1: A = side 1, B = side 2
        assert_eq!(tracks[0].side, 1);
        assert_eq!(tracks[1].side, 1);
        assert_eq!(tracks[2].side, 2);
        assert_eq!(tracks[3].side, 2);

        // Medium 2: C = side 3, D = side 4
        assert_eq!(tracks[4].side, 3);
        assert_eq!(tracks[5].side, 3);
        assert_eq!(tracks[6].side, 4);
        assert_eq!(tracks[7].side, 4);
    }

    #[test]
    fn test_single_medium_cd_all_side_one() {
        let response = make_response(vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![
                make_mb_track("1", "Track 1"),
                make_mb_track("2", "Track 2"),
                make_mb_track("3", "Track 3"),
            ],
        }]);

        let parsed = map(&response, Some(2024), None).unwrap();
        let tracks = &parsed.tracks;

        assert_eq!(tracks.len(), 3);

        // All tracks on side 1
        assert_eq!(tracks[0].side, 1);
        assert_eq!(tracks[0].track_number, Some(1));
        assert_eq!(tracks[1].side, 1);
        assert_eq!(tracks[1].track_number, Some(2));
        assert_eq!(tracks[2].side, 1);
        assert_eq!(tracks[2].track_number, Some(3));
    }

    /// A vinyl medium track without a leading side letter is malformed MB data:
    /// there's no way to assign it to a side. Surface the error instead of
    /// silently bucketing it onto side 1.
    #[test]
    fn test_vinyl_track_missing_side_letter_errors() {
        let response = make_response(vec![MbMedium {
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                make_mb_track("A1", "Track A1"),
                MbTrack {
                    position: None,
                    number: None,
                    title: None,
                    length: None,
                    recording: Some(MbRecording {
                        title: Some("Side-less Track".to_string()),
                    }),
                    artist_credit: vec![],
                },
            ],
        }]);

        let err = map(&response, Some(2024), None)
            .expect_err("expected error for vinyl track without side letter");
        assert!(
            err.contains("no side letter"),
            "unexpected error message: {}",
            err
        );
    }

    /// A track number like "1" on a vinyl medium has no side letter to derive
    /// offset from. Same failure mode as a missing number.
    #[test]
    fn test_vinyl_track_numeric_only_errors() {
        let response = make_response(vec![MbMedium {
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                make_mb_track("A1", "Track A1"),
                make_mb_track("1", "Numeric-only Track"),
            ],
        }]);

        let err = map(&response, Some(2024), None)
            .expect_err("expected error for vinyl track with numeric-only number");
        assert!(
            err.contains("no side letter"),
            "unexpected error message: {}",
            err
        );
    }

    #[test]
    fn medium_with_no_tracks_returns_err() {
        let response = make_response(vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![],
        }]);

        let result = map(&response, Some(2024), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no tracks"));
    }

    #[test]
    fn extract_discogs_release_id_cases() {
        // The leading numeric segment after `/release/` is the id: bare,
        // trailing-slash, and slug-suffixed forms all yield it; a non-numeric
        // segment, an empty path, or an unrelated host yield None.
        let cases = [
            ("https://www.discogs.com/release/12345", Some("12345")),
            ("https://www.discogs.com/release/12345/", Some("12345")),
            (
                "https://www.discogs.com/release/12345-Album-Title",
                Some("12345"),
            ),
            (
                "https://www.discogs.com/release/12345-Album-Title/",
                Some("12345"),
            ),
            ("https://www.discogs.com/release/abc", None),
            ("https://www.discogs.com/release/", None),
            ("https://example.com/something/abc", None),
        ];
        for (url, expected) in cases {
            assert_eq!(
                extract_discogs_release_id(url),
                expected.map(str::to_string),
                "url: {url}"
            );
        }
    }

    // ── identities (parsed.identities) ─────────────────────────────────

    fn discogs_release_with_master(master_id: Option<String>) -> crate::discogs::DiscogsRelease {
        crate::discogs::DiscogsRelease {
            id: "d-rel-99".to_string(),
            title: "Test Album".to_string(),
            year: Some(2024),
            genre: vec![],
            style: vec![],
            format: vec![],
            country: None,
            label: vec![],
            cover_image: None,
            thumb: None,
            catno: None,
            artists: vec![],
            tracklist: vec![],
            master_id,
        }
    }

    #[test]
    fn test_map_mb_no_cross_ref_yields_only_mb_identity() {
        let response = make_response(vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Track 1")],
        }]);

        let parsed = map(&response, None, None).unwrap();

        assert_eq!(parsed.identities.len(), 1);
        let mb = &parsed.identities[0];
        assert_eq!(mb.source, MetadataSource::MusicBrainz);
        assert_eq!(mb.source_group_id, "rg-test");
        assert_eq!(mb.source_release_id.as_deref(), Some("test-release"));
    }

    #[test]
    fn test_map_mb_cross_ref_no_master_id_yields_only_mb_identity() {
        // Cross-ref hit but the linked Discogs release has no master_id —
        // the parser doesn't fabricate a group; only the MB row is emitted.
        let response = make_response(vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Track 1")],
        }]);
        let discogs_release = discogs_release_with_master(None);

        let parsed = map(&response, None, Some(discogs_release)).unwrap();

        assert_eq!(parsed.identities.len(), 1);
        assert_eq!(parsed.identities[0].source, MetadataSource::MusicBrainz);
    }

    #[test]
    fn test_map_mb_cross_ref_with_master_id_yields_two_identity_rows() {
        // Cross-ref hit AND the linked Discogs release carries a master_id
        // — two rows: MB + Discogs. Both Exact (release IDs present).
        let response = make_response(vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Track 1")],
        }]);
        let discogs_release = discogs_release_with_master(Some("d-master-123".to_string()));

        let parsed = map(&response, None, Some(discogs_release)).unwrap();

        assert_eq!(parsed.identities.len(), 2);

        let mb = &parsed.identities[0];
        assert_eq!(mb.source, MetadataSource::MusicBrainz);
        assert_eq!(mb.source_group_id, "rg-test");
        assert_eq!(mb.source_release_id.as_deref(), Some("test-release"));

        let discogs = &parsed.identities[1];
        assert_eq!(discogs.source, MetadataSource::Discogs);
        assert_eq!(discogs.source_group_id, "d-master-123");
        assert_eq!(discogs.source_release_id.as_deref(), Some("d-rel-99"));
    }

    fn credit(id: &str, name: &str) -> MbArtistCredit {
        MbArtistCredit {
            name: name.to_string(),
            artist: Some(MbArtistRef {
                id: Some(id.to_string()),
                name: Some(name.to_string()),
                sort_name: None,
            }),
        }
    }

    #[test]
    fn track_level_artist_credit_creates_and_links_a_new_artist() {
        // Track 2 credits a guest distinct from the release artist. The
        // track-artist loop must create that artist once and link it to track 2
        // only; track 1 (no credits) gets no track-artist rows.
        let mut featured = make_mb_track("2", "Track 2 (feat. Guest)");
        featured.artist_credit = vec![credit("artist-guest", "Guest")];
        let response = make_response(vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![make_mb_track("1", "Track 1"), featured],
        }]);

        let parsed = map(&response, Some(2024), None).unwrap();

        let guest = parsed
            .artists
            .iter()
            .find(|a| a.musicbrainz_artist_id.as_deref() == Some("artist-guest"))
            .expect("guest artist created");
        assert_eq!(guest.name, "Guest");

        let track2 = &parsed.tracks[1];
        assert!(parsed
            .track_artists
            .iter()
            .any(|ta| ta.track_id == track2.id && ta.artist_id == guest.id));

        let track1 = &parsed.tracks[0];
        assert!(
            !parsed
                .track_artists
                .iter()
                .any(|ta| ta.track_id == track1.id),
            "a track with no credits gets no track-artist rows"
        );
    }

    #[test]
    fn track_level_artist_credit_dedupes_against_release_artist_by_mb_id() {
        // The track credits the release artist again by the same MB id; the loop
        // must reuse the existing artist rather than create a duplicate.
        let mut t = make_mb_track("1", "Track 1");
        t.artist_credit = vec![credit("artist-1", "Test Artist")];
        let response = make_response(vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![t],
        }]);

        let parsed = map(&response, Some(2024), None).unwrap();

        let matching: Vec<_> = parsed
            .artists
            .iter()
            .filter(|a| a.musicbrainz_artist_id.as_deref() == Some("artist-1"))
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "release artist must not be duplicated by a track credit"
        );
        assert!(parsed
            .track_artists
            .iter()
            .any(|ta| ta.track_id == parsed.tracks[0].id && ta.artist_id == matching[0].id));
    }
}
