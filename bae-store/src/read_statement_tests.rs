use crate::{Bound, DbError, ReadStatement, Schema, SchemaPart};
use bae_util::test_channels::HANG_GUARD;

static NOTES: &[SchemaPart] = &[SchemaPart::new(&[
    "CREATE TABLE notes (id INTEGER PRIMARY KEY, topic TEXT NOT NULL, body TEXT NOT NULL) STRICT;",
])];

static LIBRARY: &[&[SchemaPart]] = &[NOTES];

/// What a request for the notes of one topic binds.
#[derive(Clone, PartialEq)]
struct Topic {
    topic: String,
}

fn bind_topic(request: &Topic) -> Bound<'_> {
    vec![(":topic", &request.topic)]
}

fn read_body(row: &rusqlite::Row<'_>) -> rusqlite::Result<String> {
    row.get("body")
}

static BODIES: ReadStatement<Topic, String> = ReadStatement::new(
    "SELECT n.body FROM notes AS n WHERE n.topic = :topic ORDER BY n.id",
    bind_topic,
    read_body,
);

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

static TOPICS: ReadStatement<Topic, i64> = ReadStatement::new(
    "SELECT COUNT(*) AS notes FROM notes AS n WHERE n.topic = :topic",
    bind_topic,
    |row| row.get("notes"),
);

#[tokio::test]
async fn two_statements_answer_one_snapshot_and_rerun_for_a_write_to_what_either_read() {
    let schema = Schema::new(LIBRARY);
    let (store, _tmp) = crate::test_fixtures::temp_db(schema).await;
    let mut live: coven::ReconfigurableLiveQuery<_, _, DbError> =
        store.subscribe_reconfigurable_statements(Topic { topic: "a".into() }, &BODIES, &TOPICS);

    let first = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert_eq!(first.result.unwrap(), (Vec::<String>::new(), vec![0]));

    add_note(&store, "a", "one").await;
    let second = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert_eq!(second.result.unwrap(), (vec!["one".to_owned()], vec![1]));
}

#[tokio::test]
async fn two_statements_answer_each_request_they_are_given() {
    let schema = Schema::new(LIBRARY);
    let (store, _tmp) = crate::test_fixtures::temp_db(schema).await;
    add_note(&store, "a", "one").await;
    add_note(&store, "b", "two").await;
    let mut live: coven::ReconfigurableLiveQuery<_, _, DbError> =
        store.subscribe_reconfigurable_statements(Topic { topic: "a".into() }, &BODIES, &TOPICS);
    let first = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert_eq!(first.result.unwrap(), (vec!["one".to_owned()], vec![1]));

    let event = crate::test_fixtures::live_query::next_run_is_only_the_request(
        &mut live,
        Topic { topic: "b".into() },
    )
    .await;
    assert_eq!(event.result.unwrap(), (vec!["two".to_owned()], vec![1]));
}

#[tokio::test]
async fn a_statement_that_fails_to_read_a_row_fails_the_run() {
    static FAILING: ReadStatement<Topic, String> =
        ReadStatement::new("SELECT 1 AS body WHERE :topic IS NOT NULL", bind_topic, |row| {
            row.get::<_, String>("body")
        });
    let (store, _tmp) = crate::test_fixtures::temp_db(Schema::new(&[])).await;
    let mut live: coven::ReconfigurableLiveQuery<_, _, DbError> =
        store.subscribe_reconfigurable_statements(Topic { topic: "a".into() }, &FAILING, &FAILING);
    let first = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert!(matches!(first.result, Err(DbError::Sqlite(_))));
}
