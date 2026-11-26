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

#[tokio::test]
async fn a_statement_subscription_answers_its_rows_and_reruns_for_a_write_to_them() {
    let schema = Schema::new(LIBRARY);
    let (store, _tmp) = crate::test_fixtures::temp_db(schema).await;
    let mut live = store.subscribe_statement(&BODIES, Topic { topic: "a".into() });

    let first = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert_eq!(first.unwrap(), Vec::<String>::new());

    add_note(&store, "a", "one").await;
    let second = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert_eq!(second.unwrap(), vec!["one".to_owned()]);
}

#[tokio::test]
async fn a_reconfigurable_statement_answers_each_request_it_is_given() {
    let schema = Schema::new(LIBRARY);
    let (store, _tmp) = crate::test_fixtures::temp_db(schema).await;
    add_note(&store, "a", "one").await;
    add_note(&store, "b", "two").await;
    let mut live = store.subscribe_reconfigurable_statement(Topic { topic: "a".into() }, &BODIES);
    let first = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert_eq!(first.result.unwrap(), vec!["one".to_owned()]);

    let event = crate::test_fixtures::live_query::next_run_is_only_the_request(
        &mut live,
        Topic { topic: "b".into() },
    )
    .await;
    assert_eq!(event.result.unwrap(), vec!["two".to_owned()]);
}

#[tokio::test]
async fn a_statement_that_fails_to_read_a_row_fails_the_run() {
    static FAILING: ReadStatement<Topic, String> =
        ReadStatement::new("SELECT 1 AS body WHERE :topic IS NOT NULL", bind_topic, |row| {
            row.get::<_, String>("body")
        });
    let (store, _tmp) = crate::test_fixtures::temp_db(Schema::new(&[])).await;
    let mut live = store.subscribe_statement(&FAILING, Topic { topic: "a".into() });
    let first = tokio::time::timeout(HANG_GUARD, live.next_inline()).await.unwrap().unwrap();
    assert!(matches!(first, Err(DbError::Sqlite(_))));
}
