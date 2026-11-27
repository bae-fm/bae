//! The values a candidate's draft holds beside its own row, and the
//! statements that write and read them: its form's columns, its label and
//! barcode rows, its album and track artist assignments, its mediums, and
//! the roles and works of its release and its tracks.

use crate::tables::draft_rows::decoded;
use crate::{ArtistAssignment, Credited, RawLabelEdit, RawReleaseEdit};
use bae_catalogs::catalog::Catalog;
use bae_catalogs::credits::{ArtistCredit, CreditedRole};
use bae_catalogs::works::{GraphWork, WorkCredit, WorkGraph, WorkPart};
use bae_store::DbError;
use bae_store::WriteStatements as _;
use bae_types::credit_role::CreditedAs;
use coven::CovenWrite;
use std::str::FromStr;

/// The form's album-level fields, as the columns of a candidate's edit row
/// hold them.
#[derive(PartialEq)]
pub(super) struct FormColumns {
    pub(super) album_title: String,
    pub(super) album_year: String,
    pub(super) year: String,
    pub(super) country: Option<&'static str>,
    pub(super) region: Option<&'static str>,
    pub(super) media: String,
    pub(super) status: Option<&'static str>,
    pub(super) packaging: Option<&'static str>,
    pub(super) discogs_details: String,
}

impl FormColumns {
    pub(super) fn of(form: &RawReleaseEdit) -> Self {
        let facts = bae_records::records::pressing_columns::FactColumns::of(&form.pressing.facts);
        Self {
            album_title: form.album_title.clone(),
            album_year: form.album_year.clone(),
            year: form.pressing.year.clone(),
            country: facts.country,
            region: facts.region,
            media: bae_records::records::pressing_columns::media_column(&form.pressing.facts.media),
            status: facts.status,
            packaging: facts.packaging,
            discogs_details: facts.discogs_details,
        }
    }
}

sqlive_compose::write!(
    InsertDraftLabel(
        content_hash: &'p str,
        position: i64,
        name: &'p str,
        catalog_number: &'p str,
        musicbrainz_label_id: Option<&'p str>,
        discogs_label_id: Option<&'p str>,
    ),
    r#"
    INSERT INTO import_candidate_edit_label
        (content_hash, position, name, catalog_number, musicbrainz_label_id, discogs_label_id)
    VALUES (:content_hash, :position, :name, :catalog_number, :musicbrainz_label_id,
        :discogs_label_id)
"#
);

sqlive_compose::write!(
    InsertDraftBarcode(content_hash: &'p str, position: i64, barcode: &'p str),
    r#"
    INSERT INTO import_candidate_edit_barcode
        (content_hash, position, barcode)
    VALUES (:content_hash, :position, :barcode)
"#
);

sqlive_compose::write!(
    InsertDraftMedium(
        content_hash: &'p str,
        position: std::num::NonZeroU16,
        medium: Option<&'static str>,
        side_count: std::num::NonZeroU8,
    ),
    r#"
    INSERT INTO import_candidate_medium
        (content_hash, position, medium, side_count)
    VALUES (:content_hash, :position, :medium, :side_count)
"#
);

sqlive_compose::write!(
    DeleteAlbumArtistsFrom(content_hash: &'p str, position: i64),
    r#"
    DELETE FROM import_candidate_album_artist_assignment
    WHERE content_hash = :content_hash AND position >= :position
"#
);

sqlive_compose::write!(
    UpsertAlbumArtist(
        content_hash: &'p str,
        position: i64,
        assignment_kind: &'static str,
        artist_id: Option<&'p str>,
        name: Option<&'p str>,
        sort_name: Option<&'p str>,
        musicbrainz_artist_id: Option<&'p str>,
        discogs_artist_id: Option<&'p str>,
        credit_key: Option<&'p str>,
        credited_as: Option<&'p str>,
        join_phrase: Option<&'p str>,
    ),
    r#"
    INSERT INTO import_candidate_album_artist_assignment
        (content_hash, position, assignment_kind, artist_id, name, sort_name,
        musicbrainz_artist_id, discogs_artist_id, credit_key, credited_as, join_phrase)
    VALUES (:content_hash, :position, :assignment_kind, :artist_id, :name, :sort_name,
        :musicbrainz_artist_id, :discogs_artist_id, :credit_key, :credited_as, :join_phrase)
    ON CONFLICT (content_hash, position) DO UPDATE SET
        assignment_kind = excluded.assignment_kind, artist_id = excluded.artist_id, name =
        excluded.name, sort_name = excluded.sort_name, musicbrainz_artist_id =
        excluded.musicbrainz_artist_id, discogs_artist_id = excluded.discogs_artist_id,
        credit_key = excluded.credit_key, credited_as = excluded.credited_as, join_phrase =
        excluded.join_phrase
"#
);

sqlive_compose::write!(
    InsertTrackArtist(
        content_hash: &'p str,
        track_id: &'p str,
        position: i64,
        assignment_kind: &'static str,
        artist_id: Option<&'p str>,
        name: Option<&'p str>,
        sort_name: Option<&'p str>,
        musicbrainz_artist_id: Option<&'p str>,
        discogs_artist_id: Option<&'p str>,
        credit_key: Option<&'p str>,
        credited_as: Option<&'p str>,
        join_phrase: Option<&'p str>,
    ),
    r#"
    INSERT INTO import_candidate_track_artist_assignment
        (content_hash, track_id, position, assignment_kind, artist_id, name, sort_name,
        musicbrainz_artist_id, discogs_artist_id, credit_key, credited_as, join_phrase)
    VALUES (:content_hash, :track_id, :position, :assignment_kind, :artist_id, :name,
        :sort_name, :musicbrainz_artist_id, :discogs_artist_id, :credit_key, :credited_as,
        :join_phrase)
"#
);

sqlive_compose::write!(
    InsertDraftWork(
        content_hash: &'p str,
        musicbrainz_work_id: &'p str,
        position: i64,
        title: &'p str,
        disambiguation: Option<&'p str>,
        work_type: Option<&'p str>,
    ),
    r#"
    INSERT INTO import_candidate_work
        (content_hash, musicbrainz_work_id, position, title, disambiguation, work_type)
    VALUES (:content_hash, :musicbrainz_work_id, :position, :title, :disambiguation, :work_type)
"#
);

sqlive_compose::write!(
    InsertWorkCredit(
        content_hash: &'p str,
        musicbrainz_work_id: &'p str,
        position: i64,
        name: &'p str,
        sort_name: Option<&'p str>,
        musicbrainz_artist_id: Option<&'p str>,
        discogs_artist_id: Option<&'p str>,
        credited_as: Option<&'p str>,
        role: &'static str,
        role_words: Option<String>,
        qualifiers: String,
    ),
    r#"
    INSERT INTO import_candidate_work_credit
        (content_hash, musicbrainz_work_id, position, name, sort_name, musicbrainz_artist_id,
        discogs_artist_id, credited_as, role, role_words, qualifiers)
    VALUES (:content_hash, :musicbrainz_work_id, :position, :name, :sort_name,
        :musicbrainz_artist_id, :discogs_artist_id, :credited_as, :role, :role_words,
        :qualifiers)
"#
);

sqlive_compose::write!(
    InsertWorkPart(
        content_hash: &'p str,
        position: i64,
        parent_work_id: &'p str,
        child_work_id: &'p str,
    ),
    r#"
    INSERT INTO import_candidate_work_part
        (content_hash, position, parent_work_id, child_work_id)
    VALUES (:content_hash, :position, :parent_work_id, :child_work_id)
"#
);

sqlive_compose::write!(
    InsertReleaseRole(
        content_hash: &'p str,
        position: i64,
        name: &'p str,
        sort_name: Option<&'p str>,
        musicbrainz_artist_id: Option<&'p str>,
        discogs_artist_id: Option<&'p str>,
        credited_as: Option<&'p str>,
        source: &'p str,
        role: &'static str,
        role_words: Option<String>,
        qualifiers: String,
    ),
    r#"
    INSERT INTO import_candidate_release_role
        (content_hash, position, name, sort_name, musicbrainz_artist_id, discogs_artist_id,
        credited_as, source, role, role_words, qualifiers)
    VALUES (:content_hash, :position, :name, :sort_name, :musicbrainz_artist_id,
        :discogs_artist_id, :credited_as, :source, :role, :role_words, :qualifiers)
"#
);

sqlive_compose::write!(
    InsertTrackRole(
        content_hash: &'p str,
        track_id: &'p str,
        position: i64,
        name: &'p str,
        sort_name: Option<&'p str>,
        musicbrainz_artist_id: Option<&'p str>,
        discogs_artist_id: Option<&'p str>,
        credited_as: Option<&'p str>,
        source: &'p str,
        role: &'static str,
        role_words: Option<String>,
        qualifiers: String,
    ),
    r#"
    INSERT INTO import_candidate_track_role
        (content_hash, track_id, position, name, sort_name, musicbrainz_artist_id,
        discogs_artist_id, credited_as, source, role, role_words, qualifiers)
    VALUES (:content_hash, :track_id, :position, :name, :sort_name, :musicbrainz_artist_id,
        :discogs_artist_id, :credited_as, :source, :role, :role_words, :qualifiers)
"#
);

sqlive_compose::write!(
    InsertTrackWork(
        content_hash: &'p str,
        track_id: &'p str,
        position: i64,
        musicbrainz_work_id: &'p str,
    ),
    r#"
    INSERT INTO import_candidate_track_work
        (content_hash, track_id, position, musicbrainz_work_id)
    VALUES (:content_hash, :track_id, :position, :musicbrainz_work_id)
"#
);

/// Insert the draft's label rows, as the form holds them, where the draft
/// holds none.
pub(crate) fn insert_label_rows(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    labels: &[RawLabelEdit],
) -> Result<(), DbError> {
    for (position, label) in labels.iter().enumerate() {
        sql.run(&InsertDraftLabelParams {
            content_hash,
            position: position as i64,
            name: &label.name,
            catalog_number: &label.catalog_number,
            musicbrainz_label_id: label.musicbrainz_label_id.as_deref(),
            discogs_label_id: label.discogs_label_id.as_deref(),
        })?;
    }
    Ok(())
}

/// Insert the draft's barcode rows, as the form holds them, where the draft
/// holds none.
pub(crate) fn insert_barcode_rows(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    barcodes: &[String],
) -> Result<(), DbError> {
    for (position, barcode) in barcodes.iter().enumerate() {
        sql.run(&InsertDraftBarcodeParams { content_hash, position: position as i64, barcode })?;
    }
    Ok(())
}

/// Insert the draft's mediums, in order, where the draft holds none.
pub(crate) fn insert_medium_rows(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    mediums: &[bae_types::mediums::ReleaseMedium],
) -> Result<(), DbError> {
    for (index, medium) in mediums.iter().enumerate() {
        sql.run(&InsertDraftMediumParams {
            content_hash,
            position: bae_types::mediums::medium_position(index),
            medium: medium.carrier.map(bae_types::pressing::Medium::key),
            side_count: medium.sides,
        })?;
    }
    Ok(())
}

pub(crate) fn insert_album_artist_assignments(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        store_album_artist_assignment(sql, content_hash, position, assignment)?;
    }
    Ok(())
}

/// Rewrite in place each album artist that differs from the one stored at
/// its position, and delete the stored ones past the last.
pub(crate) fn store_album_artist_assignments(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    stored: &[Credited],
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        if stored.get(position) != Some(assignment) {
            store_album_artist_assignment(sql, content_hash, position, assignment)?;
        }
    }
    if stored.len() > assignments.len() {
        sql.run(&DeleteAlbumArtistsFromParams {
            content_hash,
            position: assignments.len() as i64,
        })?;
    }
    Ok(())
}

/// One artist assignment as a candidate's assignment table stores it: a
/// picked artist by its id and the credit name the person picked, or a
/// credit by what it says.
struct AssignmentRow<'a> {
    assignment_kind: &'static str,
    artist_id: Option<&'a str>,
    name: Option<&'a str>,
    sort_name: Option<&'a str>,
    musicbrainz_artist_id: Option<&'a str>,
    discogs_artist_id: Option<&'a str>,
    credit_key: Option<&'a str>,
    credited_as: Option<&'a str>,
    join_phrase: Option<&'a str>,
}

impl<'a> AssignmentRow<'a> {
    fn of(credited: &'a Credited) -> Self {
        let join_phrase = credited.join.as_deref();
        match &credited.artist {
            ArtistAssignment::Picked { artist, credit_name } => Self {
                assignment_kind: "picked",
                artist_id: Some(&artist.artist_id),
                name: None,
                sort_name: None,
                musicbrainz_artist_id: None,
                discogs_artist_id: None,
                credit_key: credit_name.as_ref().map(|credit_name| credit_name.key.as_str()),
                credited_as: None,
                join_phrase,
            },
            ArtistAssignment::Credit { credit } => Self {
                assignment_kind: "credit",
                artist_id: None,
                name: Some(credit.name.as_str()),
                sort_name: credit.sort_name.as_deref(),
                musicbrainz_artist_id: credit.musicbrainz_artist_id.as_deref(),
                discogs_artist_id: credit.discogs_artist_id.as_deref(),
                credit_key: None,
                credited_as: credit.credited_as.as_deref(),
                join_phrase,
            },
        }
    }
}

/// Make the album artist at `position` `assignment`, in place when one is
/// there.
fn store_album_artist_assignment(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    position: usize,
    assignment: &Credited,
) -> Result<(), DbError> {
    let row = AssignmentRow::of(assignment);
    sql.run(&UpsertAlbumArtistParams {
        content_hash,
        position: position as i64,
        assignment_kind: row.assignment_kind,
        artist_id: row.artist_id,
        name: row.name,
        sort_name: row.sort_name,
        musicbrainz_artist_id: row.musicbrainz_artist_id,
        discogs_artist_id: row.discogs_artist_id,
        credit_key: row.credit_key,
        credited_as: row.credited_as,
        join_phrase: row.join_phrase,
    })?;
    Ok(())
}

pub(crate) fn insert_track_artist_assignments(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    track_id: &str,
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        let row = AssignmentRow::of(assignment);
        sql.run(&InsertTrackArtistParams {
            content_hash,
            track_id,
            position: position as i64,
            assignment_kind: row.assignment_kind,
            artist_id: row.artist_id,
            name: row.name,
            sort_name: row.sort_name,
            musicbrainz_artist_id: row.musicbrainz_artist_id,
            discogs_artist_id: row.discogs_artist_id,
            credit_key: row.credit_key,
            credited_as: row.credited_as,
            join_phrase: row.join_phrase,
        })?;
    }
    Ok(())
}

/// Write a draft's works and the roles on its release, under
/// `key`. The caller has already written the candidate's edit row.
pub(crate) fn insert_draft_values(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    works: &WorkGraph,
    release_roles: &[CreditedRole],
) -> Result<(), DbError> {
    for (position, work) in works.works.iter().enumerate() {
        sql.run(&InsertDraftWorkParams {
            content_hash,
            musicbrainz_work_id: &work.musicbrainz_work_id,
            position: position as i64,
            title: &work.title,
            disambiguation: work.disambiguation.as_deref(),
            work_type: work.work_type.as_deref(),
        })?;
        for (position, credit) in work.credits.iter().enumerate() {
            let role = credit.role.columns();
            sql.run(&InsertWorkCreditParams {
                content_hash,
                musicbrainz_work_id: &work.musicbrainz_work_id,
                position: position as i64,
                name: &credit.artist.name,
                sort_name: credit.artist.sort_name.as_deref(),
                musicbrainz_artist_id: credit.artist.musicbrainz_artist_id.as_deref(),
                discogs_artist_id: credit.artist.discogs_artist_id.as_deref(),
                credited_as: credit.artist.credited_as.as_deref(),
                role: role.role,
                role_words: role.role_words,
                qualifiers: role.qualifiers,
            })?;
        }
    }
    for (position, part) in works.parts.iter().enumerate() {
        sql.run(&InsertWorkPartParams {
            content_hash,
            position: position as i64,
            parent_work_id: &part.parent,
            child_work_id: &part.child,
        })?;
    }
    for (position, role) in release_roles.iter().enumerate() {
        let columns = role.role.columns();
        sql.run(&InsertReleaseRoleParams {
            content_hash,
            position: position as i64,
            name: &role.artist.name,
            sort_name: role.artist.sort_name.as_deref(),
            musicbrainz_artist_id: role.artist.musicbrainz_artist_id.as_deref(),
            discogs_artist_id: role.artist.discogs_artist_id.as_deref(),
            credited_as: role.artist.credited_as.as_deref(),
            source: role.source.as_str(),
            role: columns.role,
            role_words: columns.role_words,
            qualifiers: columns.qualifiers,
        })?;
    }
    Ok(())
}

/// Write the rows keyed by the draft track `track_id`: the roles the source
/// credits on it and the works it performs, under `key`. The
/// caller has already written the track row and the works.
pub(crate) fn insert_track_values(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    track_id: &str,
    roles: &[CreditedRole],
    works: &[String],
) -> Result<(), DbError> {
    for (position, role) in roles.iter().enumerate() {
        let columns = role.role.columns();
        sql.run(&InsertTrackRoleParams {
            content_hash,
            track_id,
            position: position as i64,
            name: &role.artist.name,
            sort_name: role.artist.sort_name.as_deref(),
            musicbrainz_artist_id: role.artist.musicbrainz_artist_id.as_deref(),
            discogs_artist_id: role.artist.discogs_artist_id.as_deref(),
            credited_as: role.artist.credited_as.as_deref(),
            source: role.source.as_str(),
            role: columns.role,
            role_words: columns.role_words,
            qualifiers: columns.qualifiers,
        })?;
    }
    for (position, work) in works.iter().enumerate() {
        sql.run(&InsertTrackWorkParams {
            content_hash,
            track_id,
            position: position as i64,
            musicbrainz_work_id: work,
        })?;
    }
    Ok(())
}

/// One medium as the draft's stored mediums hold it.
#[derive(serde::Deserialize)]
struct StoredMedium {
    carrier: Option<String>,
    sides: std::num::NonZeroU8,
}

/// The mediums a JSON array of the draft's stored mediums holds, in order.
pub(crate) fn mediums_of(json: &str) -> Result<Vec<bae_types::mediums::ReleaseMedium>, DbError> {
    decoded::<Vec<StoredMedium>>(json, "mediums")?
        .into_iter()
        .map(|medium| {
            let carrier = crate::tables::medium_of("a draft medium's carrier", medium.carrier)?;
            Ok(bae_types::mediums::ReleaseMedium { carrier, sides: medium.sides })
        })
        .collect()
}

/// One stored work as the draft's stored works holds it.
#[derive(serde::Deserialize)]
pub(crate) struct StoredWork {
    musicbrainz_work_id: String,
    title: String,
    disambiguation: Option<String>,
    work_type: Option<String>,
    credits: Vec<StoredWorkCredit>,
}

/// One stored work credit as the draft's stored works holds it.
#[derive(serde::Deserialize)]
pub(crate) struct StoredWorkCredit {
    name: String,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
    credited_as: Option<String>,
    role: String,
    role_words: Option<String>,
    qualifiers: String,
}

/// One stored part as the draft's stored work parts hold it.
#[derive(serde::Deserialize)]
pub(crate) struct StoredPart {
    parent: String,
    child: String,
}

/// The work graph the draft's stored works and stored work parts hold.
pub(crate) fn works_of(
    works: Vec<StoredWork>,
    parts: Vec<StoredPart>,
) -> Result<WorkGraph, DbError> {
    Ok(WorkGraph {
        works: works
            .into_iter()
            .map(|work| {
                Ok(GraphWork {
                    musicbrainz_work_id: work.musicbrainz_work_id,
                    title: work.title,
                    disambiguation: work.disambiguation,
                    work_type: work.work_type,
                    credits: work
                        .credits
                        .into_iter()
                        .map(|credit| {
                            Ok(WorkCredit {
                                artist: ArtistCredit {
                                    name: credit.name,
                                    sort_name: credit.sort_name,
                                    musicbrainz_artist_id: credit.musicbrainz_artist_id,
                                    discogs_artist_id: credit.discogs_artist_id,
                                    credited_as: credit.credited_as,
                                },
                                role: CreditedAs::from_columns(
                                    &credit.role,
                                    credit.role_words,
                                    &credit.qualifiers,
                                )
                                .map_err(|error| {
                                    DbError::unreadable("a draft work credit's role", error)
                                })?,
                            })
                        })
                        .collect::<Result<_, DbError>>()?,
                })
            })
            .collect::<Result<_, DbError>>()?,
        parts: parts
            .into_iter()
            .map(|part| WorkPart { parent: part.parent, child: part.child })
            .collect(),
    })
}

/// One stored role as [`release_roles_json_sql`] and [`track_roles_json_sql`] hold
/// it, its source still the text [`roles_of`] reads.
#[derive(serde::Deserialize)]
pub(crate) struct StoredRole {
    name: String,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
    credited_as: Option<String>,
    source: String,
    role: String,
    role_words: Option<String>,
    qualifiers: String,
}

/// The credited roles a roles column holds.
pub(crate) fn roles_of(roles: Vec<StoredRole>) -> Result<Vec<CreditedRole>, DbError> {
    roles
        .into_iter()
        .map(|stored| {
            Ok(CreditedRole {
                artist: ArtistCredit {
                    name: stored.name,
                    sort_name: stored.sort_name,
                    musicbrainz_artist_id: stored.musicbrainz_artist_id,
                    discogs_artist_id: stored.discogs_artist_id,
                    credited_as: stored.credited_as,
                },
                source: Catalog::from_str(&stored.source).map_err(|_unknown| {
                    DbError::unreadable("a draft role's stored source", &stored.source)
                })?,
                role: CreditedAs::from_columns(&stored.role, stored.role_words, &stored.qualifiers)
                    .map_err(|error| DbError::unreadable("a draft role's stored role", error))?,
            })
        })
        .collect()
}
