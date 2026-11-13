//! The credits a release's detail lists: the release's own, each track's own,
//! and the credits of the works each track performs in a role other than a
//! writing one (a work's writing credits are its composers, named with the
//! work).

use super::query::shown_artist_id;
use super::read::credit_name_join;
use super::*;

/// One part of the read: the credit rows of `table` as the columns
/// [`release_credits_on`] orders and reads.
struct CreditPart<'a> {
    table: &'a str,
    /// 0 for the release's credits, 1 for a track's.
    part: u8,
    /// The track the credit sits on, an SQL expression.
    track_id: &'a str,
    /// Its place in playing order, an SQL expression.
    track_position: &'a str,
    /// 0 for a track's own credits, 1 for those of the works it performs.
    kind: u8,
    first_position: &'a str,
    second_position: &'a str,
    joins: &'a str,
    filter: &'a str,
}

impl CreditPart<'_> {
    fn select(&self) -> String {
        format!(
            "SELECT {part} AS part, {track_id} AS track_id, {track_position} AS track_position, \
                    {kind} AS kind, {first} AS first_position, {second} AS second_position, \
                    credit.id AS credit_id, \
                    shown.id AS artist_id, COALESCE(printed.name, shown.name) AS printed, \
                    credit.role AS role, credit.role_words AS role_words, \
                    credit.qualifiers AS qualifiers \
             FROM {table} credit {joins} \
             JOIN artists shown ON shown.id = {shown} \
             {printed_name} \
             WHERE {filter}",
            part = self.part,
            track_id = self.track_id,
            track_position = self.track_position,
            kind = self.kind,
            first = self.first_position,
            second = self.second_position,
            table = self.table,
            joins = self.joins,
            shown = shown_artist_id("credit.artist_id"),
            printed_name = credit_name_join("credit", "printed"),
            filter = self.filter,
        )
    }
}

/// The credits the detail of `release_id` lists, in the order it lists them:
/// the release's, in the order the catalog stated them; then for each track
/// in playing order, its own credits, then the credits in a role other than a
/// writing one of the works it performs, each work in the order the track
/// performs it.
pub(super) fn release_credits_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Vec<DbReleaseCredit>, DbError> {
    let release = CreditPart {
        table: "release_artist_roles",
        part: 0,
        track_id: "NULL",
        track_position: "0",
        kind: 0,
        first_position: "credit.position",
        second_position: "0",
        joins: "",
        filter: "credit.release_id = ?1",
    };
    let track = CreditPart {
        table: "track_artist_roles",
        part: 1,
        track_id: "credit.track_id",
        track_position: "track.position",
        kind: 0,
        first_position: "credit.position",
        second_position: "0",
        joins: "JOIN tracks track ON track.id = credit.track_id",
        filter: "track.release_id = ?1",
    };
    let work = CreditPart {
        table: "work_artists",
        part: 1,
        track_id: "performed.track_id",
        track_position: "track.position",
        kind: 1,
        first_position: "performed.position",
        second_position: "credit.position",
        joins: "JOIN track_works performed ON performed.work_id = credit.work_id \
                JOIN tracks track ON track.id = performed.track_id",
        filter: "track.release_id = ?1 \
                 AND credit.role NOT IN (SELECT role FROM writing_credit_roles)",
    };
    let query = format!(
        "{} UNION ALL {} UNION ALL {} \
         ORDER BY part, track_position, kind, first_position, second_position, credit_id",
        release.select(),
        track.select(),
        work.select(),
    );
    Ok(sql.query(&query, params![release_id], |row| {
        let track_id: Option<String> = row.get("track_id")?;
        let role: String = row.get("role")?;
        let words: Option<String> = row.get("role_words")?;
        let qualifiers: String = row.get("qualifiers")?;
        Ok(DbReleaseCredit {
            artist_id: row.get("artist_id")?,
            printed: row.get("printed")?,
            role: bae_types::credit_role::CreditedAs::from_columns(&role, words, &qualifiers)
                .map_err(|error| column_conversion_error(row, "role", error.to_string()))?,
            scope: match track_id {
                Some(track) => DbCreditScope::Track(track),
                None => DbCreditScope::Release,
            },
        })
    })?)
}
