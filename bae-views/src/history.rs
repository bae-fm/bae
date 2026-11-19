//! History: the plays this device holds, newest first, everyone's or one
//! person's, each with the day it was played and whose it is (§5.3). The
//! list is long, so screens read it in windows along `history`'s indexes.

use super::*;

/// One play as History lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    pub id: uuid::Uuid,
    pub track_id: String,
    pub title: String,
    /// The track's credit line: its own credits, else its album's (§2.3).
    pub artist_line: bae_types::credits::CreditLine,
    pub album_title: String,
    /// The cover of the release the track is on.
    pub cover: Option<ImageRef>,
    pub started_at: UtcDateTime,
    /// The calendar date the playing device showed when the play began.
    pub played_on: time::Date,
    /// Whether this is the newest play the list shows of its day, which
    /// opens the day's heading.
    pub opens_day: bool,
    /// Whose play it is.
    pub person: coven::MemberId,
    pub person_name: String,
    /// The version of the person's avatar; `None` without one.
    pub avatar_version: Option<String>,
}

/// Whose plays History shows: everyone's, or one person's.
pub type HistoryFilter = Option<coven::MemberId>;

/// History's order over `history` aliased `alias`: newest first, which its
/// indexes hold backwards, the id telling apart plays begun at one moment.
fn history_order(alias: &str) -> ListOrder {
    ListOrder::new(vec![
        SortTerm::descending(format!("{alias}.started_at")),
        SortTerm::descending(format!("{alias}.id")),
    ])
}

/// The condition that keeps `alias`'s plays to `:person`'s when the filter
/// names one.
fn person_condition(alias: &str, filter: &HistoryFilter) -> String {
    if filter.is_some() {
        format!("{alias}.person_id = :person")
    } else {
        "1".to_string()
    }
}

/// Whether the play `h` is the newest the list shows of its day: one index
/// seek per row.
fn opens_day_sql(filter: &HistoryFilter) -> String {
    format!(
        "((SELECT first.id FROM history first \
           WHERE first.played_on = h.played_on AND {person} \
           ORDER BY {order} LIMIT 1) = h.id)",
        person = person_condition("first", filter),
        order = history_order("first").clause(),
    )
}

/// The statement reading the plays `seek` reads in `order`, at most
/// `:limit`, each with its track, cover, person and key.
fn history_rows_query(filter: &HistoryFilter, order: &ListOrder, seek: &Seek) -> String {
    format!(
        "SELECT h.id, h.track_id, t.title, {credits} AS artist_line, a.title AS album_title, \
                t.release_id, {cover} AS cover_version, h.started_at, h.played_on, \
                {opens} AS opens_day, h.person_id, p.name AS person_name, {avatar} AS avatar, \
                {keys} \
         FROM history h JOIN tracks t ON t.id = h.track_id \
              JOIN releases r ON r.id = t.release_id JOIN albums a ON a.id = r.album_id \
              JOIN people p ON p.id = h.person_id \
         WHERE {person} AND {condition} \
         ORDER BY {ordering} {limit}",
        credits = track_credits_json_sql("t"),
        cover = cover_version_sql("t.release_id"),
        opens = opens_day_sql(filter),
        avatar = bae_store::file_version_sql("p.avatar_id"),
        keys = order.key_columns(),
        person = person_condition("h", filter),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
        limit = seek.limit_clause(),
    )
}

fn history_row(row: &Row<'_>) -> rusqlite::Result<HistoryRow> {
    let release_id: String = row.get("release_id")?;
    let played_on = played_on_column(row)?;
    Ok(HistoryRow {
        id: uuid_column(row, "id")?,
        track_id: row.get("track_id")?,
        title: row.get("title")?,
        artist_line: credit_line_column(row, "artist_line")?,
        album_title: row.get("album_title")?,
        cover: cover_ref(&release_id, row.get("cover_version")?),
        started_at: rfc3339_column(row, "started_at")?,
        played_on,
        opens_day: row.get("opens_day")?,
        person: coven::MemberId(uuid_column(row, "person_id")?),
        person_name: row.get("person_name")?,
        avatar_version: row.get("avatar")?,
    })
}

/// The day `played_on` holds, as the play's write stores it: YYYY-MM-DD.
fn played_on_column(row: &Row<'_>) -> rusqlite::Result<time::Date> {
    let text: String = row.get("played_on")?;
    time::Date::parse(&text, &time::format_description::well_known::Iso8601::DATE)
        .map_err(|error| column_conversion_error(row, "played_on", error))
}

fn uuid_column(row: &Row<'_>, column: &str) -> rusqlite::Result<uuid::Uuid> {
    let value: String = row.get(column)?;
    uuid::Uuid::parse_str(&value).map_err(|error| column_conversion_error(row, column, error))
}

/// The person `filter` names, as the statements bind it.
fn person_param(filter: &HistoryFilter) -> Option<String> {
    filter.map(|person| person.to_string())
}

/// The plays `seek` reads in `order`, at most `limit`, each with its key.
fn history_rows_on(
    sql: &SqlReadContext<'_>,
    filter: &HistoryFilter,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
) -> Result<Vec<Keyed<HistoryRow>>, DbError> {
    let statement = history_rows_query(filter, order, seek);
    let person = person_param(filter);
    Ok(seek.bound(limit, |params| {
        let mut params = params.to_vec();
        if let Some(person) = &person {
            params.push((":person", person as &dyn rusqlite::ToSql));
        }
        sql.query(&statement, params.as_slice(), |row| Ok((history_row(row)?, order.key_of(row)?)))
    })?)
}

fn history_count_on(sql: &SqlReadContext<'_>, filter: &HistoryFilter) -> Result<u64, DbError> {
    let statement =
        format!("SELECT COUNT(*) FROM history h WHERE {}", person_condition("h", filter));
    let count: i64 = match person_param(filter) {
        Some(person) => {
            sql.query_row(&statement, rusqlite::named_params! { ":person": person }, |row| {
                row.get(0)
            })?
        }
        None => sql.query_row(&statement, [], |row| row.get(0))?,
    };
    Ok(u64::try_from(count).unwrap_or(0))
}

fn history_rows_before_on(
    sql: &SqlReadContext<'_>,
    filter: &HistoryFilter,
    order: &ListOrder,
    key: &OrderKey,
) -> Result<u64, DbError> {
    let person = person_param(filter);
    let condition = filter.is_some().then(|| person_condition("h", filter));
    let params: Vec<(&str, &dyn rusqlite::ToSql)> = match &person {
        Some(person) => vec![(":person", person as &dyn rusqlite::ToSql)],
        None => Vec::new(),
    };
    bae_store::list_windows::rows_before_on(
        sql,
        "FROM history h",
        condition.as_deref(),
        &params,
        order,
        key,
    )
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
        let order = history_order("h");
        let read_order = order.clone();
        self.store.subscribe_list_window(initial, order, move |sql, seek, limit| {
            history_rows_on(sql, &filter, &read_order, seek, limit)
        })
    }

    /// Follow how many plays History shows under `filter`.
    pub fn subscribe_history_count(
        &self,
        filter: HistoryFilter,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        self.store.subscribe(move |sql| history_count_on(&sql, &filter))
    }

    /// Follow how many plays History shows under `filter` before the row at
    /// a key; the caller changes the key on the same query.
    pub fn subscribe_history_position(
        &self,
        filter: HistoryFilter,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let order = history_order("h");
        self.store.subscribe_reconfigurable(initial, move |key, sql| {
            history_rows_before_on(&sql, &filter, &order, key)
        })
    }

    /// The key of the play at `position` of History under `filter`, or of
    /// its last when it ends before it; `None` for no plays.
    pub async fn history_key_at(
        &self,
        filter: HistoryFilter,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let order = history_order("h");
        self.store
            .read(move |sql| {
                key_at(&order, position, &mut |seek, limit| {
                    history_rows_on(&sql, &filter, &order, seek, limit)
                })
            })
            .await
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
