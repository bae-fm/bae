//! The one place an import's library rows are built: from the candidate's
//! draft, as it stands.
//!
//! The draft holds every value the import writes — the album, the pressing,
//! each track's title, credits, place, roles and works, and the release's own
//! roles — so the rows are built from it alone; no source is read again.
//! Artists stay credits here: each credit becomes a row of its own, and the
//! commit resolves every one to a library artist inside the transaction that
//! writes the release, where two credits naming one artist become one.

use crate::db::{
    DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbReleaseArtistRole, DbTrackArtist,
    DbTrackArtistRole, DbTrackWork, DbWork, DbWorkArtist, DbWorkPart,
};
use crate::import::{
    ArtistAssignment, AudioFile, CandidateDraft, Catalog, ImportError, MetadataProvenance,
    ReleaseRows, WorkGraphRows, TrackArtistAssignments,
};
use chrono::{DateTime, Utc};
use coven::{Clock, IdSource};
use std::collections::{HashMap, HashSet};

/// The rows one import writes, before its artists are resolved and its
/// tracks bound to their audio.
pub(crate) struct AssembledRelease {
    pub parsed: ReleaseRows,
    /// Each track's audio, in the order of `parsed.tracks`.
    pub bindings: Vec<AudioFile>,
    /// The artist rows that are library artists a person picked; the commit
    /// links them as they are.
    pub picked: HashSet<String>,
}

/// The rows `draft` makes. The draft's form must shape: a titled album with an
/// artist, and a year that reads as one. Works are MusicBrainz's, so their
/// rows state it as their source. An album credited to the various-artists
/// artist alone is a compilation.
pub(crate) fn assemble_release(
    draft: &CandidateDraft,
    provenance: Option<&MetadataProvenance>,
    clock: &dyn Clock,
    ids: &dyn IdSource,
) -> Result<AssembledRelease, ImportError> {
    let edit = draft.release_edit().shape()?;
    let now = DateTime::<Utc>::from(clock.now());
    let mut artists = ArtistRows::new(ids, now);

    let album_artist_ids: Vec<String> = edit
        .album_artist_assignments
        .iter()
        .map(|assignment| artists.assigned(assignment))
        .collect();
    if album_artist_ids.is_empty() {
        return Err(crate::import::EditValidationError::NoAlbumArtist.into());
    }
    let album = DbAlbum {
        id: ids.new_id().to_string(),
        title: edit.album_title.clone(),
        year: edit.album_year,
        is_compilation: ArtistAssignment::credit_a_compilation(&edit.album_artist_assignments),
        created_at: now,
    };
    let album_artists = album_artist_ids
        .iter()
        .zip(0..)
        .map(|(artist_id, position)| DbAlbumArtist::new(&album.id, artist_id, position, now))
        .collect();

    let release = DbRelease {
        id: ids.new_id().to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: edit.pressing.clone(),
        draft_from_tags: matches!(provenance, Some(MetadataProvenance::FileMetadata)),
        // The folder's name and hash are the candidate's, and the loudness is
        // measured from the audio; `run_import` stamps them on.
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };

    let release_artist_roles = draft
        .release_roles
        .iter()
        .enumerate()
        .map(|(position, role)| {
            DbReleaseArtistRole::new(
                &release.id,
                &artists.credited(&role.artist),
                position as i32,
                role.source,
                role.role.clone(),
                ids.new_id().to_string(),
                now,
            )
        })
        .collect();

    let works = work_rows(draft, &mut artists, now);

    let mediums: Vec<crate::db::DbReleaseMedium> = draft
        .mediums
        .iter()
        .enumerate()
        .map(|(index, medium)| {
            crate::db::DbReleaseMedium::new(
                &release.id,
                crate::mediums::medium_position(index),
                *medium,
                now,
            )
        })
        .collect();

    let mut tracks = Vec::with_capacity(draft.tracks.len());
    let mut bindings = Vec::with_capacity(draft.tracks.len());
    let mut track_artists = Vec::new();
    let mut track_artist_roles = Vec::new();
    let mut track_works = Vec::new();
    for (track, shaped) in draft.tracks.iter().zip(&edit.tracks) {
        let row = crate::import::TrackRow {
            id: ids.new_id().to_string(),
            release_id: release.id.clone(),
            title: shaped.title.clone(),
            medium_id: mediums
                .get(track.place.medium_index())
                .ok_or_else(|| ImportError::Internal {
                    detail: format!(
                        "draft track {} sits on medium {}, which the draft does not hold",
                        track.edit.id, track.place.medium
                    ),
                })?
                .id
                .clone(),
            side: track.place.side,
            track_number: track.place.number,
            created_at: now,
        };
        if let TrackArtistAssignments::Explicit(assignments) = &shaped.artist_assignments {
            for (assignment, position) in assignments.iter().zip(0..) {
                track_artists.push(DbTrackArtist::new(
                    &row.id,
                    &artists.assigned(assignment),
                    position,
                    now,
                ));
            }
        }
        for (position, role) in track.roles.iter().enumerate() {
            track_artist_roles.push(DbTrackArtistRole::new(
                &row.id,
                &artists.credited(&role.artist),
                position as i32,
                role.source,
                role.role.clone(),
                ids.new_id().to_string(),
                now,
            ));
        }
        for (position, work) in track.works.iter().enumerate() {
            let work_id = works.ids.get(work).ok_or_else(|| ImportError::Internal {
                detail: format!(
                    "draft track {} performs work {work}, which the draft does not hold",
                    track.edit.id
                ),
            })?;
            track_works.push(DbTrackWork::new(
                &row.id,
                work_id,
                position as i32,
                Catalog::MusicBrainz,
                ids.new_id().to_string(),
                now,
            ));
        }
        tracks.push(row);
        bindings.push(track.edit.file.clone());
    }

    let ArtistRows {
        rows: artist_rows,
        picked,
        ..
    } = artists;
    Ok(AssembledRelease {
        parsed: ReleaseRows {
            album,
            release,
            mediums,
            tracks,
            artists: artist_rows,
            album_artists,
            track_artists,
            work_graph: WorkGraphRows {
                works: works.works,
                work_artists: works.work_artists,
                work_parts: works.work_parts,
                track_works,
            },
            release_artist_roles,
            track_artist_roles,
        },
        bindings,
        picked,
    })
}

/// The artist rows one import's links name: one per credit, and one per
/// picked library artist however often it is picked.
struct ArtistRows<'a> {
    rows: Vec<DbArtist>,
    picked: HashSet<String>,
    ids: &'a dyn IdSource,
    now: DateTime<Utc>,
}

impl<'a> ArtistRows<'a> {
    fn new(ids: &'a dyn IdSource, now: DateTime<Utc>) -> Self {
        Self {
            rows: Vec::new(),
            picked: HashSet::new(),
            ids,
            now,
        }
    }

    /// The row an album or track artist assignment links.
    fn assigned(&mut self, assignment: &ArtistAssignment) -> String {
        let row = assignment.write_row(self.ids, self.now);
        let id = row.id.clone();
        if matches!(assignment, ArtistAssignment::Picked { .. }) && !self.picked.insert(id.clone())
        {
            return id;
        }
        self.rows.push(row);
        id
    }

    /// The row a role or a composer credit links.
    fn credited(&mut self, credit: &crate::import::ArtistCredit) -> String {
        let row = credit.write_row(self.ids, self.now);
        let id = row.id.clone();
        self.rows.push(row);
        id
    }
}

/// The rows of the draft's works, and the row id of each by its MusicBrainz
/// id.
struct WorkRows {
    works: Vec<DbWork>,
    work_artists: Vec<DbWorkArtist>,
    work_parts: Vec<DbWorkPart>,
    ids: HashMap<String, String>,
}

/// Each work's composers are numbered in the work's order, and each part in
/// its parent's: one composer credit or one part of one work is the same row
/// on every device that imports it.
fn work_rows(draft: &CandidateDraft, artists: &mut ArtistRows<'_>, now: DateTime<Utc>) -> WorkRows {
    let mut rows = WorkRows {
        works: Vec::with_capacity(draft.works.works.len()),
        work_artists: Vec::new(),
        work_parts: Vec::new(),
        ids: HashMap::new(),
    };
    for work in &draft.works.works {
        let work_id = crate::db::identity::work_id(&work.musicbrainz_work_id);
        rows.ids
            .insert(work.musicbrainz_work_id.clone(), work_id.clone());
        for (position, composer) in work.composers.iter().enumerate() {
            rows.work_artists.push(DbWorkArtist::new(
                &work_id,
                &artists.credited(composer),
                position as i32,
                Catalog::MusicBrainz,
                now,
            ));
        }
        rows.works.push(DbWork {
            id: work_id,
            title: work.title.clone(),
            disambiguation: work.disambiguation.clone(),
            work_type: work.work_type.clone(),
            musicbrainz_work_id: work.musicbrainz_work_id.clone(),
            created_at: now,
        });
    }
    let mut parts_of: HashMap<&str, i32> = HashMap::new();
    for part in &draft.works.parts {
        let position = parts_of.entry(part.parent.as_str()).or_default();
        rows.work_parts.push(DbWorkPart::new(
            &crate::db::identity::work_id(&part.parent),
            &crate::db::identity::work_id(&part.child),
            *position,
            Catalog::MusicBrainz,
            now,
        ));
        *position += 1;
    }
    rows
}

#[cfg(test)]
#[path = "assemble_tests.rs"]
mod tests;
