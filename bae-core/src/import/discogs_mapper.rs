use super::assemble::{
    assemble_parsed_album, AlbumArtistScope, ArtistRef, ReleaseIr, ReleaseRole, TrackEvent,
    TrackIr, TrackNumber,
};
use super::ParsedAlbum;
use crate::db::{is_various_artists, Pressing, ReleaseMetadataSource};
use crate::discogs::{DiscogsArtist, DiscogsRelease, DiscogsRoleArtist};
use crate::import::types::ReleaseIdentity;
use crate::import::{ImportError, MetadataSource};
use crate::musicbrainz::MbReleaseResponse;
use coven::Clock;
use coven::IdProvider;
use std::collections::HashSet;
use tracing::{debug, warn};

fn discogs_role_is_composer(role: &str) -> bool {
    let lowered = role
        .chars()
        .filter(|c| *c != '[' && *c != ']')
        .collect::<String>()
        .to_ascii_lowercase();
    let compact = lowered.replace(['-', '_'], " ");
    compact.contains("composed by")
        || compact.contains("written by")
        || compact.contains("music by")
        || compact.contains("composer")
}

/// An [`ArtistRef`] for a Discogs artist: the display name doubles as its own
/// sort name, and the Discogs artist id (when present) is what dedups the pool.
/// Discogs artists carry no MusicBrainz id.
fn discogs_artist_ref(name: String, discogs_artist_id: Option<String>) -> ArtistRef {
    ArtistRef {
        name: name.clone(),
        sort_name: Some(name),
        musicbrainz_artist_id: None,
        discogs_artist_id,
    }
}

/// An [`ArtistRef`] for a Discogs role credit. The display name is the
/// `credited_name`, falling back to the canonical name (logged) when absent.
fn discogs_role_artist_ref(credit: &DiscogsRoleArtist) -> ArtistRef {
    let name = match credit.credited_name.clone() {
        Some(name) => name,
        None => {
            warn!(
                discogs_artist_id = ?credit.id,
                artist_name = %credit.name,
                "Discogs role artist has no credited name; using canonical name"
            );
            credit.name.clone()
        }
    };
    discogs_artist_ref(name, credit.id.clone())
}

/// An [`ArtistRef`] for a Discogs display credit, keyed on its canonical name.
fn discogs_track_artist_ref(credit: &DiscogsArtist) -> ArtistRef {
    discogs_artist_ref(credit.name.clone(), Some(credit.id.clone()))
}

/// Map Discogs release metadata into database models including artist information.
///
/// Converts a DiscogsRelease (from the API) into DbAlbum, DbRelease, DbTrack, and artist records
/// ready for database insertion. Extracts artist data from Discogs API response,
/// generates IDs, and links all entities together.
///
/// `master_year` is the original release year from the Discogs master release.
/// Falls back to the specific release year if unavailable.
///
/// `mb_xref`: optional MB release resolved via the URL endpoint
/// (`crate::musicbrainz::fetch_mb_xref`). When present, contributes a
/// second `release_identities` row so future MB-rooted imports of the
/// same release group attach to this album.
pub fn map_discogs_to_db(
    release: &DiscogsRelease,
    master_year: Option<u32>,
    mb_xref: Option<&MbReleaseResponse>,
    clock: &dyn Clock,
    ids: &dyn IdProvider,
) -> Result<ParsedAlbum, ImportError> {
    // Release artists → the artist pool. With no artists list, fall back to
    // the artist half of the "Artist - Album" title split.
    let mut release_refs: Vec<ArtistRef> = if release.artists.is_empty() {
        let artist_name = crate::discogs::split_title(&release.title)
            .and_then(|(artist, _)| artist)
            .ok_or_else(|| ImportError::SourceData {
                metadata_source: MetadataSource::Discogs,
                detail: format!(
                    "Discogs release {} has no release artist in artists list or title",
                    release.id
                ),
            })?
            .to_string();
        vec![ArtistRef {
            name: artist_name.clone(),
            sort_name: Some(artist_name),
            musicbrainz_artist_id: None,
            discogs_artist_id: None,
        }]
    } else {
        release
            .artists
            .iter()
            .map(discogs_track_artist_ref)
            .collect()
    };
    let primary_artist = release_refs.remove(0);

    let album_year = master_year
        .map(|y| y as i32)
        .or(release.year.map(|y| y as i32));
    let is_compilation = release
        .artists
        .first()
        .map(|a| is_various_artists(&a.name))
        .unwrap_or(false);
    let format = if release.format.is_empty() {
        None
    } else {
        Some(release.format.join(", "))
    };
    let pressing = Pressing {
        year: release.year.map(|y| y as i32),
        format,
        label: release.label.first().cloned(),
        catalog_number: release.catno.clone(),
        country: release.country.clone(),
        barcode: None,
    };

    // Release-level composer role credits, positions from the source order
    // (non-composer roles are skipped, so positions keep holes).
    let mut release_roles: Vec<ReleaseRole> = Vec::new();
    if let Some(extraartists) = release.extraartists.as_ref() {
        for (position, credit) in extraartists.iter().enumerate() {
            if discogs_role_is_composer(&credit.role) {
                release_roles.push(ReleaseRole {
                    position: position as i32,
                    artist: discogs_role_artist_ref(credit),
                    source: MetadataSource::Discogs,
                    source_credit: Some(credit.role.clone()),
                });
            } else {
                debug!(
                    discogs_release_id = %release.id,
                    artist_name = %credit.name,
                    role = %credit.role,
                    "Skipping Discogs release-level extraartist with non-composer role"
                );
            }
        }
    }

    let processed = process_tracklist(&release.tracklist);
    let tracks: Vec<TrackIr> = processed
        .iter()
        .map(|pt| discogs_track_ir(release, pt))
        .collect();

    // Identity rows. Discogs's `master_id` is what we group on; absence
    // means the release stands on its own (no group identity to speak
    // of) and we emit no Discogs row — the import still commits, just
    // without that source's identity.
    //
    // When `mb_xref` is present, MB has a back-link to this Discogs
    // release. Contribute a second row so future MB-rooted imports of
    // the same release group attach to this album. Both rows are
    // Exact: the user committed against the Discogs pressing, and the
    // cross-link names a specific MB release.
    let mut identities: Vec<ReleaseIdentity> = release
        .master_id
        .as_ref()
        .map(|master_id| ReleaseIdentity {
            source: MetadataSource::Discogs,
            source_group_id: master_id.clone(),
            source_release_id: Some(release.id.clone()),
        })
        .into_iter()
        .collect();
    if let Some(mb) = mb_xref {
        let rg = mb
            .release_group
            .as_ref()
            .ok_or_else(|| ImportError::SourceData {
                metadata_source: MetadataSource::MusicBrainz,
                detail: format!("MusicBrainz release {} missing release_group", mb.id),
            })?;
        identities.push(ReleaseIdentity {
            source: MetadataSource::MusicBrainz,
            source_group_id: rg.id.clone(),
            source_release_id: Some(mb.id.clone()),
        });
    }

    let ir = ReleaseIr {
        album_title: release.title.clone(),
        primary_artist,
        additional_artists: release_refs,
        album_year,
        is_compilation,
        pressing,
        metadata_source: ReleaseMetadataSource::Discogs,
        metadata_source_release_id: Some(release.id.clone()),
        album_artist_scope: AlbumArtistScope::ReleaseCredits,
        release_roles,
        tracks,
        identities,
    };

    Ok(assemble_parsed_album(ir, clock, ids))
}

/// Build one track's IR from a processed Discogs track. Role credits precede
/// display credits per source row (preserving the artist-pool discovery order);
/// display credits are deduped across a collapsed track's source rows by Discogs
/// artist id, first occurrence wins, with positions compacted `0..n`.
fn discogs_track_ir(release: &DiscogsRelease, pt: &ProcessedTrack) -> TrackIr {
    let mut events: Vec<TrackEvent> = Vec::new();
    let mut seen_credit_ids: HashSet<String> = HashSet::new();
    let mut credit_position = 0i32;

    for source_track_index in &pt.source_track_indices {
        let discogs_track = &release.tracklist[*source_track_index];
        match discogs_track.extraartists.as_ref() {
            Some(extraartists) => {
                for (role_position, credit) in extraartists.iter().enumerate() {
                    if discogs_role_is_composer(&credit.role) {
                        events.push(TrackEvent::Role {
                            position: role_position as i32,
                            artist: discogs_role_artist_ref(credit),
                            source: MetadataSource::Discogs,
                            source_credit: Some(credit.role.clone()),
                        });
                    } else {
                        debug!(
                            discogs_release_id = %release.id,
                            discogs_track_position = %discogs_track.position,
                            track_title = %discogs_track.title,
                            artist_name = %credit.name,
                            role = %credit.role,
                            "Skipping Discogs track-level extraartist with non-composer role"
                        );
                    }
                }
            }
            None => {
                debug!(
                    discogs_release_id = %release.id,
                    discogs_track_position = %discogs_track.position,
                    track_title = %discogs_track.title,
                    "Discogs track has no extraartists field; skipping per-track role credits"
                );
            }
        }

        for discogs_artist in &discogs_track.artists {
            // One display-credit row per distinct Discogs artist across the
            // collapsed track; positions compact over the rows actually emitted.
            if seen_credit_ids.insert(discogs_artist.id.clone()) {
                events.push(TrackEvent::Credit {
                    position: credit_position,
                    artist: discogs_track_artist_ref(discogs_artist),
                });
                credit_position += 1;
            }
        }
    }

    TrackIr {
        title: pt.title.clone(),
        side: pt.side,
        number: TrackNumber::PerSide,
        source_position: Some(pt.position.clone()),
        events,
    }
}

/// A processed track: heading/sub-track collapsing and index filtering applied,
/// with its side derived from the position. Per-side track numbers are assigned
/// downstream by the assembler's single numbering pass.
pub(crate) struct ProcessedTrack {
    pub title: String,
    pub position: String,
    /// Source tracklist indices. For collapsed sub-tracks, this contains all
    /// sub-track entries; for regular tracks, this contains the one source row.
    pub source_track_indices: Vec<usize>,
    pub side: i32,
}

/// Process a Discogs tracklist: filter index entries, collapse headings with
/// sub-tracks, and assign sides from the position format.
pub(crate) fn process_tracklist(tracklist: &[crate::discogs::DiscogsTrack]) -> Vec<ProcessedTrack> {
    use crate::discogs::DiscogsTrack;

    // Phase 1: Filter out "index" entries
    let filtered: Vec<(usize, &DiscogsTrack)> = tracklist
        .iter()
        .enumerate()
        .filter(|(_, track)| track.type_ != "index")
        .collect();

    // Phase 2: Collapse headings with their sub-tracks
    struct CollapsedTrack {
        title: String,
        position: String,
        /// Source tracklist indices for artist lookups.
        source_track_indices: Vec<usize>,
    }

    let flush_accumulated_sub_tracks =
        |collapsed: &mut Vec<CollapsedTrack>,
         current_heading: Option<&str>,
         sub_tracks: &mut Vec<String>,
         sub_track_indices: &mut Vec<usize>,
         heading_position: &mut Option<String>| {
            if sub_tracks.is_empty() {
                return;
            }

            let heading = current_heading.expect("sub-tracks are accumulated under a heading");
            let title = format!("{}: {}", heading, sub_tracks.join(" \u{2013} "));
            collapsed.push(CollapsedTrack {
                title,
                position: heading_position
                    .take()
                    .expect("sub-tracks have a base heading position"),
                source_track_indices: std::mem::take(sub_track_indices),
            });
            sub_tracks.clear();
        };

    let mut collapsed: Vec<CollapsedTrack> = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut sub_tracks: Vec<String> = Vec::new();
    let mut sub_track_indices: Vec<usize> = Vec::new();
    let mut heading_position: Option<String> = None;

    for &(source_track_index, entry) in &filtered {
        if entry.type_ == "heading" {
            flush_accumulated_sub_tracks(
                &mut collapsed,
                current_heading.as_deref(),
                &mut sub_tracks,
                &mut sub_track_indices,
                &mut heading_position,
            );

            if entry.title == "-" {
                // A heading with title "-" clears the current heading
                current_heading = None;
                heading_position = None;
            } else {
                current_heading = Some(entry.title.clone());
                heading_position = None;
            }
            continue;
        }

        // It's a regular track
        if current_heading.is_some() && is_sub_track_position(&entry.position) {
            // This is a sub-track under the current heading
            if heading_position.is_none() {
                heading_position = Some(extract_base_position(&entry.position));
            }
            sub_tracks.push(entry.title.clone());
            sub_track_indices.push(source_track_index);
        } else {
            flush_accumulated_sub_tracks(
                &mut collapsed,
                current_heading.as_deref(),
                &mut sub_tracks,
                &mut sub_track_indices,
                &mut heading_position,
            );
            current_heading = None;
            heading_position = None;

            collapsed.push(CollapsedTrack {
                title: entry.title.clone(),
                position: entry.position.clone(),
                source_track_indices: vec![source_track_index],
            });
        }
    }

    flush_accumulated_sub_tracks(
        &mut collapsed,
        current_heading.as_deref(),
        &mut sub_tracks,
        &mut sub_track_indices,
        &mut heading_position,
    );

    // Phase 3: Assign sides from the position format.
    collapsed
        .into_iter()
        .map(|ct| {
            let side = parse_side_from_position(&ct.position);
            ProcessedTrack {
                title: ct.title,
                position: ct.position,
                source_track_indices: ct.source_track_indices,
                side,
            }
        })
        .collect()
}

/// Check if a position string indicates a sub-track (e.g., "B1i", "B1ii", "B1iii").
/// Sub-tracks have a letter prefix, a number, then lowercase roman numeral or letter suffixes.
fn is_sub_track_position(position: &str) -> bool {
    let bytes = position.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    if !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    // Find where the digits end
    let mut digit_end = 1;
    while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
        digit_end += 1;
    }
    // Must have at least one digit, and something after the digits
    digit_end > 1 && digit_end < bytes.len()
}

/// Extract the base position from a sub-track position (e.g., "B1i" -> "B1").
fn extract_base_position(position: &str) -> String {
    let bytes = position.as_bytes();
    let mut end = 1;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    position[..end].to_string()
}

/// Parse the side number from a Discogs position string.
///
/// - Vinyl (A1, B2, C1...): each letter = side. A=1, B=2, C=3, D=4
/// - CD (1-1, 1-2, 2-1...): disc number = side
/// - Plain (1, 2, 3): side 1
pub fn parse_side_from_position(position: &str) -> i32 {
    // CD format: "1-1", "2-3"
    if let Some(dash_idx) = position.find('-') {
        if let Ok(disc) = position[..dash_idx].parse::<i32>() {
            return disc;
        }
    }

    // Vinyl format: letter prefix
    if let Some(first_char) = position.chars().next() {
        if first_char.is_ascii_alphabetic() {
            return (first_char.to_ascii_uppercase() as i32) - ('A' as i32) + 1;
        }
    }

    // Plain numbers: side 1
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discogs::models::{DiscogsArtist, DiscogsRoleArtist, DiscogsTrack};
    use coven::FixedClock;
    use coven::SequentialIdProvider;

    /// Run the mapper with deterministic fakes. Exercises the real
    /// `map_discogs_to_db`; only the clock/id inputs are faked.
    fn map(
        release: &DiscogsRelease,
        master_year: Option<u32>,
        mb_xref: Option<&MbReleaseResponse>,
    ) -> Result<ParsedAlbum, ImportError> {
        let clock = FixedClock(
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        let ids = SequentialIdProvider::new("d");
        map_discogs_to_db(release, master_year, mb_xref, &clock, &ids)
    }

    #[test]
    fn test_parse_side_from_position() {
        // CD format: disc number = side
        assert_eq!(parse_side_from_position("1-1"), 1);
        assert_eq!(parse_side_from_position("2-5"), 2);
        assert_eq!(parse_side_from_position("3-12"), 3);

        // Vinyl sides: each letter = one side
        assert_eq!(parse_side_from_position("A1"), 1);
        assert_eq!(parse_side_from_position("B3"), 2);
        assert_eq!(parse_side_from_position("C1"), 3);
        assert_eq!(parse_side_from_position("D2"), 4);

        // Plain numbers: side 1
        assert_eq!(parse_side_from_position("1"), 1);
        assert_eq!(parse_side_from_position("12"), 1);
    }

    fn make_track(position: &str, title: &str) -> DiscogsTrack {
        DiscogsTrack {
            position: position.to_string(),
            title: title.to_string(),
            duration: None,
            artists: vec![],
            type_: "track".to_string(),
            extraartists: None,
        }
    }

    fn make_heading(title: &str) -> DiscogsTrack {
        DiscogsTrack {
            position: String::new(),
            title: title.to_string(),
            duration: None,
            artists: vec![],
            type_: "heading".to_string(),
            extraartists: None,
        }
    }

    fn make_release(tracklist: Vec<DiscogsTrack>) -> DiscogsRelease {
        DiscogsRelease {
            id: "test-123".to_string(),
            title: "Album Title A".to_string(),
            year: Some(2024),
            format: vec![],
            country: None,
            label: vec![],
            cover_image: None,
            thumb: None,
            catno: None,
            artists: vec![DiscogsArtist {
                name: "Artist Name A".to_string(),
                id: "artist-1".to_string(),
            }],
            tracklist,
            extraartists: Some(vec![]),
            master_id: None,
        }
    }

    /// With no artists list, the release artist is derived by splitting the
    /// title on " - ". A title that yields no artist (no separator, or an empty
    /// artist half) leaves the release unattributed and errors.
    #[test]
    fn release_without_artists_errors_when_title_yields_no_artist() {
        // (title, why it yields no artist)
        for title in ["Album Title Without Artist", " - Album Title"] {
            let mut release = make_release(vec![]);
            release.artists = vec![];
            release.title = title.to_string();

            let err = map(&release, Some(2024), None)
                .expect_err(&format!("expected error for unattributed title {title:?}"));

            assert!(
                matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("has no release artist")),
                "unexpected error message for {title:?}: {err}"
            );
        }
    }

    /// With no artists list but a "Artist - Album" title, the release artist is
    /// taken from the artist half of the split.
    #[test]
    fn release_without_artists_derives_release_artist_from_title() {
        let mut release = make_release(vec![make_track("1", "Track 1")]);
        release.artists = vec![];
        release.title = "Artist Name A - Album Title".to_string();

        let parsed = map(&release, Some(2024), None).unwrap();

        assert_eq!(parsed.artists.len(), 1);
        assert_eq!(parsed.artists[0].name, "Artist Name A");
        assert_eq!(parsed.artists[0].discogs_artist_id, None);
    }

    #[test]
    fn extraartist_roles_import_as_role_credits_not_works_or_display_artists() {
        let mut release = make_release(vec![make_track("1", "Track Title 1")]);
        release.extraartists = Some(vec![
            DiscogsRoleArtist {
                id: Some("discogs-composer-a".to_string()),
                name: "Composer Name A".to_string(),
                role: "Composed By".to_string(),
                credited_name: Some("Composer Credit A".to_string()),
            },
            DiscogsRoleArtist {
                id: Some("discogs-producer-a".to_string()),
                name: "Producer Name A".to_string(),
                role: "Producer".to_string(),
                credited_name: Some("Producer Credit A".to_string()),
            },
        ]);
        release.tracklist[0].extraartists = Some(vec![
            DiscogsRoleArtist {
                id: Some("discogs-composer-a".to_string()),
                name: "Composer Name A".to_string(),
                role: "Written-By".to_string(),
                credited_name: Some("Composer Credit A".to_string()),
            },
            DiscogsRoleArtist {
                id: Some("discogs-engineer-a".to_string()),
                name: "Engineer Name A".to_string(),
                role: "Engineer".to_string(),
                credited_name: Some("Engineer Credit A".to_string()),
            },
        ]);

        let parsed = map(&release, Some(2024), None).unwrap();

        assert!(parsed.work_graph.works.is_empty());
        assert!(parsed.work_graph.work_artists.is_empty());
        assert!(parsed.work_graph.work_parts.is_empty());
        assert!(parsed.work_graph.track_works.is_empty());

        let composer = parsed
            .artists
            .iter()
            .find(|artist| artist.discogs_artist_id.as_deref() == Some("discogs-composer-a"))
            .expect("composer role artist imported");
        let release_artist_roles = &parsed.release_artist_roles;
        let track_artist_roles = &parsed.track_artist_roles;
        assert!(release_artist_roles.iter().any(|role| {
            role.artist_id == composer.id && role.source_credit.as_deref() == Some("Composed By")
        }));
        assert!(track_artist_roles.iter().any(|role| {
            role.artist_id == composer.id && role.source_credit.as_deref() == Some("Written-By")
        }));
        assert!(!parsed
            .artists
            .iter()
            .any(|artist| artist.discogs_artist_id.as_deref() == Some("discogs-producer-a")));
        assert!(!parsed
            .artists
            .iter()
            .any(|artist| artist.discogs_artist_id.as_deref() == Some("discogs-engineer-a")));
        assert!(!parsed
            .album_artists
            .iter()
            .any(|link| link.artist_id == composer.id));
        assert!(!parsed
            .track_artists
            .iter()
            .any(|link| link.artist_id == composer.id));
    }

    #[test]
    fn test_cd_multi_disc() {
        let release = make_release(vec![
            make_track("1-1", "Disc 1 Track 1"),
            make_track("1-2", "Disc 1 Track 2"),
            make_track("1-3", "Disc 1 Track 3"),
            make_track("2-1", "Disc 2 Track 1"),
            make_track("2-2", "Disc 2 Track 2"),
        ]);

        let parsed = map(&release, Some(2024), None).unwrap();
        let tracks = &parsed.tracks;
        assert_eq!(tracks.len(), 5);

        // Side 1 (disc 1) tracks
        assert_eq!(tracks[0].side, 1);
        assert_eq!(tracks[0].track_number, Some(1));
        assert_eq!(tracks[1].side, 1);
        assert_eq!(tracks[1].track_number, Some(2));
        assert_eq!(tracks[2].side, 1);
        assert_eq!(tracks[2].track_number, Some(3));

        // Side 2 (disc 2) tracks restart at 1
        assert_eq!(tracks[3].side, 2);
        assert_eq!(tracks[3].track_number, Some(1));
        assert_eq!(tracks[4].side, 2);
        assert_eq!(tracks[4].track_number, Some(2));
    }

    #[test]
    fn test_vinyl_sides() {
        let release = make_release(vec![
            make_track("A1", "Side A Track 1"),
            make_track("A2", "Side A Track 2"),
            make_track("B1", "Side B Track 1"),
            make_track("B2", "Side B Track 2"),
            make_track("C1", "Side C Track 1"),
            make_track("D1", "Side D Track 1"),
        ]);

        let parsed = map(&release, Some(2024), None).unwrap();
        let tracks = &parsed.tracks;
        assert_eq!(tracks.len(), 6);

        // A = side 1
        assert_eq!(tracks[0].side, 1);
        assert_eq!(tracks[0].track_number, Some(1));
        assert_eq!(tracks[1].side, 1);
        assert_eq!(tracks[1].track_number, Some(2));

        // B = side 2
        assert_eq!(tracks[2].side, 2);
        assert_eq!(tracks[2].track_number, Some(1));
        assert_eq!(tracks[3].side, 2);
        assert_eq!(tracks[3].track_number, Some(2));

        // C = side 3
        assert_eq!(tracks[4].side, 3);
        assert_eq!(tracks[4].track_number, Some(1));

        // D = side 4
        assert_eq!(tracks[5].side, 4);
        assert_eq!(tracks[5].track_number, Some(1));
    }

    #[test]
    /// 2LP vinyl with headings and sub-tracks: heading collapses sub-tracks
    /// into one track, each letter is its own side, track numbers per-side.
    fn test_2lp_vinyl_with_headings() {
        let release = make_release(vec![
            make_track("A1", "Track One"),
            make_track("A2", "Track Two"),
            make_heading("Medley"),
            DiscogsTrack {
                position: "B1i".to_string(),
                title: "Part I".to_string(),
                duration: None,
                artists: vec![],
                type_: "track".to_string(),
                extraartists: None,
            },
            DiscogsTrack {
                position: "B1ii".to_string(),
                title: "Part II".to_string(),
                duration: None,
                artists: vec![],
                type_: "track".to_string(),
                extraartists: None,
            },
            DiscogsTrack {
                position: "B1iii".to_string(),
                title: "Part III".to_string(),
                duration: None,
                artists: vec![],
                type_: "track".to_string(),
                extraartists: None,
            },
            make_heading("-"),
            make_track("B2", "Track Three"),
            make_track("C1", "Track Four"),
            make_track("C2", "Track Five"),
            make_track("C3", "Track Six"),
            make_track("D1", "Track Seven"),
            make_track("D2", "Track Eight"),
        ]);

        let parsed = map(&release, Some(2004), None).unwrap();
        let tracks = &parsed.tracks;

        // 9 tracks: 2 on A, 2 on B (heading collapsed + Track Three), 3 on C, 2 on D
        assert_eq!(tracks.len(), 9);

        // Side A (1)
        assert_eq!(tracks[0].side, 1);
        assert_eq!(tracks[0].track_number, Some(1));
        assert_eq!(tracks[0].title, "Track One");

        assert_eq!(tracks[1].side, 1);
        assert_eq!(tracks[1].track_number, Some(2));
        assert_eq!(tracks[1].title, "Track Two");

        // Side B (2) — heading collapsed sub-tracks into one
        assert_eq!(tracks[2].side, 2);
        assert_eq!(tracks[2].track_number, Some(1));
        assert_eq!(
            tracks[2].title,
            "Medley: Part I \u{2013} Part II \u{2013} Part III"
        );

        assert_eq!(tracks[3].side, 2);
        assert_eq!(tracks[3].track_number, Some(2));
        assert_eq!(tracks[3].title, "Track Three");

        // Side C (3)
        assert_eq!(tracks[4].side, 3);
        assert_eq!(tracks[4].track_number, Some(1));
        assert_eq!(tracks[4].title, "Track Four");

        assert_eq!(tracks[5].side, 3);
        assert_eq!(tracks[5].track_number, Some(2));
        assert_eq!(tracks[5].title, "Track Five");

        assert_eq!(tracks[6].side, 3);
        assert_eq!(tracks[6].track_number, Some(3));
        assert_eq!(tracks[6].title, "Track Six");

        // Side D (4)
        assert_eq!(tracks[7].side, 4);
        assert_eq!(tracks[7].track_number, Some(1));
        assert_eq!(tracks[7].title, "Track Seven");

        assert_eq!(tracks[8].side, 4);
        assert_eq!(tracks[8].track_number, Some(2));
        assert_eq!(tracks[8].title, "Track Eight");
    }

    #[test]
    fn test_single_disc() {
        let release = make_release(vec![
            make_track("1", "Track 1"),
            make_track("2", "Track 2"),
            make_track("3", "Track 3"),
        ]);

        let parsed = map(&release, Some(2024), None).unwrap();
        let tracks = &parsed.tracks;
        assert_eq!(tracks.len(), 3);

        assert_eq!(tracks[0].side, 1);
        assert_eq!(tracks[0].track_number, Some(1));
        assert_eq!(tracks[1].side, 1);
        assert_eq!(tracks[1].track_number, Some(2));
        assert_eq!(tracks[2].side, 1);
        assert_eq!(tracks[2].track_number, Some(3));
    }

    #[test]
    fn test_collapsed_sub_tracks_preserve_per_track_artists() {
        let sub_artist = DiscogsArtist {
            name: "Sub Artist".to_string(),
            id: "sub-artist-1".to_string(),
        };

        let release = make_release(vec![
            make_track("A1", "Normal Track"),
            make_heading("Suite"),
            DiscogsTrack {
                position: "B1i".to_string(),
                title: "Movement 1".to_string(),
                duration: None,
                artists: vec![sub_artist.clone()],
                type_: "track".to_string(),
                extraartists: None,
            },
            DiscogsTrack {
                position: "B1ii".to_string(),
                title: "Movement 2".to_string(),
                duration: None,
                artists: vec![sub_artist.clone()],
                type_: "track".to_string(),
                extraartists: None,
            },
        ]);

        let parsed = map(&release, Some(2024), None).unwrap();
        let tracks = &parsed.tracks;
        let artists = &parsed.artists;
        let track_artists = &parsed.track_artists;

        // 2 tracks: A1 and collapsed B1
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[1].title, "Suite: Movement 1 \u{2013} Movement 2");

        // Sub artist should be in the artists list
        assert!(
            artists.iter().any(|a| a.name == "Sub Artist"),
            "Sub artist not found in artists list"
        );

        // The collapsed track should have a track-artist link to "Sub Artist"
        let collapsed_track_id = &tracks[1].id;
        let linked_artists: Vec<_> = track_artists
            .iter()
            .filter(|ta| &ta.track_id == collapsed_track_id)
            .collect();
        assert_eq!(
            linked_artists.len(),
            1,
            "Expected exactly 1 track-artist link for collapsed track (deduped), got {}",
            linked_artists.len()
        );

        let linked_artist_id = &linked_artists[0].artist_id;
        let linked_artist = artists.iter().find(|a| &a.id == linked_artist_id).unwrap();
        assert_eq!(linked_artist.name, "Sub Artist");
    }

    // ── identities (parsed.identities) ─────────────────────────────────

    #[test]
    fn test_no_master_id_yields_no_identity_row() {
        // master_id is the group identifier; without one there's no group
        // to claim, and the parser emits zero identity rows. The release
        // still imports — it just contributes no group identity.
        let mut release = make_release(vec![make_track("1", "Track 1")]);
        release.master_id = None;

        let parsed = map(&release, None, None).unwrap();

        assert!(
            parsed.identities.is_empty(),
            "expected no identities when master_id is None, got {:?}",
            parsed.identities,
        );
    }

    #[test]
    fn test_master_id_yields_one_discogs_identity_row() {
        let mut release = make_release(vec![make_track("1", "Track 1")]);
        release.master_id = Some("d-master-42".to_string());

        let parsed = map(&release, None, None).unwrap();

        assert_eq!(parsed.identities.len(), 1);
        let identity = &parsed.identities[0];
        assert_eq!(identity.source, MetadataSource::Discogs);
        assert_eq!(identity.source_group_id, "d-master-42");
        assert_eq!(identity.source_release_id.as_deref(), Some("test-123"));
    }

    fn mb_xref_with_group(release_id: &str, group_id: &str) -> MbReleaseResponse {
        MbReleaseResponse {
            id: release_id.to_string(),
            title: "Album Title A".to_string(),
            date: None,
            country: None,
            barcode: None,
            artist_credit: vec![],
            release_group: Some(crate::musicbrainz::MbReleaseGroupRef {
                id: group_id.to_string(),
                first_release_date: None,
                relations: None,
            }),
            label_info: vec![],
            media: vec![],
            relations: vec![],
        }
    }

    #[test]
    fn test_master_id_with_mb_xref_yields_two_identity_rows() {
        // Cross-ref hit AND the linked MB release carries a release
        // group — two rows: Discogs + MB. Both Exact (release IDs
        // present).
        let mut release = make_release(vec![make_track("1", "Track 1")]);
        release.master_id = Some("d-master-99".to_string());
        let mb_xref = mb_xref_with_group("mb-rel-7", "mb-group-7");

        let parsed = map(&release, None, Some(&mb_xref)).unwrap();

        assert_eq!(parsed.identities.len(), 2);

        let discogs = &parsed.identities[0];
        assert_eq!(discogs.source, MetadataSource::Discogs);
        assert_eq!(discogs.source_group_id, "d-master-99");
        assert_eq!(discogs.source_release_id.as_deref(), Some("test-123"));

        let mb = &parsed.identities[1];
        assert_eq!(mb.source, MetadataSource::MusicBrainz);
        assert_eq!(mb.source_group_id, "mb-group-7");
        assert_eq!(mb.source_release_id.as_deref(), Some("mb-rel-7"));
    }

    #[test]
    fn mb_xref_without_release_group_returns_err() {
        let mut release = make_release(vec![make_track("1", "Track 1")]);
        release.master_id = Some("d-master-99".to_string());
        let mut mb_xref = mb_xref_with_group("mb-rel-7", "mb-group-7");
        mb_xref.release_group = None;

        let err = map(&release, None, Some(&mb_xref))
            .expect_err("expected missing MB release group to return an error");

        assert!(
            matches!(&err, ImportError::SourceData { detail, .. } if detail.contains("missing release_group")),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn test_no_master_id_with_mb_xref_yields_only_mb_identity_row() {
        // Edge case: Discogs release has no master_id (no Discogs
        // identity row), but MB has a back-link. Emit just the MB row
        // — the album is still attached via MB's group.
        let mut release = make_release(vec![make_track("1", "Track 1")]);
        release.master_id = None;
        let mb_xref = mb_xref_with_group("mb-rel-only", "mb-group-only");

        let parsed = map(&release, None, Some(&mb_xref)).unwrap();

        assert_eq!(parsed.identities.len(), 1);
        let mb = &parsed.identities[0];
        assert_eq!(mb.source, MetadataSource::MusicBrainz);
        assert_eq!(mb.source_group_id, "mb-group-only");
        assert_eq!(mb.source_release_id.as_deref(), Some("mb-rel-only"));
    }

    #[test]
    fn process_tracklist_empty_yields_no_tracks() {
        assert!(process_tracklist(&[]).is_empty());
    }

    #[test]
    fn process_tracklist_drops_headings_with_no_subtracks() {
        // Headings only contribute a track once sub-tracks accumulate under
        // them. Headings with nothing beneath (here, back-to-back) are dropped.
        let tracklist = vec![make_heading("Disc One"), make_heading("Disc Two")];
        assert!(process_tracklist(&tracklist).is_empty());
    }

    #[test]
    fn process_tracklist_filters_index_entries() {
        // Discogs "index" rows are structural, not playable tracks.
        let index = DiscogsTrack {
            position: String::new(),
            title: "Side A".to_string(),
            duration: None,
            artists: vec![],
            type_: "index".to_string(),
            extraartists: None,
        };
        let result = process_tracklist(&[index, make_track("A1", "Real Track")]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "Real Track");
    }

    #[test]
    fn discogs_role_is_composer_matches_composing_roles() {
        // Bracket-wrapped and case/separator variants all normalize before the
        // substring check; non-composing roles never match.
        for role in [
            "Composed By",
            "Written-By",
            "Written By",
            "Music By",
            "Composer",
            "[Composed By]",
            "[Written-By]",
            "composed_by",
        ] {
            assert!(
                discogs_role_is_composer(role),
                "expected {role:?} to be a composer role"
            );
        }
        for role in ["Producer", "Engineer", "Vocals", "Remix", ""] {
            assert!(
                !discogs_role_is_composer(role),
                "expected {role:?} not to be a composer role"
            );
        }
    }

    /// A composer role artist with no credited_name falls back to its canonical
    /// name and logs the fallback rather than dropping the credit silently.
    #[test]
    fn role_artist_without_credited_name_falls_back_to_canonical_name() {
        let mut release = make_release(vec![make_track("1", "Track 1")]);
        release.extraartists = Some(vec![DiscogsRoleArtist {
            id: Some("discogs-composer-a".to_string()),
            name: "Composer Canonical Name".to_string(),
            role: "Composed By".to_string(),
            credited_name: None,
        }]);

        let mut parsed = None;
        let logs = crate::test_logs::capture_warn_logs(|| {
            parsed = Some(map(&release, Some(2024), None).unwrap());
        });
        let parsed = parsed.unwrap();

        let composer = parsed
            .artists
            .iter()
            .find(|a| a.discogs_artist_id.as_deref() == Some("discogs-composer-a"))
            .expect("composer role artist imported");
        assert_eq!(composer.name, "Composer Canonical Name");
        assert!(
            logs.contains("has no credited name"),
            "expected a fallback warning, got: {logs}"
        );
    }

    /// An id-less composer role credit whose name case-insensitively matches
    /// the release artist (which carries a Discogs id) reuses that artist
    /// rather than minting a duplicate: the Discogs role-artist matcher falls
    /// back to a name match when the credit has no id, and that match is
    /// unrestricted by whether the existing artist bears a source id.
    #[test]
    fn id_less_role_credit_reuses_release_artist_by_name() {
        let mut release = make_release(vec![make_track("1", "Track 1")]);
        // Release artist: name "Artist Name A", discogs id "artist-1".
        release.extraartists = Some(vec![DiscogsRoleArtist {
            id: None,
            name: "ARTIST NAME A".to_string(),
            role: "Composed By".to_string(),
            credited_name: None,
        }]);

        let parsed = map(&release, Some(2024), None).unwrap();

        let matching: Vec<_> = parsed
            .artists
            .iter()
            .filter(|a| a.name.eq_ignore_ascii_case("Artist Name A"))
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "id-less role credit must reuse the release artist by name, not duplicate it"
        );
        let role = parsed
            .release_artist_roles
            .first()
            .expect("composer role imported");
        assert_eq!(role.artist_id, parsed.album.artist_id);
    }

    /// An id-less Discogs role credit dedups on its *credited* name — the name
    /// actually stored on the artist row. Two id-less composer credits sharing a
    /// credited name but with different canonical names collapse into one
    /// artist: the dedup key and the stored name are the same field.
    #[test]
    fn id_less_role_credits_dedup_on_credited_name() {
        let mut release = make_release(vec![make_track("1", "Track 1")]);
        release.extraartists = Some(vec![
            DiscogsRoleArtist {
                id: None,
                name: "Canonical One".to_string(),
                role: "Composed By".to_string(),
                credited_name: Some("Shared Credited Name".to_string()),
            },
            DiscogsRoleArtist {
                id: None,
                name: "Canonical Two".to_string(),
                role: "Written-By".to_string(),
                credited_name: Some("Shared Credited Name".to_string()),
            },
        ]);

        let parsed = map(&release, Some(2024), None).unwrap();

        let matching: Vec<_> = parsed
            .artists
            .iter()
            .filter(|a| a.name == "Shared Credited Name")
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "id-less role credits with the same credited name dedup into one artist"
        );
        // The stored name is the credited name; neither canonical name lands.
        assert!(!parsed
            .artists
            .iter()
            .any(|a| a.name == "Canonical One" || a.name == "Canonical Two"));
        // Both release roles point at the single deduped artist.
        assert_eq!(
            parsed
                .release_artist_roles
                .iter()
                .filter(|r| r.artist_id == matching[0].id)
                .count(),
            2
        );
    }
}
