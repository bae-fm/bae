//! SQLite's plan for a generated statement, for a test that checks how a read
//! finds its rows: by an index, or by reading a whole table.

/// The detail of each step of SQLite's plan for `statement` on `store`, run
/// with the statement's parameters, in plan order.
pub async fn query_plan<Q>(store: &crate::Store, statement: Q) -> Vec<String>
where
    Q: coven::Read<Handle = sqlive_reads::Writer> + Send + 'static,
{
    store
        .read(move |sql| {
            let explain = format!("EXPLAIN QUERY PLAN {}", <Q as coven::Statement>::SQL);
            let params = statement.params();
            Ok(sql.query(&explain, params.as_slice(), |row| row.get::<_, String>(3))?)
        })
        .await
        .expect("SQLite plans the statement")
}
