//! The rows of works and of the roles artists are credited in: works, their
//! parts and credits, the works tracks perform, and release and track roles.

use super::*;
use bae_store::WriteStatements as _;
use bae_util::timestamp::StoredTimestamp as _;

coven::write!(
    KeepWork(
        id: &'p str,
        title: &'p str,
        disambiguation: Option<&'p str>,
        work_type: Option<&'p str>,
        musicbrainz_work_id: &'p str,
        created_at: String,
    ),
    r#"
    INSERT OR IGNORE INTO works (
        id, title, disambiguation, work_type, musicbrainz_work_id, created_at
    ) VALUES (:id, :title, :disambiguation, :work_type, :musicbrainz_work_id, :created_at)
"#
);

pub fn insert_work_row(conn: &WriteContext<'_>, work: &DbWork) -> Result<(), DbError> {
    conn.run(&KeepWorkParams {
        id: &work.id,
        title: &work.title,
        disambiguation: work.disambiguation.as_deref(),
        work_type: work.work_type.as_deref(),
        musicbrainz_work_id: &work.musicbrainz_work_id,
        created_at: work.created_at.to_rfc3339(),
    })?;
    Ok(())
}

coven::write!(
    KeepWorkArtist(
        id: String,
        work_id: &'p str,
        artist_id: &'p str,
        position: i32,
        source: &'static str,
        role: &'static str,
        role_words: Option<String>,
        qualifiers: String,
        credit_key: Option<String>,
        created_at: &'p str,
    ),
    r#"
    INSERT OR IGNORE INTO work_artists (
        id, work_id, artist_id, position, source, role, role_words, qualifiers, credit_key,
        created_at
    ) VALUES (
        :id, :work_id, :artist_id, :position, :source, :role, :role_words, :qualifiers,
        :credit_key, :created_at
    )
"#
);

pub fn insert_work_artist_row(conn: &WriteContext<'_>, link: &DbWorkArtist) -> Result<(), DbError> {
    let created_at = link.created_at.to_rfc3339();
    let credit_key = settle_credit_name(conn, &link.artist_id, link.credit.as_ref(), &created_at)?;
    let columns = link.role.columns();
    conn.run(&KeepWorkArtistParams {
        id: link.id(),
        work_id: &link.work_id,
        artist_id: &link.artist_id,
        position: link.position,
        source: link.source.as_str(),
        role: columns.role,
        role_words: columns.role_words,
        qualifiers: columns.qualifiers,
        credit_key,
        created_at: &created_at,
    })?;
    Ok(())
}

coven::write!(
    KeepWorkPart(
        id: String,
        parent_work_id: &'p str,
        child_work_id: &'p str,
        position: i32,
        source: &'static str,
        created_at: String,
    ),
    r#"
    INSERT OR IGNORE INTO work_parts (
        id, parent_work_id, child_work_id, position, source, created_at
    ) VALUES (:id, :parent_work_id, :child_work_id, :position, :source, :created_at)
"#
);

pub fn insert_work_part_row(conn: &WriteContext<'_>, part: &DbWorkPart) -> Result<(), DbError> {
    conn.run(&KeepWorkPartParams {
        id: part.id(),
        parent_work_id: &part.parent_work_id,
        child_work_id: &part.child_work_id,
        position: part.position,
        source: part.source.as_str(),
        created_at: part.created_at.to_rfc3339(),
    })?;
    Ok(())
}

coven::write!(
    InsertTrackWork(
        id: &'p str,
        track_id: &'p str,
        work_id: &'p str,
        position: i32,
        source: &'static str,
        created_at: String,
    ),
    r#"
    INSERT INTO track_works (id, track_id, work_id, position, source, created_at)
    VALUES (:id, :track_id, :work_id, :position, :source, :created_at)
"#
);

pub fn insert_track_work_row(conn: &WriteContext<'_>, link: &DbTrackWork) -> Result<(), DbError> {
    conn.run(&InsertTrackWorkParams {
        id: &link.id,
        track_id: &link.track_id,
        work_id: &link.work_id,
        position: link.position,
        source: link.source.as_str(),
        created_at: link.created_at.to_rfc3339(),
    })?;
    Ok(())
}

coven::write!(
    InsertReleaseArtistRole(
        id: &'p str,
        release_id: &'p str,
        artist_id: &'p str,
        position: i32,
        source: &'static str,
        role: &'static str,
        role_words: Option<String>,
        qualifiers: String,
        credit_key: Option<String>,
        created_at: &'p str,
    ),
    r#"
    INSERT INTO release_artist_roles (
        id, release_id, artist_id, position, source, role, role_words, qualifiers,
        credit_key, created_at
    ) VALUES (
        :id, :release_id, :artist_id, :position, :source, :role, :role_words, :qualifiers,
        :credit_key, :created_at
    )
"#
);

pub fn insert_release_artist_role_row(
    conn: &WriteContext<'_>,
    role: &DbReleaseArtistRole,
) -> Result<(), DbError> {
    let created_at = role.created_at.to_rfc3339();
    let credit_key = settle_credit_name(conn, &role.artist_id, role.credit.as_ref(), &created_at)?;
    let columns = role.role.columns();
    conn.run(&InsertReleaseArtistRoleParams {
        id: &role.id,
        release_id: &role.release_id,
        artist_id: &role.artist_id,
        position: role.position,
        source: role.source.as_str(),
        role: columns.role,
        role_words: columns.role_words,
        qualifiers: columns.qualifiers,
        credit_key,
        created_at: &created_at,
    })?;
    Ok(())
}

coven::write!(
    InsertTrackArtistRole(
        id: &'p str,
        track_id: &'p str,
        artist_id: &'p str,
        position: i32,
        source: &'static str,
        role: &'static str,
        role_words: Option<String>,
        qualifiers: String,
        credit_key: Option<String>,
        created_at: &'p str,
    ),
    r#"
    INSERT INTO track_artist_roles (
        id, track_id, artist_id, position, source, role, role_words, qualifiers,
        credit_key, created_at
    ) VALUES (
        :id, :track_id, :artist_id, :position, :source, :role, :role_words, :qualifiers,
        :credit_key, :created_at
    )
"#
);

pub fn insert_track_artist_role_row(
    conn: &WriteContext<'_>,
    role: &DbTrackArtistRole,
) -> Result<(), DbError> {
    let created_at = role.created_at.to_rfc3339();
    let credit_key = settle_credit_name(conn, &role.artist_id, role.credit.as_ref(), &created_at)?;
    let columns = role.role.columns();
    conn.run(&InsertTrackArtistRoleParams {
        id: &role.id,
        track_id: &role.track_id,
        artist_id: &role.artist_id,
        position: role.position,
        source: role.source.as_str(),
        role: columns.role,
        role_words: columns.role_words,
        qualifiers: columns.qualifiers,
        credit_key,
        created_at: &created_at,
    })?;
    Ok(())
}

// The role, the catalog's words for it and the qualifiers of each work
// credit, in list order.
#[cfg(test)]
coven::read!(
    writer WorkCreditRoles,
    r#"
    SELECT credit.role, credit.role_words, credit.qualifiers
    FROM work_artists AS credit ORDER BY credit.position
"#
);

// The roles the writing roles view names.
#[cfg(test)]
coven::read!(writer WritingRoles { role: String }, "SELECT writing.role FROM writing_credit_roles AS writing");
