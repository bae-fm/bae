//! History: the plays this device holds, newest first, everyone's or one
//! person's, each with the day it was played and whose it is (§5.3). The
//! list is long, so screens read it in windows along `history`'s indexes.

use super::*;
use bae_people::people::fragments::Profile;
use bae_records::records::files::CoverVersion;
use bae_records::records::fragments::ShownTrack;
use bae_records::records::read::credit_line_of_json;
use bae_store::list_windows::{sql_bound, Keyed, SortValue};
use bae_store::statement_optional;

/// One play as History lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    pub id: uuid::Uuid,
    pub title: String,
    /// The track's credit line: its own credits, else its album's (§2.3).
    pub artist_line: bae_types::credits::CreditLine,
    /// The cover of the release the track is on.
    pub cover: Option<ImageRef>,
    pub started_at: UtcDateTime,
    /// The calendar date the playing device showed when the play began.
    pub played_on: time::Date,
    /// Whether this is the newest play the list shows of its day, which
    /// opens the day's heading.
    pub opens_day: bool,
    /// Whose play it is.
    pub person: bae_people::people::PersonRef,
}

/// Whose plays History shows: everyone's, or one person's.
pub type HistoryFilter = Option<coven::MemberId>;

/// A person whose plays History's menu offers to show.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct HistoryPerson {
    pub person: coven::MemberId,
    pub name: String,
    /// Whether History shows this person's plays.
    pub chosen: bool,
}

/// Whose plays History shows, as its menu names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryShown {
    Everyone,
    Person { name: String },
}

/// How many plays History shows under a filter, and its menu of whose plays:
/// the people of the library with a profile, this device's person first and
/// the others by name, and whose plays the filter shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryCount {
    pub plays: u64,
    pub people: Vec<HistoryPerson>,
    pub shown: HistoryShown,
}

// History's order is `PlayPlace`'s: newest first, by when a play began and
// then by its id, which tells apart plays begun at one moment. An entry's key
// is those two values. A list of everyone's plays and a list of one person's
// are two statements each, so that each seeks along its own index.

// The window of History of everyone's plays: at most `limit` plays at or after
// the place (`from_at`, `from_id`), each with its track, cover and person. The
// joins after the plays are CROSS JOINs, so that SQLite reads the plays first,
// along their index.
coven::read!(
    writer HistoryWindow(from_at: String, from_id: String, limit: i64) { opens_day: bool },
    r#"
    SELECT play.started_at, play.id, play.played_on, play.opens_day, track.*, cover.*,
           person.*
    FROM @bae_people::people::fragments::ListedPlay AS play
    CROSS JOIN @bae_records::records::fragments::ShownTrack AS track
            ON track.track_id = play.track_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover ON cover.id = track.release_id
    CROSS JOIN @bae_people::people::fragments::Profile AS person ON person.id = play.person_id
    WHERE (play.started_at, play.id) <= (:from_at, :from_id)
    ORDER BY play.started_at DESC, play.id DESC
    LIMIT :limit
"#
);

// [`HistoryWindow`] from History's top: the newest plays.
coven::read!(
    writer HistoryTop(limit: i64) { opens_day: bool },
    r#"
    SELECT play.started_at, play.id, play.played_on, play.opens_day, track.*, cover.*,
           person.*
    FROM @bae_people::people::fragments::ListedPlay AS play
    CROSS JOIN @bae_records::records::fragments::ShownTrack AS track
            ON track.track_id = play.track_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover ON cover.id = track.release_id
    CROSS JOIN @bae_people::people::fragments::Profile AS person ON person.id = play.person_id
    ORDER BY play.started_at DESC, play.id DESC
    LIMIT :limit
"#
);

// [`HistoryWindow`] of the plays of the person `person`.
coven::read!(
    writer PersonHistoryWindow(person: coven::MemberId, from_at: String, from_id: String, limit: i64)
        { opens_day: bool },
    r#"
    SELECT play.started_at, play.id, play.played_on, play.opens_day, track.*, cover.*,
           person.*
    FROM @bae_people::people::fragments::ListedPersonPlay AS play
    CROSS JOIN @bae_records::records::fragments::ShownTrack AS track
            ON track.track_id = play.track_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover ON cover.id = track.release_id
    CROSS JOIN @bae_people::people::fragments::Profile AS person ON person.id = play.person_id
    WHERE play.person_id = :person
      AND (play.started_at, play.id) <= (:from_at, :from_id)
    ORDER BY play.started_at DESC, play.id DESC
    LIMIT :limit
"#
);

// [`PersonHistoryWindow`] from History's top: the person's newest plays.
coven::read!(
    writer PersonHistoryTop(person: coven::MemberId, limit: i64)
        { opens_day: bool },
    r#"
    SELECT play.started_at, play.id, play.played_on, play.opens_day, track.*, cover.*,
           person.*
    FROM @bae_people::people::fragments::ListedPersonPlay AS play
    CROSS JOIN @bae_records::records::fragments::ShownTrack AS track
            ON track.track_id = play.track_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover ON cover.id = track.release_id
    CROSS JOIN @bae_people::people::fragments::Profile AS person ON person.id = play.person_id
    WHERE play.person_id = :person
    ORDER BY play.started_at DESC, play.id DESC
    LIMIT :limit
"#
);

// How many plays History shows, everyone's or the person `person`'s; its menu
// of whose plays, the current members with a profile, this device's person
// first and the others by name in the device's language, each marked when
// `person` names them; and `person`'s name, NULL when no profile has the id.
coven::read!(
    writer HistoryTotals(person: Option<coven::MemberId>) {
        plays: u64,
        people: json<Vec<crate::history::HistoryPerson>>,
        shown_name: Option<String>
    },
    r#"
    SELECT
        CASE WHEN :person IS NULL
             THEN (SELECT COUNT(*)
                   FROM @bae_people::people::fragments::PlayPlace AS every_play)
             ELSE (SELECT COUNT(*)
                   FROM @bae_people::people::fragments::PlayPlace AS own_play
                   WHERE own_play.person_id = :person)
        END AS plays,
        (SELECT json_group_array(json_object(
                    'person', profile.id, 'name', profile.name,
                    'chosen', json(iif(profile.id IS :person, 'true', 'false')))
                    ORDER BY member.is_self DESC,
                             bae_name_key(profile.name, sort.language), member.member_id)
         FROM @bae_people::membership::fragments::Member AS member
         JOIN @bae_people::people::fragments::Profile AS profile
              ON profile.id = member.member_id
         JOIN @bae_records::records::name_sort::NameSortLanguage AS sort ON 1
         WHERE NOT member.removed) AS people,
        (SELECT shown.name
         FROM @bae_people::people::fragments::Profile AS shown
         WHERE shown.id = :person) AS shown_name
"#
);

// How many plays of History come before the place (`at`, `id`).
coven::read!(
    writer PlaysBefore(at: String, id: String) { plays: u64 },
    r#"
    SELECT COUNT(*) AS plays
    FROM @bae_people::people::fragments::PlayPlace AS play
    WHERE (play.started_at, play.id) > (:at, :id)
"#
);

// [`PlaysBefore`] among the plays of the person `person`.
coven::read!(
    writer PersonPlaysBefore(person: coven::MemberId, at: String, id: String) { plays: u64 },
    r#"
    SELECT COUNT(*) AS plays
    FROM @bae_people::people::fragments::PlayPlace AS play
    WHERE play.person_id = :person AND (play.started_at, play.id) > (:at, :id)
"#
);

// The place of the play at `index`, counted from History's top, or of the last
// play when History ends before it. SQLite reads the offset -1 of an empty
// list as 0, where there is no row.
coven::read!(
    writer PlayAt(index: i64),
    r#"
    SELECT play.started_at, play.id
    FROM @bae_people::people::fragments::PlayPlace AS play
    ORDER BY play.started_at DESC, play.id DESC
    LIMIT 1 OFFSET min(
        :index,
        (SELECT COUNT(*) FROM @bae_people::people::fragments::PlayPlace AS counted) - 1)
"#
);

// [`PlayAt`] among the plays of the person `person`.
coven::read!(
    writer PersonPlayAt(person: coven::MemberId, index: i64),
    r#"
    SELECT play.started_at, play.id
    FROM @bae_people::people::fragments::PlayPlace AS play
    WHERE play.person_id = :person
    ORDER BY play.started_at DESC, play.id DESC
    LIMIT 1 OFFSET min(
        :index,
        (SELECT COUNT(*)
         FROM @bae_people::people::fragments::PlayPlace AS counted
         WHERE counted.person_id = :person) - 1)
"#
);

/// The key of the play that began at `started_at` with the id `id`, both as
/// their columns hold them.
fn key_of(started_at: String, id: String) -> OrderKey {
    OrderKey::new(vec![SortValue::Text(started_at), SortValue::Text(id)])
}

/// The place `key` names. Only [`key_of`] makes History's keys.
fn place_of(key: &OrderKey) -> Result<(String, String), DbError> {
    match key.values() {
        [SortValue::Text(started_at), SortValue::Text(id)] => Ok((started_at.clone(), id.clone())),
        values => Err(DbError::unreadable("a History key", values)),
    }
}

/// A play's own columns, as both windows select them.
#[derive(Clone, PartialEq)]
struct PlayColumns {
    started_at: String,
    id: String,
    played_on: String,
    opens_day: bool,
}

/// A row of either window as read: the play, its track, the track's cover and
/// the person who played it.
#[derive(Clone, PartialEq)]
struct ListedPlay {
    play: PlayColumns,
    track: ShownTrack,
    cover: Option<CoverVersion>,
    person: Profile,
}

/// A window's row as History lists it, with its key.
fn listed_play(
    ListedPlay { play, track, cover, person }: ListedPlay,
) -> Result<Keyed<HistoryRow>, DbError> {
    let id = uuid::Uuid::parse_str(&play.id)
        .map_err(|error| DbError::unreadable(format!("the id of play {}", play.id), error))?;
    let started_at = bae_util::timestamp::parse_rfc3339(&play.started_at)
        .map_err(|error| DbError::unreadable(format!("the start of play {}", play.id), error))?;
    let played_on =
        time::Date::parse(&play.played_on, &time::format_description::well_known::Iso8601::DATE)
            .map_err(|error| DbError::unreadable(format!("the day of play {}", play.id), error))?;
    let (release_id, title, credits) = track.into_parts()?;
    let row = HistoryRow {
        id,
        title,
        artist_line: credit_line_of_json(&credits)?,
        cover: cover_ref(&release_id, cover.and_then(CoverVersion::into_version)),
        started_at,
        played_on,
        opens_day: play.opens_day,
        person: person.into_person_ref(),
    };
    Ok((row, key_of(play.started_at, play.id)))
}

/// The read of History's `window` under `filter`: its plays from the window's
/// key on, or from its newest play for a window from the top, each with its
/// key.
fn history_window_read(
    filter: &HistoryFilter,
    window: &ListWindow,
) -> Result<coven::ReadAs<ListedPlay>, DbError> {
    let limit = sql_bound(window.limit);
    let from = window.from.as_ref().map(place_of).transpose()?;
    macro_rules! plays {
        ($statement:expr) => {
            coven::ReadAs::new($statement, |row| ListedPlay {
                play: PlayColumns {
                    started_at: row.started_at,
                    id: row.id,
                    played_on: row.played_on,
                    opens_day: row.opens_day,
                },
                track: row.track,
                cover: row.cover,
                person: row.person,
            })
        };
    }
    Ok(match (*filter, from) {
        (None, None) => plays!(HistoryTopParams { limit }),
        (None, Some((from_at, from_id))) => {
            plays!(HistoryWindowParams { from_at, from_id, limit })
        }
        (Some(person), None) => plays!(PersonHistoryTopParams { person, limit }),
        (Some(person), Some((from_at, from_id))) => {
            plays!(PersonHistoryWindowParams { person, from_at, from_id, limit })
        }
    })
}

/// How many plays History shows under `filter`, with its menu of whose
/// plays. A filter naming a person with no profile fails the read: the menu
/// offers only people with one, and bae never deletes a profile.
fn history_count_of(
    filter: &HistoryFilter,
    totals: HistoryTotalsRow,
) -> Result<HistoryCount, DbError> {
    let shown = match filter {
        Some(person) => HistoryShown::Person {
            name: totals.shown_name.ok_or_else(|| {
                DbError::unreadable(format!("the profile of person {person}"), None::<String>)
            })?,
        },
        None => HistoryShown::Everyone,
    };
    Ok(HistoryCount { plays: totals.plays, people: totals.people, shown })
}

/// The read of how many plays History shows under `filter` before the play
/// at `key`.
fn history_rows_before_read(
    filter: &HistoryFilter,
    key: &OrderKey,
) -> Result<coven::ReadAs<u64>, DbError> {
    let (at, id) = place_of(key)?;
    Ok(match *filter {
        None => coven::ReadAs::new(PlaysBeforeParams { at, id }, |row| row.plays),
        Some(person) => {
            coven::ReadAs::new(PersonPlaysBeforeParams { person, at, id }, |row| row.plays)
        }
    })
}

/// The key of the play at `position` of History under `filter`, or of the last
/// one when it ends before it; `None` for no plays.
fn history_key_at_on(
    sql: &ReadContext<'_>,
    filter: &HistoryFilter,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    let index = sql_bound(position);
    Ok(match *filter {
        None => statement_optional(sql, &PlayAtParams { index })?
            .map(|row| key_of(row.started_at, row.id)),
        Some(person) => statement_optional(sql, &PersonPlayAtParams { person, index })?
            .map(|row| key_of(row.started_at, row.id)),
    })
}

/// History's count as the list shows it: its plays, and its menu of whose
/// plays.
pub fn resolve_history_count(count: HistoryCount) -> crate::ListCount {
    crate::ListCount {
        rows: count.plays,
        items: count.plays,
        extra: crate::ListCountExtra::History { people: count.people, shown: count.shown },
    }
}

/// One delivery of History's window as the screen shows it.
pub fn resolve_history_window(read: WindowRead<HistoryRow>) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, crate::ListRows::History)
}

impl LibraryViews {
    /// Follow one window of History's plays under `filter`: a range of its
    /// order, which coven reruns for a write inside it alone.
    pub fn subscribe_history_window(
        &self,
        filter: HistoryFilter,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<HistoryRow>, bae_store::DbError>
    {
        self.store
            .subscribe_reconfigurable(initial, move |window| history_window_read(&filter, window))
            .process(|window, plays| {
                let rows = plays.into_iter().map(listed_play).collect::<Result<_, DbError>>()?;
                Ok(WindowRead { window: window.clone(), rows })
            })
    }

    /// Follow how many plays History shows under `filter`, with its menu of
    /// whose plays.
    pub fn subscribe_history_count(
        &self,
        filter: HistoryFilter,
    ) -> coven::LiveQuery<HistoryCount, bae_store::DbError> {
        self.store
            .subscribe_row(HistoryTotalsParams { person: filter })
            .process(move |totals| history_count_of(&filter, totals))
    }

    /// Follow how many plays History shows under `filter` before the row at
    /// a key; the caller changes the key on the same query.
    pub fn subscribe_history_position(
        &self,
        filter: HistoryFilter,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        self.store
            .subscribe_reconfigurable(initial, move |key| history_rows_before_read(&filter, key))
            .process(|_, before| exactly_one::<coven::ReadAs<u64>, _>(before))
    }

    /// The key of the play at `position` of History under `filter`, or of
    /// its last when it ends before it; `None` for no plays.
    pub async fn history_key_at(
        &self,
        filter: HistoryFilter,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        self.store.read(move |sql| history_key_at_on(&sql, &filter, position)).await
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
