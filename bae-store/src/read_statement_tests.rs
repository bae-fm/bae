use crate::{statement_optional, statement_row, DbError, Schema, SchemaPart};
use bae_util::test_channels::HANG_GUARD;
use sqlive_compose::{Read, Returning, Statement};

static NOTES: &[SchemaPart] = &[SchemaPart::new(&[
    "CREATE TABLE notes (id INTEGER PRIMARY KEY, topic TEXT NOT NULL, body TEXT NOT NULL) STRICT;",
])];

static LIBRARY: &[&[SchemaPart]] = &[NOTES];

/// What a request for the notes of one topic binds.
#[derive(Clone, PartialEq)]
struct Topic {
    topic: String,
}

/// The bodies of a topic's notes, with the traits a `read!` generates.
struct Bodies {
    topic: String,
}

impl From<&Topic> for Bodies {
    fn from(request: &Topic) -> Self {
        Self { topic: request.topic.clone() }
    }
}

impl Statement for Bodies {
    const SQL: &'static str = "SELECT n.body FROM notes AS n WHERE n.topic = :topic ORDER BY n.id";

    fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        vec![(":topic", &self.topic)]
    }
}

impl Returning for Bodies {
    type Row = String;

    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<String> {
        row.get("body")
    }
}

impl Read for Bodies {}

/// How many notes a topic has.
struct Count {
    topic: String,
}

impl From<&Topic> for Count {
    fn from(request: &Topic) -> Self {
        Self { topic: request.topic.clone() }
    }
}

impl Statement for Count {
    const SQL: &'static str = "SELECT COUNT(*) AS notes FROM notes AS n WHERE n.topic = :topic";

    fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        vec![(":topic", &self.topic)]
    }
}

impl Returning for Count {
    type Row = i64;

    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<i64> {
        row.get("notes")
    }
}

impl Read for Count {}

/// A statement whose row does not read.
#[derive(Clone, PartialEq)]
struct Failing;

impl From<&Topic> for Failing {
    fn from(_: &Topic) -> Self {
        Self
    }
}

impl Statement for Failing {
    const SQL: &'static str = "SELECT 1 AS body";

    fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        Vec::new()
    }
}

impl Returning for Failing {
    type Row = String;

    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<String> {
        row.get("body")
    }
}

impl Read for Failing {}

fn topic(name: &str) -> Topic {
    Topic { topic: name.to_owned() }
}

fn bodies(name: &str) -> Bodies {
    Bodies::from(&topic(name))
}

async fn add_note(store: &crate::Store, topic: &str, body: &str) {
    let (topic, body) = (topic.to_owned(), body.to_owned());
    store
        .call_sql(move |sql| {
            sql.execute("INSERT INTO notes (topic, body) VALUES (?1, ?2)", (&topic, &body))?;
            Ok(())
        })
        .await
        .expect("the note is written");
}

async fn notes_store() -> (crate::Store, impl Sized) {
    crate::test_fixtures::temp_db(Schema::new(LIBRARY)).await
}

fn is_row_count_error<T: std::fmt::Debug>(result: Result<T, DbError>) -> bool {
    matches!(result, Err(DbError::Unreadable { .. }))
}

#[tokio::test]
async fn a_one_shot_read_binds_the_statement_s_named_parameters() {
    let (store, _tmp) = notes_store().await;
    add_note(&store, "a", "one").await;
    add_note(&store, "b", "two").await;
    add_note(&store, "a", "three").await;

    let rows = store.read_rows(bodies("a")).await.unwrap();
    assert_eq!(rows, vec!["one".to_owned(), "three".to_owned()]);
    assert_eq!(store.read_row(Count::from(&topic("b"))).await.unwrap(), 1);
}

#[tokio::test]
async fn a_write_runs_the_statement_a_one_shot_read_runs() {
    let (store, _tmp) = notes_store().await;
    let counted = store
        .call_sql(|sql| {
            sql.execute("INSERT INTO notes (topic, body) VALUES ('a', 'one')", [])?;
            statement_row(&sql, &Count::from(&topic("a")))
        })
        .await
        .unwrap();
    assert_eq!(counted, 1);
}

async fn at_most_one_body(store: &crate::Store) -> Result<Option<String>, DbError> {
    store.read(|sql| statement_optional(&sql, &bodies("a"))).await
}

#[tokio::test]
async fn exactly_one_row_refuses_none_and_two() {
    let (store, _tmp) = notes_store().await;
    assert!(is_row_count_error(store.read_row(bodies("a")).await));
    add_note(&store, "a", "one").await;
    assert_eq!(store.read_row(bodies("a")).await.unwrap(), "one");
    add_note(&store, "a", "two").await;
    assert!(is_row_count_error(store.read_row(bodies("a")).await));
}

#[tokio::test]
async fn at_most_one_row_takes_none_and_refuses_two() {
    let (store, _tmp) = notes_store().await;
    assert_eq!(at_most_one_body(&store).await.unwrap(), None);
    add_note(&store, "a", "one").await;
    assert_eq!(at_most_one_body(&store).await.unwrap(), Some("one".to_owned()));
    add_note(&store, "a", "two").await;
    assert!(is_row_count_error(at_most_one_body(&store).await));
}

#[tokio::test]
async fn a_statement_that_fails_to_read_a_row_fails_the_read() {
    let (store, _tmp) = notes_store().await;
    let result = store.read_rows(Failing).await;
    assert!(matches!(result, Err(DbError::Sqlite(_))));
}

#[tokio::test]
async fn two_statements_answer_one_snapshot_and_rerun_for_a_write_to_what_either_read() {
    let (store, _tmp) = notes_store().await;
    let mut live: coven::ReconfigurableLiveQuery<_, _, DbError> =
        store.subscribe_reconfigurable_statements::<_, Bodies, Count, _>(topic("a"));

    let first = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert_eq!(first.result.unwrap(), (Vec::<String>::new(), vec![0]));

    add_note(&store, "a", "one").await;
    let second = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert_eq!(second.result.unwrap(), (vec!["one".to_owned()], vec![1]));
}

#[tokio::test]
async fn two_statements_answer_each_request_they_are_given() {
    let (store, _tmp) = notes_store().await;
    add_note(&store, "a", "one").await;
    add_note(&store, "b", "two").await;
    let mut live: coven::ReconfigurableLiveQuery<_, _, DbError> =
        store.subscribe_reconfigurable_statements::<_, Bodies, Count, _>(topic("a"));
    let first = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert_eq!(first.result.unwrap(), (vec!["one".to_owned()], vec![1]));

    let event =
        crate::test_fixtures::live_query::next_run_is_only_the_request(&mut live, topic("b")).await;
    assert_eq!(event.result.unwrap(), (vec!["two".to_owned()], vec![1]));
}

#[tokio::test]
async fn a_live_statement_that_fails_to_read_a_row_fails_the_run() {
    let (store, _tmp) = notes_store().await;
    let mut live: coven::ReconfigurableLiveQuery<_, _, DbError> =
        store.subscribe_reconfigurable_statements::<_, Failing, Failing, _>(topic("a"));
    let first = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert!(matches!(first.result, Err(DbError::Sqlite(_))));
}
