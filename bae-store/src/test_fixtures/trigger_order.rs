//! What only SQL shows of the app's triggers, for the tests that hold a
//! result right whichever of several triggers on one event SQLite runs first,
//! and that count how often writes ask a view's INSTEAD OF trigger to run.
//! SQLite promises no order among the triggers on one event; in practice it
//! runs the newest first. Coven changes no schema outside a migration, so
//! each instrument is a step after the schema's own, and the reads of what
//! it recorded are reads of the store.

use crate::{Schema, Store};

/// `schema`, its trigger `name` then created again from its own definition,
/// which makes it the first of the triggers on its event that SQLite runs.
pub fn run_trigger_first(schema: Schema, name: &str) -> Schema {
    let opening = format!("CREATE TRIGGER IF NOT EXISTS {name} ");
    let created = schema
        .sql()
        .find_map(|sql| {
            let start = sql.find(&opening)?;
            let end = start + sql[start..].find("\nEND;")? + "\nEND;".len();
            Some(sql[start..end].to_string())
        })
        .unwrap_or_else(|| panic!("the schema creates no trigger {name}"));
    schema.followed_by(format!("DROP TRIGGER {name};\n{created}\n"))
}

/// `schema`, noting the tables of `tables` that writes insert rows into, in
/// the order they do, for [`first_insert`] to read.
pub fn watch_first_insert(schema: Schema, tables: &[&str]) -> Schema {
    let mut sql = "CREATE TABLE watched_insert (step TEXT NOT NULL);\n".to_string();
    for table in tables {
        sql.push_str(&format!(
            "CREATE TRIGGER watched_insert_{table} AFTER INSERT ON {table}
             BEGIN
                 INSERT INTO watched_insert (step) VALUES ('{table}');
             END;\n"
        ));
    }
    schema.followed_by(sql)
}

/// The table of those [`watch_first_insert`] watches that the store's writes
/// inserted a row into first, if they inserted into any; the watch then
/// starts again.
pub async fn first_insert(store: &Store) -> Option<String> {
    store
        .call(|sql| {
            let first = sql
                .query("SELECT step FROM watched_insert ORDER BY rowid LIMIT 1", [], |row| {
                    row.get::<_, String>(0)
                })?
                .into_iter()
                .next();
            sql.execute("DELETE FROM watched_insert", [])?;
            Ok(first)
        })
        .await
        .unwrap()
}

/// `schema`, counting each row writes insert into the view `view` by its
/// `key` column, beside what the view's own INSTEAD OF trigger does with it,
/// for [`RefreshCount`] to read.
pub fn count_refreshes(schema: Schema, view: &str, key: &str) -> Schema {
    schema.followed_by(format!(
        "CREATE TABLE {view}_counted (key TEXT NOT NULL);
         CREATE TRIGGER {view}_counted INSTEAD OF INSERT ON {view}
         BEGIN
             INSERT INTO {view}_counted (key) VALUES (NEW.{key});
         END;\n"
    ))
}

/// How many times the store's writes asked the view [`count_refreshes`]
/// counts to refresh each key.
pub struct RefreshCount {
    table: String,
}

impl RefreshCount {
    /// The count [`count_refreshes`] keeps for the view `view`.
    pub fn of_view(view: &str) -> Self {
        Self { table: format!("{view}_counted") }
    }

    /// How many refreshes of `key` writes asked for since the count began or
    /// was last cleared.
    pub async fn of(&self, store: &Store, key: &str) -> i64 {
        let (statement, key) =
            (format!("SELECT count(*) FROM {} WHERE key = ?1", self.table), key.to_string());
        store
            .read(move |sql| Ok(sql.query_row(&statement, [&key], |row| row.get(0))?))
            .await
            .unwrap()
    }

    /// Start the count again from nothing.
    pub async fn clear(&self, store: &Store) {
        let statement = format!("DELETE FROM {}", self.table);
        store.call(move |sql| Ok(sql.execute(&statement, [])?)).await.unwrap();
    }
}
